//! Reading another app's controls through UI Automation, the way the Mac reads them through its accessibility API.
//!
//! One call copies a window's whole control tree, with what's needed of each element: its type, name, help text,
//! frame and state. The walk then happens over that copy. The element behind every control is kept, so checking a
//! ref again just before a click is a single call; a full walk only happens when that element has gone or something
//! new has to be found. Ids, labels and matching are shared with the Mac ([`crate::controls`]). Field contents are
//! never read, only labels.
//!
//! Frames are in the desk's points: pixels divided by the target window's scale.

use std::collections::HashMap;

use shotmaxxing_core::geometry::Rect;
use shotmaxxing_core::refs::ControlSnapshot;
use windows::Win32::Foundation::{HWND, POINT, RECT};
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomation2, IUIAutomationCacheRequest, IUIAutomationElement,
    IUIAutomationValuePattern, TreeScope, TreeScope_Children, TreeScope_Element, TreeScope_Subtree,
    UIA_BoundingRectanglePropertyId, UIA_ButtonControlTypeId, UIA_CONTROLTYPE_ID, UIA_CheckBoxControlTypeId,
    UIA_ComboBoxControlTypeId, UIA_ControlTypePropertyId, UIA_DataItemControlTypeId,
    UIA_DocumentControlTypeId, UIA_EditControlTypeId, UIA_HelpTextPropertyId, UIA_HyperlinkControlTypeId,
    UIA_IsEnabledPropertyId, UIA_IsPasswordPropertyId, UIA_ListItemControlTypeId, UIA_MenuBarControlTypeId,
    UIA_MenuControlTypeId, UIA_MenuItemControlTypeId, UIA_NamePropertyId, UIA_PROPERTY_ID,
    UIA_ProcessIdPropertyId, UIA_RadioButtonControlTypeId, UIA_SliderControlTypeId, UIA_SpinnerControlTypeId,
    UIA_SplitButtonControlTypeId, UIA_TabItemControlTypeId, UIA_TextControlTypeId, UIA_TreeItemControlTypeId,
    UIA_ValueIsReadOnlyPropertyId, UIA_ValuePatternId,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GA_ROOT, GetAncestor, GetWindowThreadProcessId, WindowFromPoint,
};
use windows::core::{IUnknown, Interface};

use crate::controls::{LIMIT, MAX_DEPTH, MAX_VISITS, best_match, clean_label, make_id};

/// What's read of each element.
const PROPERTIES: [UIA_PROPERTY_ID; 8] = [
    UIA_ControlTypePropertyId,
    UIA_NamePropertyId,
    UIA_HelpTextPropertyId,
    UIA_BoundingRectanglePropertyId,
    UIA_IsEnabledPropertyId,
    UIA_IsPasswordPropertyId,
    UIA_ProcessIdPropertyId,
    UIA_ValueIsReadOnlyPropertyId,
];

/// UI Automation, set so that a frozen app can't stall a take.
pub fn automation() -> Option<IUIAutomation> {
    // SAFETY: creates the automation object and sets its timeouts.
    unsafe {
        let automation: IUIAutomation =
            CoCreateInstance(&CUIAutomation, None::<&IUnknown>, CLSCTX_INPROC_SERVER).ok()?;
        if let Ok(timed) = automation.cast::<IUIAutomation2>() {
            let _ = timed.SetConnectionTimeout(500);
            let _ = timed.SetTransactionTimeout(1_000);
        }
        Some(automation)
    }
}

/// Whether the control with the keyboard is a password field.
pub fn password_focused(automation: &IUIAutomation) -> bool {
    // SAFETY: two questions to UI Automation.
    unsafe { automation.GetFocusedElement().and_then(|e| e.CurrentIsPassword()).is_ok_and(|p| p.as_bool()) }
}

