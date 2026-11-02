//! Built-in AI providers that run on this computer: Ollama and LM Studio. They
//! need no API key, Privileged mode allows them through the network gate's
//! loopback rule, and every request still goes to the privacy ledger.
//!
//! Kept out of `settings.rs` so merges from Handy stay small: settings adds
//! these to its defaults and runs `migrate` when it loads.

use crate::settings::{AppSettings, PostProcessApiFormat, PostProcessProvider};

pub const OLLAMA_ID: &str = "ollama";
pub const LM_STUDIO_ID: &str = "lmstudio";
const CUSTOM_ID: &str = "custom";

fn local(id: &str, label: &str, base_url: &str) -> PostProcessProvider {
    PostProcessProvider {
        id: id.to_string(),
        label: label.to_string(),
        base_url: base_url.to_string(),
        allow_base_url_edit: false,
        models_endpoint: Some("/models".to_string()),
        supports_structured_output: false,
        api_format: PostProcessApiFormat::OpenAi,
    }
}

/// The local providers, in the order they're listed: before the cloud ones.
pub fn providers() -> Vec<PostProcessProvider> {
    vec![
        local(OLLAMA_ID, "Ollama", "http://localhost:11434/v1"),
        local(LM_STUDIO_ID, "LM Studio", "http://localhost:1234/v1"),
    ]
}

/// Providers that work without an API key: these, and Custom, which is usually
/// a server on this computer or the office network.
pub fn key_optional(provider_id: &str) -> bool {
    matches!(provider_id, OLLAMA_ID | LM_STUDIO_ID | CUSTOM_ID)
}

/// The built-in provider a Custom URL is really pointing at, if any.
fn built_in_for(base_url: &str) -> Option<&'static str> {
    let url = reqwest::Url::parse(base_url.trim()).ok()?;
    let on_this_computer = matches!(url.host_str()?, "localhost" | "127.0.0.1" | "[::1]" | "::1");
    if !on_this_computer {
        return None;
    }
    match url.port()? {
        11434 => Some(OLLAMA_ID),
        1234 => Some(LM_STUDIO_ID),
        _ => None,
    }
}

