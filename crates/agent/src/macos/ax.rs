//! Reading another app's controls through the accessibility API.
//!
//! Each element costs one round trip: `AXUIElementCopyMultipleAttributeValues` fetches its role, children, frame,
//! labels and state together. The element behind every control is kept, so checking a ref again just before a click
//! is a single call. A full walk only happens when that element has gone or something new has to be found.
//!
//! Ids, labels and matching are shared with the other systems (see [`crate::controls`]). Field contents are never
//! read, only labels.

use std::collections::HashMap;
use std::ffi::c_void;

use objc2_core_foundation::{CFRetained, CFString};
use shotmaxxing_core::geometry::Rect;
use shotmaxxing_core::refs::ControlSnapshot;

use crate::controls::{LIMIT, MAX_DEPTH, MAX_VISITS, best_match, clean_label, make_id};

type Ref = *const c_void;

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    safe fn AXIsProcessTrusted() -> bool;
    fn AXUIElementCreateApplication(pid: i32) -> Ref;
    fn AXUIElementCreateSystemWide() -> Ref;
    fn AXUIElementCopyAttributeValue(element: Ref, attribute: Ref, value: *mut Ref) -> i32;
    fn AXUIElementCopyMultipleAttributeValues(
        element: Ref,
        attributes: Ref,
        options: u32,
        values: *mut Ref,
    ) -> i32;
    fn AXUIElementCopyElementAtPosition(element: Ref, x: f32, y: f32, out: *mut Ref) -> i32;
    fn AXUIElementGetPid(element: Ref, pid: *mut i32) -> i32;
    fn AXUIElementPerformAction(element: Ref, action: Ref) -> i32;
    fn AXUIElementSetAttributeValue(element: Ref, attribute: Ref, value: Ref) -> i32;
    fn AXUIElementSetMessagingTimeout(element: Ref, seconds: f32) -> i32;
    fn AXValueGetTypeID() -> usize;
    fn AXValueGetType(value: Ref) -> u32;
    fn AXValueGetValue(value: Ref, kind: u32, out: *mut c_void) -> bool;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    static kCFTypeArrayCallBacks: c_void;
    static kCFBooleanTrue: Ref;
    fn CFArrayCreate(allocator: Ref, values: *const Ref, count: isize, callbacks: *const c_void) -> Ref;
    fn CFArrayGetCount(array: Ref) -> isize;
    fn CFArrayGetValueAtIndex(array: Ref, index: isize) -> Ref;
    fn CFGetTypeID(object: Ref) -> usize;
    fn CFStringGetTypeID() -> usize;
    fn CFArrayGetTypeID() -> usize;
    fn CFBooleanGetTypeID() -> usize;
    fn CFBooleanGetValue(boolean: Ref) -> bool;
    fn CFRetain(object: Ref) -> Ref;
    fn CFRelease(object: Ref);
    fn CFHash(object: Ref) -> usize;
    fn CFEqual(a: Ref, b: Ref) -> bool;
}

const AX_POINT: u32 = 1;
const AX_SIZE: u32 = 2;
const AX_ERROR: u32 = 5;

/// Whether this process may read other apps' controls and post input.
pub fn trusted() -> bool {
    AXIsProcessTrusted()
}

/// A Core Foundation object this code owns one reference to.
struct Owned(Ref);

impl Owned {
    fn new(r: Ref) -> Option<Self> {
        (!r.is_null()).then_some(Self(r))
    }

    /// Takes a reference to an object owned by someone else.
    fn retain(r: Ref) -> Self {
        // SAFETY: `r` is a valid, non-null CF object; the retain is balanced by Drop.
        unsafe { CFRetain(r) };
        Self(r)
    }
}

impl Clone for Owned {
    fn clone(&self) -> Self {
        Self::retain(self.0)
    }
}

impl Drop for Owned {
    fn drop(&mut self) {
        // SAFETY: we own exactly one reference.
        unsafe { CFRelease(self.0) };
    }
}

fn cf_string(text: &str) -> CFRetained<CFString> {
    CFString::from_str(text)
}

fn as_ref(string: &CFRetained<CFString>) -> Ref {
    CFRetained::as_ptr(string).as_ptr() as Ref
}

/// Reads a CF value as a string, if it is one.
fn string_of(value: Ref) -> Option<String> {
    // SAFETY: `value` is a valid CF object; a CFString is read through its toll-free Rust type.
    unsafe { (CFGetTypeID(value) == CFStringGetTypeID()).then(|| (*(value as *const CFString)).to_string()) }
}

