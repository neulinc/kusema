use crate::settings::{PostProcessApiFormat, PostProcessProvider};
use futures_util::StreamExt;
use log::{debug, error, info};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE, REFERER, USER_AGENT};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use specta::Type;
use std::collections::HashSet;
use std::error::Error as StdError;
use std::sync::{Arc, Mutex, OnceLock};

#[derive(Debug, Serialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Debug, Serialize)]
struct JsonSchema {
    name: String,
    strict: bool,
    schema: Value,
}

#[derive(Debug, Serialize)]
struct ResponseFormat {
    #[serde(rename = "type")]
    format_type: String,
    json_schema: JsonSchema,
}

#[derive(Debug, Serialize, Clone, Default, PartialEq)]
struct ReasoningConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    effort: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    exclude: Option<bool>,
}

/// Request fields used to ask an endpoint to skip reasoning/thinking.
/// Providers disagree on the field name and accepted values, so at most one of
/// these is set per request (see `reasoning_disable_params`).
#[derive(Debug, Serialize, Clone, Default, PartialEq)]
struct ReasoningParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning_effort: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning: Option<ReasoningConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    thinking: Option<Value>,
}

impl ReasoningParams {
    fn is_empty(&self) -> bool {
        self.reasoning_effort.is_none() && self.reasoning.is_none() && self.thinking.is_none()
    }
}

/// Pick the reasoning-disable request fields an endpoint understands.
/// Unknown endpoints get the common OpenAI-style field; if they reject it,
/// the request is retried without it (see `send_chat_completion_with_schema`).
fn reasoning_disable_params(provider: &PostProcessProvider) -> ReasoningParams {
    let base_url = provider.base_url.to_lowercase();
    if base_url.contains("api.deepseek.com") {
        // DeepSeek rejects reasoning_effort "none" and uses its own field:
        // https://api-docs.deepseek.com/guides/thinking_mode
        ReasoningParams {
            thinking: Some(serde_json::json!({ "type": "disabled" })),
            ..Default::default()
        }
    } else if provider.id == "openrouter" {
        // OpenRouter nested object; exclude:true also keeps reasoning text out
        // of the response so it can't pollute structured-output JSON parsing
        ReasoningParams {
            reasoning: Some(ReasoningConfig {
                effort: Some("none".to_string()),
                exclude: Some(true),
            }),
            ..Default::default()
        }
    } else {
        ReasoningParams {
            reasoning_effort: Some("none".to_string()),
            ..Default::default()
        }
    }
}

/// Endpoints (base_url|model) that rejected the reasoning-disable fields with a
/// 4xx. Remembered for the lifetime of the process so every dictation after the
/// first skips the doomed attempt and goes straight to a plain request.
fn reasoning_rejections() -> &'static Mutex<HashSet<String>> {
    static REJECTED: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    REJECTED.get_or_init(|| Mutex::new(HashSet::new()))
}

fn endpoint_key(provider: &PostProcessProvider, model: &str) -> String {
    format!("{}|{}", provider.base_url.trim_end_matches('/'), model)
}

fn is_known_rejected(key: &str) -> bool {
    reasoning_rejections()
        .lock()
        .map(|set| set.contains(key))
        .unwrap_or(false)
}

fn remember_rejection(key: String) {
    if let Ok(mut set) = reasoning_rejections().lock() {
        set.insert(key);
    }
}

