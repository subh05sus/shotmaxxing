//! The plan check: a take script compared with the app on screen now, before filming, so a take doesn't stop
//! halfway. Every control the script names is looked up the way the take will look it up when it gets there: refs
//! against what `prepare` listed, labels with the same matching as `find`. What would stop the take comes back as
//! a problem, with something to try instead when the screen has a likely match.

use serde::Serialize;
use shotmaxxing_capture::WindowInfo;
use shotmaxxing_core::geometry::Point;
use shotmaxxing_core::refs::{self, ControlSnapshot, RefStatus};
use shotmaxxing_core::take::{Beat, TakeScript, TargetSpec, TypeBeat, WaitBeat, WaitCondition};

use crate::controls::best_match;

/// How alike two labels must be (0 to 1) for one to be offered in place of the other.
const ALIKE: f64 = 0.6;

/// What the plan check found.
#[derive(Clone, Debug, Default, Serialize)]
pub struct Check {
    /// Nothing on screen now would stop the take.
    pub ready: bool,
    /// What would stop the take as the screen is now, each with something to try instead when there is one.
    pub problems: Vec<Finding>,
    /// What can only be known when the take gets there, and other things worth knowing.
    pub notes: Vec<Finding>,
    /// How many controls and points the script names.
    pub targets: usize,
}

/// One thing the check found.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Finding {
    /// The beat, counting from 1. 0 is the take as a whole.
    pub beat: usize,
    pub kind: FindingKind,
    pub message: String,
    /// What to try instead.
    #[serde(rename = "try", skip_serializing_if = "Option::is_none")]
    pub instead: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FindingKind {
    /// The app has no window to film.
    NoWindow,
    /// A ref that isn't on screen now.
    Missing,
    /// A ref whose control now says something else.
    Renamed,
    /// A ref that `prepare` never listed.
    NotPrepared,
    /// A control something else covers, with the app in front.
    Covered,
    /// A control that's disabled right now.
    Disabled,
    /// Typing that would land in a password field.
    Password,
    /// A point outside the window the take films.
    OutsideWindow,
    /// A label to find that isn't on screen yet.
    NotYet,
}

/// Checks `script` against `live`, the controls on screen now, in `window`, the window the take films. `planned` is
/// what `prepare` listed when the agent planned the take (empty when it didn't), the same list the take checks refs
/// against.
pub fn check(
    script: &TakeScript,
    planned: &[ControlSnapshot],
    live: &[ControlSnapshot],
    window: Option<&WindowInfo>,
) -> Check {
    let mut checker = Checker { planned, live, window, check: Check::default() };
    let app =
        window.map_or_else(|| script.app.clone().unwrap_or_else(|| "the app".into()), |w| w.app.clone());
    if window.is_none() {
        checker.problem(
            0,
            FindingKind::NoWindow,
            format!("{app} has no window Shotmaxxing can film right now."),
            Some(format!("Open a window in {app} (or bring it back from the Dock), then check again.")),
        );
    }
    // The control that has the keyboard, as far as the script tells: typing goes there.
    let mut focus: Option<ControlSnapshot> = None;
    for (index, beat) in script.beats.iter().enumerate() {
        let n = index + 1;
        match beat {
            Beat::Click(spec) | Beat::DoubleClick(spec) | Beat::RightClick(spec) => {
                focus = checker.target(n, spec, &app);
            }
            Beat::Hover(spec) => {
                checker.target(n, spec, &app);
            }
            Beat::Drag(drag) => {
                checker.target(n, &drag.from, &app);
                checker.target(n, &drag.to, &app);
            }
            Beat::Type(typing) => {
                if let TypeBeat::Full { into: Some(spec), .. } = typing {
                    focus = checker.target(n, spec, &app);
                }
                if let Some(field) = focus.as_ref().filter(|c| c.role == "password_field") {
                    checker.problem(
                        n,
                        FindingKind::Password,
                        format!(
                            "This types into {}, and Shotmaxxing never types into a password field.",
                            field.name()
                        ),
                        Some(
                            "Make this step a user_step beat (e.g. user_step: \"Type the password\"), so the person \
                             types it and the take carries on after."
                                .into(),
                        ),
                    );
                }
                // A Tab moves the keyboard somewhere the script doesn't say.
                if typing.text().contains('\t') {
                    focus = None;
                }
            }
            Beat::Scroll(scroll) => {
                if let Some(at) = &scroll.at {
                    checker.target(n, at, &app);
                }
            }
            Beat::Wait(WaitBeat::Full { until: Some(WaitCondition::Find { find }), .. }) => {
                checker.find(n, find);
            }
            // Keys can move the keyboard (Tab, a shortcut that opens a field), and the person can do anything.
            Beat::Key(_) | Beat::UserStep(_) => focus = None,
            Beat::Wait(_) | Beat::Say(_) => {}
        }
    }
    let mut check = checker.check;
    check.ready = check.problems.is_empty();
    check
}

