//! Virtual key codes for chords, on the ANSI (US) layout. Typed text doesn't use these: it is sent as characters, so
//! it comes out right on every layout.

use objc2_core_graphics::CGEventFlags;
use shotmaxxing_core::keys::{Key, KeyChord, NamedKey, Platform};

pub const RETURN: u16 = 36;
pub const TAB: u16 = 48;
pub const ESCAPE: u16 = 53;

/// The key code for a key, and whether Shift is part of it (`!` is Shift-1).
pub fn code(key: Key) -> Option<(u16, bool)> {
    Some(match key {
        Key::Named(named) => (
            match named {
                NamedKey::Return => RETURN,
                NamedKey::Escape => ESCAPE,
                NamedKey::Tab => TAB,
                NamedKey::Space => 49,
                NamedKey::Backspace => 51,
                NamedKey::Delete => 117,
                NamedKey::Up => 126,
                NamedKey::Down => 125,
                NamedKey::Left => 123,
                NamedKey::Right => 124,
                NamedKey::Home => 115,
                NamedKey::End => 119,
                NamedKey::PageUp => 116,
                NamedKey::PageDown => 121,
                NamedKey::F(n) => function_key(n)?,
            },
            false,
        ),
        Key::Char(ch) => char_code(ch)?,
    })
}

fn function_key(n: u8) -> Option<u16> {
    const KEYS: [u16; 20] =
        [122, 120, 99, 118, 96, 97, 98, 100, 101, 109, 103, 111, 105, 107, 113, 106, 64, 79, 80, 90];
    KEYS.get(usize::from(n).checked_sub(1)?).copied()
}

fn char_code(ch: char) -> Option<(u16, bool)> {
    const LETTERS: [u16; 26] =
        [0, 11, 8, 2, 14, 3, 5, 4, 34, 38, 40, 37, 46, 45, 31, 35, 12, 15, 1, 17, 32, 9, 13, 7, 16, 6];
    const DIGITS: [u16; 10] = [29, 18, 19, 20, 21, 23, 22, 26, 28, 25];
    if ch.is_ascii_lowercase() {
        return Some((LETTERS[(ch as u8 - b'a') as usize], false));
    }
    if ch.is_ascii_uppercase() {
        return Some((LETTERS[(ch as u8 - b'A') as usize], true));
    }
    if ch.is_ascii_digit() {
        return Some((DIGITS[(ch as u8 - b'0') as usize], false));
    }
    let plain = |code| Some((code, false));
    let shifted = |code| Some((code, true));
    match ch {
        '-' => plain(27),
        '=' => plain(24),
        '[' => plain(33),
        ']' => plain(30),
        '\\' => plain(42),
        ';' => plain(41),
        '\'' => plain(39),
        ',' => plain(43),
        '.' => plain(47),
        '/' => plain(44),
        '`' => plain(50),
        ' ' => plain(49),
        '_' => shifted(27),
        '+' => shifted(24),
        '{' => shifted(33),
        '}' => shifted(30),
        '|' => shifted(42),
        ':' => shifted(41),
        '"' => shifted(39),
        '<' => shifted(43),
        '>' => shifted(47),
        '?' => shifted(44),
        '~' => shifted(50),
        '!' => shifted(18),
        '@' => shifted(19),
        '#' => shifted(20),
        '$' => shifted(21),
        '%' => shifted(23),
        '^' => shifted(22),
        '&' => shifted(26),
        '*' => shifted(28),
        '(' => shifted(25),
        ')' => shifted(29),
        _ => None,
    }
}

/// The key code and modifier flags for a chord, or `None` for a key this layout table doesn't have.
pub fn chord(chord: &KeyChord) -> Option<(u16, CGEventFlags)> {
    let (code, shift) = code(chord.key)?;
    // Chords store letters in capitals; only symbols such as `?` need Shift for themselves.
    let shift = shift && !matches!(chord.key, Key::Char(c) if c.is_ascii_alphabetic());
    let m = chord.modifiers.resolved(Platform::Macos);
    let mut flags = CGEventFlags::empty();
    for (on, flag) in [
        (m.command, CGEventFlags::MaskCommand),
        (m.control, CGEventFlags::MaskControl),
        (m.alt, CGEventFlags::MaskAlternate),
        (m.shift || shift, CGEventFlags::MaskShift),
    ] {
        if on {
            flags |= flag;
        }
    }
    Some((code, flags))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chords_map_to_key_codes() {
        let save = KeyChord::parse("mod+s").unwrap();
        assert_eq!(chord(&save), Some((1, CGEventFlags::MaskCommand)));
        let redo = KeyChord::parse("cmd+shift+z").unwrap();
        assert_eq!(chord(&redo), Some((6, CGEventFlags::MaskCommand | CGEventFlags::MaskShift)));
        assert_eq!(chord(&KeyChord::parse("Return").unwrap()), Some((RETURN, CGEventFlags::empty())));
        assert_eq!(chord(&KeyChord::parse("F5").unwrap()).map(|c| c.0), Some(96));
        assert_eq!(code(Key::Char('?')), Some((44, true)));
    }
}
