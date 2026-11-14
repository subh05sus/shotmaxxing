//! Take scripts: a planned performance an agent (or a person) hands to Shotmaxxing to film.
//!
//! People write YAML, agents send JSON, and both follow one JSON Schema (see [`json_schema`]). Short forms are
//! accepted wherever they are unambiguous, so `click: toolbar.address` means `click: { ref: toolbar.address }` and
//! `wait: 500` means half a second.
//!
//! ```yaml
//! app: Safari
//! framing: window
//! beats:
//!   - click: { ref: toolbar.address, label: Address bar }
//!   - type: "shotmaxxing.dev\n"
//!   - wait: { for: idle }
//!   - say: Open the docs
//!   - click: { find: nav.docs }
//!   - key: mod+f
//!   - type: auto zoom
//! output: { mode: editor }
//! ```

use crate::geometry::Point;
use crate::keys::KeyChord;
use crate::project::ExportPreset;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const MAX_BEATS: usize = 500;
const MAX_TYPE_CHARS: usize = 5_000;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TakeScript {
    /// A short title for the take, used for the project name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// The app to film, by name ("Safari") or bundle id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app: Option<String>,
    /// Part of the window title, when the app has several windows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window: Option<String>,
    #[serde(default)]
    pub framing: Framing,
    /// The performance, in order.
    pub beats: Vec<Beat>,
    #[serde(default)]
    pub output: TakeOutput,
    /// Capture frame rate ceiling (frames are only captured when the screen changes). Default 60.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fps: Option<u32>,
    /// Quiet time after recording starts, before the first beat. Default 600 ms.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lead_ms: Option<u32>,
    /// Quiet time after the last beat, so the result is readable. Default 900 ms.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tail_ms: Option<u32>,
    /// Absolute paths the take must create or update (for save and export demos). Checked before delivery.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub expected_files: Vec<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Framing {
    /// Only the app's main window.
    #[default]
    Window,
    /// The whole display, including menus, popovers and file dialogs.
    Display,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TakeOutput {
    #[serde(default)]
    pub mode: OutputMode,
    /// Export preset when `mode` is `video`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset: Option<ExportPreset>,
    /// Added to the exported file name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OutputMode {
    /// Open the recording in the editor.
    #[default]
    Editor,
    /// Render a finished video file.
    Video,
    /// Save the project without opening it.
    Project,
}

/// One step of the performance. Each beat is a single-key object, e.g. `{ "click": { "ref": "w0.c3" } }`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Beat {
    Click(TargetSpec),
    DoubleClick(TargetSpec),
    RightClick(TargetSpec),
    /// Move to a target and rest there.
    Hover(TargetSpec),
    Drag(DragBeat),
    /// Type text one character at a time. `\n` presses Return and `\t` presses Tab.
    Type(TypeBeat),
    /// A key or chord, e.g. `Return`, `Escape`, `mod+s`, `cmd+shift+z`.
    Key(String),
    Scroll(ScrollBeat),
    Wait(WaitBeat),
    /// Show a caption while the next beats happen.
    Say(SayBeat),
    /// Hand control to the person for a step only they can do (sign in, solve a check), then continue.
    UserStep(UserStepBeat),
}

/// A target: a control ref, a screen point or a control that will appear later.
/// A plain string is a ref: `click: toolbar.export`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum TargetSpec {
    Ref(String),
    Target(Target),
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Target {
    /// A control ref from `prepare`, e.g. `w0.c12` or `toolbar.export`.
    #[serde(rename = "ref", default, skip_serializing_if = "Option::is_none")]
    pub reference: Option<String>,
    /// A global screen point, read off the prepare screenshot (origin + pixel).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub point: Option<Point>,
    /// A control that doesn't exist yet, found by id or label when the take reaches this beat.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub find: Option<String>,
    /// What this target is, for captions, zoom reasons and the take card.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// How long to rest after this beat.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dwell_ms: Option<u32>,
}

