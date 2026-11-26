//! Kusema's cloud AI providers beyond Handy's: DeepSeek and Google Gemini,
//! both through their OpenAI-compatible APIs, plus the order the provider list
//! is shown in, the model suggested once a key works, and moving an OpenAI
//! setup on the Custom provider to the OpenAI one.
//!
//! Kept out of `settings.rs` so merges from Handy stay small: settings adds
//! these to its defaults and runs `apply` when it loads.
//!
//! Checked 2026-11-26 against the providers' own documentation:
//! - DeepSeek: base URL https://api.deepseek.com, OpenAI format, `GET /models`;
//!   current models `deepseek-flash` and `deepseek-v4-pro` (`deepseek-chat`
//!   and `deepseek-reasoner` were retired on 2026-07-24). Thinking output
//!   comes in `reasoning_content`, which Kusema never reads or shows.
//!   https://api-docs.deepseek.com/ , https://api-docs.deepseek.com/quick_start/pricing ,
//!   https://api-docs.deepseek.com/guides/thinking_mode/
//! - Gemini: the OpenAI-compatible endpoint
//!   https://generativelanguage.googleapis.com/v1beta/openai/ with the key as a
//!   Bearer token; chat, streaming and `GET /models`. Grounding with Google
//!   Search isn't offered there for chat (only the native API has it), so
//!   Gemini uses Brave or SearXNG for web search.
//!   https://ai.google.dev/gemini-api/docs/openai
//! - Suggested models (stable, from each provider's models page):
//!   https://platform.claude.com/docs/en/about-claude/models/overview ,
//!   https://developers.openai.com/api/docs/models ,
//!   https://ai.google.dev/gemini-api/docs/models

use crate::settings::{AppSettings, PostProcessApiFormat, PostProcessProvider};

pub const DEEPSEEK_ID: &str = "deepseek";
pub const GEMINI_ID: &str = "gemini";
const OPENAI_ID: &str = "openai";
const CUSTOM_ID: &str = "custom";