#[derive(Debug, Serialize)]
struct ChatCompletionRequest<M = ChatMessage> {
    model: String,
    messages: Vec<M>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_format: Option<ResponseFormat>,
    #[serde(flatten)]
    reasoning: ReasoningParams,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatMessageResponse,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ChatMessageResponse {
    content: Option<String>,
}

/// Output budget for Anthropic Messages requests, where `max_tokens` is
/// required. Ample for a cleaned-up dictation.
const ANTHROPIC_MAX_TOKENS: u32 = 4096;

#[derive(Debug, Serialize)]
struct AnthropicMessagesRequest<M = ChatMessage> {
    model: String,
    max_tokens: u32,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    messages: Vec<M>,
}

#[derive(Debug, Deserialize)]
struct AnthropicMessagesResponse {
    content: Vec<AnthropicContentBlock>,
    stop_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum AnthropicContentBlock {
    Text {
        text: String,
    },
    /// Thinking and any other non-text blocks
    #[serde(other)]
    Other,
}

impl AnthropicMessagesResponse {
    fn into_text(self) -> Option<String> {
        let text: String = self
            .content
            .into_iter()
            .filter_map(|block| match block {
                AnthropicContentBlock::Text { text } => Some(text),
                AnthropicContentBlock::Other => None,
            })
            .collect();
        (!text.is_empty()).then_some(text)
    }
}

/// Build headers for API requests based on provider type
fn build_headers(provider: &PostProcessProvider, api_key: &str) -> Result<HeaderMap, String> {
    let mut headers = HeaderMap::new();

    // Common headers
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert(
        REFERER,
        HeaderValue::from_static("https://github.com/neulinc/kusema"),
    );
    headers.insert(
        USER_AGENT,
        HeaderValue::from_static("Kusema/1.0 (+https://github.com/neulinc/kusema)"),
    );
    headers.insert("X-Title", HeaderValue::from_static("Kusema"));

    // Provider-specific auth headers
    if !api_key.is_empty() {
        if provider.api_format == PostProcessApiFormat::Anthropic {
            headers.insert(
                "x-api-key",
                HeaderValue::from_str(api_key)
                    .map_err(|e| format!("Invalid API key header value: {}", e))?,
            );
            headers.insert("anthropic-version", HeaderValue::from_static("2023-06-01"));
        } else {
            headers.insert(
                AUTHORIZATION,
                HeaderValue::from_str(&format!("Bearer {}", api_key))
                    .map_err(|e| format!("Invalid authorization header value: {}", e))?,
            );
        }
    }

    Ok(headers)
}

/// Create an HTTP client with provider-specific headers
fn create_client(provider: &PostProcessProvider, api_key: &str) -> Result<reqwest::Client, String> {
    let headers = build_headers(provider, api_key)?;
    reqwest::Client::builder()
        .default_headers(headers)
        .build()
        .map_err(|e| report_reqwest_error("Failed to build HTTP client", &e))
}

/// Format a bounded error source chain.
///
/// `reqwest::Error`'s Display implementation intentionally gives only a short
/// summary. Nested causes contain the useful transport details, such as a
/// certificate validation failure, an HTTP/2 error, or a connection reset.
/// Callers must skip source types whose Display text can quote payload data.
fn error_source_chain(error: &(dyn StdError + 'static)) -> Vec<String> {
    let mut causes = Vec::new();
    let mut source = error.source();

    // Defensive cap in case a third-party error exposes a cyclic source chain.
    for _ in 0..16 {
        let Some(cause) = source else {
            break;
        };
        causes.push(cause.to_string());
        source = cause.source();
    }

    causes
}

fn reqwest_error_kinds(error: &reqwest::Error) -> String {
    let mut kinds = Vec::new();

    if error.is_builder() {
        kinds.push("builder");
    }
    if error.is_connect() {
        kinds.push("connect");
    }
    if error.is_request() {
        kinds.push("request");
    }
    if error.is_redirect() {
        kinds.push("redirect");
    }
    if error.is_timeout() {
        kinds.push("timeout");
    }
    if error.is_status() {
        kinds.push("status");
    }
    if error.is_body() {
        kinds.push("body");
    }
    if error.is_decode() {
        kinds.push("decode");
    }
    if error.is_upgrade() {
        kinds.push("upgrade");
    }

    if kinds.is_empty() {
        "unknown".to_string()
    } else {
        kinds.join(", ")
    }
}

fn sanitized_url(url: &reqwest::Url) -> String {
    let mut url = url.clone();

    // Custom endpoints should not contain credentials or query-string tokens,
    // but omit them from diagnostics in case one does.
    let _ = url.set_username("");
    let _ = url.set_password(None);
    url.set_query(None);
    url.set_fragment(None);

    url.to_string()
}

fn sanitized_url_for_log(url: &str) -> String {
    reqwest::Url::parse(url)
        .map(|url| sanitized_url(&url))
        // Do not echo an invalid URL: the parse failure might have been caused
        // by sensitive data entered in the custom endpoint field.
        .unwrap_or_else(|_| "<invalid URL>".to_string())
}

fn report_reqwest_error(context: &str, error: &reqwest::Error) -> String {
    let kinds = reqwest_error_kinds(error);
    let url = error
        .url()
        .map(sanitized_url)
        .map(|url| format!(", url: {url}"))
        .unwrap_or_default();

    // serde_json's error text can quote values from a malformed response. That
    // response may contain transcription content, so retain the useful decode
    // classification but never put its nested source in logs or UI errors.
    let causes = if error.is_decode() {
        Vec::new()
    } else {
        error_source_chain(error)
    };
    let cause_details = if !causes.is_empty() {
        format!(": caused by: {}", causes.join(" -> "))
    } else if error.url().is_none() {
        // Reqwest's short Display text is safe when it cannot append a raw URL.
        format!(": {error}")
    } else {
        // The sanitized URL is already included above. Avoid formatting the
        // original error because its Display implementation includes the raw URL.
        String::new()
    };

    let details = format!("{context} (kind: {kinds}{url}){cause_details}");
    error!("{details}");
    details
}

/// Send a chat completion request to an OpenAI-compatible API
/// Returns Ok(Some(content)) on success, Ok(None) if response has no content,
/// or Err on actual errors (HTTP, parsing, etc.)
pub async fn send_chat_completion(
    provider: &PostProcessProvider,
    api_key: String,
    model: &str,
    prompt: String,
    disable_reasoning: bool,
) -> Result<Option<String>, String> {
    if provider.api_format == PostProcessApiFormat::Anthropic {
        return send_anthropic_message(provider, api_key, model, prompt).await;
    }

    send_chat_completion_with_schema(
        provider,
        api_key,
        model,
        prompt,
        None,
        None,
        disable_reasoning,
    )
    .await
}

/// Send a chat completion request with structured output support.
/// When json_schema is provided, uses structured outputs mode.
/// system_prompt is used as the system message when provided.
///
/// When disable_reasoning is set, the request carries the reasoning-disable
/// fields the endpoint is expected to understand. Not every OpenAI-compatible
/// endpoint accepts them (DeepSeek, Gemini's compat layer, and some OpenRouter
/// upstreams reject with 400), so a 400/422 answer to such a request triggers
/// one retry without the fields, and the rejection is remembered per
/// (base_url, model) so later requests skip the failing attempt entirely.
pub async fn send_chat_completion_with_schema(
    provider: &PostProcessProvider,
    api_key: String,
    model: &str,
    user_content: String,
    system_prompt: Option<String>,
    json_schema: Option<Value>,
    disable_reasoning: bool,
) -> Result<Option<String>, String> {
    let base_url = provider.base_url.trim_end_matches('/');
    let url = format!("{}/chat/completions", base_url);

    debug!(
        "Sending chat completion request to: {}",
        sanitized_url_for_log(&url)
    );

    let client = create_client(provider, &api_key)?;

    // Build messages vector
    let mut messages = Vec::new();

    // Add system prompt if provided
    if let Some(system) = system_prompt {
        messages.push(ChatMessage {
            role: "system".to_string(),
            content: system,
        });
    }

    // Add user message
    messages.push(ChatMessage {
        role: "user".to_string(),
        content: user_content,
    });

    // Build response_format if schema is provided
    let response_format = json_schema.map(|schema| ResponseFormat {
        format_type: "json_schema".to_string(),
        json_schema: JsonSchema {
            name: "transcription_output".to_string(),
            strict: true,
            schema,
        },
    });

    let key = endpoint_key(provider, model);
    let reasoning = if disable_reasoning && !is_known_rejected(&key) {
        reasoning_disable_params(provider)
    } else {
        ReasoningParams::default()
    };

    let mut request_body = ChatCompletionRequest {
        model: model.to_string(),
        messages,
        stream: false,
        response_format,
        reasoning,
    };

    let mut response = client
        .post(&url)
        .json(&request_body)
        .send()
        .await
        .map_err(|e| report_reqwest_error("HTTP request failed", &e))?;
    let mut status = response.status();
    debug!(
        "Chat completion response received with status {} over {:?} from {}",
        status,
        response.version(),
        sanitized_url(response.url())
    );

    // A 400/422 on a request carrying reasoning-disable fields is almost always
    // the endpoint rejecting those fields — retry once without them.
    if !status.is_success()
        && matches!(status.as_u16(), 400 | 422)
        && !request_body.reasoning.is_empty()
    {
        let error_text = response.text().await.unwrap_or_else(|e| {
            report_reqwest_error("Failed to read reasoning rejection response", &e)
        });
        info!(
            "Endpoint rejected request with reasoning disabled (status {}): {}. Retrying without reasoning fields",
            status, error_text
        );

        request_body.reasoning = ReasoningParams::default();
        response = client
            .post(&url)
            .json(&request_body)
            .send()
            .await
            .map_err(|e| report_reqwest_error("HTTP retry failed", &e))?;
        status = response.status();
        debug!(
            "Chat completion retry response received with status {} over {:?} from {}",
            status,
            response.version(),
            sanitized_url(response.url())
        );

        if status.is_success() {
            info!(
                "Retry without reasoning fields succeeded; '{}' (model '{}') will skip them from now on",
                sanitized_url_for_log(base_url), model
            );
            remember_rejection(key);
        }
    }

    if !status.is_success() {
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|e| report_reqwest_error("Failed to read API error response", &e));
        return Err(format!(
            "API request failed with status {}: {}",
            status, error_text
        ));
    }

    let completion: ChatCompletionResponse = response
        .json()
        .await
        .map_err(|e| report_reqwest_error("Failed to parse API response", &e))?;

    let Some(choice) = completion.choices.into_iter().next() else {
        return Ok(None);
    };

    // A response cut off at the provider's output limit is incomplete; pasting
    // it would silently drop the end of the dictation (or, for structured
    // output, paste unparseable JSON).
    if choice.finish_reason.as_deref() == Some("length") {
        return Err("Response was truncated at the provider's output token limit".to_string());
    }

    Ok(choice.message.content)
}

/// Send a single-prompt request to an Anthropic Messages API (`/messages`).
/// Used by every provider with the Anthropic format (the built-in Anthropic
/// provider and a custom provider set to it). None of them use structured
/// output, and thinking is off unless requested, so no reasoning fields are
/// needed.
async fn send_anthropic_message(
    provider: &PostProcessProvider,
    api_key: String,
    model: &str,
    prompt: String,
) -> Result<Option<String>, String> {
    let url = format!("{}/messages", provider.base_url.trim_end_matches('/'));

    debug!(
        "Sending Anthropic Messages request to: {}",
        sanitized_url_for_log(&url)
    );

    let request_body = AnthropicMessagesRequest {
        model: model.to_string(),
        max_tokens: ANTHROPIC_MAX_TOKENS,
        stream: false,
        system: None,
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: prompt,
        }],
    };