fn request(automation: &IUIAutomation, scope: TreeScope) -> Option<IUIAutomationCacheRequest> {
    // SAFETY: builds a cache request from valid objects.
    unsafe {
        let request = automation.CreateCacheRequest().ok()?;
        for property in PROPERTIES {
            request.AddProperty(property).ok()?;
        }
        request.AddPattern(UIA_ValuePatternId).ok()?;
        request.SetTreeScope(scope).ok()?;
        // What a person sees as the interface, without the layout panes in between.
        request.SetTreeFilter(&automation.ControlViewCondition().ok()?).ok()?;
        Some(request)
    }
}

/// One element, as copied.
struct Node {
    kind: UIA_CONTROLTYPE_ID,
    label: Option<String>,
    frame: Option<Rect>,
    enabled: bool,
    password: bool,
    /// Whether it takes typing: a value that isn't read-only.
    editable: bool,
}

/// A control from the last walk: the element behind it, its role and its label.
struct Known {
    element: IUIAutomationElement,
    role: String,
    label: Option<String>,
}

/// The controls of one app's window, with the elements behind them.
pub struct Tree {
    automation: IUIAutomation,
    window: HWND,
    /// The process that owns the window.
    pid: u32,
    /// Pixels per point.
    scale: f64,
    /// Copies a whole subtree, or one element.
    subtree: IUIAutomationCacheRequest,
    single: IUIAutomationCacheRequest,
    known: HashMap<String, Known>,
    pub truncated: bool,
}

impl Tree {
    pub fn new(automation: IUIAutomation, window: HWND, scale: f64) -> Option<Self> {
        let mut pid = 0u32;
        // SAFETY: a question about a window handle.
        unsafe { GetWindowThreadProcessId(window, Some(&mut pid)) };
        let subtree = request(&automation, TreeScope_Subtree)?;
        let single = request(&automation, TreeScope_Element)?;
        Some(Self {
            automation,
            window,
            pid,
            scale,
            subtree,
            single,
            known: HashMap::new(),
            truncated: false,
        })
    }

    /// A pixel rectangle in points, or `None` when it has no size.
    fn points(&self, r: RECT) -> Option<Rect> {
        let (width, height) =
            (f64::from(r.right - r.left) / self.scale, f64::from(r.bottom - r.top) / self.scale);
        (width >= 1.0 && height >= 1.0)
            .then(|| Rect::new(f64::from(r.left) / self.scale, f64::from(r.top) / self.scale, width, height))
    }

    fn read(&self, element: &IUIAutomationElement) -> Option<Node> {
        // SAFETY: reads values copied with the element; nothing here crosses to the app.
        unsafe {
            let kind = element.CachedControlType().ok()?;
            let name = element.CachedName().ok().map(|b| b.to_string());
            let help = element.CachedHelpText().ok().map(|b| b.to_string());
            let editable = element
                .GetCachedPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
                .and_then(|value| value.CachedIsReadOnly())
                .is_ok_and(|read_only| !read_only.as_bool());
            Some(Node {
                kind,
                label: [name, help].into_iter().flatten().find_map(|l| clean_label(&l)),
                frame: element.CachedBoundingRectangle().ok().and_then(|r| self.points(r)),
                enabled: element.CachedIsEnabled().map_or(true, |e| e.as_bool()),
                password: element.CachedIsPassword().is_ok_and(|p| p.as_bool()),
                editable,
            })
        }
    }

    /// The app's name, as its window shows it.
    pub fn app_name(&self) -> Option<String> {
        // SAFETY: two questions to UI Automation.
        let title = unsafe { self.automation.ElementFromHandle(self.window).and_then(|w| w.CurrentName()) };
        title.ok().map(|t| t.to_string()).filter(|t| !t.trim().is_empty())
    }

