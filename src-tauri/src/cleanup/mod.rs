//! Local cleanup of dictated text: rule-based, on by default, and entirely on this
//! computer. Runs after Handy's filler-word removal and before any AI cleanup.
//!
//! Spoken punctuation is here. Adding a missing period between continuous
//! dictation segments needs the next segment too, so it's in `dictation::join`.

mod punctuation;

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::AppHandle;

use crate::network::{self, Purpose};
use crate::settings::{get_settings, write_settings, AppSettings, APPLE_INTELLIGENCE_PROVIDER_ID};

/// Voice commands, which "literal" also protects. Mirrors `dictation::voice_commands`.
const COMMAND_WORDS: &[&[&str]] = &[
    &["new", "paragraph"],
    &["new", "line"],
    &["newline"],
    &["scratch", "that"],
    &["stop", "dictation"],
    &["stop", "dictating"],
];

/// Whether this text is in a language the rules were written for. They're
/// English words, so anything known to be another language passes through.
fn rules_apply_to(language: Option<&str>) -> bool {
    language.is_none_or(|code| {
        let code = code.to_ascii_lowercase();
        code == "auto" || code == "en" || code.starts_with("en-") || code.starts_with("en_")
    })
}

/// The language to judge by: what was detected, or else what the user set.
fn effective_language<'a>(settings: &'a AppSettings, detected: Option<&'a str>) -> Option<&'a str> {
    detected.or(Some(settings.selected_language.as_str()))
}

fn rules_are_on(settings: &AppSettings, language: Option<&str>) -> bool {
    settings.local_cleanup_enabled
        && settings.spoken_punctuation_enabled
        && rules_apply_to(effective_language(settings, language))
}

/// Cleans up one piece of dictated text. `language` is what was detected, if known.
pub fn apply(settings: &AppSettings, text: &str, language: Option<&str>) -> String {
    if !rules_are_on(settings, language) {
        return text.to_string();
    }
    punctuation::apply(text)
}

/// The same for a segment of continuous dictation, whose own setting can turn
/// voice commands off: then "new paragraph" is typed as said.
pub fn apply_to_segment(settings: &AppSettings, text: &str, voice_commands: bool) -> String {
    if !rules_are_on(settings, None) {
        return text.to_string();
    }
    if voice_commands {
        punctuation::apply(text)
    } else {
        punctuation::apply_without_line_commands(text)
    }
}

/// What the Cleanup settings say about AI cleanup's provider.
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct AiCleanupStatus {
    pub provider: Option<String>,
    pub model: Option<String>,
    /// On this computer, not a cloud service.
    pub is_local: bool,
    pub has_key: bool,
    /// Privileged mode would refuse it.
    pub blocked: bool,
}

fn ai_cleanup_status_for(settings: &AppSettings) -> AiCleanupStatus {
    let Some(provider) = settings.active_post_process_provider() else {
        return AiCleanupStatus {
            provider: None,
            model: None,
            is_local: false,
            has_key: false,
            blocked: false,
        };
    };
    let on_device = provider.id == APPLE_INTELLIGENCE_PROVIDER_ID;
    let is_local = on_device || network::is_on_this_computer(&provider.base_url);
    AiCleanupStatus {
        provider: Some(provider.label.clone()),
        model: settings
            .post_process_models
            .get(&provider.id)
            .map(|model| model.trim().to_string())
            .filter(|model| !model.is_empty()),
        is_local,
        // A server on this computer usually needs no key.
        has_key: is_local
            || settings
                .post_process_api_keys
                .get(&provider.id)
                .is_some_and(|key| !key.trim().is_empty()),
        blocked: !on_device && network::would_block(Purpose::PostProcessing, &provider.base_url),
    }
}

#[tauri::command]
#[specta::specta]
pub fn get_ai_cleanup_status(app: AppHandle) -> AiCleanupStatus {
    ai_cleanup_status_for(&get_settings(&app))
}

