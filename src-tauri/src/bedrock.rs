//! Kusema: Amazon Bedrock through its `bedrock-mantle` endpoint, completing
//! the provider Handy added as "AWS Bedrock (Mantle)" (id `bedrock_mantle`,
//! kept so existing settings, keys and the HiddenProviders policy still work).
//!
//! Mantle serves two request formats, by model:
//! - Anthropic's Messages API for Claude (`anthropic.*` model ids), at
//!   `https://bedrock-mantle.{region}.api.aws/anthropic/v1/messages`, with the
//!   Bedrock API key in `x-api-key` and `anthropic-version: 2023-06-01`.
//!   Claude doesn't accept Chat Completions on Bedrock.
//! - OpenAI Chat Completions for the other models, at
//!   `https://bedrock-mantle.{region}.api.aws/v1/chat/completions`, with the
//!   key as `Authorization: Bearer`; `GET /v1/models` lists the models.
//!
//! Checked 2026-11-26 against AWS's documentation:
//! https://docs.aws.amazon.com/bedrock/latest/userguide/bedrock-mantle.html ,
//! https://docs.aws.amazon.com/bedrock/latest/userguide/inference-messages-api.html ,
//! https://docs.aws.amazon.com/bedrock/latest/userguide/models-endpoint-availability.html ,
//! https://docs.aws.amazon.com/bedrock/latest/userguide/endpoints-region-availability.html ,
//! https://docs.aws.amazon.com/bedrock/latest/userguide/api-keys.html ,
//! https://docs.aws.amazon.com/bedrock/latest/userguide/troubleshooting-api-error-codes.html .
//! Anthropic's server-side tools (web search) aren't available on Bedrock
//! (https://platform.claude.com/docs/en/build-with-claude/claude-in-amazon-bedrock),
//! so web search with Bedrock uses Brave or SearXNG.

use crate::assistant::failure::Failure;
use crate::settings::{
    get_settings, write_settings, AppSettings, PostProcessApiFormat, PostProcessProvider,
};
use tauri::{AppHandle, Emitter};

pub const ID: &str = "bedrock_mantle";
pub const LABEL: &str = "Amazon Bedrock";
pub const DEFAULT_REGION: &str = "us-east-1";

/// Commercial regions with the bedrock-mantle endpoint
/// (endpoints-region-availability, 2026-11-26).
pub const REGIONS: &[&str] = &[
    "us-east-1",
    "us-east-2",
    "us-west-2",
    "eu-central-1",
    "eu-north-1",
    "eu-south-1",
    "eu-west-1",
    "eu-west-2",
    "ap-northeast-1",
    "ap-south-1",
    "ap-southeast-2",
    "ap-southeast-3",
    "sa-east-1",
];

/// AWS GovCloud (US): only with a GovCloud account.
pub const GOVCLOUD_REGIONS: &[&str] = &["us-gov-east-1", "us-gov-west-1"];

pub fn is_region(region: &str) -> bool {
    REGIONS.contains(&region) || GOVCLOUD_REGIONS.contains(&region)
}

/// The OpenAI-compatible base for `region`.
pub fn base_url(region: &str) -> String {
    format!("https://bedrock-mantle.{region}.api.aws/v1")
}

/// The region in a bedrock-mantle address, if it is one.
pub fn region_of(base_url: &str) -> Option<String> {
    let host = reqwest::Url::parse(base_url).ok()?.host_str()?.to_string();
    let region = host
        .strip_prefix("bedrock-mantle.")?
        .strip_suffix(".api.aws")?;
    is_region(region).then(|| region.to_string())
}

/// Claude models use the Messages API; every other model Chat Completions.
pub fn uses_messages_api(model: &str) -> bool {
    model.trim().starts_with("anthropic.")
}

/// A Claude id's version (`anthropic.claude-haiku-4-5` → [4, 5]) and family.
/// Long numbers (dates) aren't versions.
fn claude_version(model: &str) -> Option<(Vec<u32>, String)> {
    let rest = model.trim().strip_prefix("anthropic.claude-")?;
    let mut family = String::new();
    let mut version = Vec::new();
    for part in rest.split(['-', ':']) {
        match part.parse::<u32>() {
            Ok(n) if part.len() <= 2 => version.push(n),
            Ok(_) => {}
            Err(_) if family.is_empty() => family = part.to_string(),
            Err(_) => {}
        }
    }
    (!version.is_empty()).then_some((version, family))
}