    /// The app's open menus, which Windows shows as windows of their own.
    fn menus(&self) -> Vec<IUIAutomationElement> {
        // SAFETY: questions to UI Automation with valid objects.
        unsafe {
            let Ok(root) = self.automation.GetRootElement() else { return Vec::new() };
            let Ok(all) = self.automation.CreateTrueCondition() else { return Vec::new() };
            let Ok(top) = root.FindAllBuildCache(TreeScope_Children, &all, &self.single) else {
                return Vec::new();
            };
            (0..top.Length().unwrap_or(0))
                .filter_map(|i| top.GetElement(i).ok())
                .filter(|e| e.CachedProcessId().is_ok_and(|pid| pid as u32 == self.pid))
                .filter(|e| e.CachedControlType().is_ok_and(|kind| kind == UIA_MenuControlTypeId))
                .filter_map(|menu| menu.BuildUpdatedCache(&self.subtree).ok())
                .collect()
        }
    }

    /// Every control in the window and its open menus, in reading order.
    pub fn snapshot(&mut self) -> Vec<ControlSnapshot> {
        self.known.clear();
        self.truncated = false;
        // SAFETY: copies the window's tree in one call.
        let window = unsafe { self.automation.ElementFromHandleBuildCache(self.window, &self.subtree) };
        let roots: Vec<IUIAutomationElement> = window.into_iter().chain(self.menus()).collect();
        let mut controls = Vec::new();
        let mut taken = HashMap::new();
        let mut stack: Vec<(IUIAutomationElement, usize, bool)> =
            roots.into_iter().rev().map(|e| (e, 0, false)).collect();
        let mut visits = 0;
        while let Some((element, depth, in_menu_bar)) = stack.pop() {
            visits += 1;
            if visits > MAX_VISITS || controls.len() >= LIMIT {
                self.truncated = true;
                break;
            }
            let Some(node) = self.read(&element) else { continue };
            if let Some(role) = role(&node, in_menu_bar)
                && let Some(frame) = node.frame
            {
                let label = node.label.clone().or_else(|| words_inside(&element));
                let id = make_id(&role, label.as_deref(), &mut taken);
                controls.push(ControlSnapshot {
                    id: id.clone(),
                    role: role.clone(),
                    label: label.clone(),
                    enabled: node.enabled,
                    bounds: frame,
                    window_id: None,
                    occluded: false,
                });
                self.known.insert(id, Known { element: element.clone(), role, label });
            }
            if depth < MAX_DEPTH {
                let bar = node.kind == UIA_MenuBarControlTypeId;
                stack.extend(children(&element).into_iter().rev().map(|child| (child, depth + 1, bar)));
            }
        }
        controls
    }

    /// Whether another app's window covers a point. The window at the point is compared rather than the element:
    /// web content in apps built on WebView2 belongs to another process but sits in the app's own window.
    fn covered(&self, at: Rect) -> bool {
        let centre = at.center();
        let point =
            POINT { x: (centre.x * self.scale).round() as i32, y: (centre.y * self.scale).round() as i32 };
        // SAFETY: questions about the window under a point.
        unsafe {
            let hit = WindowFromPoint(point);
            if hit.is_invalid() {
                return false;
            }
            let top = GetAncestor(hit, GA_ROOT);
            let mut pid = 0u32;
            GetWindowThreadProcessId(top, Some(&mut pid));
            top != self.window && pid != self.pid
        }
    }

    /// A control as it is now, by its id from the last walk.
    pub fn control(&mut self, id: &str) -> Option<ControlSnapshot> {
        let fresh = self.known.get(id).and_then(|known| {
            // SAFETY: copies one element again, in a single call.
            let element = unsafe { known.element.BuildUpdatedCache(&self.single) }.ok()?;
            let node = self.read(&element)?;
            // Still the same kind of control.
            (role(&node, known.role == "menu_bar_item")? == known.role).then_some(())?;
            Some(ControlSnapshot {
                id: id.to_string(),
                role: known.role.clone(),
                label: node.label.or_else(|| known.label.clone()),
                enabled: node.enabled,
                bounds: node.frame?,
                window_id: None,
                occluded: false,
            })
        });
        // The element went away (or this id is new): look again from the top.
        let mut control = fresh.or_else(|| self.snapshot().into_iter().find(|c| c.id == id))?;
        control.occluded = self.covered(control.bounds);
        Some(control)
    }