impl TargetSpec {
    pub fn target(&self) -> Target {
        match self {
            Self::Ref(r) => Target { reference: Some(r.clone()), ..Default::default() },
            Self::Target(t) => t.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DragBeat {
    pub from: TargetSpec,
    pub to: TargetSpec,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum TypeBeat {
    Text(String),
    Full {
        text: String,
        /// Click this target first, then type.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        into: Option<TargetSpec>,
        /// Characters per second. Default is a readable pace (about 18).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cps: Option<f64>,
    },
}

impl TypeBeat {
    pub fn text(&self) -> &str {
        match self {
            Self::Text(t) | Self::Full { text: t, .. } => t,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScrollBeat {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<TargetSpec>,
    #[serde(default)]
    pub dx: f64,
    #[serde(default)]
    pub dy: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum WaitBeat {
    /// Milliseconds.
    Ms(u32),
    Full {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ms: Option<u32>,
        /// Wait until a condition holds.
        #[serde(rename = "for", default, skip_serializing_if = "Option::is_none")]
        until: Option<WaitCondition>,
        /// Give up after this long. Default 15 s.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        timeout_ms: Option<u32>,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum WaitCondition {
    Named(WaitNamed),
    Find {
        /// A control id or label that should appear.
        find: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WaitNamed {
    /// The screen has stopped changing (a page loaded, an animation ended).
    Idle,
    /// Something on screen changes.
    Change,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum SayBeat {
    Text(String),
    Full {
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ms: Option<u32>,
    },
}

impl SayBeat {
    pub fn text(&self) -> &str {
        match self {
            Self::Text(t) | Self::Full { text: t, .. } => t,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum UserStepBeat {
    Message(String),
    Full {
        message: String,
        /// How long to wait for the person before the take reports that it is still waiting. Default 600 s.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        wait_s: Option<u32>,
    },
}

impl UserStepBeat {
    pub fn message(&self) -> &str {
        match self {
            Self::Message(m) | Self::Full { message: m, .. } => m,
        }
    }
}

#[derive(Debug, Error)]
pub enum TakeParseError {
    #[error("the take script isn't valid YAML or JSON: {0}")]
    Syntax(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Issue {
    /// Index of the beat, when the issue belongs to one.
    pub beat: Option<usize>,
    pub severity: Severity,
    pub message: String,
}

impl Issue {
    fn error(beat: Option<usize>, message: impl Into<String>) -> Self {
        Self { beat, severity: Severity::Error, message: message.into() }
    }
    fn warning(beat: Option<usize>, message: impl Into<String>) -> Self {
        Self { beat, severity: Severity::Warning, message: message.into() }
    }
}

impl TakeScript {
    /// Parses YAML or JSON.
    ///
    /// YAML is read into a plain value first, so beats are written as ordinary maps (`click: {...}`) rather than
    /// YAML tags. Errors name the path of the problem, e.g. `beats[3].click: unknown field 'poin'`.
    pub fn parse(text: &str) -> Result<Self, TakeParseError> {
        let trimmed = text.trim_start();
        let value: serde_json::Value = if trimmed.starts_with('{') {
            serde_json::from_str(trimmed).map_err(|e| TakeParseError::Syntax(e.to_string()))?
        } else {
            serde_yaml_ng::from_str(text).map_err(|e| TakeParseError::Syntax(e.to_string()))?
        };
        Self::from_value(value)
    }

    pub fn from_value(value: serde_json::Value) -> Result<Self, TakeParseError> {
        serde_path_to_error::deserialize(value).map_err(|e| {
            let path = e.path().to_string();
            let inner = e.into_inner().to_string();
            TakeParseError::Syntax(if path == "." || path.is_empty() {
                inner
            } else {
                format!("{path}: {inner}")
            })
        })
    }

    /// Writes the script as YAML, with beats as plain single-key maps.
    pub fn to_yaml(&self) -> String {
        serde_json::to_value(self).ok().and_then(|v| serde_yaml_ng::to_string(&v).ok()).unwrap_or_default()
    }

    /// Checks the script before anything is filmed. Errors stop the take; warnings are reported.
    pub fn validate(&self) -> Vec<Issue> {
        let mut issues = Vec::new();
        if self.beats.is_empty() {
            issues.push(Issue::error(None, "add at least one beat"));
        }
        if self.beats.len() > MAX_BEATS {
            issues.push(Issue::error(
                None,
                format!("a take can have at most {MAX_BEATS} beats; split it into parts"),
            ));
        }
        if let Some(fps) = self.fps
            && !(1..=120).contains(&fps)
        {
            issues.push(Issue::error(None, "fps must be between 1 and 120"));
        }
        if self.app.is_none() && self.framing == Framing::Window {
            issues.push(Issue::warning(None, "name the app to film, or use framing: display"));
        }
        for file in &self.expected_files {
            if !is_absolute_any_platform(file) {
                issues.push(Issue::error(None, format!("expected file '{file}' must be an absolute path")));
            }
        }

        let mut located = false;
        for (i, beat) in self.beats.iter().enumerate() {
            let at = Some(i);
            match beat {
                Beat::Click(t) | Beat::DoubleClick(t) | Beat::RightClick(t) | Beat::Hover(t) => {
                    check_target(&t.target(), at, &mut issues);
                    located = true;
                }
                Beat::Drag(d) => {
                    check_target(&d.from.target(), at, &mut issues);
                    check_target(&d.to.target(), at, &mut issues);
                    located = true;
                }
                Beat::Type(t) => {
                    let text = t.text();
                    if text.is_empty() {
                        issues.push(Issue::error(at, "type needs some text"));
                    }
                    if text.chars().count() > MAX_TYPE_CHARS {
                        issues.push(Issue::error(
                            at,
                            format!("type text is longer than {MAX_TYPE_CHARS} characters"),
                        ));
                    }
                    if let TypeBeat::Full { into, cps, .. } = t {
                        if let Some(into) = into {
                            check_target(&into.target(), at, &mut issues);
                            located = true;
                        }
                        if cps.is_some_and(|c| !(1.0..=60.0).contains(&c)) {
                            issues.push(Issue::error(
                                at,
                                "cps (characters per second) must be between 1 and 60",
                            ));
                        }
                    }
                    if !located {
                        issues.push(Issue::warning(
                            at,
                            "type goes to the window the previous beat used; click into a field first",
                        ));
                    }
                }
                Beat::Key(k) => {
                    if let Err(e) = KeyChord::parse(k) {
                        issues.push(Issue::error(at, e.to_string()));
                    }
                    if !located {
                        issues.push(Issue::warning(
                            at,
                            "key goes to the window the previous beat used; add a beat before it",
                        ));
                    }
                }
                Beat::Scroll(s) => {
                    if let Some(at_target) = &s.at {
                        check_target(&at_target.target(), at, &mut issues);
                        located = true;
                    }
                    if s.dx == 0.0 && s.dy == 0.0 {
                        issues.push(Issue::error(at, "scroll needs dx or dy"));
                    }
                }
                Beat::Wait(w) => match w {
                    WaitBeat::Ms(ms) if *ms > 120_000 => {
                        issues.push(Issue::warning(at, "waits longer than 2 minutes make long, empty videos"))
                    }
                    WaitBeat::Full { ms: None, until: None, .. } => {
                        issues.push(Issue::error(at, "wait needs ms or for"))
                    }
                    _ => {}
                },
                Beat::Say(s) => {
                    if s.text().trim().is_empty() {
                        issues.push(Issue::error(at, "say needs text"));
                    }
                }
                Beat::UserStep(u) => {
                    if u.message().trim().is_empty() {
                        issues.push(Issue::error(
                            at,
                            "user_step needs a message telling the person what to do",
                        ));
                    }
                }
            }
        }
        issues
    }

    pub fn is_valid(&self) -> bool {
        !self.validate().iter().any(|i| i.severity == Severity::Error)
    }
}

fn check_target(t: &Target, at: Option<usize>, issues: &mut Vec<Issue>) {
    let count =
        usize::from(t.reference.is_some()) + usize::from(t.point.is_some()) + usize::from(t.find.is_some());
    if count != 1 {
        issues.push(Issue::error(at, "a target needs exactly one of ref, point or find"));
    }
    if let Some(p) = t.point
        && !p.is_finite()
    {
        issues.push(Issue::error(at, "point must be finite numbers"));
    }
    if t.reference.as_deref().is_some_and(|r| r.trim().is_empty())
        || t.find.as_deref().is_some_and(|f| f.trim().is_empty())
    {
        issues.push(Issue::error(at, "ref and find can't be empty"));
    }
}

fn is_absolute_any_platform(path: &str) -> bool {
    let b = path.as_bytes();
    path.starts_with('/')
        || path.starts_with("\\\\")
        || (b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b[2] == b'\\' || b[2] == b'/'))
}

/// The JSON Schema every take script follows. Published at `docs/schemas/take.schema.json`.
pub fn json_schema() -> serde_json::Value {
    serde_json::to_value(schemars::schema_for!(TakeScript)).unwrap_or(serde_json::Value::Null)
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXAMPLE: &str = r#"
app: Safari
framing: window
beats:
  - click: { ref: toolbar.address, label: Address bar }
  - type: "shotmaxxing.dev\n"
  - wait: { for: idle }
  - say: Open the docs
  - click: { find: nav.docs }
  - key: mod+f
  - type: auto zoom
  - click: toolbar.export
  - wait: 500
  - user_step: Sign in with your account
output: { mode: editor }
"#;

    #[test]
    fn parses_yaml_with_short_forms() {
        let script = TakeScript::parse(EXAMPLE).unwrap();
        assert_eq!(script.app.as_deref(), Some("Safari"));
        assert_eq!(script.beats.len(), 10);
        assert!(matches!(&script.beats[7], Beat::Click(TargetSpec::Ref(r)) if r == "toolbar.export"));
        assert!(matches!(&script.beats[8], Beat::Wait(WaitBeat::Ms(500))));
        assert!(matches!(
            &script.beats[2],
            Beat::Wait(WaitBeat::Full { until: Some(WaitCondition::Named(WaitNamed::Idle)), .. })
        ));
        assert!(script.is_valid(), "{:?}", script.validate());
    }

    #[test]
    fn parses_json_from_agents() {
        let json = r#"{"app":"Notes","beats":[{"click":{"point":{"x":10,"y":20}}},{"type":{"text":"hi","cps":20}},{"key":"cmd+s"}],"output":{"mode":"video","preset":"social"}}"#;
        let script = TakeScript::parse(json).unwrap();
        assert_eq!(script.output.mode, OutputMode::Video);
        assert!(script.is_valid());
        // Round-trips through YAML.
        let again = TakeScript::parse(&script.to_yaml()).unwrap();
        assert_eq!(again, script);
    }

    #[test]
    fn catches_mistakes_with_clear_messages() {
        let bad = r#"
beats:
  - click: { ref: a, point: { x: 1, y: 2 } }
  - key: cmd+banana
  - type: ""
  - scroll: {}
  - wait: {}
  - say: "  "
expected_files: [relative/path.png]
fps: 500
"#;
        let script = TakeScript::parse(bad).unwrap();
        let issues = script.validate();
        let errors: Vec<_> = issues.iter().filter(|i| i.severity == Severity::Error).collect();
        assert!(errors.iter().any(|i| i.beat == Some(0) && i.message.contains("exactly one")));
        assert!(errors.iter().any(|i| i.beat == Some(1)));
        assert!(errors.iter().any(|i| i.beat == Some(2)));
        assert!(errors.iter().any(|i| i.beat == Some(3)));
        assert!(errors.iter().any(|i| i.beat == Some(4)));
        assert!(errors.iter().any(|i| i.beat == Some(5)));
        assert!(errors.iter().any(|i| i.message.contains("absolute")));
        assert!(errors.iter().any(|i| i.message.contains("fps")));
        assert!(issues.iter().any(|i| i.severity == Severity::Warning && i.message.contains("name the app")));
    }

    #[test]
    fn rejects_unknown_fields_so_typos_surface() {
        assert!(TakeScript::parse("beats: []\nframeing: display\n").is_err());
        let err = TakeScript::parse("beats:\n  - click: { ref: a }\n  - hover: { poin: { x: 1, y: 2 } }\n")
            .unwrap_err();
        assert!(err.to_string().contains("beats[1]"), "{err}");
    }

    #[test]
    fn windows_paths_count_as_absolute() {
        assert!(is_absolute_any_platform(r"C:\Users\me\out.png"));
        assert!(is_absolute_any_platform("/Users/me/out.png"));
        assert!(!is_absolute_any_platform("out.png"));
    }

    #[test]
    fn schema_is_generated() {
        let schema = json_schema();
        assert!(schema.get("properties").and_then(|p| p.get("beats")).is_some());
    }
}