fn copy(element: Ref, attribute: &str) -> Option<Owned> {
    let name = cf_string(attribute);
    let mut value: Ref = std::ptr::null();
    // SAFETY: valid element and attribute; the out pointer is ours and the result is owned (Copy rule).
    let status = unsafe { AXUIElementCopyAttributeValue(element, as_ref(&name), &mut value) };
    if status == 0 { Owned::new(value) } else { None }
}

fn copy_elements(element: Ref, attribute: &str) -> Vec<Owned> {
    let Some(array) = copy(element, attribute) else { return Vec::new() };
    elements_in(array.0)
}

fn elements_in(array: Ref) -> Vec<Owned> {
    // SAFETY: `array` is a valid CF object; its items are borrowed and retained before use.
    unsafe {
        if CFGetTypeID(array) != CFArrayGetTypeID() {
            return Vec::new();
        }
        (0..CFArrayGetCount(array))
            .map(|i| CFArrayGetValueAtIndex(array, i))
            .filter(|v| !v.is_null())
            .map(Owned::retain)
            .collect()
    }
}

const NAMES: [&str; 10] = [
    "AXRole",
    "AXSubrole",
    "AXChildren",
    "AXPosition",
    "AXSize",
    "AXTitle",
    "AXDescription",
    "AXPlaceholderValue",
    "AXEnabled",
    "AXHelp",
];

/// One element's attributes, read in a single call.
struct Node {
    role: String,
    subrole: Option<String>,
    children: Vec<Owned>,
    frame: Option<Rect>,
    title: Option<String>,
    description: Option<String>,
    placeholder: Option<String>,
    help: Option<String>,
    enabled: Option<bool>,
}

impl Node {
    fn label(&self) -> Option<String> {
        [&self.title, &self.description, &self.placeholder, &self.help]
            .into_iter()
            .flatten()
            .find_map(|l| clean_label(l))
    }
}

/// The controls of one app's window, with the elements behind them.
pub struct Tree {
    pid: i32,
    app: Owned,
    system: Owned,
    names: Owned,
    /// The window to look in, roughly: where the recorder sees it.
    hint: Option<Rect>,
    cache: HashMap<String, Owned>,
    pub truncated: bool,
}

impl Tree {
    pub fn new(pid: i32, hint: Option<Rect>) -> Option<Self> {
        // SAFETY: creating AX elements (owned, +1) and the attribute-name array (owned; it retains the strings).
        unsafe {
            let app = Owned::new(AXUIElementCreateApplication(pid))?;
            let system = Owned::new(AXUIElementCreateSystemWide())?;
            // A frozen app mustn't stall the take.
            AXUIElementSetMessagingTimeout(app.0, 0.5);
            AXUIElementSetMessagingTimeout(system.0, 0.25);
            let strings: Vec<CFRetained<CFString>> = NAMES.iter().map(|n| cf_string(n)).collect();
            let refs: Vec<Ref> = strings.iter().map(as_ref).collect();
            let names = Owned::new(CFArrayCreate(
                std::ptr::null(),
                refs.as_ptr(),
                refs.len() as isize,
                &raw const kCFTypeArrayCallBacks,
            ))?;
            Some(Self { pid, app, system, names, hint, cache: HashMap::new(), truncated: false })
        }
    }

