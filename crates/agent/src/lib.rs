//! Performing take scripts: Shotmaxxing moves the pointer, clicks and types like a person, on one clock, while it
//! records the screen.
//!
//! [`perform`] plays a script on a [`Desk`], one platform's screen, pointer and keyboard. Each beat is located just
//! before it plays:
//! - controls named by ref are looked up again and compared with what the agent saw when it prepared the take;
//! - controls that only appear later are found by id or label.
//!
//! The beats compiler then turns the beat into human-paced motion (Fitts's law timing, minimum-jerk arcs, a settle
//! before each press), and the performer plays it at 120 Hz.
//!
//! A take stops by itself, and says why, when the person steps in or the screen isn't as planned:
//! - Esc pressed three times, the person moving the pointer, or the caller cancelling;
//! - a planned control that has gone, been disabled, covered or relabelled;
//! - a control to find that never appears;
//! - a password field taking the typing: Shotmaxxing never types into one.
//!
//! A `user_step` beat hands control to the person (to sign in, say) and waits for them to press ⌥⌘Return. That
//! stretch is cut from the video afterwards.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;
use shotmaxxing_capture::{DeviceChoice, FinishedProject, WindowInfo};
use shotmaxxing_core::beats::{self, Aim, Cue, Performance, ResolvedBeat, Timing};
use shotmaxxing_core::geometry::Point;
use shotmaxxing_core::keys::{Key, KeyChord, NamedKey};
use shotmaxxing_core::refs::{self, ControlSnapshot};
use shotmaxxing_core::take::{
    Beat, SayBeat, TakeScript, TargetSpec, TypeBeat, UserStepBeat, WaitBeat, WaitCondition, WaitNamed,
};
use shotmaxxing_core::telemetry::MouseButton;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::{prepare, run_take};

#[cfg(not(target_os = "macos"))]
mod unsupported;
#[cfg(not(target_os = "macos"))]
pub use unsupported::{prepare, run_take};

/// How often the pointer moves while a beat plays.
const FRAME_MS: f64 = 1000.0 / 120.0;
/// How far the pointer may drift from where the take put it before the person counts as having taken over.
const TAKEOVER_POINTS: f64 = 24.0;
/// How long a `find` target may take to appear.
const FIND_TIMEOUT_MS: f64 = 10_000.0;
/// How long the screen has to stay still to count as settled.
const SETTLED_MS: f64 = 600.0;

const TOOK_OVER: &str = "you moved the pointer, so the take stopped and left the rest to you";
const PASSWORD: &str = "a password field has the keyboard, and Shotmaxxing never types into one. Use a user_step beat \
for that part";

#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    /// The script has problems; each is a sentence.
    #[error("the take script has problems: {}", .0.join("; "))]
    Script(Vec<String>),
    #[error("{0}")]
    Permission(String),
    #[error("{0}")]
    App(String),
    #[error(transparent)]
    Capture(#[from] shotmaxxing_capture::CaptureError),
    #[error("{0}")]
    Project(String),
    #[error("performing takes isn't available on this platform yet")]
    Unsupported,
}

/// One platform's screen, pointer and keyboard, as a take sees them.
pub trait Desk {
    /// Milliseconds on a steady clock.
    fn now_ms(&self) -> f64;
    fn sleep_ms(&mut self, ms: f64);
    /// A count that rises whenever something on screen changes.
    fn screen_changes(&self) -> u64;
    /// A control as it is now, by the id it had when the take was prepared.
    fn control(&mut self, id: &str) -> Option<ControlSnapshot>;
    /// The control on screen that best matches an id or a label.
    fn find(&mut self, query: &str) -> Option<ControlSnapshot>;
    fn pointer(&self) -> Point;
    /// Moves the pointer, dragging while a button is held.
    fn move_pointer(&mut self, to: Point, held: Option<MouseButton>);
    fn press(&mut self, at: Point, button: MouseButton, down: bool, count: u8);
    fn type_char(&mut self, ch: char);
    fn key(&mut self, chord: &KeyChord);
    /// Scrolls by points: positive `dy` moves down the page, positive `dx` to the right.
    fn scroll(&mut self, at: Point, dx: f64, dy: f64);
    /// Whether a password field has the keyboard.
    fn secure_input(&self) -> bool;
    /// Whether Escape is held down right now.
    fn escape_down(&self) -> bool;
    /// Whether the person is pressing ⌥⌘Return to hand control back.
    fn hand_back_down(&self) -> bool;
    /// Whether any modifier key is held.
    fn modifiers_held(&self) -> bool;
}

