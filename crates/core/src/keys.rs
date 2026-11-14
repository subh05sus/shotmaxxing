//! Keyboard chords: parsed forgivingly, shown the way each platform expects.
//!
//! `cmd+shift+z`, `Cmd-Shift-Z`, `⌘⇧Z` and `command shift z` all mean the same chord. `mod` (or `primary`) is the
//! platform's main modifier: Command on macOS, Control on Windows and Linux. Display follows the platform:
//! `⌘⇧Z` on macOS, `Ctrl+Shift+Z` on Windows.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt;
use thiserror::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Platform {
    Macos,
    Windows,
    Linux,
}

impl Platform {
    pub const fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::Macos
        } else if cfg!(target_os = "windows") {
            Self::Windows
        } else {
            Self::Linux
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct Modifiers {
    /// The platform's main modifier (Command on macOS, Control elsewhere).
    #[serde(default, skip_serializing_if = "is_false")]
    pub primary: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub command: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub control: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub alt: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub shift: bool,
}

fn is_false(b: &bool) -> bool {
    !*b
}

impl Modifiers {
    /// Resolves `primary` into the real modifier for a platform.
    pub fn resolved(self, platform: Platform) -> Self {
        let mut m = self;
        if m.primary {
            m.primary = false;
            match platform {
                Platform::Macos => m.command = true,
                Platform::Windows | Platform::Linux => m.control = true,
            }
        }
        m
    }

    pub fn is_empty(self) -> bool {
        !(self.primary || self.command || self.control || self.alt || self.shift)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum NamedKey {
    Return,
    Escape,
    Tab,
    Space,
    Backspace,
    Delete,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    F(u8),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Key {
    Char(char),
    Named(NamedKey),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct KeyChord {
    pub modifiers: Modifiers,
    pub key: Key,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum KeyParseError {
    #[error("enter a key, for example Return or cmd+s")]
    Empty,
    #[error(
        "'{0}' is not a key we know; try Return, Escape, Tab, Space, an arrow, F1-F24 or a single character"
    )]
    UnknownKey(String),
    #[error("a chord needs exactly one key after its modifiers, got '{0}'")]
    MissingKey(String),
}

impl KeyChord {
    pub fn parse(input: &str) -> Result<Self, KeyParseError> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err(KeyParseError::Empty);
        }
        // Expand modifier symbols into words so "⌘⇧Z" becomes "cmd+shift+Z".
        let mut expanded = String::with_capacity(trimmed.len() + 16);
        for ch in trimmed.chars() {
            match ch {
                '⌘' => expanded.push_str("cmd+"),
                '⇧' => expanded.push_str("shift+"),
                '⌥' => expanded.push_str("alt+"),
                '⌃' => expanded.push_str("ctrl+"),
                _ => expanded.push(ch),
            }
        }
        let tokens = split_tokens(&expanded);
        let mut modifiers = Modifiers::default();
        let mut key: Option<Key> = None;
        for token in &tokens {
            let lower = token.to_lowercase();
            match lower.as_str() {
                "cmd" | "command" | "meta" | "super" | "win" | "windows" => modifiers.command = true,
                "ctrl" | "control" | "ctl" => modifiers.control = true,
                "alt" | "option" | "opt" => modifiers.alt = true,
                "shift" => modifiers.shift = true,
                "mod" | "primary" | "cmdorctrl" | "commandorcontrol" => modifiers.primary = true,
                _ => {
                    if key.is_some() {
                        return Err(KeyParseError::MissingKey(trimmed.to_string()));
                    }
                    key = Some(parse_key(token)?);
                }
            }
        }
        let key = key.ok_or_else(|| KeyParseError::MissingKey(trimmed.to_string()))?;
        Ok(Self { modifiers, key })
    }

    /// How the chord is shown to a person on a platform.
    pub fn display(&self, platform: Platform) -> String {
        let m = self.modifiers.resolved(platform);
        let key = key_label(self.key, platform);
        match platform {
            Platform::Macos => {
                let mut s = String::new();
                if m.control {
                    s.push('⌃');
                }
                if m.alt {
                    s.push('⌥');
                }
                if m.shift {
                    s.push('⇧');
                }
                if m.command {
                    s.push('⌘');
                }
                s.push_str(&key);
                s
            }
            Platform::Windows | Platform::Linux => {
                let mut parts: Vec<&str> = Vec::new();
                if m.control {
                    parts.push("Ctrl");
                }
                if m.command {
                    parts.push(if platform == Platform::Windows { "Win" } else { "Super" });
                }
                if m.alt {
                    parts.push("Alt");
                }
                if m.shift {
                    parts.push("Shift");
                }
                let mut s = parts.join("+");
                if !s.is_empty() {
                    s.push('+');
                }
                s.push_str(&key);
                s
            }
        }
    }
}

impl fmt::Display for KeyChord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.display(Platform::current()))
    }
}

