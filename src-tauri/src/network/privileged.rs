//! Privileged mode and the Privacy page's commands.

use std::sync::atomic::{AtomicU64, Ordering};

use chrono::{Local, TimeZone};
use log::info;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Emitter};

use super::ledger::to_csv;
use super::{ledger, prune_now, DestinationCount, LedgerEntry, LedgerFilter, Purpose};
use crate::actions::ShortcutAction;
use crate::settings::{get_settings, write_settings};

/// Its hotkey, which is off unless the user turns it on.
pub const TOGGLE_BINDING: &str = "toggle_privileged";

/// The list on the Privacy page stops here; the CSV has everything.
const LIST_LIMIT: i64 = 500;
const EXPORT_LIMIT: i64 = i64::MAX;
pub const RETENTION_DAYS_RANGE: (u32, u32) = (1, 3650);
/// Ignores key-repeat: a held hotkey mustn't flip the mode back and forth.
const TOGGLE_DEBOUNCE_MS: u64 = 500;

/// What's shown and spoken when Privileged mode refuses an AI request.
pub fn refusal_message(provider: &str) -> String {
    format!(
        "Privileged mode is on, so this wasn't sent to {provider}. Use a local model or turn off Privileged mode."
    )
}

/// Shows and says the refusal.
pub fn announce_refusal(app: &AppHandle, provider: &str) {
    crate::screen::card::show_notice(app, refusal_message(provider));
}

/// Turns Privileged mode on or off, from anywhere.
pub fn set_privileged(app: &AppHandle, enabled: bool) {
    let mut settings = get_settings(app);
    if settings.privileged_mode == enabled {
        return;
    }
    settings.privileged_mode = enabled;
    write_settings(app, settings);
    info!("Privileged mode {}", if enabled { "on" } else { "off" });
    crate::tray::update_tray_menu(app);
    let _ = app.emit(
        "settings-changed",
        serde_json::json!({ "setting": "privileged_mode", "value": enabled }),
    );
    let _ = app.emit("privileged-changed", enabled);
}

pub fn toggle_privileged(app: &AppHandle) {
    set_privileged(app, !get_settings(app).privileged_mode);
}

pub struct TogglePrivilegedAction;

static LAST_TOGGLE_MS: AtomicU64 = AtomicU64::new(0);

impl ShortcutAction for TogglePrivilegedAction {
    fn start(&self, app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {
        let now = chrono::Utc::now().timestamp_millis() as u64;
        let last = LAST_TOGGLE_MS.swap(now, Ordering::Relaxed);
        if now.saturating_sub(last) < TOGGLE_DEBOUNCE_MS {
            return;
        }
        toggle_privileged(app);
        // There's nothing else to see from a hotkey, and it matters which way it went.
        let message = if get_settings(app).privileged_mode {
            "Privileged mode is on. Nothing will be sent to cloud AI."
        } else {
            "Privileged mode is off."
        };
        crate::screen::card::show_notice(app, message.to_string());
    }

    fn stop(&self, _app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {}
}

/// The top of the Privacy page.
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct PrivacyOverview {
    pub today: Vec<DestinationCount>,
    pub last_7_days: Vec<DestinationCount>,
}

/// Seconds since the epoch at the start of the local day `days_back` days ago.
fn start_of_local_day(days_back: i64) -> i64 {
    let date = Local::now().date_naive() - chrono::Duration::days(days_back);
    date.and_hms_opt(0, 0, 0)
        .and_then(|time| Local.from_local_datetime(&time).earliest())
        .map_or_else(
            || chrono::Utc::now().timestamp() - days_back * 86_400,
            |time| time.timestamp(),
        )
}

#[tauri::command]
#[specta::specta]
pub fn get_privacy_overview() -> Result<PrivacyOverview, String> {
    let ledger = ledger()?;
    Ok(PrivacyOverview {
        today: ledger
            .by_destination(start_of_local_day(0))
            .map_err(|e| e.to_string())?,
        last_7_days: ledger
            .by_destination(start_of_local_day(6))
            .map_err(|e| e.to_string())?,
    })
}

#[tauri::command]
#[specta::specta]
pub fn get_privacy_ledger(filter: LedgerFilter) -> Result<Vec<LedgerEntry>, String> {
    ledger()?
        .list(&filter, LIST_LIMIT)
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn clear_privacy_ledger(app: AppHandle) -> Result<(), String> {
    ledger()?.clear().map_err(|e| e.to_string())?;
    let _ = app.emit("privacy-ledger-changed", ());
    Ok(())
}

/// Writes the whole ledger, whatever the page's filters show, to `path`.
#[tauri::command]
#[specta::specta]
pub fn export_privacy_ledger(path: String) -> Result<u32, String> {
    let entries = ledger()?
        .list(&LedgerFilter::default(), EXPORT_LIMIT)
        .map_err(|e| e.to_string())?;
    let csv = to_csv(&entries, |timestamp| {
        Local
            .timestamp_opt(timestamp, 0)
            .single()
            .map_or_else(String::new, |time| {
                time.format("%Y-%m-%d %H:%M:%S").to_string()
            })
    });
    std::fs::write(&path, csv).map_err(|e| e.to_string())?;
    Ok(entries.len() as u32)
}

/// Logs the updater plugin's request, and says whether it may go. The plugin
/// makes it from the webview, where it can't be stopped from Rust, so this is
/// the nearest point.
#[tauri::command]
#[specta::specta]
pub fn authorize_update_check(app: AppHandle) -> bool {
    let url = updater_endpoint(&app);
    super::announce(Purpose::UpdateCheck, &url).is_ok()
}

/// The endpoint in `tauri.conf.json`.
fn updater_endpoint(app: &AppHandle) -> String {
    app.config()
        .plugins
        .0
        .get("updater")
        .and_then(|updater| updater.get("endpoints"))
        .and_then(|endpoints| endpoints.get(0))
        .and_then(|endpoint| endpoint.as_str())
        .unwrap_or("https://github.com")
        .to_string()
}

#[tauri::command]
#[specta::specta]
pub fn change_privileged_mode_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    set_privileged(&app, enabled);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_privileged_allow_lan_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = get_settings(&app);
    settings.privileged_allow_lan = enabled;
    write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_privileged_block_downloads_setting(
    app: AppHandle,
    enabled: bool,
) -> Result<(), String> {
    let mut settings = get_settings(&app);
    settings.privileged_block_downloads = enabled;
    write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_privileged_hotkey_enabled_setting(
    app: AppHandle,
    enabled: bool,
) -> Result<(), String> {
    let mut settings = get_settings(&app);
    settings.privileged_hotkey_enabled = enabled;
    let binding = settings.bindings.get(TOGGLE_BINDING).cloned();
    write_settings(&app, settings);
    if let Some(binding) = binding {
        let result = if enabled {
            crate::shortcut::register_shortcut(&app, binding)
        } else {
            crate::shortcut::unregister_shortcut(&app, binding)
        };
        if let Err(e) = result {
            log::warn!("Couldn't change the Privileged mode hotkey: {e}");
        }
    }
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_ledger_retention_days_setting(app: AppHandle, days: u32) -> Result<(), String> {
    let mut settings = get_settings(&app);
    settings.ledger_retention_days = days.clamp(RETENTION_DAYS_RANGE.0, RETENTION_DAYS_RANGE.1);
    write_settings(&app, settings);
    prune_now();
    Ok(())
}
