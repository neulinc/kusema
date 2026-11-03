//! Moving to the Kusema key, once per settings store.
//!
//! New installs get the Kusema key with the classic per-action shortcuts off.
//! Someone upgrading who changed one of the classic shortcuts keeps them all;
//! everyone else moves to the new scheme. Both see the "New: simpler keys"
//! card once, so nobody's keys change without a word.

use std::collections::HashMap;

use super::keys::KusemaKey;
use super::CLASSIC;
use crate::settings::{AppSettings, ShortcutBinding};

/// `keys_scheme` once this has run.
pub const SCHEME: u32 = 1;
/// The Ask box's key before 0.5.0 settled on Ctrl+Alt+K.
const OLD_ASK_BOX_KEY: &str = "ctrl+alt+shift+backspace";

/// The handy-keys spelling of the Kusema key, when a classic shortcut could
/// be bound to the very same key (the Kusema key then wins).
fn same_key(key: &KusemaKey) -> &'static [&'static str] {
    match key {
        KusemaKey::RightCtrl => &["ctrl_right", "rctrl"],
        KusemaKey::NumpadPlus => &["keypadplus", "numpadadd"],
        _ => &[],
    }
}

/// The classic shortcuts the user changed from their defaults, leaving out
/// one set to the Kusema key itself (Right Ctrl for editing, from 0.5's
/// "Use Right Ctrl"): the Kusema key takes that key over.
pub fn customized(
    bindings: &HashMap<String, ShortcutBinding>,
    defaults: &HashMap<String, ShortcutBinding>,
    key: &KusemaKey,
) -> Vec<String> {
    let mut changed: Vec<String> = CLASSIC
        .iter()
        .filter(|id| {
            let (Some(now), Some(default)) = (bindings.get(**id), defaults.get(**id)) else {
                return false;
            };
            let current = now.current_binding.trim().to_lowercase();
            current != default.default_binding.to_lowercase()
                && !same_key(key).contains(&current.as_str())
        })
        .map(|id| id.to_string())
        .collect();
    changed.sort();
    changed
}

/// Runs once; `true` if it changed anything.
pub fn apply(settings: &mut AppSettings, defaults: &AppSettings) -> bool {
    if settings.keys_scheme >= SCHEME {
        return false;
    }
    settings.keys_scheme = SCHEME;
    if let Some(ask_box) = settings.bindings.get_mut(crate::ask_box::BINDING) {
        if ask_box.current_binding == OLD_ASK_BOX_KEY {
            ask_box.current_binding = ask_box_default(defaults);
        }
        ask_box.default_binding = ask_box_default(defaults);
    }
    // Setup not finished: a new install in all but name.
    if !settings.onboarding_completed {
        settings.classic_shortcuts = false;
        return true;
    }
    let changed = customized(&settings.bindings, &defaults.bindings, &settings.kusema_key);
    // A classic key now the Kusema key's goes back to its default.
    for id in CLASSIC {
        let (Some(binding), Some(default)) =
            (settings.bindings.get_mut(*id), defaults.bindings.get(*id))
        else {
            continue;
        };
        if same_key(&settings.kusema_key)
            .contains(&binding.current_binding.trim().to_lowercase().as_str())
        {
            binding.current_binding = default.default_binding.clone();
        }
    }
    settings.classic_shortcuts = !changed.is_empty();
    settings.simpler_keys_card = true;
    true
}

fn ask_box_default(defaults: &AppSettings) -> String {
    defaults
        .bindings
        .get(crate::ask_box::BINDING)
        .map(|b| b.default_binding.clone())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::get_default_settings;

    /// A store from before the Kusema key: setup done, no scheme yet.
    fn upgrading() -> AppSettings {
        let mut settings = get_default_settings();
        settings.keys_scheme = 0;
        settings.onboarding_completed = true;
        settings
    }

    fn set(settings: &mut AppSettings, id: &str, key: &str) {
        settings.bindings.get_mut(id).unwrap().current_binding = key.to_string();
    }

    #[test]
    fn new_installs_get_the_kusema_key_with_classic_shortcuts_off() {
        let fresh = get_default_settings();
        assert_eq!(fresh.keys_scheme, SCHEME);
        assert!(!fresh.classic_shortcuts);
        assert!(fresh.kusema_key_enabled);
        assert_eq!(fresh.kusema_key, KusemaKey::RightCtrl);
        assert!(!fresh.simpler_keys_card);
        // Nothing to migrate.
        let mut again = fresh.clone();
        assert!(!apply(&mut again, &get_default_settings()));
    }

    #[test]
    fn upgrading_with_default_keys_moves_to_the_new_scheme_and_says_so() {
        let mut settings = upgrading();
        assert!(apply(&mut settings, &get_default_settings()));
        assert!(!settings.classic_shortcuts);
        assert!(settings.simpler_keys_card);
        // Once only.
        settings.simpler_keys_card = false;
        assert!(!apply(&mut settings, &get_default_settings()));
        assert!(!settings.simpler_keys_card);
    }

    #[test]
    fn a_customized_hotkey_keeps_the_classic_shortcuts() {
        let mut settings = upgrading();
        set(&mut settings, crate::dictation::BINDING, "keypadplus");
        apply(&mut settings, &get_default_settings());
        assert!(settings.classic_shortcuts);
        assert_eq!(
            settings.bindings[crate::dictation::BINDING].current_binding,
            "keypadplus"
        );
        assert!(settings.simpler_keys_card);
    }

    #[test]
    fn right_ctrl_for_editing_is_taken_over_by_the_kusema_key() {
        let mut settings = upgrading();
        set(&mut settings, crate::voice_edit::BINDING, "ctrl_right");
        apply(&mut settings, &get_default_settings());
        assert!(!settings.classic_shortcuts);
        let edit = &settings.bindings[crate::voice_edit::BINDING];
        assert_eq!(edit.current_binding, edit.default_binding);
    }

    #[test]
    fn setup_not_finished_counts_as_new() {
        let mut settings = upgrading();
        settings.onboarding_completed = false;
        set(&mut settings, "transcribe", "f9");
        apply(&mut settings, &get_default_settings());
        assert!(!settings.classic_shortcuts);
        assert!(!settings.simpler_keys_card);
    }

    #[test]
    fn the_ask_box_moves_to_ctrl_alt_k() {
        let mut settings = upgrading();
        set(&mut settings, crate::ask_box::BINDING, OLD_ASK_BOX_KEY);
        settings
            .bindings
            .get_mut(crate::ask_box::BINDING)
            .unwrap()
            .default_binding = OLD_ASK_BOX_KEY.to_string();
        apply(&mut settings, &get_default_settings());
        let ask_box = &settings.bindings[crate::ask_box::BINDING];
        assert_eq!(ask_box.current_binding, "ctrl+alt+k");
        assert_eq!(ask_box.default_binding, "ctrl+alt+k");
        // The Ask box isn't a classic shortcut: changing it isn't customizing one.
        assert!(!settings.classic_shortcuts);
    }
}
