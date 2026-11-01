//! The "First three wins" checklist: ticks itself as the user does each thing.

use crate::settings;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Emitter};

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq, Type)]
pub struct FirstWins {
    /// Setup finished, so wins count. Practice in setup doesn't.
    pub active: bool,
    pub dictated: bool,
    pub new_paragraph: bool,
    pub asked: bool,
    pub dismissed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Win {
    /// Dictated into an app, either way.
    Dictated,
    /// Said "new paragraph" in continuous dictation.
    NewParagraph,
    /// Got an answer from Ask Kusema or Ask about my screen.
    Asked,
}

impl FirstWins {
    /// Ticks `win`. True when that changed anything.
    pub fn record(&mut self, win: Win) -> bool {
        if !self.active || self.dismissed {
            return false;
        }
        let tick = match win {
            Win::Dictated => &mut self.dictated,
            Win::NewParagraph => &mut self.new_paragraph,
            Win::Asked => &mut self.asked,
        };
        !std::mem::replace(tick, true)
    }
}

fn save(app: &AppHandle, wins: FirstWins) {
    let mut settings = settings::get_settings(app);
    settings.first_wins = wins;
    settings::write_settings(app, settings);
    let _ = app.emit(
        "settings-changed",
        serde_json::json!({ "setting": "first_wins" }),
    );
}

/// Ticks `win` on the checklist, if it's showing and not yet ticked.
pub fn record(app: &AppHandle, win: Win) {
    let mut wins = settings::get_settings(app).first_wins;
    if wins.record(win) {
        save(app, wins);
    }
}

/// Setup finished: the checklist appears on the main page.
#[tauri::command]
#[specta::specta]
pub fn activate_first_wins(app: AppHandle) {
    save(
        &app,
        FirstWins {
            active: true,
            ..Default::default()
        },
    );
}

#[tauri::command]
#[specta::specta]
pub fn dismiss_first_wins(app: AppHandle) {
    let mut wins = settings::get_settings(&app).first_wins;
    wins.dismissed = true;
    save(&app, wins);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn active() -> FirstWins {
        FirstWins {
            active: true,
            ..Default::default()
        }
    }

    #[test]
    fn each_win_ticks_its_own_box_once() {
        let mut wins = active();
        assert!(wins.record(Win::Dictated));
        assert!(wins.dictated && !wins.new_paragraph && !wins.asked);
        assert!(
            !wins.record(Win::Dictated),
            "already ticked: nothing to save"
        );
        assert!(wins.record(Win::NewParagraph));
        assert!(wins.new_paragraph && !wins.asked);
        assert!(wins.record(Win::Asked));
        assert!(wins.asked);
    }

    #[test]
    fn nothing_counts_before_setup_finishes() {
        let mut wins = FirstWins::default();
        assert!(!wins.record(Win::Dictated));
        assert_eq!(wins, FirstWins::default());
    }

    #[test]
    fn nothing_counts_after_dismissing() {
        let mut wins = active();
        wins.dismissed = true;
        assert!(!wins.record(Win::Asked));
        assert!(!wins.asked);
    }

    #[test]
    fn settings_from_before_the_checklist_keep_it_hidden() {
        let settings: crate::settings::AppSettings = {
            let mut value = serde_json::to_value(crate::settings::get_default_settings()).unwrap();
            value.as_object_mut().unwrap().remove("first_wins");
            serde_json::from_value(value).unwrap()
        };
        assert!(!settings.first_wins.active);
    }
}