struct Checker<'a> {
    planned: &'a [ControlSnapshot],
    live: &'a [ControlSnapshot],
    window: Option<&'a WindowInfo>,
    check: Check,
}

impl Checker<'_> {
    fn problem(&mut self, beat: usize, kind: FindingKind, message: String, instead: Option<String>) {
        self.check.problems.push(Finding { beat, kind, message, instead });
    }

    fn note(&mut self, beat: usize, kind: FindingKind, message: String, instead: Option<String>) {
        self.check.notes.push(Finding { beat, kind, message, instead });
    }

    /// Looks one target up as the take will, and returns the control it lands on now, if any.
    fn target(&mut self, beat: usize, spec: &TargetSpec, app: &str) -> Option<ControlSnapshot> {
        self.check.targets += 1;
        let target = spec.target();
        if let Some(point) = target.point {
            if let Some(window) = self.window
                && !inside(window, point)
            {
                self.note(
                    beat,
                    FindingKind::OutsideWindow,
                    format!(
                        "The point {:.0}, {:.0} is outside the {app} window, so this lands on whatever is there, \
                         and a window take doesn't show it.",
                        point.x, point.y
                    ),
                    Some("Use a ref or { find: \"label\" } from prepare_take, or a point inside the window.".into()),
                );
            }
            return control_at(self.live, point).cloned();
        }
        if let Some(id) = &target.reference {
            return self.reference(beat, id, app);
        }
        if let Some(query) = &target.find {
            return self.find(beat, query);
        }
        None
    }

    fn reference(&mut self, beat: usize, id: &str, app: &str) -> Option<ControlSnapshot> {
        let now = self.live.iter().find(|c| c.id == id);
        if self.planned.is_empty() {
            match now {
                None => {
                    let instead =
                        alike_id(self.live, id).map(|c| format!("Did you mean {} ({})?", c.id, c.name()));
                    self.problem(
                        beat,
                        FindingKind::Missing,
                        format!("There's no control called {id} on screen now."),
                        instead.or_else(|| Some(PREPARE_AGAIN.into())),
                    );
                }
                Some(control) => self.state(beat, control, app),
            }
            return now.cloned();
        }
        let Some(seen) = self.planned.iter().find(|c| c.id == id) else {
            let instead = alike_id(self.planned, id)
                .or_else(|| alike_id(self.live, id))
                .map(|c| format!("Did you mean {} ({})?", c.id, c.name()));
            self.problem(
                beat,
                FindingKind::NotPrepared,
                format!("{id} isn't one of the controls prepare_take listed, so the take would stop here."),
                instead.or_else(|| Some("Use a ref from prepare_take, or { find: \"label\" }.".into())),
            );
            return now.cloned();
        };
        match refs::check(seen, now) {
            RefStatus::Gone => {
                let instead = self.same_elsewhere(seen).or_else(|| {
                    Some(match seen.label.as_deref() {
                        Some(label) => format!(
                            "Run prepare_take again for fresh refs. If an earlier step opens it, use {{ find: \
                             \"{label}\" }} so the take looks for it when it gets there."
                        ),
                        None => PREPARE_AGAIN.into(),
                    })
                });
                self.problem(
                    beat,
                    FindingKind::Missing,
                    format!("{} ({id}) isn't on screen now.", seen.name()),
                    instead,
                );
            }
            RefStatus::Relabelled { from, to } => {
                let instead = self.same_elsewhere(seen).or_else(|| Some(PREPARE_AGAIN.into()));
                self.problem(
                    beat,
                    FindingKind::Renamed,
                    format!(
                        "{id} now says {} (it said {} when the take was prepared), so the take would stop here \
                         rather than click the wrong thing.",
                        quoted(to.as_deref()),
                        quoted(from.as_deref())
                    ),
                    instead,
                );
            }
            _ => {
                if let Some(control) = now {
                    self.state(beat, control, app);
                }
            }
        }
        now.cloned()
    }

    /// Notes on a control that's on screen but can't be used right now.
    fn state(&mut self, beat: usize, control: &ControlSnapshot, app: &str) {
        if control.occluded {
            self.problem(
                beat,
                FindingKind::Covered,
                format!("Something covers {}, even with {app} in front.", control.name()),
                Some(
                    "Close or move what's on top of it (a panel or a notification), or scroll it into view."
                        .into(),
                ),
            );
        } else if !control.enabled {
            self.note(
                beat,
                FindingKind::Disabled,
                format!(
                    "{} is disabled right now. The take stops here if it still is, which is fine when an earlier \
                     step enables it (filling in a form, say).",
                    control.name()
                ),
                None,
            );
        }
    }

    fn find(&mut self, beat: usize, query: &str) -> Option<ControlSnapshot> {
        if let Some(control) = best_match(self.live, query) {
            return Some(control.clone());
        }
        let instead = alike_label(self.live, query, None)
            .map(|c| format!("The closest on screen now is {} ({}).", c.name(), c.id));
        self.note(
            beat,
            FindingKind::NotYet,
            format!(
                "“{}” isn't on screen yet. The take waits up to {:.0} s for it when it gets there.",
                query.trim(),
                crate::FIND_TIMEOUT_MS / 1000.0
            ),
            instead,
        );
        None
    }

    /// The control the agent saw, under another id now: the same role and label.
    fn same_elsewhere(&self, seen: &ControlSnapshot) -> Option<String> {
        let label = seen.label.as_deref()?;
        if let Some(moved) =
            self.live.iter().find(|c| c.id != seen.id && c.role == seen.role && c.label == seen.label)
        {
            return Some(format!(
                "“{label}” is {} now: run prepare_take again and use that ref, or {{ find: \"{label}\" }}.",
                moved.id
            ));
        }
        alike_label(self.live, label, Some(seen.role.as_str()))
            .map(|c| format!("The closest on screen now is {} ({}).", c.name(), c.id))
    }
}

