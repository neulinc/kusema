//! Kusema web search with the AI provider's own search tool. The provider
//! chooses and runs the searches; Kusema caps them at 3, lists each in the
//! ledger with what it found, and shows the pages as sources.
//!
//! Claude: the `web_search_20250305` server tool with `max_uses`, on every
//! current model, $10 per 1,000 searches plus tokens; results arrive in the
//! stream as `web_search_tool_result` blocks (`content_block_start`, with
//! each result's `url` and `title`), the search text as the `server_tool_use`
//! block's input. https://platform.claude.com/docs/en/agents-and-tools/tool-use/web-search-tool
//! (checked 2026-11-26).
//!
//! OpenAI: the Responses API with `tools: [{"type": "web_search"}]`,
//! `max_tool_calls` (all built-in tools) and
//! `include: ["web_search_call.action.sources"]`; `web_search_call` items
//! carry `action.query`/`queries` and `sources`, the answer's `output_text`
//! carries `url_citation` annotations. $10 per 1,000 calls plus search
//! tokens. `store: false`, so OpenAI doesn't keep the response (it would
//! otherwise for at least 30 days).
//! https://developers.openai.com/api/docs/guides/tools-web-search and
//! https://developers.openai.com/api/reference/resources/responses/methods/create
//! (checked 2026-11-26). Not streamed: the streaming event names couldn't be
//! verified, so the answer arrives in one piece.

use super::{search::MAX_RESULTS, search::MAX_SEARCHES, WebSource};
use crate::llm_client::{self, ChatTurn};
use crate::network::{self, DataKind, Purpose, RequestResult, WebKind};
use crate::settings::{PostProcessApiFormat, PostProcessProvider};
use futures_util::StreamExt;
use serde_json::{json, Value};
use std::collections::HashMap;

pub const ANTHROPIC_TOOL: &str = "web_search_20250305";

/// What a built-in search answer brought.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Found {
    pub text: Option<String>,
    pub sources: Vec<WebSource>,
    /// Each search and how many results it returned.
    pub searches: Vec<(String, usize)>,
}

fn add_source(sources: &mut Vec<WebSource>, title: &str, url: &str) {
    if url.starts_with("http") && !sources.iter().any(|s| s.url == url) {
        sources.push(WebSource {
            title: if title.trim().is_empty() {
                url.to_string()
            } else {
                title.trim().to_string()
            },
            url: url.to_string(),
        });
    }
}

/// Lists one built-in search in the ledger.
fn log_search(provider: &str, query: &str, results: usize, ok: bool) {
    let id = network::record_web(WebKind::Search, provider, query, RequestResult::Sent);
    network::finish_web(
        id,
        if ok {
            RequestResult::Sent
        } else {
            RequestResult::Failed
        },
        Some(results),
        None,
    );
}

/// Asks with the provider's search. Claude streams (`on_text` gets each
/// piece); OpenAI answers in one piece.
pub async fn ask<F: FnMut(&str)>(
    provider: &PostProcessProvider,
    api_key: &str,
    model: &str,
    system_prompt: &str,
    turns: &[ChatTurn],
    on_text: F,
) -> Result<Found, String> {
    if turns.iter().any(|t| t.image.is_some()) {
        // Never with a screenshot: `decide` keeps private context away.
        return Err("The built-in search isn't used with a screenshot.".into());
    }
    match (provider.id.as_str(), provider.api_format) {
        ("anthropic", PostProcessApiFormat::Anthropic) => {
            anthropic(provider, api_key, model, system_prompt, turns, on_text).await
        }
        ("openai", _) => openai(provider, api_key, model, system_prompt, turns, on_text).await,
        _ => Err(format!("{} has no built-in web search.", provider.label)),
    }
}

/// The Claude request body: a normal conversation plus the search tool.
pub fn anthropic_body(model: &str, system_prompt: &str, turns: &[ChatTurn]) -> Value {
    let mut body = serde_json::to_value(llm_client::anthropic_conversation_request(
        model,
        system_prompt,
        turns,
    ))
    .expect("request serializes");
    body["tools"] = json!([{
        "type": ANTHROPIC_TOOL,
        "name": "web_search",
        "max_uses": MAX_SEARCHES,
    }]);
    body
}