/// How far a take got.
#[derive(Clone, Debug, Default, Serialize)]
pub struct Performed {
    pub beats: usize,
    pub beats_done: usize,
    /// Why the take stopped early, in a sentence. `None` when every beat played.
    pub stopped: Option<String>,
    pub warnings: Vec<String>,
    /// Captions from `say` beats: start, end (desk milliseconds) and text.
    #[serde(skip)]
    pub captions: Vec<(f64, f64, String)>,
    /// When the person had control, on the desk clock, to cut from the video.
    #[serde(skip)]
    pub handoffs: Vec<(f64, f64)>,
}

/// A step along the way, for progress messages.
#[derive(Clone, Debug)]
pub struct Progress {
    /// From 0 to 1.
    pub done: f64,
    pub message: String,
}

/// What `prepare` found: the window to film and the controls an agent can name by ref.
#[derive(Clone, Debug, Serialize)]
pub struct Prepared {
    pub app: String,
    pub window: Option<WindowInfo>,
    pub controls: Vec<ControlSnapshot>,
    /// More controls exist than were listed.
    pub truncated: bool,
}

/// How to film a take.
#[derive(Clone, Debug)]
pub struct TakeOptions {
    /// Where the project goes.
    pub folder: PathBuf,
    /// The controls the agent saw when it prepared the take, to check refs against.
    pub planned: Vec<ControlSnapshot>,
    pub microphone: DeviceChoice,
    pub system_audio: bool,
    /// The agent filming, e.g. "Claude Code".
    pub client: Option<String>,
    pub cancel: Arc<AtomicBool>,
}

/// A filmed take: the saved project and how the performance went.
#[derive(Debug)]
pub struct Take {
    pub finished: FinishedProject,
    pub performed: Performed,
}