fn split_tokens(s: &str) -> Vec<String> {
    // '+' and whitespace separate tokens. A trailing '+' after a separator is the plus key itself ("cmd++").
    let mut tokens = Vec::new();
    let mut current = String::new();
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '+' || c.is_whitespace() {
            if current.is_empty() && c == '+' && i + 1 == chars.len() && !tokens.is_empty() {
                current.push('+');
            } else if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
        } else {
            current.push(c);
        }
        i += 1;
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    // "ctrl-s" style: one token holding modifier-dash-key.
    if tokens.len() == 1 && tokens[0].len() > 2 && tokens[0].contains('-') && !tokens[0].ends_with('-') {
        let parts: Vec<String> = tokens[0].split('-').map(str::to_string).collect();
        if parts.iter().all(|p| !p.is_empty()) {
            return parts;
        }
    }
    tokens
}

fn parse_key(token: &str) -> Result<Key, KeyParseError> {
    let lower = token.to_lowercase();
    let named = match lower.as_str() {
        "return" | "enter" | "↩" | "⏎" => Some(NamedKey::Return),
        "escape" | "esc" | "⎋" => Some(NamedKey::Escape),
        "tab" | "⇥" => Some(NamedKey::Tab),
        "space" | "spacebar" | "␣" => Some(NamedKey::Space),
        "backspace" | "⌫" => Some(NamedKey::Backspace),
        "delete" | "del" | "forwarddelete" | "⌦" => Some(NamedKey::Delete),
        "up" | "arrowup" | "↑" => Some(NamedKey::Up),
        "down" | "arrowdown" | "↓" => Some(NamedKey::Down),
        "left" | "arrowleft" | "←" => Some(NamedKey::Left),
        "right" | "arrowright" | "→" => Some(NamedKey::Right),
        "home" => Some(NamedKey::Home),
        "end" => Some(NamedKey::End),
        "pageup" | "pgup" => Some(NamedKey::PageUp),
        "pagedown" | "pgdn" => Some(NamedKey::PageDown),
        _ => None,
    };
    if let Some(named) = named {
        return Ok(Key::Named(named));
    }
    if let Some(rest) = lower.strip_prefix('f')
        && let Ok(n) = rest.parse::<u8>()
        && (1..=24).contains(&n)
    {
        return Ok(Key::Named(NamedKey::F(n)));
    }
    let mut chars = token.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) => Ok(Key::Char(c.to_ascii_uppercase())),
        _ => Err(KeyParseError::UnknownKey(token.to_string())),
    }
}

fn key_label(key: Key, platform: Platform) -> String {
    match key {
        Key::Char(c) => c.to_string(),
        Key::Named(named) => {
            let mac = platform == Platform::Macos;
            match named {
                NamedKey::Return => if mac { "↩" } else { "Enter" }.into(),
                NamedKey::Escape => if mac { "⎋" } else { "Esc" }.into(),
                NamedKey::Tab => if mac { "⇥" } else { "Tab" }.into(),
                NamedKey::Space => "Space".into(),
                NamedKey::Backspace => if mac { "⌫" } else { "Backspace" }.into(),
                NamedKey::Delete => if mac { "⌦" } else { "Del" }.into(),
                NamedKey::Up => "↑".into(),
                NamedKey::Down => "↓".into(),
                NamedKey::Left => "←".into(),
                NamedKey::Right => "→".into(),
                NamedKey::Home => "Home".into(),
                NamedKey::End => "End".into(),
                NamedKey::PageUp => "PgUp".into(),
                NamedKey::PageDown => "PgDn".into(),
                NamedKey::F(n) => format!("F{n}"),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn many_spellings_one_chord() {
        let expected = KeyChord::parse("cmd+shift+z").unwrap();
        for input in ["Cmd-Shift-Z", "⌘⇧Z", "command shift z", "CMD + SHIFT + z", "⇧⌘z"] {
            assert_eq!(KeyChord::parse(input).unwrap(), expected, "{input}");
        }
    }

    #[test]
    fn named_keys_and_function_keys() {
        assert_eq!(KeyChord::parse("Return").unwrap().key, Key::Named(NamedKey::Return));
        assert_eq!(KeyChord::parse("esc").unwrap().key, Key::Named(NamedKey::Escape));
        assert_eq!(KeyChord::parse("f12").unwrap().key, Key::Named(NamedKey::F(12)));
        assert!(KeyChord::parse("ctrl-s").unwrap().modifiers.control);
        assert_eq!(KeyChord::parse("cmd++").unwrap().key, Key::Char('+'));
    }

    #[test]
    fn errors_are_helpful() {
        assert_eq!(KeyChord::parse(""), Err(KeyParseError::Empty));
        assert!(matches!(KeyChord::parse("cmd+shift"), Err(KeyParseError::MissingKey(_))));
        assert!(matches!(KeyChord::parse("hyperdrive"), Err(KeyParseError::UnknownKey(_))));
        assert!(matches!(KeyChord::parse("a+b"), Err(KeyParseError::MissingKey(_))));
    }

    #[test]
    fn displays_per_platform() {
        let chord = KeyChord::parse("mod+shift+z").unwrap();
        assert_eq!(chord.display(Platform::Macos), "⇧⌘Z");
        assert_eq!(chord.display(Platform::Windows), "Ctrl+Shift+Z");
        assert_eq!(KeyChord::parse("return").unwrap().display(Platform::Windows), "Enter");
    }
}