const PREPARE_AGAIN: &str = "Run prepare_take again to see what's on screen now, and use its refs.";

fn quoted(label: Option<&str>) -> String {
    label.map_or_else(|| "nothing".into(), |l| format!("“{l}”"))
}

fn inside(window: &WindowInfo, p: Point) -> bool {
    p.x >= window.x && p.y >= window.y && p.x <= window.x + window.width && p.y <= window.y + window.height
}

/// The smallest control under a point.
fn control_at(controls: &[ControlSnapshot], p: Point) -> Option<&ControlSnapshot> {
    controls
        .iter()
        .filter(|c| {
            let b = c.bounds;
            p.x >= b.x && p.y >= b.y && p.x <= b.x + b.width && p.y <= b.y + b.height
        })
        .min_by(|a, b| (a.bounds.width * a.bounds.height).total_cmp(&(b.bounds.width * b.bounds.height)))
}

/// The control whose id is most like `id`, when it's alike enough to suggest.
fn alike_id<'a>(controls: &'a [ControlSnapshot], id: &str) -> Option<&'a ControlSnapshot> {
    closest(controls, |c| Some(similarity(&c.id, id)))
}

/// The control whose label is most like `label` (of `role`, when given), when it's alike enough to suggest.
fn alike_label<'a>(
    controls: &'a [ControlSnapshot],
    label: &str,
    role: Option<&str>,
) -> Option<&'a ControlSnapshot> {
    closest(controls, |c| {
        let theirs = c.label.as_deref()?;
        (role.is_none_or(|r| r == c.role)).then(|| similarity(theirs, label))
    })
}