/// The timeline a script would play, worked out without touching anything.
#[derive(Clone, Debug, Serialize)]
pub struct Plan {
    pub seconds: f64,
    pub beats: Vec<PlannedBeat>,
    pub notes: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct PlannedBeat {
    pub beat: usize,
    pub at_seconds: f64,
    pub what: String,
}

fn ellipsize(text: &str, max: usize) -> String {
    let flat: String = text.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    let flat = flat.trim();
    if flat.chars().count() <= max {
        flat.to_string()
    } else {
        format!("{}…", flat.chars().take(max).collect::<String>().trim_end())
    }
}

/// A beat in a few words, for progress messages and plans.
pub fn describe(beat: &Beat) -> String {
    let target = |spec: &TargetSpec| {
        let t = spec.target();
        t.label
            .map(|l| format!("“{l}”"))
            .or(t.reference)
            .or(t.find.map(|f| format!("“{f}”")))
            .or(t.point.map(|p| format!("{:.0}, {:.0}", p.x, p.y)))
            .unwrap_or_default()
    };
    match beat {
        Beat::Click(t) => format!("click {}", target(t)),
        Beat::DoubleClick(t) => format!("double-click {}", target(t)),
        Beat::RightClick(t) => format!("right-click {}", target(t)),
        Beat::Hover(t) => format!("point at {}", target(t)),
        Beat::Drag(d) => format!("drag {} to {}", target(&d.from), target(&d.to)),
        Beat::Type(t) => format!("type “{}”", ellipsize(t.text(), 24)),
        Beat::Key(k) => format!("press {k}"),
        Beat::Scroll(_) => "scroll".into(),
        Beat::Wait(_) => "wait".into(),
        Beat::Say(s) => format!("caption “{}”", ellipsize(s.text(), 32)),
        Beat::UserStep(u) => format!("hand over: {}", u.message()),
    }
}

/// Turns a beat into the compiler's terms, locating each target with `aim`. Waits and hand-overs aren't handled
/// here.
fn resolve(
    beat: &Beat,
    aim: &mut dyn FnMut(&TargetSpec) -> Result<Aim, String>,
) -> Result<Vec<ResolvedBeat>, String> {
    let click =
        |aim: &mut dyn FnMut(&TargetSpec) -> Result<Aim, String>, spec: &TargetSpec, button, count| {
            Ok::<_, String>(ResolvedBeat::Click {
                target: aim(spec)?,
                button,
                count,
                dwell_ms: spec.target().dwell_ms,
            })
        };
    Ok(match beat {
        Beat::Click(t) => vec![click(aim, t, MouseButton::Left, 1)?],
        Beat::DoubleClick(t) => vec![click(aim, t, MouseButton::Left, 2)?],
        Beat::RightClick(t) => vec![click(aim, t, MouseButton::Right, 1)?],
        Beat::Hover(t) => vec![ResolvedBeat::Hover { target: aim(t)?, dwell_ms: t.target().dwell_ms }],
        Beat::Drag(d) => vec![ResolvedBeat::Drag { from: aim(&d.from)?, to: aim(&d.to)? }],
        Beat::Type(t) => {
            let mut out = Vec::new();
            let cps = match t {
                TypeBeat::Full { into, cps, .. } => {
                    if let Some(into) = into {
                        out.push(click(aim, into, MouseButton::Left, 1)?);
                    }
                    *cps
                }
                TypeBeat::Text(_) => None,
            };
            out.push(ResolvedBeat::Type { text: t.text().to_string(), cps });
            out
        }
        Beat::Key(k) => vec![ResolvedBeat::Key { chord: KeyChord::parse(k).map_err(|e| e.to_string())? }],
        Beat::Scroll(s) => {
            vec![ResolvedBeat::Scroll { at: s.at.as_ref().map(&mut *aim).transpose()?, dx: s.dx, dy: s.dy }]
        }
        Beat::Say(s) => vec![ResolvedBeat::Say {
            text: s.text().to_string(),
            ms: match s {
                SayBeat::Full { ms, .. } => *ms,
                SayBeat::Text(_) => None,
            },
        }],
        Beat::Wait(_) | Beat::UserStep(_) => Vec::new(),
    })
}

/// Works out a script's timeline from the controls `prepare` listed, without touching anything. Controls that only
/// appear later, and waits on the screen, are estimated.
pub fn plan(script: &TakeScript, planned: &[ControlSnapshot]) -> Plan {
    let mut notes = Vec::new();
    let mut resolved = Vec::new();
    let mut owners = Vec::new();
    let mut last = Point::ZERO;
    for (index, beat) in script.beats.iter().enumerate() {
        let beats = match beat {
            Beat::Wait(WaitBeat::Ms(ms)) => vec![ResolvedBeat::Wait { ms: *ms }],
            Beat::Wait(WaitBeat::Full { ms, until, .. }) => {
                // The screen decides how long these take; a second is typical.
                let guess = if until.is_some() { 1_000 } else { 0 };
                vec![ResolvedBeat::Wait { ms: ms.unwrap_or(0).max(guess) }]
            }
            Beat::UserStep(step) => {
                notes.push(format!("Beat {} waits for you: {}", index + 1, step.message()));
                vec![ResolvedBeat::UserStep { message: step.message().to_string() }]
            }
            _ => {
                let mut aim = |spec: &TargetSpec| -> Result<Aim, String> {
                    let t = spec.target();
                    let aim = if let Some(point) = t.point {
                        Aim { point, bounds: None, label: t.label }
                    } else if let Some(id) = &t.reference {
                        match planned.iter().find(|c| &c.id == id) {
                            Some(c) => Aim::control(c.bounds, t.label.or(c.label.clone())),
                            None => {
                                notes.push(format!(
                                    "Beat {}: {id} isn't one of the prepared controls",
                                    index + 1
                                ));
                                Aim::point(last)
                            }
                        }
                    } else {
                        let what = t.find.unwrap_or_default();
                        notes.push(format!("Beat {}: “{what}” is found when the take reaches it", index + 1));
                        Aim::point(last)
                    };
                    last = aim.point;
                    Ok(aim)
                };
                match resolve(beat, &mut aim) {
                    Ok(beats) => beats,
                    Err(problem) => {
                        notes.push(format!("Beat {}: {problem}", index + 1));
                        Vec::new()
                    }
                }
            }
        };
        owners.extend(std::iter::repeat_n(index, beats.len()));
        resolved.extend(beats);
    }
    let mut timing = Timing::default();
    if let Some(ms) = script.lead_ms {
        timing.lead_ms = f64::from(ms);
    }
    if let Some(ms) = script.tail_ms {
        timing.tail_ms = f64::from(ms);
    }
    let performance = beats::compile(&resolved, None, &timing);
    let mut beats: Vec<PlannedBeat> = Vec::new();
    for cue in &performance.cues {
        if let Cue::BeatStart { at_ms, beat } = cue
            && let Some(&owner) = owners.get(*beat)
            && beats.last().is_none_or(|b| b.beat != owner)
        {
            let what = describe(&script.beats[owner]);
            beats.push(PlannedBeat { beat: owner, at_seconds: (at_ms / 10.0).round() / 100.0, what });
        }
    }
    Plan { seconds: (performance.duration_ms / 10.0).round() / 100.0, beats, notes }
}

/// The take in progress: the desk, and what's needed to notice the person stepping in.
struct Stage<'a> {
    desk: &'a mut dyn Desk,
    cancel: &'a AtomicBool,
    /// Where the take last put the pointer.
    placed: Option<Point>,
    escapes: Vec<f64>,
    escape_was_down: bool,
    /// The take's own Escape presses don't count towards stopping it.
    own_escape_until: f64,
}