fn cloud(id: &str, label: &str, base_url: &str) -> PostProcessProvider {
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

/// DeepSeek and Gemini, for settings' defaults.
pub fn providers() -> Vec<PostProcessProvider> {
    vec![
        cloud(DEEPSEEK_ID, "DeepSeek", "https://api.deepseek.com"),
        cloud(
            GEMINI_ID,
            "Google Gemini",
            "https://generativelanguage.googleapis.com/v1beta/openai",
        ),
    ]
}

/// The list's order: on this computer first, then the cloud providers Kusema
/// recommends, then the rest as Handy had them, Custom last.
const ORDER: &[&str] = &[
    "ollama",
    "lmstudio",
    "anthropic",
    DEEPSEEK_ID,
    GEMINI_ID,
    OPENAI_ID,
    "zai",
    "openrouter",
    "groq",
    "cerebras",
    "apple_intelligence",
    "bedrock_mantle",
];

fn rank(id: &str) -> usize {
    if id == CUSTOM_ID {
        return usize::MAX;
    }
    ORDER
        .iter()
        .position(|known| *known == id)
        .unwrap_or(ORDER.len())
}

/// Settings' defaults: adds DeepSeek and Gemini, and puts the list in order.
pub fn add_to(providers: &mut Vec<PostProcessProvider>) {
    providers.extend(self::providers());
    providers.sort_by_key(|p| rank(&p.id));
}

/// Puts the stored list in `ORDER` (installs from before 0.5.5 have Handy's).
fn sort(settings: &mut AppSettings) -> bool {
    let before: Vec<String> = settings
        .post_process_providers
        .iter()
        .map(|p| p.id.clone())
        .collect();
    settings.post_process_providers.sort_by_key(|p| rank(&p.id));
    settings
        .post_process_providers
        .iter()
        .map(|p| p.id.as_str())
        .ne(before.iter().map(String::as_str))
}

fn points_at_openai(base_url: &str) -> bool {
    reqwest::Url::parse(base_url.trim())
        .ok()
        .and_then(|u| {
            u.host_str()
                .map(|h| h.eq_ignore_ascii_case("api.openai.com"))
        })
        .unwrap_or(false)
}

/// Someone who reached OpenAI through Custom (Handy had no other way for some
/// setups) is moved to the OpenAI provider: the same key and model, nothing
/// lost. Custom itself is left as it was. True if anything changed.
fn migrate_openai(settings: &mut AppSettings) -> bool {
    let Some(custom) = settings
        .post_process_providers
        .iter()
        .find(|p| p.id == CUSTOM_ID)
    else {
        return false;
    };
    if !points_at_openai(&custom.base_url) {
        return false;
    }
    let mut changed = false;
    let key = |s: &AppSettings, id: &str| {
        s.post_process_api_keys
            .get(id)
            .map(|k| k.trim().to_string())
            .unwrap_or_default()
    };
    let custom_key = key(settings, CUSTOM_ID);
    if !custom_key.is_empty() && key(settings, OPENAI_ID).is_empty() {
        settings
            .post_process_api_keys
            .insert(OPENAI_ID.to_string(), custom_key);
        changed = true;
    }
    let custom_model = settings
        .post_process_models
        .get(CUSTOM_ID)
        .map(|m| m.trim().to_string())
        .unwrap_or_default();
    let openai_model = settings
        .post_process_models
        .get(OPENAI_ID)
        .map(|m| m.trim().to_string())
        .unwrap_or_default();
    if !custom_model.is_empty() && openai_model.is_empty() {
        settings
            .post_process_models
            .insert(OPENAI_ID.to_string(), custom_model);
        changed = true;
    }
    if settings.post_process_provider_id == CUSTOM_ID {
        settings.post_process_provider_id = OPENAI_ID.to_string();
        changed = true;
    }
    if settings.assistant_screen_provider_id.as_deref() == Some(CUSTOM_ID) {
        settings.assistant_screen_provider_id = Some(OPENAI_ID.to_string());
        changed = true;
    }
    if changed {
        log::info!("Moved an OpenAI setup on the Custom provider to the OpenAI provider");
    }
    changed
}

/// Runs when settings load. True if anything changed.
pub fn apply(settings: &mut AppSettings) -> bool {
    migrate_openai(settings) | sort(settings)
}

/// The model to start with once a key works: a stable model the provider
/// recommends, if its model list has it. Nothing otherwise: the list decides.
pub fn suggested_model(provider_id: &str, models: &[String]) -> Option<String> {
    let preferred: &[&str] = match provider_id {
        "anthropic" => &["claude-sonnet-5-5", "claude-opus-5-5"],
        OPENAI_ID => &["gpt-6.2-sol", "gpt-6.2-astra", "gpt-6-luna"],
        DEEPSEEK_ID => &["deepseek-flash", "deepseek-v4-pro"],
        GEMINI_ID => &["gemini-3.8-flash", "gemini-3.6-flash"],
        // Claude on Amazon Bedrock (bedrock-mantle), when the region lists it.
        crate::bedrock::ID => &[
            "anthropic.claude-sonnet-5-5",
            "anthropic.claude-haiku-5-5",
            "anthropic.claude-opus-5-5",
        ],
        _ => &[],
    };
    preferred
        .iter()
        .find(|want| models.iter().any(|m| m == *want))
        .map(|m| m.to_string())
        .or_else(|| {
            // Bedrock without the preferred ones: the newest Claude it lists.
            (provider_id == crate::bedrock::ID)
                .then(|| crate::bedrock::newest_claude(models))
                .flatten()
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::get_default_settings;

    fn ids(settings: &AppSettings) -> Vec<String> {
        settings
            .post_process_providers
            .iter()
            .map(|p| p.id.clone())
            .collect()
    }

    #[test]
    fn local_first_then_anthropic_deepseek_gemini_openai() {
        let settings = get_default_settings();
        let ids = ids(&settings);
        assert_eq!(
            &ids[..6],
            [
                "ollama",
                "lmstudio",
                "anthropic",
                "deepseek",
                "gemini",
                "openai"
            ]
        );
        assert_eq!(ids.last().map(String::as_str), Some("custom"));
    }

    #[test]
    fn an_older_install_is_put_in_the_new_order_and_keeps_everything() {
        let mut settings = get_default_settings();
        // Handy's order, with the new providers added at the end.
        settings
            .post_process_providers
            .sort_by_key(|p| match p.id.as_str() {
                "openai" => 0,
                "zai" => 1,
                "custom" => 50,
                "deepseek" | "gemini" => 40,
                _ => 10,
            });
        settings
            .post_process_api_keys
            .insert("openai".into(), "sk-keep".into());
        assert!(apply(&mut settings));
        assert_eq!(
            &ids(&settings)[2..6],
            ["anthropic", "deepseek", "gemini", "openai"]
        );
        assert_eq!(settings.post_process_api_keys["openai"], "sk-keep");
        // Running again changes nothing.
        assert!(!apply(&mut settings));
    }

    #[test]
    fn deepseek_and_gemini_use_their_openai_compatible_apis() {
        let providers = providers();
        let deepseek = &providers[0];
        assert_eq!(deepseek.base_url, "https://api.deepseek.com");
        assert_eq!(deepseek.api_format, PostProcessApiFormat::OpenAi);
        let gemini = &providers[1];
        assert_eq!(
            gemini.base_url,
            "https://generativelanguage.googleapis.com/v1beta/openai"
        );
        assert!(providers.iter().all(|p| !p.allow_base_url_edit));
    }

    fn on_custom(url: &str) -> AppSettings {
        let mut settings = get_default_settings();
        settings.post_process_provider_id = CUSTOM_ID.into();
        settings
            .post_process_providers
            .iter_mut()
            .find(|p| p.id == CUSTOM_ID)
            .unwrap()
            .base_url = url.into();
        settings
            .post_process_api_keys
            .insert(CUSTOM_ID.into(), "sk-custom".into());
        settings
            .post_process_models
            .insert(CUSTOM_ID.into(), "gpt-6.2-sol".into());
        settings
    }

    #[test]
    fn openai_on_custom_moves_to_the_openai_provider_with_key_and_model() {
        let mut settings = on_custom("https://api.openai.com/v1");
        settings.assistant_screen_provider_id = Some(CUSTOM_ID.into());
        assert!(apply(&mut settings));
        assert_eq!(settings.post_process_provider_id, "openai");
        assert_eq!(
            settings.assistant_screen_provider_id.as_deref(),
            Some("openai")
        );
        assert_eq!(settings.post_process_api_keys["openai"], "sk-custom");
        assert_eq!(settings.post_process_models["openai"], "gpt-6.2-sol");
        // Custom keeps its own settings.
        assert_eq!(settings.post_process_api_keys[CUSTOM_ID], "sk-custom");
        assert!(!apply(&mut settings), "running again changes nothing");
    }

    #[test]
    fn an_openai_key_already_saved_is_never_overwritten() {
        let mut settings = on_custom("https://API.OPENAI.COM/v1/");
        settings
            .post_process_api_keys
            .insert("openai".into(), "sk-own".into());
        settings
            .post_process_models
            .insert("openai".into(), "gpt-6-luna".into());
        apply(&mut settings);
        assert_eq!(settings.post_process_api_keys["openai"], "sk-own");
        assert_eq!(settings.post_process_models["openai"], "gpt-6-luna");
        assert_eq!(settings.post_process_provider_id, "openai");
    }

    #[test]
    fn other_custom_servers_are_left_alone() {
        for url in [
            "http://localhost:8080/v1",
            "https://api.openai.com.evil.example/v1",
            "https://my-proxy.example/openai/v1",
        ] {
            let mut settings = on_custom(url);
            migrate_openai(&mut settings);
            assert_eq!(settings.post_process_provider_id, CUSTOM_ID, "{url}");
        }
    }

    /// A server on loopback answering one request with `response`.
    fn serve_once(response: String) -> String {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buffer = [0u8; 16384];
                let _ = stream.read(&mut buffer);
                let _ = stream.write_all(response.as_bytes());
            }
        });
        format!("http://{address}")
    }

    fn deepseek_at(base_url: String) -> PostProcessProvider {
        PostProcessProvider {
            base_url,
            ..providers().remove(0)
        }
    }

    const THOUGHT: &str = "SECRET CHAIN OF THOUGHT";

    #[test]
    fn deepseeks_reasoning_never_reaches_the_answer_when_streamed() {
        let events = [
            format!(
                r#"{{"choices":[{{"delta":{{"role":"assistant","reasoning_content":"{THOUGHT} part 1"}}}}]}}"#
            ),
            format!(
                r#"{{"choices":[{{"delta":{{"reasoning_content":" {THOUGHT} part 2","content":null}}}}]}}"#
            ),
            r#"{"choices":[{"delta":{"content":"The deadline "}}]}"#.to_string(),
            r#"{"choices":[{"delta":{"content":"is 30 days."},"finish_reason":"stop"}]}"#
                .to_string(),
        ];
        let body: String = events
            .iter()
            .map(|e| format!("data: {e}\n\n"))
            .chain(std::iter::once("data: [DONE]\n\n".to_string()))
            .collect();
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let provider = deepseek_at(serve_once(response));
        let mut streamed = String::new();
        let answer = tauri::async_runtime::block_on(crate::llm_client::send_conversation(
            &provider,
            "key".into(),
            "deepseek-flash",
            "sys",
            &[crate::llm_client::ChatTurn::new(
                crate::llm_client::ChatRole::User,
                "q",
            )],
            |t| streamed.push_str(t),
        ))
        .unwrap()
        .unwrap();
        assert_eq!(answer, "The deadline is 30 days.");
        // What's shown and spoken as it streams has none of it either.
        assert_eq!(streamed, "The deadline is 30 days.");
        assert!(!answer.contains("SECRET"));
    }

    #[test]
    fn deepseeks_reasoning_never_reaches_the_answer_in_one_piece() {
        let body = format!(
            r#"{{"choices":[{{"message":{{"role":"assistant","reasoning_content":"{THOUGHT}","content":"Thirty days."}},"finish_reason":"stop"}}]}}"#
        );
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let provider = deepseek_at(serve_once(response));
        let answer = tauri::async_runtime::block_on(crate::llm_client::send_conversation(
            &provider,
            "key".into(),
            "deepseek-flash",
            "",
            &[crate::llm_client::ChatTurn::new(
                crate::llm_client::ChatRole::User,
                "q",
            )],
            |_| {},
        ))
        .unwrap()
        .unwrap();
        assert_eq!(answer, "Thirty days.");
    }

    #[test]
    fn a_suggested_model_only_if_the_provider_lists_it() {
        let listed = vec!["deepseek-v4-pro".to_string(), "deepseek-flash".to_string()];
        assert_eq!(
            suggested_model("deepseek", &listed).as_deref(),
            Some("deepseek-flash")
        );
        assert_eq!(suggested_model("deepseek", &["other".into()]), None);
        assert_eq!(suggested_model("ollama", &listed), None);
        assert_eq!(
            suggested_model("gemini", &["gemini-3.6-flash".into()]).as_deref(),
            Some("gemini-3.6-flash")
        );
    }
}