    let response = create_client(provider, &api_key)?
        .post(&url)
        .json(&request_body)
        .send()
        .await
        .map_err(|e| report_reqwest_error("HTTP request failed", &e))?;
    let status = response.status();
    debug!(
        "Anthropic Messages response received with status {} over {:?} from {}",
        status,
        response.version(),
        sanitized_url(response.url())
    );

    if !status.is_success() {
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|e| report_reqwest_error("Failed to read API error response", &e));
        return Err(format!(
            "API request failed with status {}: {}",
            status, error_text
        ));
    }

    let message: AnthropicMessagesResponse = response
        .json()
        .await
        .map_err(|e| report_reqwest_error("Failed to parse API response", &e))?;

    // A response cut off at the token budget is incomplete; pasting it would
    // silently drop the end of the dictation.
    if message.stop_reason.as_deref() == Some("max_tokens") {
        return Err(format!(
            "Response was truncated at the {} token limit",
            ANTHROPIC_MAX_TOKENS
        ));
    }

    Ok(message.into_text())
}

/// Who sent a message in a multi-turn conversation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum ChatRole {
    User,
    Assistant,
}

impl ChatRole {
    fn as_str(self) -> &'static str {
        match self {
            ChatRole::User => "user",
            ChatRole::Assistant => "assistant",
        }
    }
}

/// An image sent with a question (Kusema: a window screenshot). Held in
/// memory only; `Debug` prints its size, never its bytes, so it can't end up
/// in a log.
#[derive(Clone, PartialEq, Eq)]
pub struct ChatImage {
    /// "image/png" or "image/jpeg".
    pub media_type: &'static str,
    pub data: Arc<[u8]>,
    /// Id of the provider the user agreed to send this image to.
    pub approved_for: String,
}

impl std::fmt::Debug for ChatImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChatImage")
            .field("media_type", &self.media_type)
            .field("bytes", &self.data.len())
            .finish()
    }
}

impl ChatImage {
    pub fn base64(&self) -> String {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD.encode(&self.data)
    }

    pub fn data_url(&self) -> String {
        format!("data:{};base64,{}", self.media_type, self.base64())
    }
}

/// One message of a multi-turn conversation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct ChatTurn {
    pub role: ChatRole,
    pub content: String,
    /// The question was asked about a screenshot (the UI shows a marker).
    #[serde(default)]
    pub screenshot: bool,
    /// The screenshot itself, while it's still attached. Never sent to the UI.
    #[serde(skip)]
    pub image: Option<ChatImage>,
}

impl ChatTurn {
    pub fn new(role: ChatRole, content: impl Into<String>) -> Self {
        Self {
            role,
            content: content.into(),
            screenshot: false,
            image: None,
        }
    }
}