/// Follows Claude's stream: text, searches and their results.
#[derive(Default)]
pub struct AnthropicStream {
    /// Search text being streamed in, by content block index.
    inputs: HashMap<u64, String>,
    /// Searches in the order they ran, waiting for their results.
    pending: Vec<String>,
    pub found: Found,
    pub done: bool,
    pub truncated: bool,
}

impl AnthropicStream {
    /// Takes one event; returns text to show, if any.
    pub fn event(&mut self, data: &str) -> Result<Option<String>, String> {
        let Ok(event) = serde_json::from_str::<Value>(data) else {
            return Ok(None);
        };
        let index = event["index"].as_u64().unwrap_or(0);
        match event["type"].as_str() {
            Some("content_block_start") => {
                let block = &event["content_block"];
                match block["type"].as_str() {
                    Some("server_tool_use") => {
                        // Usually empty here, streamed in as JSON pieces.
                        let query = block["input"]["query"].as_str().unwrap_or_default();
                        self.inputs.insert(index, query.to_string());
                    }
                    Some("web_search_tool_result") => {
                        let query = if self.pending.is_empty() {
                            String::new()
                        } else {
                            self.pending.remove(0)
                        };
                        match block["content"].as_array() {
                            Some(results) => {
                                for result in results.iter().take(MAX_RESULTS) {
                                    add_source(
                                        &mut self.found.sources,
                                        result["title"].as_str().unwrap_or_default(),
                                        result["url"].as_str().unwrap_or_default(),
                                    );
                                }
                                log_search("Claude (built-in search)", &query, results.len(), true);
                                self.found.searches.push((query, results.len()));
                            }
                            None => {
                                log_search("Claude (built-in search)", &query, 0, false);
                                self.found.searches.push((query, 0));
                            }
                        }
                    }
                    _ => {}
                }
                Ok(None)
            }
            Some("content_block_delta") => match event["delta"]["type"].as_str() {
                Some("text_delta") => Ok(event["delta"]["text"].as_str().map(str::to_string)),
                Some("input_json_delta") => {
                    if let Some(input) = self.inputs.get_mut(&index) {
                        input.push_str(event["delta"]["partial_json"].as_str().unwrap_or_default());
                    }
                    Ok(None)
                }
                _ => Ok(None),
            },
            Some("content_block_stop") => {
                if let Some(input) = self.inputs.remove(&index) {
                    let query = serde_json::from_str::<Value>(&input)
                        .ok()
                        .and_then(|v| v["query"].as_str().map(str::to_string))
                        .unwrap_or(input);
                    self.pending.push(query);
                }
                Ok(None)
            }
            Some("message_delta") => {
                match event["delta"]["stop_reason"].as_str() {
                    Some("max_tokens") => self.truncated = true,
                    // A long search turn the API paused: what's there is the
                    // answer so far.
                    Some("pause_turn") => self.done = true,
                    _ => {}
                }
                Ok(None)
            }
            Some("message_stop") => {
                self.done = true;
                Ok(None)
            }
            Some("error") => Err(format!(
                "API error: {}",
                event["error"]["message"]
                    .as_str()
                    .unwrap_or("unknown error")
            )),
            _ => Ok(None),
        }
    }
}

async fn anthropic<F: FnMut(&str)>(
    provider: &PostProcessProvider,
    api_key: &str,
    model: &str,
    system_prompt: &str,
    turns: &[ChatTurn],
    mut on_text: F,
) -> Result<Found, String> {
    let url = format!("{}/messages", provider.base_url.trim_end_matches('/'));
    let body = anthropic_body(model, system_prompt, turns);
    let response = llm_client::post_json(
        provider,
        api_key,
        &url,
        &body,
        Purpose::AssistantQuestion,
        DataKind::Text,
    )
    .await?;
    let mut parser = llm_client::SseParser::default();
    let mut stream = AnthropicStream::default();
    let mut reply = String::new();
    let mut chunks = response.bytes_stream();
    'read: while let Some(chunk) = chunks.next().await {
        let chunk = chunk.map_err(|e| format!("Failed to read streamed response: {e}"))?;
        for data in parser.push(&chunk) {
            if let Some(text) = stream.event(&data)?.filter(|t| !t.is_empty()) {
                on_text(&text);
                reply.push_str(&text);
            }
            if stream.done {
                break 'read;
            }
        }
    }
    if stream.truncated {
        return Err("Response was truncated at the token limit".into());
    }
    let mut found = stream.found;
    found.text = (!reply.is_empty()).then_some(reply);
    Ok(found)
}