/// The newest Claude model in `models`; on equal versions, Sonnet, then
/// Haiku, then Opus (the order Kusema prefers), then the rest.
pub fn newest_claude(models: &[String]) -> Option<String> {
    let rank = |family: &str| match family {
        "sonnet" => 0,
        "haiku" => 1,
        "opus" => 2,
        _ => 3,
    };
    models
        .iter()
        .filter_map(|m| claude_version(m).map(|(v, f)| (v, rank(&f), m)))
        .max_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)))
        .map(|(_, _, m)| m.clone())
}

/// The provider as a request for `model` must see it: for a Claude model on
/// Bedrock, the Messages API at `/anthropic/v1` (with `x-api-key` and
/// `anthropic-version`, which the Anthropic format sends); otherwise as is.
pub fn for_model(provider: &PostProcessProvider, model: &str) -> PostProcessProvider {
    if provider.id != ID || !uses_messages_api(model) {
        return provider.clone();
    }
    // `https://bedrock-mantle.{region}.api.aws/v1` → `…/anthropic/v1`.
    let root = provider.base_url.trim_end_matches('/');
    let root = root.strip_suffix("/v1").unwrap_or(root);
    PostProcessProvider {
        base_url: format!("{root}/anthropic/v1"),
        api_format: PostProcessApiFormat::Anthropic,
        supports_structured_output: false,
        ..provider.clone()
    }
}

/// When settings load: the name "Amazon Bedrock", and an address that's a
/// supported region (an older install has us-east-1, which stays). The key
/// is untouched. True if anything changed.
pub fn apply(settings: &mut AppSettings) -> bool {
    let Some(provider) = settings.post_process_provider_mut(ID) else {
        return false;
    };
    let mut changed = false;
    if provider.label != LABEL {
        provider.label = LABEL.to_string();
        changed = true;
    }
    let region = region_of(&provider.base_url).unwrap_or_else(|| DEFAULT_REGION.to_string());
    let url = base_url(&region);
    if provider.base_url != url {
        provider.base_url = url;
        changed = true;
    }
    if provider.api_format != PostProcessApiFormat::OpenAi {
        provider.api_format = PostProcessApiFormat::OpenAi;
        changed = true;
    }
    changed
}

/// Bedrock's answer to a failed request, in plain words. `None` when it's
/// nothing Bedrock-specific (the general classifier takes over).
pub fn classify(error: &str, model: &str, with_image: bool) -> Option<Failure> {
    let lower = error.to_ascii_lowercase();
    // Chat errors say "status 401 …", the model list "(401 Unauthorized)".
    let status = |code: &str| {
        lower.contains(&format!("status {code}")) || lower.contains(&format!("({code} "))
    };
    if !(status("400") || status("401") || status("403") || status("404")) {
        return None;
    }
    if with_image
        && status("400")
        && [
            "image",
            "vision",
            "multimodal",
            "content type",
            "image_url",
            "media",
        ]
        .iter()
        .any(|w| lower.contains(w))
    {
        return Some(Failure::ScreenNotForModel);
    }
    if lower.contains("expired") || lower.contains("expiredtoken") {
        return Some(Failure::BedrockKeyExpired);
    }
    if (status("401") || status("403"))
        && (lower.contains("region") || lower.contains("credential should be scoped"))
    {
        return Some(Failure::BedrockWrongRegion);
    }
    let access = [
        "ftuformnotfilled",
        "use case details",
        "mpagreement",
        "marketplace",
        "model access",
        "not authorized to access this model",
        "isn't supported on this route",
        "is not supported on this route",
        "not available",
        "unavailable",
        "don't have access to the model",
        "do not have access to the model",
    ];
    if access.iter().any(|w| lower.contains(w))
        || (status("404") && (lower.contains("model") || lower.contains("not found")))
    {
        return Some(Failure::BedrockModelAccess);
    }
    if status("401") || status("403") {
        return Some(Failure::BedrockKeyInvalid);
    }
    None
}

fn changed(app: &AppHandle) {
    let _ = app.emit(
        "settings-changed",
        serde_json::json!({ "setting": "post_process_providers" }),
    );
}