    fn read(&self, element: &Owned) -> Option<Node> {
        let mut out: Ref = std::ptr::null();
        // SAFETY: valid element and name array; the result array is owned.
        let status = unsafe { AXUIElementCopyMultipleAttributeValues(element.0, self.names.0, 0, &mut out) };
        let values = (status == 0).then(|| Owned::new(out)).flatten()?;
        // SAFETY: reading items of the result array; each is a CF object or an AXValue holding an error.
        let item = |i: usize| -> Option<Ref> {
            unsafe {
                if CFArrayGetCount(values.0) <= i as isize {
                    return None;
                }
                let v = CFArrayGetValueAtIndex(values.0, i as isize);
                if v.is_null() || (CFGetTypeID(v) == AXValueGetTypeID() && AXValueGetType(v) == AX_ERROR) {
                    None
                } else {
                    Some(v)
                }
            }
        };
        let pair = |i: usize, kind: u32| -> Option<(f64, f64)> {
            let v = item(i)?;
            let mut out = [0.0f64; 2];
            // SAFETY: an AXValue of `kind` holds a CGPoint or CGSize: two f64s.
            let ok = unsafe {
                CFGetTypeID(v) == AXValueGetTypeID()
                    && AXValueGetValue(v, kind, out.as_mut_ptr() as *mut c_void)
            };
            ok.then_some((out[0], out[1]))
        };
        let text =
            |i: usize| item(i).and_then(string_of).map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
        let frame = match (pair(3, AX_POINT), pair(4, AX_SIZE)) {
            (Some((x, y)), Some((w, h))) => Some(Rect::new(x, y, w, h)),
            _ => None,
        };
        Some(Node {
            role: text(0)?,
            subrole: text(1),
            children: item(2).map(elements_in).unwrap_or_default(),
            frame,
            title: text(5),
            description: text(6),
            placeholder: text(7),
            enabled: item(8).and_then(|v| {
                // SAFETY: checking the type before reading a CFBoolean.
                unsafe { (CFGetTypeID(v) == CFBooleanGetTypeID()).then(|| CFBooleanGetValue(v)) }
            }),
            help: text(9),
        })
    }

    /// The words inside a control that has no label of its own, such as a link's text.
    fn words_inside(&self, node: &Node) -> Option<String> {
        let mut queue: Vec<Owned> = node.children.clone();
        for _ in 0..8 {
            if queue.is_empty() {
                break;
            }
            let element = queue.remove(0);
            let role = copy(element.0, "AXRole").and_then(|r| string_of(r.0));
            if role.as_deref() == Some("AXStaticText") {
                let words = copy(element.0, "AXValue").and_then(|v| string_of(v.0));
                if let Some(words) = words.as_deref().and_then(clean_label) {
                    return Some(words);
                }
            }
            queue.extend(copy_elements(element.0, "AXChildren"));
        }
        None
    }

    /// The window to look in: the one where the recorder sees it, else the focused or main one.
    fn window(&self) -> Option<Owned> {
        let windows = copy_elements(self.app.0, "AXWindows");
        if let Some(hint) = self.hint {
            let off = |r: Rect| (r.x - hint.x).abs() + (r.y - hint.y).abs() + (r.width - hint.width).abs();
            let best = windows
                .iter()
                .filter_map(|w| self.read(w).and_then(|n| n.frame).map(|f| (w, off(f))))
                .min_by(|a, b| a.1.total_cmp(&b.1));
            if let Some((window, _)) = best {
                return Some(window.clone());
            }
        }
        // A locked screen answers with the app itself, which isn't a window.
        let is_window =
            |w: &Owned| copy(w.0, "AXRole").and_then(|r| string_of(r.0)).as_deref() == Some("AXWindow");
        copy(self.app.0, "AXFocusedWindow")
            .filter(is_window)
            .or_else(|| copy(self.app.0, "AXMainWindow").filter(is_window))
            .or_else(|| windows.into_iter().find(is_window))
    }

    /// The app's name, as it shows in the menu bar.
    pub fn app_name(&self) -> Option<String> {
        copy(self.app.0, "AXTitle").and_then(|t| string_of(t.0))
    }

    /// Brings the window forward.
    pub fn raise(&self) {
        let frontmost = cf_string("AXFrontmost");
        let raise = cf_string("AXRaise");
        // SAFETY: valid elements and CF strings; kCFBooleanTrue is a constant.
        unsafe {
            AXUIElementSetAttributeValue(self.app.0, as_ref(&frontmost), kCFBooleanTrue);
            if let Some(window) = self.window() {
                AXUIElementPerformAction(window.0, as_ref(&raise));
            }
        }
    }