/// The OpenAI Responses request body.
pub fn openai_body(model: &str, system_prompt: &str, turns: &[ChatTurn]) -> Value {
    let input: Vec<Value> = turns
        .iter()
        .map(|turn| {
            json!({
                "role": match turn.role {
                    llm_client::ChatRole::User => "user",
                    llm_client::ChatRole::Assistant => "assistant",
                },
                "content": turn.content,
            })
        })
        .collect();
    let mut body = json!({
        "model": model,
        "input": input,
        "tools": [{ "type": "web_search" }],
        "max_tool_calls": MAX_SEARCHES,
        "include": ["web_search_call.action.sources"],
        "store": false,
    });
    if !system_prompt.is_empty() {
        body["instructions"] = json!(system_prompt);
    }
    body
}

/// Reads a Responses API answer: its text, searches and cited pages.
pub fn parse_openai(response: &Value) -> Found {
    let mut found = Found::default();
    let mut text = String::new();
    for item in response["output"].as_array().into_iter().flatten() {
        match item["type"].as_str() {
            Some("web_search_call") => {
                let action = &item["action"];
                if action["type"].as_str() != Some("search") {
                    continue;
                }
                let mut queries: Vec<String> = action["queries"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|q| q.as_str().map(str::to_string))
                    .collect();
                if let Some(query) = action["query"].as_str() {
                    if !queries.iter().any(|q| q == query) {
                        queries.insert(0, query.to_string());
                    }
                }
                let results = action["sources"].as_array().map_or(0, Vec::len);
                let ok = item["status"].as_str() != Some("failed");
                let query = queries.join(" | ");
                log_search("OpenAI (built-in search)", &query, results, ok);
                found.searches.push((query, results));
            }
            Some("message") => {
                for part in item["content"].as_array().into_iter().flatten() {
                    if part["type"].as_str() != Some("output_text") {
                        continue;
                    }
                    text.push_str(part["text"].as_str().unwrap_or_default());
                    for note in part["annotations"].as_array().into_iter().flatten() {
                        if note["type"].as_str() == Some("url_citation") {
                            add_source(
                                &mut found.sources,
                                note["title"].as_str().unwrap_or_default(),
                                note["url"].as_str().unwrap_or_default(),
                            );
                        }
                    }
                }
            }
            _ => {}
        }
    }
    found.text = (!text.trim().is_empty()).then_some(text);
    found
}

