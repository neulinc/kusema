//! Kusema first-run setup: what the setup screens need from the backend, and
//! the "First three wins" checklist they leave on the main page.

pub mod first_wins;
pub mod local_ai;
pub mod mic_test;

use crate::settings;
use tauri::{AppHandle, Emitter};

/// Shows every model on the Models page, not just the short list.
#[tauri::command]
#[specta::specta]
pub fn change_show_all_models_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.show_all_models = enabled;
    settings::write_settings(&app, settings);
    let _ = app.emit(
        "settings-changed",
        serde_json::json!({ "setting": "show_all_models", "value": enabled }),
    );
    Ok(())
}