/// A conversation message whose content is either plain text or, for a turn
/// with an image, a list of content blocks.
#[derive(Debug, Serialize)]
struct ConversationMessage {
    role: String,
    content: Value,
}

/// Messages for a conversation request. Images go before the question, as
/// Anthropic recommends, in each format's own block shape.
fn conversation_messages(turns: &[ChatTurn], anthropic: bool) -> Vec<ConversationMessage> {
    turns
        .iter()
        .map(|turn| {
            let content = match &turn.image {
                None => Value::String(turn.content.clone()),
                Some(image) if anthropic => serde_json::json!([
                    {
                        "type": "image",
                        "source": {
                            "type": "base64",
                            "media_type": image.media_type,
                            "data": image.base64(),
                        },
                    },
                    { "type": "text", "text": turn.content },
                ]),
                Some(image) => serde_json::json!([
                    {
                        "type": "image_url",
                        "image_url": { "url": image.data_url() },
                    },
                    { "type": "text", "text": turn.content },
                ]),
            };
            ConversationMessage {
                role: turn.role.as_str().to_string(),
                content,
            }
        })
        .collect()
}

/// Anthropic Messages body for a conversation: the system prompt goes in the
/// top-level `system` field, never in the message list.
fn anthropic_conversation_request(
    model: &str,
    system_prompt: &str,
    turns: &[ChatTurn],
) -> AnthropicMessagesRequest<ConversationMessage> {
    AnthropicMessagesRequest {
        model: model.to_string(),
        max_tokens: ANTHROPIC_MAX_TOKENS,
        stream: true,
        system: (!system_prompt.is_empty()).then(|| system_prompt.to_string()),
        messages: conversation_messages(turns, true),
    }
}

/// OpenAI chat body for a conversation: the system prompt is the first message.
fn chat_conversation_request(
    model: &str,
    system_prompt: &str,
    turns: &[ChatTurn],
) -> ChatCompletionRequest<ConversationMessage> {
    let mut messages = Vec::with_capacity(turns.len() + 1);
    if !system_prompt.is_empty() {
        messages.push(ConversationMessage {
            role: "system".to_string(),
            content: Value::String(system_prompt.to_string()),
        });
    }
    messages.extend(conversation_messages(turns, false));

    ChatCompletionRequest {
        model: model.to_string(),
        messages,
        stream: true,
        response_format: None,
        reasoning: ReasoningParams::default(),
    }
}

/// POST a JSON body; returns the response once its status is a success.
async fn post_json<B: Serialize>(
    provider: &PostProcessProvider,
    api_key: &str,
    url: &str,
    body: &B,
) -> Result<reqwest::Response, String> {
    debug!(
        "Sending conversation request to: {}",
        sanitized_url_for_log(url)
    );

    let response = create_client(provider, api_key)?
        .post(url)
        .json(body)
        .send()
        .await
        .map_err(|e| report_reqwest_error("HTTP request failed", &e))?;
    let status = response.status();
    debug!(
        "Conversation response received with status {} over {:?} from {}",
        status,
        response.version(),
        sanitized_url(response.url())
    );

    if !status.is_success() {
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|e| report_reqwest_error("Failed to read API error response", &e));
        return Err(format!(
            "API request failed with status {}: {}",
            status, error_text
        ));
    }
    Ok(response)
}

/// Collects the payloads of `data:` lines from a server-sent event stream,
/// whatever way the bytes are chunked.
#[derive(Default)]
struct SseParser {
    buf: Vec<u8>,
}

impl SseParser {
    fn push(&mut self, chunk: &[u8]) -> Vec<String> {
        self.buf.extend_from_slice(chunk);
        let mut payloads = Vec::new();
        while let Some(newline) = self.buf.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = self.buf.drain(..=newline).collect();
            let line = String::from_utf8_lossy(&line);
            let line = line.trim_end_matches(['\r', '\n']);
            if let Some(data) = line.strip_prefix("data:") {
                payloads.push(data.strip_prefix(' ').unwrap_or(data).to_string());
            }
        }
        payloads
    }
}

/// What one streamed event contributes to the reply.
#[derive(Debug, Default, PartialEq)]
struct StreamPiece {
    text: Option<String>,
    truncated: bool,
    done: bool,
}

fn json_str<'a>(value: &'a Value, pointer: &str) -> Option<&'a str> {
    value.pointer(pointer).and_then(Value::as_str)
}

fn stream_error(value: &Value) -> Option<String> {
    let error = value.get("error")?;
    Some(format!(
        "API error: {}",
        json_str(error, "/message")
            .or_else(|| error.as_str())
            .unwrap_or("unknown error")
    ))
}

/// Interpret one Anthropic Messages streaming event.
fn anthropic_stream_piece(data: &str) -> Result<StreamPiece, String> {
    let Ok(event) = serde_json::from_str::<Value>(data) else {
        return Ok(StreamPiece::default());
    };
    match json_str(&event, "/type") {
        Some("content_block_delta") if json_str(&event, "/delta/type") == Some("text_delta") => {
            Ok(StreamPiece {
                text: json_str(&event, "/delta/text").map(str::to_string),
                ..Default::default()
            })
        }
        Some("message_delta") => Ok(StreamPiece {
            truncated: json_str(&event, "/delta/stop_reason") == Some("max_tokens"),
            ..Default::default()
        }),
        Some("message_stop") => Ok(StreamPiece {
            done: true,
            ..Default::default()
        }),
        Some("error") => Err(stream_error(&event).unwrap_or_else(|| "API error".to_string())),
        _ => Ok(StreamPiece::default()),
    }
}

/// Interpret one OpenAI chat streaming chunk.
fn chat_stream_piece(data: &str) -> Result<StreamPiece, String> {
    if data.trim() == "[DONE]" {
        return Ok(StreamPiece {
            done: true,
            ..Default::default()
        });
    }
    let Ok(chunk) = serde_json::from_str::<Value>(data) else {
        return Ok(StreamPiece::default());
    };
    if let Some(error) = stream_error(&chunk) {
        return Err(error);
    }
    Ok(StreamPiece {
        text: json_str(&chunk, "/choices/0/delta/content").map(str::to_string),
        truncated: json_str(&chunk, "/choices/0/finish_reason") == Some("length"),
        done: false,
    })
}