fn closest(
    controls: &[ControlSnapshot],
    score: impl Fn(&ControlSnapshot) -> Option<f64>,
) -> Option<&ControlSnapshot> {
    controls
        .iter()
        .filter_map(|c| score(c).filter(|s| *s >= ALIKE).map(|s| (c, s)))
        // Enabled controls first, then the most alike.
        .max_by(|(a, x), (b, y)| a.enabled.cmp(&b.enabled).then(x.total_cmp(y)))
        .map(|(c, _)| c)
}

/// How alike two labels read, from 0 to 1, ignoring case, punctuation and spacing: one minus the edit distance over
/// the longer length.
fn similarity(a: &str, b: &str) -> f64 {
    let norm = |s: &str| -> Vec<char> {
        s.chars().flat_map(char::to_lowercase).filter(|c| c.is_alphanumeric()).collect()
    };
    let (a, b) = (norm(a), norm(b));
    let longest = a.len().max(b.len());
    if longest == 0 {
        return 0.0;
    }
    1.0 - distance(&a, &b) as f64 / longest as f64
}

/// Levenshtein distance.
fn distance(a: &[char], b: &[char]) -> usize {
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut diagonal = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let above = row[j + 1];
            row[j + 1] = if ca == cb { diagonal } else { 1 + diagonal.min(above).min(row[j]) };
            diagonal = above;
        }
    }
    row[b.len()]
}

#[cfg(test)]
mod tests {
    use shotmaxxing_core::geometry::Rect;

    use super::*;

    fn control(id: &str, role: &str, label: &str, x: f64, y: f64) -> ControlSnapshot {
        ControlSnapshot {
            id: id.into(),
            role: role.into(),
            label: (!label.is_empty()).then(|| label.into()),
            enabled: true,
            bounds: Rect::new(x, y, 120.0, 30.0),
            window_id: Some(1),
            occluded: false,
        }
    }

    fn window() -> WindowInfo {
        WindowInfo {
            id: 1,
            title: "Sign up".into(),
            app: "Safari".into(),
            bundle_id: "com.apple.Safari".into(),
            x: 0.0,
            y: 0.0,
            width: 1200.0,
            height: 800.0,
            full_screen: false,
            away: false,
        }
    }

    fn script(yaml: &str) -> TakeScript {
        TakeScript::parse(yaml).unwrap()
    }

    fn screen() -> Vec<ControlSnapshot> {
        vec![
            control("button.get-started", "button", "Get started", 100.0, 100.0),
            control("text_field.email", "text_field", "Email", 100.0, 200.0),
            control("password_field.password", "password_field", "Password", 100.0, 260.0),
            control("button.create-account", "button", "Create account", 100.0, 320.0),
        ]
    }

    #[test]
    fn a_plan_that_matches_the_screen_is_ready() {
        let live = screen();
        let s = script(
            "app: Safari\nbeats:\n  - click: button.get-started\n  - type: { text: maya@example.com, into: \
             text_field.email }\n  - click: button.create-account\n  - wait: { for: { find: Check your inbox } }\n",
        );
        let check = check(&s, &live, &live, Some(&window()));
        assert!(check.ready, "{check:?}");
        assert_eq!(check.targets, 3);
        // What only appears later is a note, not a problem.
        assert_eq!(check.notes.len(), 1, "{check:?}");
        assert_eq!(check.notes[0].kind, FindingKind::NotYet);
        assert_eq!(check.notes[0].beat, 4);
    }

    #[test]
    fn a_control_that_went_away_is_a_problem_with_its_new_ref() {
        let planned = screen();
        // The page changed: the button has a new id, and the email field is gone.
        let mut live = screen();
        live[3].id = "button.create-account-2".into();
        live.remove(1);
        let s = script("beats:\n  - click: text_field.email\n  - click: button.create-account\n");
        let check = check(&s, &planned, &live, Some(&window()));
        assert!(!check.ready);
        assert_eq!(check.problems.len(), 2, "{check:?}");
        assert_eq!(check.problems[0].kind, FindingKind::Missing);
        assert_eq!(check.problems[0].beat, 1);
        assert!(check.problems[0].instead.as_deref().unwrap().contains("find: \"Email\""), "{check:?}");
        let moved = check.problems[1].instead.as_deref().unwrap();
        assert!(moved.contains("button.create-account-2"), "{moved}");
    }