impl Stage<'_> {
    fn check(&mut self) -> Result<(), String> {
        if self.cancel.load(Ordering::SeqCst) {
            return Err("the take was cancelled".into());
        }
        let now = self.desk.now_ms();
        let down = self.desk.escape_down();
        if down && !self.escape_was_down && now > self.own_escape_until {
            self.escapes.push(now);
        }
        self.escape_was_down = down;
        self.escapes.retain(|t| now - t < 2_000.0);
        if self.escapes.len() >= 3 {
            return Err("Esc was pressed three times, so the take stopped".into());
        }
        if self.placed.is_some_and(|p| self.desk.pointer().distance(p) > TAKEOVER_POINTS) {
            return Err(TOOK_OVER.into());
        }
        Ok(())
    }

    /// Waits, watching for the person stepping in.
    fn rest(&mut self, ms: f64) -> Result<(), String> {
        let end = self.desk.now_ms() + ms.max(0.0);
        loop {
            self.check()?;
            let left = end - self.desk.now_ms();
            if left <= 0.0 {
                return Ok(());
            }
            self.desk.sleep_ms(left.min(25.0));
        }
    }

    fn wait_for(&mut self, query: &str, timeout_ms: f64) -> Result<ControlSnapshot, String> {
        let start = self.desk.now_ms();
        loop {
            if let Some(control) = self.desk.find(query) {
                return Ok(control);
            }
            if self.desk.now_ms() - start >= timeout_ms {
                return Err(format!("“{query}” didn't appear within {:.0} s", timeout_ms / 1000.0));
            }
            self.rest(200.0)?;
        }
    }

    fn aim(&mut self, spec: &TargetSpec, planned: &[ControlSnapshot]) -> Result<Aim, String> {
        let target = spec.target();
        if let Some(point) = target.point {
            return Ok(Aim { point, bounds: None, label: target.label });
        }
        if let Some(id) = &target.reference {
            let current = self.desk.control(id);
            let control = match planned.iter().find(|c| &c.id == id) {
                Some(seen) => {
                    let status = refs::check(seen, current.as_ref());
                    if status.blocks_take() {
                        return Err(status.describe(seen));
                    }
                    current.ok_or_else(|| refs::RefStatus::Gone.describe(seen))?
                }
                None if planned.is_empty() => {
                    current.ok_or_else(|| format!("there's no control called {id} on screen"))?
                }
                None => return Err(format!("{id} isn't one of the controls prepare_take listed")),
            };
            return Ok(Aim::control(control.bounds, target.label.or(control.label)));
        }
        if let Some(query) = &target.find {
            let control = self.wait_for(query, FIND_TIMEOUT_MS)?;
            return Ok(Aim::control(control.bounds, target.label.or(control.label)));
        }
        Err("the target needs a ref, a point or something to find".into())
    }

    fn wait(&mut self, wait: &WaitBeat, warnings: &mut Vec<String>) -> Result<(), String> {
        let (ms, until, timeout) = match wait {
            WaitBeat::Ms(ms) => (Some(*ms), None, None),
            WaitBeat::Full { ms, until, timeout_ms } => (*ms, until.clone(), *timeout_ms),
        };
        let at_least = f64::from(ms.unwrap_or(0));
        let timeout = f64::from(timeout.unwrap_or(15_000));
        let start = self.desk.now_ms();
        match until {
            None => self.rest(at_least),
            Some(WaitCondition::Find { find }) => self.wait_for(&find, timeout).map(|_| ()),
            Some(WaitCondition::Named(WaitNamed::Change)) => {
                let before = self.desk.screen_changes();
                while self.desk.screen_changes() == before {
                    if self.desk.now_ms() - start >= timeout {
                        warnings.push(format!(
                            "nothing changed on screen within {:.0} s, so the take went on",
                            timeout / 1000.0
                        ));
                        break;
                    }
                    self.rest(40.0)?;
                }
                self.rest(at_least - (self.desk.now_ms() - start))
            }
            Some(WaitCondition::Named(WaitNamed::Idle)) => {
                let mut seen = self.desk.screen_changes();
                let mut still_since = start;
                loop {
                    self.rest(40.0)?;
                    let now = self.desk.now_ms();
                    let changes = self.desk.screen_changes();
                    if changes != seen {
                        seen = changes;
                        still_since = now;
                    }
                    if now - still_since >= SETTLED_MS && now - start >= at_least {
                        return Ok(());
                    }
                    if now - start >= timeout {
                        warnings.push(format!(
                            "the screen kept changing for {:.0} s, so the take went on",
                            timeout / 1000.0
                        ));
                        return Ok(());
                    }
                }
            }
        }
    }

    fn hand_over(
        &mut self,
        step: &UserStepBeat,
        done: f64,
        progress: &mut dyn FnMut(Progress),
        handoffs: &mut Vec<(f64, f64)>,
    ) -> Result<(), String> {
        let message = step.message();
        let wait_s = match step {
            UserStepBeat::Full { wait_s, .. } => wait_s.unwrap_or(600),
            UserStepBeat::Message(_) => 600,
        };
        progress(Progress {
            done,
            message: format!("Waiting for you: {message}. Press ⌥⌘Return when you're done."),
        });
        let start = self.desk.now_ms();
        // The pointer is the person's now, and their Escape presses are part of what they're doing.
        self.placed = None;
        loop {
            if self.cancel.load(Ordering::SeqCst) {
                return Err("the take was cancelled".into());
            }
            if self.desk.hand_back_down() {
                break;
            }
            if self.desk.now_ms() - start > f64::from(wait_s) * 1000.0 {
                return Err(format!("waited {} s for you to {message}, then stopped", wait_s));
            }
            self.desk.sleep_ms(40.0);
        }
        // Typing with ⌥⌘ still held would turn letters into shortcuts.
        let released = self.desk.now_ms();
        while (self.desk.hand_back_down() || self.desk.modifiers_held())
            && self.desk.now_ms() - released < 3_000.0
        {
            self.desk.sleep_ms(30.0);
        }
        handoffs.push((start, self.desk.now_ms()));
        self.escapes.clear();
        self.escape_was_down = self.desk.escape_down();
        progress(Progress { done, message: "Thanks, carrying on.".into() });
        Ok(())
    }

    fn play(
        &mut self,
        performance: &Performance,
        chords: &[KeyChord],
        out: &mut Performed,
    ) -> Result<(), String> {
        let t0 = self.desk.now_ms();
        let mut next = 0;
        let mut chords = chords.iter();
        let mut held: Option<MouseButton> = None;
        let result = 'play: loop {
            let t = self.desk.now_ms() - t0;
            if let Err(reason) = self.check() {
                break Err(reason);
            }
            let at = performance.cursor_at(t.min(performance.duration_ms));
            if self.placed.is_none_or(|p| p.distance(at) > 0.2) {
                self.desk.move_pointer(at, held);
                self.placed = Some(at);
            }
            while let Some(cue) = performance.cues.get(next).filter(|c| c.at_ms() <= t) {
                next += 1;
                match cue {
                    Cue::MouseDown { point, button, count, .. } => {
                        self.desk.press(*point, *button, true, *count);
                        held = Some(*button);
                    }
                    Cue::MouseUp { point, button, .. } => {
                        self.desk.press(*point, *button, false, 1);
                        held = None;
                    }
                    Cue::Char { ch, .. } => {
                        if self.desk.secure_input() {
                            break 'play Err(PASSWORD.into());
                        }
                        self.desk.type_char(*ch);
                    }
                    Cue::Key { .. } => {
                        if let Some(chord) = chords.next() {
                            if chord.key == Key::Named(NamedKey::Escape) {
                                self.own_escape_until = self.desk.now_ms() + 400.0;
                            }
                            self.desk.key(chord);
                        }
                    }
                    Cue::Scroll { point, dx, dy, .. } => self.desk.scroll(*point, *dx, *dy),
                    Cue::Caption { at_ms, until_ms, text } => {
                        out.captions.push((t0 + at_ms, t0 + until_ms, text.clone()));
                    }
                    Cue::BeatStart { .. } | Cue::UserStep { .. } => {}
                }
            }
            if next >= performance.cues.len() && t >= performance.duration_ms {
                break Ok(());
            }
            self.desk.sleep_ms(FRAME_MS);
        };
        // Never leave a button held down.
        if let Some(button) = held {
            let at = self.placed.unwrap_or_else(|| self.desk.pointer());
            self.desk.press(at, button, false, 1);
        }
        result
    }

    fn beat(
        &mut self,
        index: usize,
        beat: &Beat,
        planned: &[ControlSnapshot],
        out: &mut Performed,
    ) -> Result<(), String> {
        let resolved = resolve(beat, &mut |spec| self.aim(spec, planned))?;
        let base = Timing::default();
        let timing = Timing {
            lead_ms: 0.0,
            tail_ms: 0.0,
            seed: base.seed ^ (index as u64 + 1).wrapping_mul(0x9e37_79b9_7f4a_7c15),
            ..base
        };
        let performance = beats::compile(&resolved, Some(self.desk.pointer()), &timing);
        let chords: Vec<KeyChord> = resolved
            .iter()
            .filter_map(|b| match b {
                ResolvedBeat::Key { chord } => Some(*chord),
                _ => None,
            })
            .collect();
        self.play(&performance, &chords, out)
    }
}