    /// Every control in the window and its menus, in reading order.
    pub fn snapshot(&mut self) -> Vec<ControlSnapshot> {
        self.cache.clear();
        self.truncated = false;
        let mut roots: Vec<Owned> = self.window().into_iter().collect();
        // The menu bar, and menus that are open (a closed menu has no size, so it is skipped below).
        for child in copy_elements(self.app.0, "AXChildren") {
            let role = copy(child.0, "AXRole").and_then(|r| string_of(r.0));
            if matches!(role.as_deref(), Some("AXMenuBar" | "AXMenu")) {
                roots.push(child);
            }
        }
        let mut controls = Vec::new();
        let mut taken: HashMap<String, usize> = HashMap::new();
        let mut stack: Vec<(Owned, usize)> = roots.into_iter().rev().map(|e| (e, 0)).collect();
        // Some apps list an ancestor among an element's children; each element is visited once.
        let mut seen: HashMap<usize, Vec<Owned>> = HashMap::new();
        let mut visits = 0;
        while let Some((element, depth)) = stack.pop() {
            // SAFETY: hashing and comparing valid CF objects.
            let hash = unsafe { CFHash(element.0) };
            let twins = seen.entry(hash).or_default();
            // SAFETY: as above.
            if twins.iter().any(|t| unsafe { CFEqual(t.0, element.0) }) {
                continue;
            }
            twins.push(element.clone());
            visits += 1;
            if visits > MAX_VISITS || controls.len() >= LIMIT {
                self.truncated = true;
                break;
            }
            let Some(node) = self.read(&element) else { continue };
            // Closed menus have no size, and an app element inside a window leads back to the start.
            let closed_menu =
                node.role == "AXMenu" && node.frame.is_none_or(|f| f.width < 1.0 || f.height < 1.0);
            if closed_menu || (node.role == "AXApplication" && depth > 0) {
                continue;
            }
            if let Some(role) = control_role(&node)
                && let Some(frame) = node.frame.filter(|f| f.width >= 1.0 && f.height >= 1.0)
            {
                let label = node.label().or_else(|| self.words_inside(&node));
                let id = make_id(&role, label.as_deref(), &mut taken);
                controls.push(ControlSnapshot {
                    id: id.clone(),
                    role,
                    label,
                    enabled: node.enabled.unwrap_or(true),
                    bounds: frame,
                    window_id: None,
                    occluded: false,
                });
                self.cache.insert(id, element.clone());
            }
            if depth < MAX_DEPTH {
                stack.extend(node.children.into_iter().rev().map(|child| (child, depth + 1)));
            }
        }
        controls
    }

    /// Whether another app's window covers a point.
    fn covered(&self, at: Rect) -> bool {
        let centre = at.center();
        let mut hit: Ref = std::ptr::null();
        // SAFETY: hit-testing with the system-wide element; the result is owned and released.
        unsafe {
            if AXUIElementCopyElementAtPosition(self.system.0, centre.x as f32, centre.y as f32, &mut hit)
                != 0
            {
                return false;
            }
            let Some(hit) = Owned::new(hit) else { return false };
            let mut pid = 0;
            AXUIElementGetPid(hit.0, &mut pid) == 0 && pid != self.pid
        }
    }

    /// A control as it is now, by its id from the last walk.
    pub fn control(&mut self, id: &str) -> Option<ControlSnapshot> {
        let fresh = self.cache.get(id).cloned().and_then(|element| {
            let node = self.read(&element)?;
            let role = control_role(&node)?;
            let frame = node.frame.filter(|f| f.width >= 1.0 && f.height >= 1.0)?;
            let label = node.label().or_else(|| self.words_inside(&node));
            Some(ControlSnapshot {
                id: id.to_string(),
                role,
                label,
                enabled: node.enabled.unwrap_or(true),
                bounds: frame,
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

/// The plain role name for a control, or `None` for elements that aren't controls.
fn control_role(node: &Node) -> Option<String> {
    let sub = node.subrole.as_deref().unwrap_or("");
    let role = match node.role.as_str() {
        "AXButton" => "button",
        "AXCheckBox" if sub == "AXSwitch" => "switch",
        "AXCheckBox" => "checkbox",
        "AXRadioButton" if sub == "AXTabButton" => "tab",
        "AXRadioButton" => "option",
        "AXPopUpButton" => "popup",
        "AXMenuButton" => "menu_button",
        "AXComboBox" => "combo_box",
        "AXTextField" if sub == "AXSecureTextField" => "password_field",
        "AXTextField" if sub == "AXSearchField" => "search_field",
        "AXTextField" => "text_field",
        "AXTextArea" => "text_area",
        "AXLink" => "link",
        "AXMenuItem" => "menu_item",
        "AXMenuBarItem" => "menu_bar_item",
        "AXSlider" => "slider",
        "AXIncrementor" => "stepper",
        "AXDisclosureTriangle" => "disclosure",
        "AXCell" => "cell",
        "AXRow" => "row",
        "AXColorWell" => "color_well",
        _ => return None,
    };
    Some(role.to_string())
}
