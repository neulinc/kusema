//! Kusema styles per app: AI cleanup, AI voice edits and answers typed into a
//! document follow a style chosen by the program they go into (Outlook gets
//! email format, Teams short and casual). Only the program's file name is
//! used, read locally; window titles and contents aren't. Local, rule-based
//! cleanup doesn't change.

use log::debug;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Emitter};

use crate::settings::{get_settings, write_settings, AppSettings};

/// One style and the programs it's for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct AppStyle {
    /// "email", "formal" and "casual" are built in; others are the user's.
    pub id: String,
    /// `None`: the built-in style's (translated) name.
    #[serde(default)]
    pub name: Option<String>,
    /// Program file names, lowercase ("outlook.exe").
    #[serde(default)]
    pub apps: Vec<String>,
    /// In plain language. `None`: the built-in style's.
    #[serde(default)]
    pub instruction: Option<String>,
}

/// The built-in styles' instructions, as sent to the AI.
const BUILT_IN: &[(&str, &str)] = &[
    (
        "email",
        "Format it as an email: a short greeting line, brief paragraphs, and a sign-off when it's \
a whole message. Keep the author's voice.",
    ),
    (
        "formal",
        "Use a formal, professional tone: complete sentences, no slang, no contractions.",
    ),
    (
        "casual",
        "Keep it concise and casual: short sentences and plain, friendly words, with no formal \
greeting or sign-off.",
    ),
];

pub fn default_styles() -> Vec<AppStyle> {
    let style = |id: &str, apps: &[&str]| AppStyle {
        id: id.to_string(),
        name: None,
        apps: apps.iter().map(|a| a.to_string()).collect(),
        instruction: None,
    };
    vec![
        // Classic Outlook, and the new one.
        style("email", &["outlook.exe", "olk.exe"]),
        // Word; OnlyOffice's desktop editors.
        style(
            "formal",
            &["winword.exe", "desktopeditors.exe", "editors.exe"],
        ),
        // Teams (new and classic) and Slack.
        style("casual", &["ms-teams.exe", "teams.exe", "slack.exe"]),
    ]
}

/// "Outlook", "OUTLOOK.EXE" and "C:\...\outlook.exe" are the same program.
pub fn normalize_app(name: &str) -> String {
    normalize_for(name, cfg!(target_os = "windows"))
}

/// `normalize_app` as on Windows (`windows`), where programs end in .exe, or
/// elsewhere, where they don't. Apart so both can be tested on either.
fn normalize_for(name: &str, windows: bool) -> String {
    let name = name.trim().rsplit(['\\', '/']).next().unwrap_or("").trim();
    let lower = name.to_lowercase();
    if lower.is_empty() || lower.ends_with(".exe") || !windows {
        lower
    } else {
        format!("{lower}.exe")
    }
}

fn built_in_instruction(id: &str) -> Option<&'static str> {
    BUILT_IN
        .iter()
        .find(|(i, _)| *i == id)
        .map(|(_, text)| *text)
}

impl AppStyle {
    /// What the AI is told, if anything.
    pub fn instruction(&self) -> Option<String> {
        let text = match &self.instruction {
            Some(own) => own.trim().to_string(),
            None => built_in_instruction(&self.id)?.to_string(),
        };
        (!text.is_empty()).then_some(text)
    }
}

/// The style for `process` (a file name, any case), if one lists it.
pub fn style_for<'a>(styles: &'a [AppStyle], process: &str) -> Option<&'a AppStyle> {
    let process = normalize_app(process);
    if process.is_empty() {
        return None;
    }
    styles
        .iter()
        .find(|style| style.apps.iter().any(|app| normalize_app(app) == process))
        .filter(|style| style.instruction().is_some())
}

/// The style in use for `process`, unless styles are off.
pub fn active(settings: &AppSettings, process: &str) -> Option<Active> {
    if !settings.app_styles_enabled {
        return None;
    }
    let style = style_for(&settings.app_styles, process)?;
    Some(Active {
        id: style.id.clone(),
        name: style.name.clone().filter(|n| !n.trim().is_empty()),
        instruction: style.instruction()?,
    })
}

/// A style that applies to some text, and what the overlay shows for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
pub struct Active {
    pub id: String,
    pub name: Option<String>,
    #[serde(skip)]
    pub instruction: String,
}

/// The program that owns `window`.
pub fn process_of(window: isize) -> String {
    crate::voice_edit::process_name(window)
}

/// The style for the window in front.
pub fn for_foreground(settings: &AppSettings) -> Option<Active> {
    if !settings.app_styles_enabled {
        return None;
    }
    let window = crate::screen::foreground_target().ok()?;
    active(settings, &process_of(window.handle()))
}

/// Shows the style in the overlay, as it's about to change the text.
pub fn announce(app: &AppHandle, style: &Active) {
    debug!("App style: {}", style.id);
    let _ = app.emit("app-style", style.clone());
}

/// For an answer typed into a document.
pub fn instruction_for_prompt(instruction: &str) -> String {
    format!("Write it in the style of the program it goes into: {instruction}")
}

/// For AI cleanup: the user's prompt plus the app's style, which mustn't add
/// anything that wasn't said.
pub fn with_style(prompt: &str, instruction: &str) -> String {
    format!(
        "{prompt}\n\nThe text goes into a program with its own style. Follow it as far as the \
text allows, without adding anything that wasn't said: {instruction}"
    )
}