/// Send a multi-turn conversation (oldest message first) and return the reply.
/// Works for both the Anthropic Messages format and the OpenAI chat format.
/// The reply is streamed: `on_text` receives each piece as it arrives, and the
/// full text is returned at the end. Used by Assistant mode; the
/// post-processing requests above are unchanged.
pub async fn send_conversation<F: FnMut(&str)>(
    provider: &PostProcessProvider,
    api_key: String,
    model: &str,
    system_prompt: &str,
    turns: &[ChatTurn],
    mut on_text: F,
) -> Result<Option<String>, String> {
    let base_url = provider.base_url.trim_end_matches('/');
    let anthropic = provider.api_format == PostProcessApiFormat::Anthropic;
    let truncated_error = || {
        if anthropic {
            format!(
                "Response was truncated at the {} token limit",
                ANTHROPIC_MAX_TOKENS
            )
        } else {
            "Response was truncated at the provider's output token limit".to_string()
        }
    };

    let response = if anthropic {
        let body = anthropic_conversation_request(model, system_prompt, turns);
        post_json(provider, &api_key, &format!("{}/messages", base_url), &body).await?
    } else {
        let body = chat_conversation_request(model, system_prompt, turns);
        post_json(
            provider,
            &api_key,
            &format!("{}/chat/completions", base_url),
            &body,
        )
        .await?
    };

    let is_event_stream = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.contains("text/event-stream"));
    if !is_event_stream {
        // Some OpenAI-compatible servers ignore `stream: true` and answer in
        // one piece.
        let reply = if anthropic {
            let message: AnthropicMessagesResponse = response
                .json()
                .await
                .map_err(|e| report_reqwest_error("Failed to parse API response", &e))?;
            if message.stop_reason.as_deref() == Some("max_tokens") {
                return Err(truncated_error());
            }
            message.into_text()
        } else {
            let completion: ChatCompletionResponse = response
                .json()
                .await
                .map_err(|e| report_reqwest_error("Failed to parse API response", &e))?;
            let Some(choice) = completion.choices.into_iter().next() else {
                return Ok(None);
            };
            if choice.finish_reason.as_deref() == Some("length") {
                return Err(truncated_error());
            }
            choice.message.content
        };
        if let Some(text) = &reply {
            on_text(text);
        }
        return Ok(reply);
    }

    let mut parser = SseParser::default();
    let mut reply = String::new();
    let mut truncated = false;
    let mut chunks = response.bytes_stream();
    'stream: while let Some(chunk) = chunks.next().await {
        let chunk =
            chunk.map_err(|e| report_reqwest_error("Failed to read streamed response", &e))?;
        for data in parser.push(&chunk) {
            let piece = if anthropic {
                anthropic_stream_piece(&data)?
            } else {
                chat_stream_piece(&data)?
            };
            if let Some(text) = piece.text.filter(|t| !t.is_empty()) {
                on_text(&text);
                reply.push_str(&text);
            }
            truncated |= piece.truncated;
            if piece.done {
                break 'stream;
            }
        }
    }

    if truncated {
        return Err(truncated_error());
    }
    Ok((!reply.is_empty()).then_some(reply))
}