#[tauri::command]
#[specta::specta]
pub fn change_local_cleanup_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = get_settings(&app);
    settings.local_cleanup_enabled = enabled;
    write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_spoken_punctuation_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = get_settings(&app);
    settings.spoken_punctuation_enabled = enabled;
    write_settings(&app, settings);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio_toolkit::{
        normalize_transcription_output, remove_filler_words, OutputLanguageEvidence,
    };
    use crate::dictation::command_phrases;
    use crate::settings::get_default_settings;

    fn on() -> AppSettings {
        get_default_settings()
    }

    const SPOKEN: &str = "Dear Sir comma the limitations period expired period";

    #[test]
    fn local_cleanup_and_spoken_punctuation_are_on_by_default() {
        let settings = on();
        assert!(settings.local_cleanup_enabled);
        assert!(settings.spoken_punctuation_enabled);
        assert_eq!(
            apply(&settings, SPOKEN, None),
            "Dear Sir, the limitations period expired."
        );
    }

    #[test]
    fn ai_cleanup_is_off_by_default() {
        assert!(!on().post_process_enabled);
    }

    #[test]
    fn local_cleanup_off_leaves_the_text_unchanged() {
        let settings = AppSettings {
            local_cleanup_enabled: false,
            ..on()
        };
        assert_eq!(apply(&settings, SPOKEN, None), SPOKEN);
    }

    #[test]
    fn spoken_punctuation_off_leaves_the_text_unchanged() {
        let settings = AppSettings {
            spoken_punctuation_enabled: false,
            ..on()
        };
        assert_eq!(apply(&settings, SPOKEN, None), SPOKEN);
    }

    #[test]
    fn other_languages_pass_through() {
        let settings = on();
        let text = "El colon period";
        assert_eq!(apply(&settings, text, Some("es")), text);
        assert_eq!(apply(&settings, "We won period", Some("en")), "We won.");
        assert_eq!(apply(&settings, "We won period", Some("en-US")), "We won.");
        let spanish_user = AppSettings {
            selected_language: "es".to_string(),
            ..on()
        };
        assert_eq!(apply(&spanish_user, text, None), text);
        assert_eq!(apply(&spanish_user, "We won period", Some("en")), "We won.");
    }

    /// As it runs in the app: filler removal in transcription, then this.
    fn through_the_pipeline(raw: &str, settings: &AppSettings) -> String {
        let without_fillers = remove_filler_words(
            raw,
            &OutputLanguageEvidence::Detected("en".to_string()),
            &settings.custom_filler_words,
            settings.filler_removal_active(),
        );
        apply(
            settings,
            &normalize_transcription_output(&without_fillers),
            Some("en"),
        )
    }

    #[test]
    fn composes_with_filler_removal() {
        let settings = on();
        assert_eq!(
            through_the_pipeline("Um, the motion is uh denied, um, period", &settings),
            "The motion is denied."
        );
        assert_eq!(
            through_the_pipeline("uh dear sir comma um thank you", &settings),
            "dear sir, thank you"
        );
        assert_eq!(
            through_the_pipeline("the um limitations period expired", &settings),
            "the limitations period expired"
        );
    }

    #[test]
    fn line_commands_compose_with_filler_removal() {
        assert_eq!(
            through_the_pipeline("Thank you, um, new paragraph. Uh, the end", &on()),
            "Thank you,\n\nThe end"
        );
    }

    #[test]
    fn line_commands_follow_the_cleanup_settings() {
        let said = "Thank you new paragraph The end";
        for settings in [
            AppSettings {
                local_cleanup_enabled: false,
                ..on()
            },
            AppSettings {
                spoken_punctuation_enabled: false,
                ..on()
            },
        ] {
            assert_eq!(apply(&settings, said, None), said);
            assert_eq!(apply_to_segment(&settings, said, true), said);
        }
        assert_eq!(apply(&on(), said, Some("es")), said);
    }

    #[test]
    fn the_voice_command_setting_only_turns_off_line_commands() {
        assert_eq!(
            apply_to_segment(&on(), "Thank you new paragraph The end comma", false),
            "Thank you new paragraph The end,"
        );
        assert_eq!(
            apply_to_segment(&on(), "Thank you new paragraph The end comma", true),
            "Thank you\n\nThe end,"
        );
    }

    #[test]
    fn local_cleanup_off_also_keeps_the_fillers() {
        let settings = AppSettings {
            local_cleanup_enabled: false,
            ..on()
        };
        let raw = "um the motion comma";
        assert_eq!(through_the_pipeline(raw, &settings), raw);
    }

    #[test]
    fn the_command_list_matches_voice_commands() {
        let mut here: Vec<String> = COMMAND_WORDS.iter().map(|words| words.join(" ")).collect();
        let mut there: Vec<String> = command_phrases().map(str::to_string).collect();
        here.sort();
        there.sort();
        assert_eq!(here, there);
    }

    #[test]
    fn status_describes_a_cloud_provider() {
        let mut settings = on();
        settings.post_process_provider_id = "anthropic".to_string();
        settings
            .post_process_models
            .insert("anthropic".to_string(), "claude".to_string());
        settings
            .post_process_api_keys
            .insert("anthropic".to_string(), "key".to_string());
        let status = ai_cleanup_status_for(&settings);
        assert_eq!(status.provider.as_deref(), Some("Anthropic"));
        assert_eq!(status.model.as_deref(), Some("claude"));
        assert!(!status.is_local);
        assert!(status.has_key);
    }

    #[test]
    fn status_recognizes_a_model_on_this_computer() {
        let mut settings = on();
        settings.post_process_provider_id = "custom".to_string();
        if let Some(custom) = settings
            .post_process_providers
            .iter_mut()
            .find(|p| p.id == "custom")
        {
            custom.base_url = "http://localhost:11434/v1".to_string();
        }
        let status = ai_cleanup_status_for(&settings);
        assert!(status.is_local);
        assert!(status.has_key, "a local server needs no key");
    }
}