    #[test]
    fn a_relabelled_control_is_a_problem() {
        let planned = screen();
        let mut live = screen();
        live[0].label = Some("Start free trial".into());
        let s = script("beats:\n  - click: button.get-started\n");
        let check = check(&s, &planned, &live, Some(&window()));
        assert_eq!(check.problems.len(), 1, "{check:?}");
        assert_eq!(check.problems[0].kind, FindingKind::Renamed);
        assert!(check.problems[0].message.contains("“Start free trial”"), "{check:?}");
    }

    #[test]
    fn a_ref_prepare_never_listed_is_a_problem_with_a_close_one() {
        let live = screen();
        let s = script("beats:\n  - click: button.create-acount\n");
        let check = check(&s, &live, &live, Some(&window()));
        assert_eq!(check.problems[0].kind, FindingKind::NotPrepared, "{check:?}");
        assert!(check.problems[0].instead.as_deref().unwrap().contains("button.create-account"), "{check:?}");
    }

    #[test]
    fn typing_into_a_password_field_is_a_problem() {
        let live = screen();
        let s = script("beats:\n  - click: password_field.password\n  - type: hunter2\n");
        let check = check(&s, &live, &live, Some(&window()));
        assert_eq!(check.problems.len(), 1, "{check:?}");
        assert_eq!(check.problems[0].kind, FindingKind::Password);
        assert_eq!(check.problems[0].beat, 2);
        // After a Tab, the script no longer says where typing goes.
        let s = script("beats:\n  - click: text_field.email\n  - type: \"a@b.c\\t\"\n  - type: more\n");
        assert!(check_ready(&s, &live));
    }

    fn check_ready(s: &TakeScript, live: &[ControlSnapshot]) -> bool {
        check(s, live, live, Some(&window())).ready
    }

    #[test]
    fn disabled_now_is_a_note_since_an_earlier_step_may_enable_it() {
        let mut live = screen();
        live[3].enabled = false;
        let s = script("beats:\n  - click: button.create-account\n");
        let check = check(&s, &live, &live, Some(&window()));
        assert!(check.ready, "{check:?}");
        assert_eq!(check.notes[0].kind, FindingKind::Disabled);
    }

    #[test]
    fn a_label_to_find_suggests_a_close_one() {
        let live = screen();
        let s = script("beats:\n  - click: { find: Create acount now }\n");
        let check = check(&s, &live, &live, Some(&window()));
        assert!(check.ready);
        assert_eq!(check.notes[0].kind, FindingKind::NotYet);
        assert!(check.notes[0].instead.as_deref().unwrap().contains("button.create-account"), "{check:?}");
    }

    #[test]
    fn points_outside_the_window_and_no_window_are_reported() {
        let live = screen();
        let s = script("app: Safari\nbeats:\n  - click: { point: { x: 1500, y: 40 } }\n");
        let check1 = check(&s, &live, &live, Some(&window()));
        assert!(check1.ready);
        assert_eq!(check1.notes[0].kind, FindingKind::OutsideWindow);
        let check2 = check(&s, &live, &live, None);
        assert!(!check2.ready);
        assert_eq!(check2.problems[0].kind, FindingKind::NoWindow);
        assert!(check2.problems[0].message.starts_with("Safari"), "{check2:?}");
    }

    #[test]
    fn without_a_prepared_list_refs_are_looked_up_on_screen() {
        let live = screen();
        let s = script("beats:\n  - click: button.get-startd\n  - click: button.get-started\n");
        let check = check(&s, &[], &live, Some(&window()));
        assert_eq!(check.problems.len(), 1, "{check:?}");
        assert_eq!(check.problems[0].kind, FindingKind::Missing);
        assert!(check.problems[0].instead.as_deref().unwrap().contains("button.get-started"));
    }

    #[test]
    fn labels_compare_by_letters() {
        assert!((similarity("Create account", "create-account") - 1.0).abs() < 1e-9);
        assert!(similarity("Create account", "Create acount") > 0.9);
        assert!(similarity("Export", "Settings") < ALIKE);
        assert_eq!(similarity("", ""), 0.0);
    }
}