/// Fetch available models from an OpenAI-compatible API
/// Returns a list of model IDs
pub async fn fetch_models(
    provider: &PostProcessProvider,
    api_key: String,
) -> Result<Vec<String>, String> {
    let base_url = provider.base_url.trim_end_matches('/');
    let url = format!("{}/models", base_url);

    debug!("Fetching models from: {}", sanitized_url_for_log(&url));

    let client = create_client(provider, &api_key)?;

    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| report_reqwest_error("Failed to fetch models", &e))?;

    let status = response.status();
    debug!(
        "Model list response received with status {} over {:?} from {}",
        status,
        response.version(),
        sanitized_url(response.url())
    );
    if !status.is_success() {
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|e| report_reqwest_error("Failed to read model list error", &e));
        return Err(format!(
            "Model list request failed ({}): {}",
            status, error_text
        ));
    }

    let parsed: serde_json::Value = response
        .json()
        .await
        .map_err(|e| report_reqwest_error("Failed to parse model list response", &e))?;

    let mut models = Vec::new();

    // Handle OpenAI format: { data: [ { id: "..." }, ... ] }
    if let Some(data) = parsed.get("data").and_then(|d| d.as_array()) {
        for entry in data {
            if let Some(id) = entry.get("id").and_then(|i| i.as_str()) {
                models.push(id.to_string());
            } else if let Some(name) = entry.get("name").and_then(|n| n.as_str()) {
                models.push(name.to_string());
            }
        }
    }
    // Handle array format: [ "model1", "model2", ... ]
    else if let Some(array) = parsed.as_array() {
        for entry in array {
            if let Some(model) = entry.as_str() {
                models.push(model.to_string());
            }
        }
    }

    Ok(models)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fmt;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[derive(Debug)]
    struct TestError {
        message: &'static str,
        source: Option<Box<TestError>>,
    }

    impl fmt::Display for TestError {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str(self.message)
        }
    }

    impl StdError for TestError {
        fn source(&self) -> Option<&(dyn StdError + 'static)> {
            self.source
                .as_deref()
                .map(|source| source as &(dyn StdError + 'static))
        }
    }

    fn provider(id: &str, base_url: &str) -> PostProcessProvider {
        PostProcessProvider {
            id: id.to_string(),
            label: id.to_string(),
            base_url: base_url.to_string(),
            allow_base_url_edit: true,
            models_endpoint: None,
            supports_structured_output: false,
            api_format: PostProcessApiFormat::OpenAi,
        }
    }

    fn anthropic_format_provider(base_url: &str) -> PostProcessProvider {
        PostProcessProvider {
            api_format: PostProcessApiFormat::Anthropic,
            ..provider("custom", base_url)
        }
    }

    fn request_json(reasoning: ReasoningParams) -> Value {
        let request = ChatCompletionRequest {
            model: "test-model".to_string(),
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: "hi".to_string(),
            }],
            stream: false,
            response_format: None,
            reasoning,
        };
        serde_json::to_value(&request).unwrap()
    }

    async fn serve_one_response(status: &str, body: &str) -> String {
        serve_response(status, "", body).await
    }

    async fn serve_event_stream(body: &str) -> String {
        serve_response("200 OK", "Content-Type: text/event-stream\r\n", body).await
    }

    async fn serve_response(status: &str, headers: &str, body: &str) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let response = format!(
            "HTTP/1.1 {status}\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );

        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = [0_u8; 2048];
            let _ = stream.read(&mut request).await.unwrap();
            stream.write_all(response.as_bytes()).await.unwrap();
        });

        format!("http://{address}")
    }

    #[test]
    fn error_source_chain_includes_all_nested_causes() {
        let error = TestError {
            message: "request failed",
            source: Some(Box::new(TestError {
                message: "TLS handshake failed",
                source: Some(Box::new(TestError {
                    message: "unknown certificate authority",
                    source: None,
                })),
            })),
        };

        assert_eq!(
            error_source_chain(&error),
            vec!["TLS handshake failed", "unknown certificate authority"]
        );
    }

    #[test]
    fn log_url_sanitization_removes_credentials_and_tokens() {
        let url = "https://user:password@example.com/v1/models?api_key=secret#private";
        assert_eq!(sanitized_url_for_log(url), "https://example.com/v1/models");
    }

    #[test]
    fn invalid_log_urls_are_not_echoed() {
        assert_eq!(
            sanitized_url_for_log("not a URL containing secret"),
            "<invalid URL>"
        );
    }

    #[tokio::test]
    async fn decode_error_does_not_echo_response_values() {
        let base_url =
            serve_one_response("200 OK", r#"{"choices":"PRIVATE TRANSCRIPTION CONTENT"}"#).await;
        let error = reqwest::get(base_url)
            .await
            .unwrap()
            .json::<ChatCompletionResponse>()
            .await
            .unwrap_err();

        let details = report_reqwest_error("Failed to parse API response", &error);
        assert!(details.contains("kind: decode"));
        assert!(!details.contains("PRIVATE TRANSCRIPTION CONTENT"));
    }

    #[tokio::test]
    async fn raw_error_url_is_not_reintroduced_without_a_source() {
        let base_url = serve_one_response("400 Bad Request", "bad request").await;
        let error = reqwest::get(format!(
            "{base_url}/private?api_key=SECRET_QUERY_TOKEN#private"
        ))
        .await
        .unwrap()
        .error_for_status()
        .unwrap_err();

        let details = report_reqwest_error("Request failed", &error);
        assert!(details.contains(&format!("url: {base_url}/private")));
        assert!(!details.contains("SECRET_QUERY_TOKEN"));
        assert!(!details.contains("#private"));
    }

    #[test]
    fn requests_explicitly_disable_streaming() {
        let json = request_json(ReasoningParams::default());
        assert_eq!(json["stream"], false);
    }

    #[test]
    fn default_reasoning_params_serialize_to_no_fields() {
        let json = request_json(ReasoningParams::default());
        assert!(json.get("reasoning_effort").is_none());
        assert!(json.get("reasoning").is_none());
        assert!(json.get("thinking").is_none());
    }

    #[test]
    fn custom_provider_uses_top_level_reasoning_effort() {
        let params = reasoning_disable_params(&provider("custom", "http://localhost:11434/v1"));
        let json = request_json(params);
        assert_eq!(json["reasoning_effort"], "none");
        assert!(json.get("reasoning").is_none());
        assert!(json.get("thinking").is_none());
    }

    #[test]
    fn openrouter_uses_nested_reasoning_object() {
        let params =
            reasoning_disable_params(&provider("openrouter", "https://openrouter.ai/api/v1"));
        let json = request_json(params);
        assert!(json.get("reasoning_effort").is_none());
        assert_eq!(json["reasoning"]["effort"], "none");
        assert_eq!(json["reasoning"]["exclude"], true);
        assert!(json.get("thinking").is_none());
    }

    #[test]
    fn deepseek_base_url_uses_thinking_disabled() {
        let params = reasoning_disable_params(&provider("custom", "https://api.deepseek.com"));
        let json = request_json(params);
        assert!(json.get("reasoning_effort").is_none());
        assert!(json.get("reasoning").is_none());
        assert_eq!(json["thinking"]["type"], "disabled");
    }

    #[test]
    fn reasoning_params_is_empty_tracks_all_fields() {
        assert!(ReasoningParams::default().is_empty());
        assert!(!ReasoningParams {
            reasoning_effort: Some("none".to_string()),
            ..Default::default()
        }
        .is_empty());
        assert!(!ReasoningParams {
            thinking: Some(serde_json::json!({ "type": "disabled" })),
            ..Default::default()
        }
        .is_empty());
    }

    #[test]
    fn rejection_memo_is_keyed_by_base_url_and_model() {
        let deepseek = provider("custom", "https://api.deepseek.com/");
        let key = endpoint_key(&deepseek, "deepseek-chat");
        assert_eq!(key, "https://api.deepseek.com|deepseek-chat");
        assert!(!is_known_rejected(&key));
        remember_rejection(key.clone());
        assert!(is_known_rejected(&key));
        // A different model on the same endpoint is tracked separately
        assert!(!is_known_rejected(&endpoint_key(&deepseek, "other-model")));
    }

    #[test]
    fn anthropic_format_uses_api_key_headers() {
        let headers = build_headers(
            &anthropic_format_provider("https://example.com/anthropic/v1"),
            "key",
        )
        .unwrap();
        assert_eq!(headers["x-api-key"], "key");
        assert_eq!(headers["anthropic-version"], "2023-06-01");
        assert!(headers.get(AUTHORIZATION).is_none());

        let headers =
            build_headers(&provider("custom", "http://localhost:11434/v1"), "key").unwrap();
        assert_eq!(headers[AUTHORIZATION], "Bearer key");
        assert!(headers.get("x-api-key").is_none());
    }

    #[test]
    fn anthropic_request_carries_required_max_tokens() {
        let request = AnthropicMessagesRequest {
            model: "claude-haiku".to_string(),
            max_tokens: ANTHROPIC_MAX_TOKENS,
            stream: false,
            system: None,
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: "hi".to_string(),
            }],
        };
        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(json["max_tokens"], ANTHROPIC_MAX_TOKENS);
        assert_eq!(json["messages"][0]["role"], "user");
        assert!(json.get("stream").is_none());
        assert!(json.get("reasoning_effort").is_none());
        assert!(json.get("system").is_none());
    }

    fn conversation() -> Vec<ChatTurn> {
        vec![
            ChatTurn::new(ChatRole::User, "What's the capital of Ghana?"),
            ChatTurn::new(ChatRole::Assistant, "Accra."),
            ChatTurn::new(ChatRole::User, "What's its population?"),
        ]
    }

    #[test]
    fn anthropic_conversation_puts_system_prompt_at_top_level() {
        let json = serde_json::to_value(anthropic_conversation_request(
            "claude-haiku",
            "Be brief.",
            &conversation(),
        ))
        .unwrap();
        assert_eq!(json["system"], "Be brief.");
        assert_eq!(json["max_tokens"], ANTHROPIC_MAX_TOKENS);
        assert_eq!(json["stream"], true);
        let messages = json["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0]["role"], "user");
        assert_eq!(messages[1]["role"], "assistant");
        assert_eq!(messages[1]["content"], "Accra.");
        assert_eq!(messages[2]["role"], "user");
    }

    #[test]
    fn chat_conversation_puts_system_prompt_first() {
        let json = serde_json::to_value(chat_conversation_request(
            "gpt",
            "Be brief.",
            &conversation(),
        ))
        .unwrap();
        let messages = json["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 4);
        assert_eq!(messages[0]["role"], "system");
        assert_eq!(messages[0]["content"], "Be brief.");
        assert_eq!(messages[1]["role"], "user");
        assert_eq!(messages[2]["role"], "assistant");
        assert_eq!(json["stream"], true);
        assert!(json.get("response_format").is_none());
        assert!(json.get("reasoning_effort").is_none());
    }

    #[test]
    fn empty_system_prompt_is_omitted() {
        let anthropic =
            serde_json::to_value(anthropic_conversation_request("m", "", &conversation())).unwrap();
        assert!(anthropic.get("system").is_none());
        let chat = chat_conversation_request("m", "", &conversation());
        assert_eq!(chat.messages[0].role, "user");
    }

    fn with_image() -> Vec<ChatTurn> {
        let mut turns = conversation();
        turns[2].screenshot = true;
        turns[2].image = Some(ChatImage {
            media_type: "image/png",
            data: Arc::from(&b"\x89PNG"[..]),
            approved_for: "anthropic".to_string(),
        });
        turns
    }

    #[test]
    fn anthropic_image_is_a_base64_block_before_the_question() {
        let json =
            serde_json::to_value(anthropic_conversation_request("m", "", &with_image())).unwrap();
        let messages = json["messages"].as_array().unwrap();
        // Text-only turns stay plain strings.
        assert_eq!(messages[0]["content"], "What's the capital of Ghana?");
        let blocks = messages[2]["content"].as_array().unwrap();
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0]["type"], "image");
        assert_eq!(blocks[0]["source"]["type"], "base64");
        assert_eq!(blocks[0]["source"]["media_type"], "image/png");
        assert_eq!(blocks[0]["source"]["data"], "iVBORw==");
        assert_eq!(blocks[1]["type"], "text");
        assert_eq!(blocks[1]["text"], "What's its population?");
    }

    #[test]
    fn chat_image_is_a_data_url() {
        let json =
            serde_json::to_value(chat_conversation_request("m", "Sys", &with_image())).unwrap();
        let messages = json["messages"].as_array().unwrap();
        assert_eq!(messages[0]["content"], "Sys");
        assert_eq!(messages[1]["content"], "What's the capital of Ghana?");
        let blocks = messages[3]["content"].as_array().unwrap();
        assert_eq!(blocks[0]["type"], "image_url");
        assert_eq!(
            blocks[0]["image_url"]["url"],
            "data:image/png;base64,iVBORw=="
        );
        assert_eq!(blocks[1]["type"], "text");
        assert_eq!(blocks[1]["text"], "What's its population?");
    }

    #[test]
    fn image_bytes_never_appear_in_debug_output() {
        let turns = with_image();
        let debug = format!("{:?}", turns[2]);
        assert!(debug.contains("bytes: 4"));
        assert!(!debug.contains("iVBOR"));
        // Nor in what the UI receives.
        let json = serde_json::to_value(&turns[2]).unwrap();
        assert!(json.get("image").is_none());
        assert_eq!(json["screenshot"], true);
    }

    async fn converse(
        provider: &PostProcessProvider,
    ) -> (Result<Option<String>, String>, Vec<String>) {
        let mut pieces = Vec::new();
        let result = send_conversation(
            provider,
            "key".to_string(),
            "model",
            "Be brief.",
            &conversation(),
            |text| pieces.push(text.to_string()),
        )
        .await;
        (result, pieces)
    }

    #[tokio::test]
    async fn non_streamed_replies_still_parse() {
        let base_url = serve_one_response(
            "200 OK",
            r#"{"type":"message","role":"assistant","content":[{"type":"text","text":"About 2.5 million."}]}"#,
        )
        .await;
        let (result, pieces) = converse(&anthropic_format_provider(&base_url)).await;
        assert_eq!(result, Ok(Some("About 2.5 million.".to_string())));
        assert_eq!(pieces, ["About 2.5 million."]);

        let base_url = serve_one_response(
            "200 OK",
            r#"{"choices":[{"message":{"content":"About 2.5 million."},"finish_reason":"stop"}]}"#,
        )
        .await;
        let (result, _) = converse(&provider("custom", &base_url)).await;
        assert_eq!(result, Ok(Some("About 2.5 million.".to_string())));
    }

    #[tokio::test]
    async fn anthropic_reply_is_streamed() {
        let base_url = serve_event_stream(concat!(
            "event: message_start\n",
            "data: {\"type\":\"message_start\",\"message\":{}}\n\n",
            "event: content_block_delta\n",
            "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"thinking_delta\",\"thinking\":\"hmm\"}}\n\n",
            "event: content_block_delta\n",
            "data: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"text_delta\",\"text\":\"Accra. \"}}\n\n",
            "event: ping\n",
            "data: {\"type\":\"ping\"}\n\n",
            "event: content_block_delta\n",
            "data: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"text_delta\",\"text\":\"Yes.\"}}\n\n",
            "event: message_delta\n",
            "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"}}\n\n",
            "event: message_stop\n",
            "data: {\"type\":\"message_stop\"}\n\n",
        ))
        .await;
        let (result, pieces) = converse(&anthropic_format_provider(&base_url)).await;
        assert_eq!(result, Ok(Some("Accra. Yes.".to_string())));
        assert_eq!(pieces, ["Accra. ", "Yes."]);
    }

    #[tokio::test]
    async fn chat_reply_is_streamed() {
        let base_url = serve_event_stream(concat!(
            "data: {\"choices\":[{\"delta\":{\"role\":\"assistant\"}}]}\r\n\r\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\"Accra.\"}}]}\r\n\r\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\" Yes.\"},\"finish_reason\":\"stop\"}]}\r\n\r\n",
            "data: [DONE]\r\n\r\n",
        ))
        .await;
        let (result, pieces) = converse(&provider("custom", &base_url)).await;
        assert_eq!(result, Ok(Some("Accra. Yes.".to_string())));
        assert_eq!(pieces, ["Accra.", " Yes."]);
    }

    #[tokio::test]
    async fn streamed_errors_and_truncation_are_reported() {
        let base_url = serve_event_stream(
            "event: error\ndata: {\"type\":\"error\",\"error\":{\"type\":\"overloaded_error\",\"message\":\"Overloaded\"}}\n\n",
        )
        .await;
        let (result, _) = converse(&anthropic_format_provider(&base_url)).await;
        assert_eq!(result, Err("API error: Overloaded".to_string()));

        let base_url = serve_event_stream(concat!(
            "data: {\"choices\":[{\"delta\":{\"content\":\"Cut\"},\"finish_reason\":\"length\"}]}\n\n",
            "data: [DONE]\n\n",
        ))
        .await;
        let (result, _) = converse(&provider("custom", &base_url)).await;
        assert!(result.unwrap_err().contains("truncated"));
    }

    #[test]
    fn sse_parser_handles_split_chunks() {
        let mut parser = SseParser::default();
        assert!(parser.push(b"event: x\ndata: {\"a\"").is_empty());
        assert_eq!(
            parser.push(b":1}\r\n\r\ndata:[DONE]\n"),
            ["{\"a\":1}", "[DONE]"]
        );
        // A multi-byte character split across chunks survives.
        let text = "data: caf\u{e9}\n".as_bytes();
        assert!(parser.push(&text[..9]).is_empty());
        assert_eq!(parser.push(&text[9..]), ["caf\u{e9}"]);
    }

    #[tokio::test]
    async fn conversation_reports_http_errors() {
        let base_url = serve_one_response("401 Unauthorized", r#"{"error":"bad key"}"#).await;
        let (result, _) = converse(&provider("custom", &base_url)).await;
        assert!(result.unwrap_err().contains("401"));
    }

    #[test]
    fn anthropic_response_keeps_only_text_blocks() {
        let response: AnthropicMessagesResponse = serde_json::from_str(
            r#"{"content":[{"type":"thinking","thinking":"hmm"},{"type":"text","text":"Hello"},{"type":"text","text":" world"}]}"#,
        )
        .unwrap();
        assert_eq!(response.into_text().as_deref(), Some("Hello world"));

        let empty: AnthropicMessagesResponse = serde_json::from_str(r#"{"content":[]}"#).unwrap();
        assert_eq!(empty.into_text(), None);
    }

    #[tokio::test]
    async fn anthropic_format_parses_messages_response() {
        let base_url = serve_one_response(
            "200 OK",
            r#"{"type":"message","role":"assistant","content":[{"type":"text","text":"Cleaned text"}]}"#,
        )
        .await;
        let result = send_chat_completion(
            &anthropic_format_provider(&base_url),
            "key".to_string(),
            "claude-haiku",
            "raw text".to_string(),
            true,
        )
        .await;
        assert_eq!(result, Ok(Some("Cleaned text".to_string())));
    }

    #[tokio::test]
    async fn chat_completion_rejects_truncated_response() {
        let base_url = serve_one_response(
            "200 OK",
            r#"{"choices":[{"message":{"content":"Cut off"},"finish_reason":"length"}]}"#,
        )
        .await;
        let result = send_chat_completion(
            &provider("custom", &base_url),
            "key".to_string(),
            "test-model",
            "raw text".to_string(),
            false,
        )
        .await;
        assert!(result.unwrap_err().contains("truncated"));
    }

    #[tokio::test]
    async fn anthropic_format_rejects_truncated_response() {
        let base_url = serve_one_response(
            "200 OK",
            r#"{"type":"message","stop_reason":"max_tokens","content":[{"type":"text","text":"Cut off"}]}"#,
        )
        .await;
        let result = send_chat_completion(
            &anthropic_format_provider(&base_url),
            "key".to_string(),
            "claude-haiku",
            "raw text".to_string(),
            true,
        )
        .await;
        assert!(result.unwrap_err().contains("truncated"));
    }
}