/// Plays a script on a desk. `planned` holds the controls the agent saw when it prepared the take (empty when it
/// didn't), to check refs against.
pub fn perform(
    script: &TakeScript,
    planned: &[ControlSnapshot],
    desk: &mut dyn Desk,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(Progress),
) -> Performed {
    let total = script.beats.len();
    let mut out = Performed { beats: total, ..Performed::default() };
    let escape_was_down = desk.escape_down();
    let mut stage =
        Stage { desk, cancel, placed: None, escapes: Vec::new(), escape_was_down, own_escape_until: 0.0 };
    let base = Timing::default();
    if let Err(reason) = stage.rest(script.lead_ms.map_or(base.lead_ms, f64::from)) {
        out.stopped = Some(reason);
        return out;
    }
    for (index, beat) in script.beats.iter().enumerate() {
        let done = index as f64 / total.max(1) as f64;
        progress(Progress { done, message: format!("Beat {} of {total}: {}", index + 1, describe(beat)) });
        let result = match beat {
            Beat::Wait(wait) => stage.wait(wait, &mut out.warnings),
            Beat::UserStep(step) => stage.hand_over(step, done, progress, &mut out.handoffs),
            _ => stage.beat(index, beat, planned, &mut out),
        };
        if let Err(reason) = result {
            out.stopped = Some(format!("Beat {} ({}): {reason}", index + 1, describe(beat)));
            return out;
        }
        out.beats_done = index + 1;
    }
    // The ending stays on screen for a moment. Stepping in now doesn't spoil anything.
    let _ = stage.rest(script.tail_ms.map_or(base.tail_ms, f64::from));
    progress(Progress { done: 1.0, message: "Done".into() });
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use shotmaxxing_core::geometry::Rect;

    #[derive(Debug, Clone, PartialEq)]
    enum Act {
        Down(Point, MouseButton, u8),
        Up(Point),
        Char(char),
        Key(String),
        Scroll(f64, f64),
    }

    /// A desk on a pretend clock. Things the person does are scheduled by time.
    #[derive(Default)]
    struct Fake {
        now: f64,
        pointer: Point,
        acts: Vec<Act>,
        moves: usize,
        controls: Vec<(f64, ControlSnapshot)>,
        escape_at: Vec<f64>,
        person_moves_at: Option<f64>,
        hand_back_at: Option<f64>,
        secure: bool,
        changes_until: f64,
    }

    impl Fake {
        fn visible(&self) -> impl Iterator<Item = &ControlSnapshot> {
            self.controls.iter().filter(|(from, _)| self.now >= *from).map(|(_, c)| c)
        }
    }

    impl Desk for Fake {
        fn now_ms(&self) -> f64 {
            self.now
        }
        fn sleep_ms(&mut self, ms: f64) {
            self.now += ms.max(0.5);
            if let Some(at) = self.person_moves_at
                && self.now >= at
            {
                self.pointer = Point::new(5.0, 5.0);
                self.person_moves_at = None;
            }
        }
        fn screen_changes(&self) -> u64 {
            // The screen changes every 100 ms until `changes_until`, then sits still.
            (self.now.min(self.changes_until) / 100.0) as u64
        }
        fn control(&mut self, id: &str) -> Option<ControlSnapshot> {
            self.visible().find(|c| c.id == id).cloned()
        }
        fn find(&mut self, query: &str) -> Option<ControlSnapshot> {
            self.visible().find(|c| c.id == query || c.label.as_deref() == Some(query)).cloned()
        }
        fn pointer(&self) -> Point {
            self.pointer
        }
        fn move_pointer(&mut self, to: Point, _held: Option<MouseButton>) {
            self.pointer = to;
            self.moves += 1;
        }
        fn press(&mut self, at: Point, button: MouseButton, down: bool, count: u8) {
            self.acts.push(if down { Act::Down(at, button, count) } else { Act::Up(at) });
        }
        fn type_char(&mut self, ch: char) {
            self.acts.push(Act::Char(ch));
        }
        fn key(&mut self, chord: &KeyChord) {
            self.acts.push(Act::Key(chord.display(shotmaxxing_core::keys::Platform::Macos)));
        }
        fn scroll(&mut self, _at: Point, dx: f64, dy: f64) {
            self.acts.push(Act::Scroll(dx, dy));
        }
        fn secure_input(&self) -> bool {
            self.secure
        }
        fn escape_down(&self) -> bool {
            self.escape_at.iter().any(|t| self.now >= *t && self.now < t + 80.0)
        }
        fn hand_back_down(&self) -> bool {
            self.hand_back_at.is_some_and(|t| self.now >= t && self.now < t + 150.0)
        }
        fn modifiers_held(&self) -> bool {
            self.hand_back_down()
        }
    }

    fn control(id: &str, label: &str, x: f64, y: f64) -> ControlSnapshot {
        ControlSnapshot {
            id: id.into(),
            role: "button".into(),
            label: Some(label.into()),
            enabled: true,
            bounds: Rect::new(x, y, 80.0, 24.0),
            window_id: None,
            occluded: false,
        }
    }

    fn script(yaml: &str) -> TakeScript {
        TakeScript::parse(yaml).unwrap()
    }

    fn run(desk: &mut Fake, script: &TakeScript, planned: &[ControlSnapshot]) -> Performed {
        perform(script, planned, desk, &AtomicBool::new(false), &mut |_| {})
    }

    #[test]
    fn clicks_a_planned_control_where_it_is_now() {
        let seen = control("button.export", "Export", 100.0, 100.0);
        let moved = control("button.export", "Export", 300.0, 200.0);
        let mut desk =
            Fake { pointer: Point::new(10.0, 10.0), controls: vec![(0.0, moved)], ..Fake::default() };
        let done = run(&mut desk, &script("beats:\n  - click: button.export\n"), &[seen]);
        assert_eq!(done.stopped, None);
        assert_eq!(done.beats_done, 1);
        let centre = Point::new(340.0, 212.0);
        assert_eq!(desk.acts, vec![Act::Down(centre, MouseButton::Left, 1), Act::Up(centre)]);
        assert!(desk.moves > 20, "the pointer travels, it doesn't jump ({} moves)", desk.moves);
    }

    #[test]
    fn stops_by_name_when_a_planned_control_is_gone() {
        let seen = control("button.export", "Export", 100.0, 100.0);
        let mut desk = Fake::default();
        let done = run(&mut desk, &script("beats:\n  - click: button.export\n"), &[seen]);
        assert!(done.stopped.as_deref().is_some_and(|s| s.contains("no longer on screen")), "{done:?}");
        assert!(desk.acts.is_empty(), "nothing was clicked");
    }

    #[test]
    fn waits_for_a_control_to_appear() {
        let later = control("link.docs", "Docs", 50.0, 400.0);
        let mut desk = Fake { controls: vec![(2_500.0, later)], ..Fake::default() };
        let done = run(&mut desk, &script("beats:\n  - click: { find: Docs }\n"), &[]);
        assert_eq!(done.stopped, None);
        assert!(matches!(desk.acts[0], Act::Down(p, _, _) if p == Point::new(90.0, 412.0)));

        let mut desk = Fake::default();
        let done = run(&mut desk, &script("beats:\n  - click: { find: Docs }\n"), &[]);
        assert!(done.stopped.as_deref().is_some_and(|s| s.contains("didn't appear")), "{done:?}");
    }

    #[test]
    fn the_person_can_stop_a_take() {
        let script = script("beats:\n  - wait: 5000\n  - type: hello\n");
        let mut desk = Fake { escape_at: vec![1_000.0, 1_300.0, 1_600.0], ..Fake::default() };
        let done = run(&mut desk, &script, &[]);
        assert!(done.stopped.as_deref().is_some_and(|s| s.contains("Esc")), "{done:?}");
        assert!(desk.acts.is_empty());

        // Grabbing the mouse mid-take stops it too.
        let target = control("button.go", "Go", 600.0, 400.0);
        let mut desk =
            Fake { controls: vec![(0.0, target)], person_moves_at: Some(1_400.0), ..Fake::default() };
        let done = run(
            &mut desk,
            &TakeScript::parse("beats:\n  - click: button.go\n  - wait: 3000\n").unwrap(),
            &[],
        );
        assert!(done.stopped.as_deref().is_some_and(|s| s.contains("moved the pointer")), "{done:?}");
    }

    #[test]
    fn never_types_into_a_password_field() {
        let mut desk = Fake { secure: true, ..Fake::default() };
        let done = run(&mut desk, &script("beats:\n  - type: hunter2\n"), &[]);
        assert!(done.stopped.as_deref().is_some_and(|s| s.contains("password")), "{done:?}");
        assert!(desk.acts.is_empty());
    }

    #[test]
    fn types_keys_scrolls_and_captions() {
        let yaml = "beats:\n  - say: Saving the file\n  - type: \"Hi\\n\"\n  - key: cmd+s\n  - scroll: { dy: 300 }\n";
        let mut desk = Fake::default();
        let done = run(&mut desk, &script(yaml), &[]);
        assert_eq!(done.stopped, None);
        let typed: Vec<&Act> = desk.acts.iter().filter(|a| matches!(a, Act::Char(_))).collect();
        assert_eq!(typed, [&Act::Char('H'), &Act::Char('i'), &Act::Char('\n')]);
        assert!(desk.acts.contains(&Act::Key("⌘S".into())), "{:?}", desk.acts);
        let scrolled: f64 =
            desk.acts.iter().map(|a| if let Act::Scroll(_, dy) = a { *dy } else { 0.0 }).sum();
        assert!((scrolled - 300.0).abs() < 1e-6);
        assert_eq!(done.captions.len(), 1);
        assert_eq!(done.captions[0].2, "Saving the file");
        assert!(done.captions[0].1 > done.captions[0].0);
    }

    #[test]
    fn hands_over_and_carries_on() {
        let yaml = "beats:\n  - user_step: Sign in\n  - type: ok\n";
        let mut desk = Fake { hand_back_at: Some(4_000.0), ..Fake::default() };
        let mut messages = Vec::new();
        let done = perform(&script(yaml), &[], &mut desk, &AtomicBool::new(false), &mut |p| {
            messages.push(p.message)
        });
        assert_eq!(done.stopped, None, "{messages:?}");
        assert_eq!(done.handoffs.len(), 1);
        let (start, end) = done.handoffs[0];
        assert!(start < 4_000.0 && end >= 4_150.0, "{start}..{end}");
        assert!(messages.iter().any(|m| m.contains("⌥⌘Return")));
        assert_eq!(desk.acts, vec![Act::Char('o'), Act::Char('k')]);
    }

    #[test]
    fn waits_for_the_screen_to_settle() {
        let mut desk = Fake { changes_until: 3_000.0, ..Fake::default() };
        let done = run(&mut desk, &script("lead_ms: 0\ntail_ms: 0\nbeats:\n  - wait: { for: idle }\n"), &[]);
        assert_eq!(done.stopped, None);
        assert!(desk.now >= 3_000.0 + SETTLED_MS && desk.now < 4_000.0, "{}", desk.now);
        assert!(done.warnings.is_empty());
    }

    #[test]
    fn plans_without_touching_anything() {
        let planned = [control("button.export", "Export", 900.0, 600.0)];
        let plan = plan(
            &script("beats:\n  - click: button.export\n  - type: hello\n  - click: { find: Done }\n"),
            &planned,
        );
        assert_eq!(plan.beats.len(), 3);
        assert!(plan.beats.windows(2).all(|w| w[0].at_seconds < w[1].at_seconds));
        assert!(plan.seconds > 2.0 && plan.seconds < 10.0, "{}", plan.seconds);
        assert!(plan.notes.iter().any(|n| n.contains("Done")), "{:?}", plan.notes);
        assert_eq!(plan.beats[0].what, "click button.export");
    }
}