/// Moves people whose Custom provider points at Ollama or LM Studio on this
/// computer to the matching built-in provider, keeping their model. True if
/// anything changed. Running it again changes nothing.
pub fn migrate(settings: &mut AppSettings) -> bool {
    let Some(custom) = settings
        .post_process_providers
        .iter()
        .find(|p| p.id == CUSTOM_ID)
    else {
        return false;
    };
    let Some(target) = built_in_for(&custom.base_url) else {
        return false;
    };
    if !settings
        .post_process_providers
        .iter()
        .any(|p| p.id == target)
    {
        return false;
    }
    let custom_model = settings
        .post_process_models
        .get(CUSTOM_ID)
        .map(|m| m.trim().to_string())
        .unwrap_or_default();

    let mut changed = false;
    let switch = |chosen: &mut String| {
        if *chosen == CUSTOM_ID {
            *chosen = target.to_string();
            true
        } else {
            false
        }
    };
    let switched_main = switch(&mut settings.post_process_provider_id);
    let mut screen = settings
        .assistant_screen_provider_id
        .clone()
        .unwrap_or_default();
    let switched_screen = switch(&mut screen);
    if switched_screen {
        settings.assistant_screen_provider_id = Some(screen);
    }
    changed |= switched_main || switched_screen;

    // Their model goes with them; one already set for the built-in stays,
    // unless they were using Custom, where the model is what they chose.
    let target_model = settings
        .post_process_models
        .get(target)
        .cloned()
        .unwrap_or_default();
    if !custom_model.is_empty()
        && (switched_main || target_model.trim().is_empty())
        && target_model != custom_model
    {
        settings
            .post_process_models
            .insert(target.to_string(), custom_model);
        changed = true;
    }
    if changed {
        log::info!("Moved the Custom provider's settings to the built-in {target} provider");
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::get_default_settings;

    fn pointing_at(url: &str, model: &str) -> AppSettings {
        let mut settings = get_default_settings();
        settings.post_process_provider_id = CUSTOM_ID.to_string();
        settings
            .post_process_providers
            .iter_mut()
            .find(|p| p.id == CUSTOM_ID)
            .unwrap()
            .base_url = url.to_string();
        settings
            .post_process_models
            .insert(CUSTOM_ID.to_string(), model.to_string());
        settings
    }

    #[test]
    fn local_providers_come_first_and_need_no_key() {
        let settings = get_default_settings();
        let ids: Vec<_> = settings
            .post_process_providers
            .iter()
            .map(|p| p.id.as_str())
            .collect();
        assert_eq!(&ids[..2], [OLLAMA_ID, LM_STUDIO_ID]);
        assert_eq!(ids.last(), Some(&CUSTOM_ID));
        assert!(key_optional(OLLAMA_ID) && key_optional(LM_STUDIO_ID) && key_optional(CUSTOM_ID));
        assert!(!key_optional("openai"));
        for provider in providers() {
            assert!(crate::network::is_on_this_computer(&provider.base_url));
            assert!(!provider.allow_base_url_edit);
        }
    }

    #[test]
    fn custom_on_ollama_moves_to_ollama_with_its_model() {
        let mut settings = pointing_at("http://localhost:11434/v1", "llama3.2");
        assert!(migrate(&mut settings));
        assert_eq!(settings.post_process_provider_id, OLLAMA_ID);
        assert_eq!(settings.post_process_models[OLLAMA_ID], "llama3.2");
        assert!(!migrate(&mut settings), "running it again changes nothing");
    }

    #[test]
    fn custom_on_lm_studio_moves_to_lm_studio() {
        for url in [
            "http://127.0.0.1:1234/v1",
            "http://localhost:1234/v1/",
            "http://[::1]:1234/v1",
        ] {
            let mut settings = pointing_at(url, "qwen2.5-7b-instruct");
            assert!(migrate(&mut settings), "{url}");
            assert_eq!(settings.post_process_provider_id, LM_STUDIO_ID, "{url}");
            assert_eq!(
                settings.post_process_models[LM_STUDIO_ID],
                "qwen2.5-7b-instruct"
            );
        }
    }

    #[test]
    fn other_custom_servers_are_left_alone() {
        for url in [
            "http://192.168.1.20:11434/v1",
            "https://api.deepseek.com/v1",
            "http://localhost:8080/v1",
            "not a url",
        ] {
            let mut settings = pointing_at(url, "m");
            let before = settings.clone();
            assert!(!migrate(&mut settings), "{url}");
            assert_eq!(
                settings.post_process_provider_id,
                before.post_process_provider_id
            );
        }
    }

    #[test]
    fn the_screen_question_provider_moves_too() {
        let mut settings = pointing_at("http://localhost:11434/v1", "llava");
        settings.post_process_provider_id = "anthropic".to_string();
        settings.assistant_screen_provider_id = Some(CUSTOM_ID.to_string());
        assert!(migrate(&mut settings));
        assert_eq!(settings.post_process_provider_id, "anthropic");
        assert_eq!(
            settings.assistant_screen_provider_id.as_deref(),
            Some(OLLAMA_ID)
        );
        assert_eq!(settings.post_process_models[OLLAMA_ID], "llava");
    }

    #[test]
    fn an_unused_custom_only_fills_an_empty_model() {
        let mut settings = pointing_at("http://localhost:11434/v1", "llama3.2");
        settings.post_process_provider_id = "openai".to_string();
        settings
            .post_process_models
            .insert(OLLAMA_ID.to_string(), "gemma3".to_string());
        assert!(!migrate(&mut settings));
        assert_eq!(settings.post_process_models[OLLAMA_ID], "gemma3");
        assert_eq!(settings.post_process_provider_id, "openai");
    }

    #[test]
    fn settings_from_before_these_providers_get_them_and_are_migrated() {
        let mut settings = pointing_at("http://localhost:11434/v1", "llama3.2");
        settings
            .post_process_providers
            .retain(|p| p.id != OLLAMA_ID && p.id != LM_STUDIO_ID);
        // As at load: defaults first, then the migration.
        let changed = crate::settings::ensure_post_process_defaults_for_test(&mut settings)
            | migrate(&mut settings);
        assert!(changed);
        assert_eq!(settings.post_process_provider_id, OLLAMA_ID);
        assert_eq!(settings.post_process_models[OLLAMA_ID], "llama3.2");
    }
}