/// Picks the region (one of `REGIONS` or `GOVCLOUD_REGIONS`).
#[tauri::command]
#[specta::specta]
pub fn set_bedrock_region(app: AppHandle, region: String) -> Result<(), String> {
    if !is_region(&region) {
        return Err(format!(
            "{region} isn't a region with Amazon Bedrock's Mantle endpoint."
        ));
    }
    let mut settings = get_settings(&app);
    let Some(provider) = settings.post_process_provider_mut(ID) else {
        return Err("Amazon Bedrock isn't in the provider list.".into());
    };
    provider.base_url = base_url(&region);
    write_settings(&app, settings);
    changed(&app);
    Ok(())
}

/// What "Test connection" found.
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
pub struct BedrockTest {
    /// Models this region offers (unavailable ones left out).
    pub models: u32,
    /// The chosen model answered, if one is chosen.
    pub model_answered: bool,
    /// A failure, as a translation key with its `{{name}}`, and the English.
    pub problem_key: Option<String>,
    pub problem_name: Option<String>,
    pub problem: Option<String>,
}

fn problem(test: &mut BedrockTest, failure: Failure) {
    test.problem_key = Some(format!("overlay.ai.{}", failure.key()));
    test.problem_name = failure.name();
    test.problem = Some(failure.english());
}