/// The user's styles, as edited: names and apps tidied, empty apps dropped.
fn tidy(styles: Vec<AppStyle>) -> Vec<AppStyle> {
    styles
        .into_iter()
        .filter(|style| !style.id.trim().is_empty())
        .map(|mut style| {
            let mut apps: Vec<String> = Vec::new();
            for app in style.apps.iter().map(|a| normalize_app(a)) {
                if !app.is_empty() && !apps.contains(&app) {
                    apps.push(app);
                }
            }
            style.apps = apps;
            style.name = style.name.map(|n| n.trim().to_string());
            style
        })
        .collect()
}

fn changed(app: &AppHandle, settings: AppSettings, name: &str) {
    write_settings(app, settings);
    let _ = app.emit("settings-changed", serde_json::json!({ "setting": name }));
}

#[tauri::command]
#[specta::specta]
pub fn set_app_styles(app: AppHandle, styles: Vec<AppStyle>) -> Result<(), String> {
    let mut settings = get_settings(&app);
    settings.app_styles = tidy(styles);
    changed(&app, settings, "app_styles");
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn reset_app_styles(app: AppHandle) -> Result<(), String> {
    let mut settings = get_settings(&app);
    settings.app_styles = default_styles();
    changed(&app, settings, "app_styles");
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_app_styles_enabled_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = get_settings(&app);
    settings.app_styles_enabled = enabled;
    changed(&app, settings, "app_styles_enabled");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id_for(process: &str) -> Option<String> {
        style_for(&default_styles(), process).map(|s| s.id.clone())
    }

    #[test]
    fn the_defaults_map_programs_to_styles() {
        assert_eq!(id_for("OUTLOOK.EXE").as_deref(), Some("email"));
        assert_eq!(id_for("olk.exe").as_deref(), Some("email"));
        assert_eq!(id_for("WINWORD.EXE").as_deref(), Some("formal"));
        assert_eq!(id_for("DesktopEditors.exe").as_deref(), Some("formal"));
        assert_eq!(id_for("ms-teams.exe").as_deref(), Some("casual"));
        assert_eq!(id_for("slack.exe").as_deref(), Some("casual"));
        // Everything else is neutral: no style at all.
        for other in [
            "notepad.exe",
            "chrome.exe",
            "code.exe",
            "",
            "outlook-helper.exe",
        ] {
            assert_eq!(id_for(other), None, "{other}");
        }
    }

    #[test]
    fn program_names_are_compared_loosely() {
        // Windows: a name without .exe gets it.
        assert_eq!(normalize_for("  Outlook ", true), "outlook.exe");
        assert_eq!(
            normalize_for(r"C:\Program Files\Slack\slack.exe", true),
            "slack.exe"
        );
        assert_eq!(normalize_for("Notepad.EXE", true), "notepad.exe");
        assert_eq!(normalize_for("   ", true), "");
        // Elsewhere program names have no .exe, so none is added.
        assert_eq!(normalize_for("  Slack ", false), "slack");
        assert_eq!(normalize_for("/usr/bin/Slack", false), "slack");
        assert_eq!(normalize_for("   ", false), "");
        // And this build uses its own platform's rule.
        assert_eq!(
            normalize_app("Notepad"),
            normalize_for("Notepad", cfg!(target_os = "windows"))
        );
    }

    #[test]
    fn styles_are_edited_in_plain_language_and_apps_can_be_added() {
        let mut styles = default_styles();
        styles[0].instruction = Some("Short emails, always signed \"Ross\".".to_string());
        styles.push(AppStyle {
            id: "custom-1".to_string(),
            name: Some("Notes".to_string()),
            apps: vec![
                "Notepad".to_string(),
                "notepad".to_string(),
                " ".to_string(),
            ],
            instruction: Some("Bullet points.".to_string()),
        });
        let styles = tidy(styles);
        // One entry, spelled as this platform names programs; no blank.
        assert_eq!(styles[3].apps, [normalize_app("notepad")]);
        let notes = style_for(&styles, "NOTEPAD").unwrap();
        assert_eq!(notes.instruction().as_deref(), Some("Bullet points."));
        let email = style_for(&styles, "outlook.exe").unwrap();
        assert!(email.instruction().unwrap().contains("Ross"));
    }

    #[test]
    fn an_emptied_style_is_neutral() {
        let mut styles = default_styles();
        styles[2].instruction = Some("  ".to_string());
        assert!(style_for(&styles, "slack.exe").is_none());
    }

    #[test]
    fn styles_can_be_turned_off() {
        let mut settings = crate::settings::get_default_settings();
        assert_eq!(
            active(&settings, "outlook.exe").map(|a| a.id).as_deref(),
            Some("email")
        );
        settings.app_styles_enabled = false;
        assert_eq!(active(&settings, "outlook.exe"), None);
    }

    #[test]
    fn cleanup_prompts_keep_the_users_prompt_first() {
        let prompt = with_style("Clean up: ${output}", "Formal.");
        assert!(prompt.starts_with("Clean up: ${output}"));
        assert!(prompt.ends_with("Formal."));
        assert!(prompt.contains("without adding anything"));
    }
}