    /// The control best matching an id or a label.
    pub fn find(&mut self, query: &str) -> Option<ControlSnapshot> {
        let controls = self.snapshot();
        best_match(&controls, query).cloned()
    }
}

/// The copied children of an element.
fn children(element: &IUIAutomationElement) -> Vec<IUIAutomationElement> {
    // SAFETY: reads the copy made with the element.
    unsafe {
        let Ok(array) = element.GetCachedChildren() else { return Vec::new() };
        (0..array.Length().unwrap_or(0)).filter_map(|i| array.GetElement(i).ok()).collect()
    }
}

/// The words inside a control that has no label of its own, such as a link around some text.
fn words_inside(element: &IUIAutomationElement) -> Option<String> {
    let mut queue = children(element);
    for _ in 0..8 {
        if queue.is_empty() {
            break;
        }
        let next = queue.remove(0);
        // SAFETY: reads values copied with the element.
        let (kind, name) = unsafe { (next.CachedControlType(), next.CachedName()) };
        if kind.is_ok_and(|k| k == UIA_TextControlTypeId)
            && let Some(words) = name.ok().and_then(|n| clean_label(&n.to_string()))
        {
            return Some(words);
        }
        queue.extend(children(&next));
    }
    None
}

/// The plain role name for a control, the same names the Mac uses, or `None` for elements that aren't controls.
// The patterns are Windows' own constant names.
#[allow(non_upper_case_globals)]
fn role(node: &Node, in_menu_bar: bool) -> Option<String> {
    let role = match node.kind {
        UIA_ButtonControlTypeId => "button",
        UIA_CheckBoxControlTypeId => "checkbox",
        UIA_RadioButtonControlTypeId => "option",
        UIA_TabItemControlTypeId => "tab",
        UIA_ComboBoxControlTypeId => "combo_box",
        UIA_EditControlTypeId if node.password => "password_field",
        UIA_EditControlTypeId => "text_field",
        // A web page is a document too, but only one that takes typing is a text area.
        UIA_DocumentControlTypeId if node.editable => "text_area",
        UIA_HyperlinkControlTypeId => "link",
        UIA_MenuItemControlTypeId if in_menu_bar => "menu_bar_item",
        UIA_MenuItemControlTypeId => "menu_item",
        UIA_SliderControlTypeId => "slider",
        UIA_SpinnerControlTypeId => "stepper",
        UIA_SplitButtonControlTypeId => "menu_button",
        UIA_ListItemControlTypeId | UIA_TreeItemControlTypeId | UIA_DataItemControlTypeId => "row",
        _ => return None,
    };
    Some(role.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(kind: UIA_CONTROLTYPE_ID) -> Node {
        Node { kind, label: None, frame: None, enabled: true, password: false, editable: false }
    }

    #[test]
    fn roles_use_the_macs_names() {
        assert_eq!(role(&node(UIA_ButtonControlTypeId), false).as_deref(), Some("button"));
        assert_eq!(
            role(&Node { password: true, ..node(UIA_EditControlTypeId) }, false).as_deref(),
            Some("password_field")
        );
        assert_eq!(role(&node(UIA_MenuItemControlTypeId), true).as_deref(), Some("menu_bar_item"));
        assert_eq!(role(&node(UIA_DocumentControlTypeId), false), None, "a web page isn't a text area");
        assert_eq!(
            role(&Node { editable: true, ..node(UIA_DocumentControlTypeId) }, false).as_deref(),
            Some("text_area")
        );
        assert_eq!(role(&node(UIA_TextControlTypeId), false), None);
    }
}