/// "Test connection": lists the region's models, then (if one is chosen)
/// asks it for a one-word reply, so key, region and model access are all
/// checked. Listed in the privacy ledger like any request.
#[tauri::command]
#[specta::specta]
pub async fn test_bedrock_connection(app: AppHandle) -> Result<BedrockTest, String> {
    let settings = get_settings(&app);
    let provider = settings
        .post_process_provider(ID)
        .cloned()
        .ok_or("Amazon Bedrock isn't in the provider list.")?;
    let key = settings
        .post_process_api_keys
        .get(ID)
        .map(|k| k.trim().to_string())
        .unwrap_or_default();
    let model = settings
        .post_process_models
        .get(ID)
        .map(|m| m.trim().to_string())
        .unwrap_or_default();
    let mut test = BedrockTest {
        models: 0,
        model_answered: false,
        problem_key: None,
        problem_name: None,
        problem: None,
    };
    if key.is_empty() {
        problem(&mut test, Failure::NoKey(provider.label.clone()));
        return Ok(test);
    }
    match crate::llm_client::fetch_models(&provider, key.clone()).await {
        Ok(models) => test.models = models.len() as u32,
        Err(e) => {
            log::warn!("Amazon Bedrock test: listing models failed: {e}");
            problem(
                &mut test,
                classify(&e, &model, false)
                    .unwrap_or_else(|| crate::assistant::failure::classify(&provider, &e, false)),
            );
            return Ok(test);
        }
    }
    if model.is_empty() {
        return Ok(test);
    }
    let turns = [crate::llm_client::ChatTurn::new(
        crate::llm_client::ChatRole::User,
        "Reply with the single word OK.",
    )];
    match crate::llm_client::send_conversation(&provider, key, &model, "", &turns, |_| {}).await {
        Ok(_) => test.model_answered = true,
        Err(e) => {
            log::warn!("Amazon Bedrock test: {model} failed: {e}");
            problem(
                &mut test,
                classify(&e, &model, false)
                    .unwrap_or_else(|| crate::assistant::failure::classify(&provider, &e, false)),
            );
        }
    }
    Ok(test)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::get_default_settings;

    fn provider() -> PostProcessProvider {
        get_default_settings()
            .post_process_provider(ID)
            .cloned()
            .unwrap()
    }

    #[test]
    fn named_amazon_bedrock_in_us_east_1_by_default() {
        let p = provider();
        assert_eq!(p.label, "Amazon Bedrock");
        assert_eq!(p.base_url, "https://bedrock-mantle.us-east-1.api.aws/v1");
        assert_eq!(region_of(&p.base_url).as_deref(), Some("us-east-1"));
    }

    #[test]
    fn an_older_install_is_renamed_and_keeps_its_key_and_model() {
        let mut settings = get_default_settings();
        {
            let p = settings.post_process_provider_mut(ID).unwrap();
            p.label = "AWS Bedrock (Mantle)".into();
        }
        settings
            .post_process_api_keys
            .insert(ID.into(), "bedrock-key".into());
        settings
            .post_process_models
            .insert(ID.into(), "openai.gpt-oss-120b".into());
        assert!(apply(&mut settings));
        let p = settings.post_process_provider(ID).unwrap();
        assert_eq!(p.label, "Amazon Bedrock");
        assert_eq!(p.base_url, base_url("us-east-1"));
        assert_eq!(settings.post_process_api_keys[ID], "bedrock-key");
        assert_eq!(settings.post_process_models[ID], "openai.gpt-oss-120b");
        assert!(!apply(&mut settings), "running again changes nothing");
    }

    #[test]
    fn a_chosen_region_is_kept() {
        let mut settings = get_default_settings();
        settings.post_process_provider_mut(ID).unwrap().base_url = base_url("eu-central-1");
        apply(&mut settings);
        assert_eq!(
            region_of(&settings.post_process_provider(ID).unwrap().base_url).as_deref(),
            Some("eu-central-1")
        );
    }

    #[test]
    fn regions_are_mantles_and_govcloud_is_separate() {
        assert_eq!(REGIONS.len(), 13);
        assert!(REGIONS.contains(&"us-east-1"));
        assert!(!REGIONS.contains(&"us-west-1"), "no Mantle there");
        assert!(GOVCLOUD_REGIONS.iter().all(|r| r.starts_with("us-gov-")));
        assert!(is_region("us-gov-west-1"));
        assert!(!is_region("mars-north-1"));
        assert_eq!(region_of("https://evil.example/v1"), None);
        assert_eq!(
            region_of("https://bedrock-mantle.xx-fake-1.api.aws/v1"),
            None
        );
    }

    #[test]
    fn claude_uses_the_messages_api_and_other_models_chat_completions() {
        let p = provider();
        let claude = for_model(&p, "anthropic.claude-sonnet-5-5");
        assert_eq!(claude.api_format, PostProcessApiFormat::Anthropic);
        assert_eq!(
            claude.base_url,
            "https://bedrock-mantle.us-east-1.api.aws/anthropic/v1"
        );
        assert!(!claude.supports_structured_output);
        let other = for_model(&p, "openai.gpt-oss-120b");
        assert_eq!(other.api_format, PostProcessApiFormat::OpenAi);
        assert_eq!(other.base_url, p.base_url);
        // Only Bedrock is rerouted.
        let anthropic = get_default_settings()
            .post_process_provider("anthropic")
            .cloned()
            .unwrap();
        assert_eq!(
            for_model(&anthropic, "anthropic.claude-sonnet-5-5").base_url,
            anthropic.base_url
        );
    }

    /// A local stand-in for bedrock-mantle: answers each request with
    /// `response`, and passes back what it was sent.
    fn serve(responses: Vec<String>) -> (String, std::sync::mpsc::Receiver<String>) {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for response in responses {
                if let Ok((mut stream, _)) = listener.accept() {
                    let mut buffer = vec![0u8; 65536];
                    let n = stream.read(&mut buffer).unwrap_or(0);
                    let _ = tx.send(String::from_utf8_lossy(&buffer[..n]).to_string());
                    let _ = stream.write_all(response.as_bytes());
                }
            }
        });
        (format!("http://{address}/v1"), rx)
    }

    fn sse(events: &[&str]) -> String {
        let body: String = events.iter().map(|e| format!("data: {e}\n\n")).collect();
        format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    }

    fn at(url: String) -> PostProcessProvider {
        PostProcessProvider {
            base_url: url,
            ..provider()
        }
    }

    fn ask(p: &PostProcessProvider, model: &str) -> (String, Vec<String>) {
        let mut pieces = Vec::new();
        let answer = tauri::async_runtime::block_on(crate::llm_client::send_conversation(
            p,
            "test-key".into(),
            model,
            "sys",
            &[crate::llm_client::ChatTurn::new(
                crate::llm_client::ChatRole::User,
                "q",
            )],
            |t| pieces.push(t.to_string()),
        ))
        .unwrap()
        .unwrap();
        (answer, pieces)
    }

    #[test]
    fn claude_is_asked_through_the_messages_route_with_its_headers() {
        let (url, sent) = serve(vec![sse(&[
            r#"{"type":"message_start","message":{}}"#,
            r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Thirty "}}"#,
            r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"days."}}"#,
            r#"{"type":"message_stop"}"#,
        ])]);
        let (answer, pieces) = ask(&at(url), "anthropic.claude-sonnet-5-5");
        assert_eq!(answer, "Thirty days.");
        assert_eq!(pieces.len(), 2, "streamed");
        let request = sent.recv().unwrap();
        let lower = request.to_ascii_lowercase();
        assert!(
            request.starts_with("POST /anthropic/v1/messages "),
            "{request}"
        );
        assert!(lower.contains("x-api-key: test-key"));
        assert!(lower.contains("anthropic-version: 2023-06-01"));
        assert!(!lower.contains("authorization: bearer"));
        assert!(request.contains("\"model\":\"anthropic.claude-sonnet-5-5\""));
    }

    #[test]
    fn other_models_use_chat_completions_with_a_bearer_key() {
        let (url, sent) = serve(vec![sse(&[
            r#"{"choices":[{"delta":{"content":"Hello"}}]}"#,
            r#"{"choices":[{"delta":{"content":" there"},"finish_reason":"stop"}]}"#,
            "[DONE]",
        ])]);
        let (answer, pieces) = ask(&at(url), "openai.gpt-oss-120b");
        assert_eq!(answer, "Hello there");
        assert_eq!(pieces.len(), 2, "streamed");
        let request = sent.recv().unwrap();
        assert!(
            request.starts_with("POST /v1/chat/completions "),
            "{request}"
        );
        assert!(request
            .to_ascii_lowercase()
            .contains("authorization: bearer test-key"));
    }

    #[test]
    fn unavailable_models_are_left_out_of_the_list() {
        let body = r#"{"object":"list","data":[
            {"id":"anthropic.claude-sonnet-5-5","object":"model","status":"available"},
            {"id":"anthropic.claude-fable-5","object":"model","status":"unavailable","status_reason":"data retention"},
            {"id":"openai.gpt-oss-120b","object":"model"}]}"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let (url, sent) = serve(vec![response]);
        let models = tauri::async_runtime::block_on(crate::llm_client::fetch_models(
            &at(url),
            "test-key".into(),
        ))
        .unwrap();
        assert_eq!(
            models,
            ["anthropic.claude-sonnet-5-5", "openai.gpt-oss-120b"]
        );
        assert!(sent.recv().unwrap().starts_with("GET /v1/models "));
        assert_eq!(
            crate::cloud_providers::suggested_model(ID, &models).as_deref(),
            Some("anthropic.claude-sonnet-5-5")
        );
        assert_eq!(
            crate::cloud_providers::suggested_model(ID, &["openai.gpt-oss-120b".into()]),
            None
        );
    }

    #[test]
    fn without_the_preferred_claude_the_newest_listed_is_suggested() {
        let list = |ids: &[&str]| ids.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let suggest = |ids: &[&str]| crate::cloud_providers::suggested_model(ID, &list(ids));
        // What us-east-2 listed in the live test.
        assert_eq!(
            suggest(&[
                "openai.gpt-5.6-terra",
                "anthropic.claude-haiku-4-5",
                "deepseek.v3.2"
            ])
            .as_deref(),
            Some("anthropic.claude-haiku-4-5")
        );
        assert_eq!(
            suggest(&[
                "anthropic.claude-haiku-4-5",
                "anthropic.claude-opus-4-8",
                "anthropic.claude-opus-4-7"
            ])
            .as_deref(),
            Some("anthropic.claude-opus-4-8")
        );
        // 5 is newer than 4.8; on a tie Sonnet comes before Opus.
        assert_eq!(
            suggest(&[
                "anthropic.claude-opus-4-8",
                "anthropic.claude-opus-5",
                "anthropic.claude-sonnet-5"
            ])
            .as_deref(),
            Some("anthropic.claude-sonnet-5")
        );
        // The preferred ones still win when listed.
        assert_eq!(
            suggest(&["anthropic.claude-fable-5-1", "anthropic.claude-haiku-5-5"]).as_deref(),
            Some("anthropic.claude-haiku-5-5")
        );
        assert_eq!(suggest(&["qwen.qwen3-32b"]), None);
    }

    #[test]
    fn a_rejected_screenshot_says_screen_questions_are_unavailable() {
        let p = provider();
        let failure = crate::assistant::failure::classify(
            &p,
            "API request failed with status 400 Bad Request: {\"message\":\"This model does not support image input\"}",
            true,
        );
        assert_eq!(failure, Failure::ScreenNotForModel);
        assert!(failure
            .english()
            .starts_with("Screen questions aren't available with this model"));
    }

    #[test]
    fn privileged_mode_still_blocks_bedrock() {
        let p = provider();
        let blocked = crate::network::Blocked {
            host: "bedrock-mantle.us-east-1.api.aws".to_string(),
            verdict: crate::network::BlockReason::CloudAi,
        }
        .to_string();
        assert_eq!(
            crate::assistant::failure::classify(&p, &blocked, false),
            Failure::Privileged("Amazon Bedrock".into())
        );
    }

    /// Server text with anything key-like taken out, for the live test's report.
    fn redacted(text: &str) -> String {
        let long = regex::Regex::new(r"[A-Za-z0-9+/=_\-]{24,}").unwrap();
        let text = long.replace_all(text, "[redacted]");
        text.chars().take(400).collect()
    }

    /// A small red PNG, for the image question.
    fn red_png() -> Vec<u8> {
        let img = image::RgbImage::from_pixel(64, 64, image::Rgb([220, 30, 30]));
        let mut out = std::io::Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }

    /// Live, only when `AWS_BEARER_TOKEN_BEDROCK` holds a Bedrock API key
    /// for us-east-1: the model list, a Claude answer (Messages), another
    /// model (Chat Completions), streaming, an image question, and the real
    /// errors for a bad key, the wrong region and a model Mantle doesn't
    /// serve. Prints statuses and redacted server text; never the key.
    /// `cargo test --lib live_bedrock -- --nocapture`
    #[test]
    fn live_bedrock_when_a_key_is_set() {
        let Ok(key) = std::env::var("AWS_BEARER_TOKEN_BEDROCK") else {
            eprintln!("AWS_BEARER_TOKEN_BEDROCK not set: live Bedrock test skipped");
            return;
        };
        let key = key.trim().to_string();
        // The key's own region (BEDROCK_LIVE_REGION, us-east-1 by default);
        // the wrong-region probe uses another one.
        let region = std::env::var("BEDROCK_LIVE_REGION").unwrap_or_else(|_| DEFAULT_REGION.into());
        let other_region = if region == "us-east-1" {
            "eu-west-1"
        } else {
            "us-east-1"
        };
        eprintln!("REGION: {region} (wrong-region probe: {other_region})");
        let p = PostProcessProvider {
            base_url: base_url(&region),
            ..provider()
        };
        let run = |f: std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<Option<String>, String>> + '_>,
        >| { tauri::async_runtime::block_on(f) };
        let turn =
            |text: &str| crate::llm_client::ChatTurn::new(crate::llm_client::ChatRole::User, text);

        // 1. Models.
        let models =
            tauri::async_runtime::block_on(crate::llm_client::fetch_models(&p, key.clone()));
        let models = match models {
            Ok(models) => {
                let claude: Vec<&String> = models.iter().filter(|m| uses_messages_api(m)).collect();
                eprintln!(
                    "MODELS: {} listed, {} Claude: {:?}",
                    models.len(),
                    claude.len(),
                    claude
                );
                eprintln!(
                    "OTHERS (first 10): {:?}",
                    models
                        .iter()
                        .filter(|m| !uses_messages_api(m))
                        .take(10)
                        .collect::<Vec<_>>()
                );
                eprintln!(
                    "RECOMMENDED: {:?}",
                    crate::cloud_providers::suggested_model(ID, &models)
                );
                models
            }
            Err(e) => {
                eprintln!(
                    "MODELS FAILED: {} -> {:?}",
                    redacted(&e),
                    classify(&e, "", false)
                );
                return;
            }
        };

        // 2. Claude, streamed.
        let claude = crate::cloud_providers::suggested_model(ID, &models)
            .or_else(|| models.iter().find(|m| uses_messages_api(m)).cloned());
        if let Some(model) = &claude {
            let mut pieces = 0;
            let answer = run(Box::pin(crate::llm_client::send_conversation(
                &p,
                key.clone(),
                model,
                "Answer in one short sentence.",
                &[turn("What is the capital of Ghana?")],
                |_| pieces += 1,
            )));
            match answer {
                Ok(a) => eprintln!(
                    "CLAUDE {model}: {:?} ({pieces} streamed pieces)",
                    a.map(|a| redacted(&a))
                ),
                Err(e) => eprintln!(
                    "CLAUDE {model} FAILED: {} -> {:?}",
                    redacted(&e),
                    classify(&e, model, false)
                ),
            }
            // 4. Image question to Claude.
            let image = crate::llm_client::ChatImage {
                media_type: "image/png",
                data: std::sync::Arc::from(red_png()),
                approved_for: ID.to_string(),
            };
            let mut t = turn("What colour is this image? One word.");
            t.image = Some(image.clone());
            let answer = run(Box::pin(crate::llm_client::send_conversation(
                &p,
                key.clone(),
                model,
                "",
                &[t],
                |_| {},
            )));
            match answer {
                Ok(a) => eprintln!("IMAGE {model}: {:?}", a.map(|a| redacted(&a))),
                Err(e) => eprintln!(
                    "IMAGE {model} FAILED: {} -> {:?}",
                    redacted(&e),
                    classify(&e, model, true)
                ),
            }
        } else {
            eprintln!("CLAUDE: no anthropic.* model listed in us-east-1");
        }

        // 3. Another model, Chat Completions, streamed.
        let other = models
            .iter()
            .find(|m| m.contains("gpt-oss") || m.contains("llama") || m.contains("mistral"))
            .or_else(|| models.iter().find(|m| !uses_messages_api(m)))
            .cloned();
        if let Some(model) = &other {
            let mut pieces = 0;
            let answer = run(Box::pin(crate::llm_client::send_conversation(
                &p,
                key.clone(),
                model,
                "Be brief.",
                &[turn(
                    "Name the capitals of six West African countries, one per line.",
                )],
                |_| pieces += 1,
            )));
            match answer {
                Ok(a) => eprintln!(
                    "OTHER {model}: {:?} ({pieces} streamed pieces)",
                    a.map(|a| redacted(&a))
                ),
                Err(e) => eprintln!(
                    "OTHER {model} FAILED: {} -> {:?}",
                    redacted(&e),
                    classify(&e, model, false)
                ),
            }
            let mut t = turn("What colour is this image? One word.");
            t.image = Some(crate::llm_client::ChatImage {
                media_type: "image/png",
                data: std::sync::Arc::from(red_png()),
                approved_for: ID.to_string(),
            });
            let answer = run(Box::pin(crate::llm_client::send_conversation(
                &p,
                key.clone(),
                model,
                "",
                &[t],
                |_| {},
            )));
            match answer {
                Ok(a) => eprintln!("IMAGE {model}: {:?}", a.map(|a| redacted(&a))),
                Err(e) => eprintln!(
                    "IMAGE {model} FAILED: {} -> {:?}",
                    redacted(&e),
                    crate::assistant::failure::classify(&p, &e, true)
                ),
            }
        }

        // 5. Errors, as Bedrock really sends them.
        let model = claude
            .clone()
            .unwrap_or_else(|| "anthropic.claude-sonnet-5-5".into());
        let bad_key = run(Box::pin(crate::llm_client::send_conversation(
            &p,
            "not-a-real-bedrock-key".into(),
            &model,
            "",
            &[turn("hi")],
            |_| {},
        )));
        if let Err(e) = bad_key {
            eprintln!(
                "BAD KEY: {} -> {:?}",
                redacted(&e),
                classify(&e, &model, false)
            );
        }
        let bad_list = tauri::async_runtime::block_on(crate::llm_client::fetch_models(
            &p,
            "not-a-real-bedrock-key".into(),
        ));
        if let Err(e) = bad_list {
            eprintln!(
                "BAD KEY (models): {} -> {:?}",
                redacted(&e),
                classify(&e, "", false)
            );
        }
        let elsewhere = PostProcessProvider {
            base_url: base_url(other_region),
            ..provider()
        };
        let wrong_region = run(Box::pin(crate::llm_client::send_conversation(
            &elsewhere,
            key.clone(),
            &model,
            "",
            &[turn("hi")],
            |_| {},
        )));
        match wrong_region {
            Ok(_) => eprintln!("WRONG REGION ({other_region}): answered (the key works there)"),
            Err(e) => eprintln!(
                "WRONG REGION ({other_region}): {} -> {:?}",
                redacted(&e),
                classify(&e, &model, false)
            ),
        }
        let wrong_list = tauri::async_runtime::block_on(crate::llm_client::fetch_models(
            &elsewhere,
            key.clone(),
        ));
        match wrong_list {
            Ok(_) => eprintln!("WRONG REGION (models, {other_region}): listed"),
            Err(e) => eprintln!(
                "WRONG REGION (models, {other_region}): {} -> {:?}",
                redacted(&e),
                classify(&e, "", false)
            ),
        }
        let not_served = run(Box::pin(crate::llm_client::send_conversation(
            &p,
            key.clone(),
            "anthropic.claude-fable-5-2",
            "",
            &[turn("hi")],
            |_| {},
        )));
        match not_served {
            Ok(_) => eprintln!("MODEL NOT ON MANTLE: answered"),
            Err(e) => eprintln!(
                "MODEL NOT ON MANTLE (anthropic.claude-fable-5-2): {} -> {:?}",
                redacted(&e),
                classify(&e, "anthropic.claude-fable-5-2", false)
            ),
        }
        // The key must never appear in anything printed above: it isn't
        // passed to any print, and server text goes through `redacted`.
        assert!(!key.is_empty());
    }

    /// Bodies Bedrock really sent in the live test (2026-11-26, us-east-2),
    /// with request ids left out.
    #[test]
    fn real_bedrock_errors_are_mapped() {
        let cases = [
            (
                "Model list request failed (401 Unauthorized): {\"error\":{\"code\":\"invalid_api_key\",\"message\":\"Credential should be scoped to a valid region.\",\"param\":null,\"type\":\"permission_denied_error\"}}",
                "",
                false,
                Some(Failure::BedrockWrongRegion),
            ),
            (
                "API request failed with status 401 Unauthorized: {\"type\":\"error\",\"error\":{\"type\":\"authentication_error\",\"message\":\"Credential should be scoped to a valid region.\"}}",
                "anthropic.claude-haiku-4-5",
                false,
                Some(Failure::BedrockWrongRegion),
            ),
            (
                "Model list request failed (401 Unauthorized): {\"error\":{\"code\":\"invalid_api_key\",\"message\":\"Invalid bearer token\",\"param\":null,\"type\":\"permission_denied_error\"}}",
                "",
                false,
                Some(Failure::BedrockKeyInvalid),
            ),
            (
                "API request failed with status 401 Unauthorized: {\"type\":\"error\",\"error\":{\"type\":\"authentication_error\",\"message\":\"Invalid bearer token\"}}",
                "anthropic.claude-haiku-4-5",
                false,
                Some(Failure::BedrockKeyInvalid),
            ),
            (
                "API request failed with status 403 Forbidden: {\"type\":\"error\",\"error\":{\"type\":\"permission_error\",\"message\":\"anthropic.claude-haiku-4-5 is not available for this account. You can explore other available models on Amazon Bedrock.\"}}",
                "anthropic.claude-haiku-4-5",
                true,
                Some(Failure::BedrockModelAccess),
            ),
            (
                "API request failed with status 404 Not Found: {\"type\":\"error\",\"error\":{\"type\":\"not_found_error\",\"message\":\"The model 'anthropic.claude-fable-5-2' does not exist\"}}",
                "anthropic.claude-fable-5-2",
                false,
                Some(Failure::BedrockModelAccess),
            ),
            (
                "API request failed with status 400 Bad Request: {\"error\":{\"code\":\"validation_error\",\"message\":\"Model does not support image modality\",\"param\":null,\"type\":\"invalid_request_error\"}}",
                "mistral.devstral-2-123b",
                true,
                Some(Failure::ScreenNotForModel),
            ),
        ];
        for (error, model, image, want) in cases {
            assert_eq!(classify(error, model, image), want, "{error}");
        }
    }

    #[test]
    fn bedrock_errors_are_said_plainly() {
        let m = "anthropic.claude-sonnet-5-5";
        assert_eq!(
            classify(
                "API request failed with status 403 Forbidden: {\"message\":\"Bearer Token has expired\"}",
                m,
                false
            ),
            Some(Failure::BedrockKeyExpired)
        );
        assert_eq!(
            classify(
                "API request failed with status 403 Forbidden: {\"message\":\"Credential should be scoped to a valid region.\"}",
                m,
                false
            ),
            Some(Failure::BedrockWrongRegion)
        );
        assert_eq!(
            classify(
                "API request failed with status 404 Not Found: {\"message\":\"Model use case details have not been submitted for this account. FTUFormNotFilled\"}",
                m,
                false
            ),
            Some(Failure::BedrockModelAccess)
        );
        assert_eq!(
            classify(
                "API request failed with status 400 Bad Request: model `x` isn't supported on this route",
                m,
                false
            ),
            Some(Failure::BedrockModelAccess)
        );
        assert_eq!(
            classify(
                "API request failed with status 401 Unauthorized: {\"message\":\"Invalid API key\"}",
                m,
                false
            ),
            Some(Failure::BedrockKeyInvalid)
        );
        assert_eq!(
            classify(
                "API request failed with status 400 Bad Request: image content is not supported",
                m,
                true
            ),
            Some(Failure::ScreenNotForModel)
        );
        assert_eq!(classify("status 429 Too Many Requests", m, false), None);
        assert_eq!(
            classify("error sending request (kind: connect)", m, false),
            None
        );
    }
}
