//! Which physical key is the Kusema key: the presets, a key the user picked,
//! and which keys can't be one.

use serde::{Deserialize, Serialize};
use specta::Type;

/// Windows virtual-key codes, as a low-level keyboard hook reports them
/// (left and right modifiers apart). Some are named for the tests only.
#[allow(dead_code)]
pub mod vk {
    pub const BACK: u16 = 0x08;
    pub const TAB: u16 = 0x09;
    pub const RETURN: u16 = 0x0D;
    pub const SHIFT: u16 = 0x10;
    pub const CONTROL: u16 = 0x11;
    pub const MENU: u16 = 0x12;
    pub const CAPITAL: u16 = 0x14;
    pub const ESCAPE: u16 = 0x1B;
    pub const SPACE: u16 = 0x20;
    pub const T: u16 = 0x54;
    pub const PRIOR: u16 = 0x21;
    pub const LEFT: u16 = 0x25;
    pub const DOWN: u16 = 0x28;
    pub const DELETE: u16 = 0x2E;
    pub const LWIN: u16 = 0x5B;
    pub const RWIN: u16 = 0x5C;
    pub const APPS: u16 = 0x5D;
    pub const ADD: u16 = 0x6B;
    pub const F1: u16 = 0x70;
    pub const F23: u16 = 0x86;
    pub const F24: u16 = 0x87;
    pub const LSHIFT: u16 = 0xA0;
    pub const RSHIFT: u16 = 0xA1;
    pub const LCONTROL: u16 = 0xA2;
    pub const RCONTROL: u16 = 0xA3;
    pub const LMENU: u16 = 0xA4;
    pub const RMENU: u16 = 0xA5;
}

/// The Kusema key.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum KusemaKey {
    #[default]
    RightCtrl,
    /// The Copilot key. Copilot keyboards send it as Left Win + Left Shift +
    /// F23 (Microsoft's keyboard requirements for the key); F23 is the part
    /// that's swallowed.
    Copilot,
    /// Dragon's microphone key.
    NumpadPlus,
    /// A key the user pressed, by virtual-key code, with its name on the
    /// keyboard as Windows gives it.
    Custom { vk: u16, name: String },
}

impl KusemaKey {
    /// The virtual key the hook watches for.
    pub fn vk(&self) -> u16 {
        match self {
            KusemaKey::RightCtrl => vk::RCONTROL,
            KusemaKey::Copilot => vk::F23,
            KusemaKey::NumpadPlus => vk::ADD,
            KusemaKey::Custom { vk, .. } => *vk,
        }
    }

    /// Whether a key event is this key. `win_shift`: Win and Shift are held
    /// (how the Copilot key arrives). A Win+Shift+F23 is always the Copilot
    /// key, so a custom F23 is told apart from it.
    pub fn matches(&self, vk_code: u16, win_shift: bool) -> bool {
        match self {
            KusemaKey::Copilot => vk_code == vk::F23 && win_shift,
            KusemaKey::Custom { vk: v, .. } if *v == vk::F23 => vk_code == vk::F23 && !win_shift,
            key => vk_code == key.vk(),
        }
    }
}

/// Why a key can't be the Kusema key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum KeyRefusal {
    /// Letters, digits, punctuation, Space, Enter, Tab, Backspace: the key
    /// would stop typing what it types.
    Types,
    /// Escape cancels; the left modifiers, Win, Alt Gr and Caps Lock are
    /// needed for everything else.
    Needed,
    /// Arrows and the keys around them move the cursor.
    Moves,
}

/// Whether `vk_code` may be the Kusema key. Swallowed for good, so it must
/// be a key that's rarely needed for anything else.
pub fn check_custom(vk_code: u16) -> Result<(), KeyRefusal> {
    match vk_code {
        vk::BACK | vk::TAB | vk::RETURN | vk::SPACE => Err(KeyRefusal::Types),
        // Digits and letters.
        0x30..=0x39 | 0x41..=0x5A => Err(KeyRefusal::Types),
        // The OEM keys: ; = , - . / ` [ \ ] ' and the extra one on ISO boards.
        0xBA..=0xC0 | 0xDB..=0xDF | 0xE2 => Err(KeyRefusal::Types),
        vk::ESCAPE | vk::CAPITAL | vk::SHIFT | vk::CONTROL | vk::MENU => Err(KeyRefusal::Needed),
        vk::LSHIFT | vk::LCONTROL | vk::LMENU | vk::LWIN | vk::RWIN => Err(KeyRefusal::Needed),
        // Right Alt is Alt Gr on most layouts outside the US.
        vk::RMENU => Err(KeyRefusal::Needed),
        // Page Up/Down, End, Home, the arrows, and Delete.
        vk::PRIOR..=vk::DOWN | vk::DELETE => Err(KeyRefusal::Moves),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_presets_watch_their_keys() {
        assert!(KusemaKey::RightCtrl.matches(vk::RCONTROL, false));
        assert!(!KusemaKey::RightCtrl.matches(vk::LCONTROL, false));
        assert!(KusemaKey::NumpadPlus.matches(vk::ADD, false));
        // The Copilot key is F23 with Win and Shift down; a bare F23 isn't.
        assert!(KusemaKey::Copilot.matches(vk::F23, true));
        assert!(!KusemaKey::Copilot.matches(vk::F23, false));
        let f23 = KusemaKey::Custom {
            vk: vk::F23,
            name: "F23".into(),
        };
        assert!(f23.matches(vk::F23, false));
        assert!(!f23.matches(vk::F23, true));
    }

    #[test]
    fn keys_that_type_or_are_needed_are_refused() {
        for code in [
            b'A' as u16,
            b'Z' as u16,
            b'0' as u16,
            vk::SPACE,
            vk::RETURN,
            vk::TAB,
            vk::BACK,
            0xBA,
            0xDE,
        ] {
            assert_eq!(check_custom(code), Err(KeyRefusal::Types), "{code:#x}");
        }
        for code in [
            vk::ESCAPE,
            vk::LCONTROL,
            vk::LSHIFT,
            vk::LMENU,
            vk::LWIN,
            vk::RMENU,
            vk::CAPITAL,
        ] {
            assert_eq!(check_custom(code), Err(KeyRefusal::Needed), "{code:#x}");
        }
        for code in [vk::LEFT, vk::DOWN, vk::PRIOR, vk::DELETE] {
            assert_eq!(check_custom(code), Err(KeyRefusal::Moves), "{code:#x}");
        }
    }

    #[test]
    fn spare_keys_are_allowed() {
        for code in [
            vk::RCONTROL,
            vk::RSHIFT,
            vk::ADD,
            vk::APPS,
            vk::F1,
            vk::F24,
            0x13, /* Pause */
            0x91, /* Scroll Lock */
            0x2D, /* Insert */
            0x6D, /* Numpad - */
        ] {
            assert_eq!(check_custom(code), Ok(()), "{code:#x}");
        }
    }

    #[test]
    fn right_ctrl_is_the_default() {
        assert_eq!(KusemaKey::default(), KusemaKey::RightCtrl);
        let json = serde_json::to_string(&KusemaKey::Custom {
            vk: 0x13,
            name: "Pause".into(),
        })
        .unwrap();
        assert_eq!(json, r#"{"kind":"custom","vk":19,"name":"Pause"}"#);
    }
}
