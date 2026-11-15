//! Virtual keys for chords. Typed text doesn't use these: it goes in as characters, so it comes out right on every
//! layout. Letters, digits and named keys are the same on every layout; other characters are looked up on the
//! keyboard layout in use, so `ctrl+/` presses whatever makes `/` there.

use shotmaxxing_core::keys::{Key, KeyChord, NamedKey, Platform};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    VIRTUAL_KEY, VK_APPS, VK_BACK, VK_CONTROL, VK_DELETE, VK_DIVIDE, VK_DOWN, VK_END, VK_ESCAPE, VK_F1,
    VK_HOME, VK_INSERT, VK_LEFT, VK_LWIN, VK_MENU, VK_NEXT, VK_NUMLOCK, VK_PRIOR, VK_RCONTROL, VK_RETURN,
    VK_RIGHT, VK_RMENU, VK_RWIN, VK_SHIFT, VK_SNAPSHOT, VK_SPACE, VK_TAB, VK_UP, VkKeyScanW,
};

/// A key to press, and the modifiers to hold around it, in the order they go down.
#[derive(Debug, PartialEq, Eq)]
pub struct Press {
    pub key: VIRTUAL_KEY,
    pub modifiers: Vec<VIRTUAL_KEY>,
}

pub fn named(key: NamedKey) -> Option<VIRTUAL_KEY> {
    Some(match key {
        NamedKey::Return => VK_RETURN,
        NamedKey::Escape => VK_ESCAPE,
        NamedKey::Tab => VK_TAB,
        NamedKey::Space => VK_SPACE,
        NamedKey::Backspace => VK_BACK,
        NamedKey::Delete => VK_DELETE,
        NamedKey::Up => VK_UP,
        NamedKey::Down => VK_DOWN,
        NamedKey::Left => VK_LEFT,
        NamedKey::Right => VK_RIGHT,
        NamedKey::Home => VK_HOME,
        NamedKey::End => VK_END,
        NamedKey::PageUp => VK_PRIOR,
        NamedKey::PageDown => VK_NEXT,
        NamedKey::F(n @ 1..=24) => VIRTUAL_KEY(VK_F1.0 + u16::from(n) - 1),
        NamedKey::F(_) => return None,
    })
}

/// The key and modifiers for a chord. `command` is the Windows key, which sits where Command does.
pub fn chord(chord: &KeyChord) -> Option<Press> {
    let m = chord.modifiers.resolved(Platform::Windows);
    let (key, shift, control, alt) = match chord.key {
        Key::Named(key) => (named(key)?, false, false, false),
        // Chords keep letters in capitals; Shift isn't part of them.
        Key::Char(ch) if ch.is_ascii_alphanumeric() => {
            (VIRTUAL_KEY(ch.to_ascii_uppercase() as u16), false, false, false)
        }
        Key::Char(ch) => {
            let mut units = [0u16; 2];
            let [unit] = ch.encode_utf16(&mut units) else { return None };
            // SAFETY: a lookup in this thread's keyboard layout.
            let found = unsafe { VkKeyScanW(*unit) };
            if found == -1 {
                return None;
            }
            let state = (found as u16) >> 8;
            (VIRTUAL_KEY(found as u16 & 0xff), state & 1 != 0, state & 2 != 0, state & 4 != 0)
        }
    };
    let modifiers = [
        (m.control || control, VK_CONTROL),
        (m.alt || alt, VK_MENU),
        (m.shift || shift, VK_SHIFT),
        (m.command, VK_LWIN),
    ]
    .into_iter()
    .filter_map(|(on, key)| on.then_some(key))
    .collect();
    Some(Press { key, modifiers })
}

/// Keys on the arrow and editing blocks (and the other copies of some keys) that Windows marks as extended.
pub fn extended(key: VIRTUAL_KEY) -> bool {
    matches!(
        key,
        VK_UP
            | VK_DOWN
            | VK_LEFT
            | VK_RIGHT
            | VK_HOME
            | VK_END
            | VK_PRIOR
            | VK_NEXT
            | VK_INSERT
            | VK_DELETE
            | VK_LWIN
            | VK_RWIN
            | VK_APPS
            | VK_RCONTROL
            | VK_RMENU
            | VK_DIVIDE
            | VK_NUMLOCK
            | VK_SNAPSHOT
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chords_map_to_virtual_keys() {
        let save = KeyChord::parse("mod+s").unwrap();
        assert_eq!(chord(&save), Some(Press { key: VIRTUAL_KEY(b'S' as u16), modifiers: vec![VK_CONTROL] }));
        let redo = KeyChord::parse("ctrl+shift+z").unwrap();
        assert_eq!(chord(&redo).unwrap().modifiers, vec![VK_CONTROL, VK_SHIFT]);
        assert_eq!(chord(&KeyChord::parse("Return").unwrap()).unwrap().key, VK_RETURN);
        assert_eq!(chord(&KeyChord::parse("F5").unwrap()).unwrap().key, VIRTUAL_KEY(VK_F1.0 + 4));
        assert_eq!(chord(&KeyChord::parse("cmd+d").unwrap()).unwrap().modifiers, vec![VK_LWIN]);
        assert!(extended(VK_LEFT) && !extended(VK_RETURN));
    }
}