async fn openai<F: FnMut(&str)>(
    provider: &PostProcessProvider,
    api_key: &str,
    model: &str,
    system_prompt: &str,
    turns: &[ChatTurn],
    mut on_text: F,
) -> Result<Found, String> {
    let url = format!("{}/responses", provider.base_url.trim_end_matches('/'));
    let body = openai_body(model, system_prompt, turns);
    let response = llm_client::post_json(
        provider,
        api_key,
        &url,
        &body,
        Purpose::AssistantQuestion,
        DataKind::Text,
    )
    .await?;
    let value: Value = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse API response: {e}"))?;
    if value["status"].as_str() == Some("incomplete") && value["output"].as_array().is_none() {
        return Err("Response was truncated at the provider's output token limit".into());
    }
    let found = parse_openai(&value);
    if let Some(text) = &found.text {
        on_text(text);
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm_client::ChatRole;

    #[test]
    fn claude_is_asked_with_at_most_three_searches() {
        let body = anthropic_body(
            "claude-sonnet-5-5",
            "sys",
            &[ChatTurn::new(ChatRole::User, "q")],
        );
        assert_eq!(body["tools"][0]["type"], ANTHROPIC_TOOL);
        assert_eq!(body["tools"][0]["name"], "web_search");
        assert_eq!(body["tools"][0]["max_uses"], 3);
        assert_eq!(body["stream"], true);
        assert_eq!(body["system"], "sys");
    }

    #[test]
    fn openai_is_asked_with_at_most_three_searches_and_nothing_stored() {
        let body = openai_body(
            "gpt-6.2-sol",
            "sys",
            &[
                ChatTurn::new(ChatRole::User, "a"),
                ChatTurn::new(ChatRole::Assistant, "b"),
                ChatTurn::new(ChatRole::User, "c"),
            ],
        );
        assert_eq!(body["tools"][0]["type"], "web_search");
        assert_eq!(body["max_tool_calls"], 3);
        assert_eq!(body["store"], false);
        assert_eq!(body["instructions"], "sys");
        assert_eq!(body["input"][1]["role"], "assistant");
        assert_eq!(body["include"][0], "web_search_call.action.sources");
    }

    #[test]
    fn claudes_stream_gives_text_searches_and_sources() {
        let events = [
            r#"{"type":"message_start","message":{}}"#,
            r#"{"type":"content_block_start","index":0,"content_block":{"type":"server_tool_use","id":"srvtoolu_1","name":"web_search","input":{}}}"#,
            r#"{"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"{\"query\": \"court "}}"#,
            r#"{"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"holidays\"}"}}"#,
            r#"{"type":"content_block_stop","index":0}"#,
            r#"{"type":"content_block_start","index":1,"content_block":{"type":"web_search_tool_result","tool_use_id":"srvtoolu_1","content":[{"type":"web_search_result","title":"Holidays","url":"https://court.example/holidays","encrypted_content":"x"},{"type":"web_search_result","title":"Other","url":"https://other.example/"}]}}"#,
            r#"{"type":"content_block_stop","index":1}"#,
            r#"{"type":"content_block_start","index":2,"content_block":{"type":"text","text":""}}"#,
            r#"{"type":"content_block_delta","index":2,"delta":{"type":"text_delta","text":"Closed on "}}"#,
            r#"{"type":"content_block_delta","index":2,"delta":{"type":"citations_delta","citation":{"type":"web_search_result_location","url":"https://court.example/holidays"}}}"#,
            r#"{"type":"content_block_delta","index":2,"delta":{"type":"text_delta","text":"Nov 27."}}"#,
            r#"{"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"server_tool_use":{"web_search_requests":1}}}"#,
            r#"{"type":"message_stop"}"#,
        ];
        let mut stream = AnthropicStream::default();
        let mut text = String::new();
        for event in events {
            if let Some(piece) = stream.event(event).unwrap() {
                text.push_str(&piece);
            }
        }
        assert_eq!(text, "Closed on Nov 27.");
        assert!(stream.done);
        assert_eq!(stream.found.searches, [("court holidays".to_string(), 2)]);
        assert_eq!(stream.found.sources.len(), 2);
        assert_eq!(
            stream.found.sources[0].url,
            "https://court.example/holidays"
        );
    }

    #[test]
    fn a_failed_claude_search_is_noted_and_the_answer_goes_on() {
        let mut stream = AnthropicStream::default();
        for event in [
            r#"{"type":"content_block_start","index":0,"content_block":{"type":"server_tool_use","id":"s","name":"web_search","input":{"query":"x"}}}"#,
            r#"{"type":"content_block_stop","index":0}"#,
            r#"{"type":"content_block_start","index":1,"content_block":{"type":"web_search_tool_result","tool_use_id":"s","content":{"type":"web_search_tool_result_error","error_code":"max_uses_exceeded"}}}"#,
        ] {
            stream.event(event).unwrap();
        }
        assert_eq!(stream.found.searches, [("x".to_string(), 0)]);
        assert!(stream.found.sources.is_empty());
    }

    #[test]
    fn openais_answer_gives_text_searches_and_cited_pages() {
        let response = json!({
            "id": "resp_1",
            "status": "completed",
            "output": [
                {"type": "web_search_call", "id": "ws_1", "status": "completed",
                 "action": {"type": "search", "query": "filing fee ohio",
                            "sources": [{"type": "url", "url": "https://a.example/"},
                                        {"type": "url", "url": "https://b.example/"}]}},
                {"type": "web_search_call", "id": "ws_2", "status": "completed",
                 "action": {"type": "open_page", "url": "https://a.example/"}},
                {"type": "message", "id": "msg_1", "status": "completed", "role": "assistant",
                 "content": [{"type": "output_text", "text": "It's $250.",
                   "annotations": [{"type": "url_citation", "start_index": 0, "end_index": 9,
                                    "url": "https://a.example/", "title": "Fees"}]}]}
            ]
        });
        let found = parse_openai(&response);
        assert_eq!(found.text.as_deref(), Some("It's $250."));
        assert_eq!(found.searches, [("filing fee ohio".to_string(), 2)]);
        assert_eq!(
            found.sources,
            [WebSource {
                title: "Fees".into(),
                url: "https://a.example/".into()
            }]
        );
    }
}
