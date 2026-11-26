//! Kusema web search through Brave Search (the user's own key) or a SearXNG
//! server (the user's or the firm's). Titles, addresses and snippets only:
//! never the pages themselves. At most `MAX_RESULTS` per search.
//!
//! Brave Search API, checked 2026-11-26 at
//! https://api-dashboard.search.brave.com/app/documentation/web-search/get-started:
//! GET https://api.search.brave.com/res/v1/web/search, the key in the
//! `X-Subscription-Token` header, `q` and `count` (max 20); results in
//! `web.results[]` with `title`, `url` and `description`.
//!
//! SearXNG search API, https://docs.searxng.org/dev/search_api.html:
//! GET <server>/search?q=…&format=json (the server must list `json` under
//! `search: formats:`, or it answers 403); results in `results[]` with
//! `title`, `url` and `content` (searx/webutils.py).

use crate::network::{self, DataKind, Purpose, RequestResult, WebKind};
use serde::Deserialize;
use std::time::Duration;

/// Results kept from each search.
pub const MAX_RESULTS: usize = 5;
/// Searches for one question.
pub const MAX_SEARCHES: usize = 3;
const TIMEOUT: Duration = Duration::from_secs(15);

pub const BRAVE_URL: &str = "https://api.search.brave.com/res/v1/web/search";

/// One result: what the AI sees, and what the answer's sources list shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchResult {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

/// Where searches go.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Engine {
    Brave { endpoint: String, key: String },
    Searxng { server: String },
}

impl Engine {
    pub fn brave(key: &str) -> Self {
        // Debug builds: a stand-in server for isolated test runs.
        #[cfg(debug_assertions)]
        if let Ok(endpoint) = std::env::var("KUSEMA_DEBUG_BRAVE_URL") {
            return Engine::Brave {
                endpoint,
                key: key.to_string(),
            };
        }
        Engine::Brave {
            endpoint: BRAVE_URL.to_string(),
            key: key.to_string(),
        }
    }

    /// As the ledger and the answer show it.
    pub fn label(&self) -> String {
        match self {
            Engine::Brave { .. } => "Brave Search".to_string(),
            Engine::Searxng { server } => {
                let host = reqwest::Url::parse(server)
                    .ok()
                    .and_then(|u| u.host_str().map(str::to_string))
                    .unwrap_or_else(|| server.clone());
                format!("SearXNG ({host})")
            }
        }
    }
}

#[derive(Deserialize)]
struct BraveResponse {
    #[serde(default)]
    web: Option<BraveWeb>,
}

#[derive(Deserialize)]
struct BraveWeb {
    #[serde(default)]
    results: Vec<BraveResult>,
}

#[derive(Deserialize)]
struct BraveResult {
    #[serde(default)]
    title: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    description: String,
}

#[derive(Deserialize)]
struct SearxngResponse {
    #[serde(default)]
    results: Vec<SearxngResult>,
}

#[derive(Deserialize)]
struct SearxngResult {
    #[serde(default)]
    title: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    content: String,
}

/// Brave marks matched words with <strong>; the AI needs only the words.
fn plain(text: &str) -> String {
    let tags = regex::Regex::new(r"</?[a-zA-Z][^>]*>").expect("valid regex");
    let text = tags.replace_all(text, "");
    text.replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&#x27;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", " ")
}

/// Keeps http(s) results with an address, at most `MAX_RESULTS`.
fn keep(results: impl Iterator<Item = SearchResult>) -> Vec<SearchResult> {
    results
        .filter(|r| r.url.starts_with("https://") || r.url.starts_with("http://"))
        .take(MAX_RESULTS)
        .collect()
}

pub fn parse_brave(body: &str) -> Result<Vec<SearchResult>, String> {
    let response: BraveResponse = serde_json::from_str(body).map_err(|e| {
        format!(
            "Brave sent an answer Kusema couldn't read ({e}): {}",
            redact(body)
        )
    })?;
    Ok(keep(
        response
            .web
            .map(|w| w.results)
            .unwrap_or_default()
            .into_iter()
            .map(|r| SearchResult {
                title: plain(&r.title),
                url: r.url,
                snippet: plain(&r.description),
            }),
    ))
}

pub fn parse_searxng(body: &str) -> Result<Vec<SearchResult>, String> {
    let response: SearxngResponse =
        serde_json::from_str(body).map_err(|e| format!("unexpected answer: {e}"))?;
    Ok(keep(response.results.into_iter().map(|r| SearchResult {
        title: plain(&r.title),
        url: r.url,
        snippet: plain(&r.content),
    })))
}

/// The request for `query`, before it goes through the network gate.
fn request(engine: &Engine, query: &str) -> Result<reqwest::RequestBuilder, String> {
    let (url, purpose_url) = match engine {
        Engine::Brave { endpoint, .. } => (endpoint.clone(), endpoint.clone()),
        Engine::Searxng { server } => (
            format!("{}/search", server.trim_end_matches('/')),
            server.clone(),
        ),
    };
    let client = network::client_builder(Purpose::WebSearch, &purpose_url)
        .timeout(TIMEOUT)
        .connect_timeout(TIMEOUT)
        .user_agent(concat!("Kusema/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| e.to_string())?;
    Ok(match engine {
        Engine::Brave { key, .. } => client
            .get(&url)
            .header("Accept", "application/json")
            .header("X-Subscription-Token", key)
            .query(&[
                ("q", query),
                ("count", &MAX_RESULTS.to_string()),
                ("safesearch", "moderate"),
            ]),
        Engine::Searxng { .. } => client
            .get(&url)
            .header("Accept", "application/json")
            .query(&[("q", query), ("format", "json"), ("safesearch", "1")]),
    })
}

/// Starts the error for a key Brave refused, so the assistant can say so plainly.
pub const BRAVE_KEY_REJECTED: &str = "Brave rejected the key";

/// Long runs of letters and digits (keys, tokens, ids) in text from a server,
/// replaced before it's shown or logged.
fn redact(text: &str) -> String {
    // Error codes ("RATE_LIMITED") are capitals and underscores; anything
    // else that long could be a key or an id.
    let long = regex::Regex::new(r"[A-Za-z0-9_\-]{20,}").expect("valid regex");
    let text = long.replace_all(text, |caps: &regex::Captures| {
        let run = &caps[0];
        if run.chars().all(|c| c.is_ascii_uppercase() || c == '_') {
            run.to_string()
        } else {
            "[redacted]".to_string()
        }
    });
    let text: String = text.chars().take(300).collect();
    text.trim().to_string()
}

/// Brave's own words about an error: its code and detail, when the body is
/// JSON (`{"error": {"code", "detail"}}`, or a top-level `message`), else the
/// start of the body. Redacted.
pub fn brave_error_detail(body: &str) -> Option<String> {
    let value: serde_json::Value = match serde_json::from_str(body) {
        Ok(value) => value,
        Err(_) => {
            let text = redact(body);
            return (!text.is_empty()).then_some(text);
        }
    };
    let error = value.get("error").unwrap_or(&value);
    let code = error["code"].as_str().unwrap_or_default();
    let detail = error["detail"]
        .as_str()
        .or_else(|| error["message"].as_str())
        .or_else(|| value["message"].as_str())
        .unwrap_or_default();
    let text = match (code.is_empty(), detail.is_empty()) {
        (true, true) => return None,
        (false, true) => code.to_string(),
        (true, false) => detail.to_string(),
        (false, false) => format!("{code}: {detail}"),
    };
    Some(redact(&text))
}

/// Whether Brave's answer means the key itself is the problem: unknown,
/// revoked, or not subscribed to a plan that includes web search.
fn key_problem(status: u16, detail: &str) -> bool {
    let detail = detail.to_ascii_lowercase();
    matches!(status, 401 | 403)
        || detail.contains("subscription")
        || detail.contains("token")
        || detail.contains("api key")
        || detail.contains("not in plan")
        || detail.contains("not_in_plan")
}

fn describe_status(engine: &Engine, status: reqwest::StatusCode, body: &str) -> String {
    let code = status.as_u16();
    match engine {
        Engine::Brave { .. } => {
            let detail = brave_error_detail(body).unwrap_or_default();
            let said = if detail.is_empty() {
                format!("HTTP {code}")
            } else {
                format!("HTTP {code}, {detail}")
            };
            if key_problem(code, &detail) {
                format!("{BRAVE_KEY_REJECTED}. Check that it's subscribed to a Search plan. (Brave: {said})")
            } else if code == 402 {
                format!("the Brave Search plan has no credit left (Brave: {said})")
            } else if code == 429 {
                format!("Brave says too many searches; wait a moment (Brave: {said})")
            } else {
                format!("Brave answered {said}")
            }
        }
        Engine::Searxng { .. } if code == 403 => {
            "the SearXNG server doesn't allow JSON results (add json under search: formats: in its settings.yml)".to_string()
        }
        Engine::Searxng { .. } if code == 429 => "too many searches; wait a moment".to_string(),
        Engine::Searxng { .. } => format!("the search server answered {code}"),
    }
}

/// Searches for `query`, listing the search in the ledger with how it went.
pub async fn search(engine: &Engine, query: &str) -> Result<Vec<SearchResult>, String> {
    let label = engine.label();
    let id = network::record_web(WebKind::Search, &label, query, RequestResult::Sent);
    let outcome = async {
        let response = network::send(Purpose::WebSearch, DataKind::Text, request(engine, query)?)
            .await
            .map_err(|e| e.to_string())?;
        let status = response.status();
        let body = response.text().await.map_err(|e| e.to_string())?;
        if !status.is_success() {
            let error = describe_status(engine, status, &body);
            log::warn!("Web search failed ({}): {error}", engine.label());
            return Err(error);
        }
        match engine {
            Engine::Brave { .. } => parse_brave(&body),
            Engine::Searxng { .. } => parse_searxng(&body),
        }
    }
    .await;
    match &outcome {
        Ok(results) => network::finish_web(id, RequestResult::Sent, Some(results.len()), None),
        Err(e) => {
            let result = if network::is_blocked(e) || network::is_blocked_by_admin(e) {
                RequestResult::Blocked
            } else {
                RequestResult::Failed
            };
            network::finish_web(id, result, Some(0), None);
        }
    }
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;

    fn brave_body(n: usize) -> String {
        let results: Vec<String> = (0..n)
            .map(|i| {
                format!(
                    r#"{{"title":"Result <strong>{i}</strong>","url":"https://r{i}.example/","description":"Snippet &amp; more {i}","extra_snippets":["x"]}}"#
                )
            })
            .collect();
        format!(
            r#"{{"type":"search","web":{{"type":"search","results":[{}]}}}}"#,
            results.join(",")
        )
    }

    #[test]
    fn brave_results_are_capped_at_five_with_plain_text() {
        let results = parse_brave(&brave_body(12)).unwrap();
        assert_eq!(results.len(), MAX_RESULTS);
        assert_eq!(results[0].title, "Result 0");
        assert_eq!(results[0].snippet, "Snippet & more 0");
        assert_eq!(results[4].url, "https://r4.example/");
        assert!(parse_brave(r#"{"type":"search"}"#).unwrap().is_empty());
        assert!(parse_brave("not json").is_err());
    }

    #[test]
    fn searxng_results_are_capped_and_odd_addresses_dropped() {
        let body = r#"{"query":"q","results":[
            {"title":"A","url":"https://a.example/","content":"a"},
            {"title":"Bad","url":"javascript:alert(1)","content":"b"},
            {"title":"B","url":"http://b.example/","content":"b"},
            {"title":"C","url":"https://c.example/","content":"c"},
            {"title":"D","url":"https://d.example/","content":"d"},
            {"title":"E","url":"https://e.example/","content":"e"},
            {"title":"F","url":"https://f.example/","content":"f"}
        ],"answers":[],"suggestions":[]}"#;
        let results = parse_searxng(body).unwrap();
        assert_eq!(results.len(), MAX_RESULTS);
        assert!(results.iter().all(|r| r.url.starts_with("http")));
        assert_eq!(results[1].title, "B");
    }

    /// A local server that answers one request, returning what it was asked.
    fn serve_once(
        status: &'static str,
        body: String,
    ) -> (String, std::sync::mpsc::Receiver<String>) {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buffer = [0u8; 8192];
                let n = stream.read(&mut buffer).unwrap_or(0);
                let _ = tx.send(String::from_utf8_lossy(&buffer[..n]).to_string());
                let response = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes());
            }
        });
        (format!("http://{address}"), rx)
    }

    #[test]
    fn a_brave_search_sends_the_key_and_asks_for_five_results() {
        let (server, asked) = serve_once("200 OK", brave_body(9));
        let engine = Engine::Brave {
            endpoint: format!("{server}/res/v1/web/search"),
            key: "test-key".into(),
        };
        let results = tauri::async_runtime::block_on(search(&engine, "filing fees")).unwrap();
        assert_eq!(results.len(), 5);
        let request = asked.recv().unwrap();
        assert!(request.starts_with("GET /res/v1/web/search?q=filing+fees&count=5"));
        assert!(request
            .to_ascii_lowercase()
            .contains("x-subscription-token: test-key"));
    }

    #[test]
    fn a_searxng_search_asks_for_json() {
        let (server, asked) = serve_once(
            "200 OK",
            r#"{"results":[{"title":"A","url":"https://a.example/","content":"x"}]}"#.into(),
        );
        let engine = Engine::Searxng { server };
        let results = tauri::async_runtime::block_on(search(&engine, "rule 26")).unwrap();
        assert_eq!(results.len(), 1);
        assert!(asked
            .recv()
            .unwrap()
            .starts_with("GET /search?q=rule+26&format=json"));
    }

    #[test]
    fn refusals_are_explained() {
        let (server, _) = serve_once("403 Forbidden", "{}".into());
        let engine = Engine::Searxng { server };
        let error = tauri::async_runtime::block_on(search(&engine, "x")).unwrap_err();
        assert!(error.contains("JSON"), "{error}");
        let (server, _) = serve_once("401 Unauthorized", "{}".into());
        let engine = Engine::Brave {
            endpoint: server,
            key: "bad".into(),
        };
        let error = tauri::async_runtime::block_on(search(&engine, "x")).unwrap_err();
        assert!(error.contains("key"), "{error}");
    }

    fn serve_brave_error(status: &'static str, body: &'static str) -> Engine {
        let (server, _) = serve_once(status, body.to_string());
        Engine::Brave {
            endpoint: server,
            key: "k".into(),
        }
    }

    #[test]
    fn a_rejected_brave_key_is_said_plainly_with_braves_own_words() {
        let engine = serve_brave_error(
            "422 Unprocessable Entity",
            r#"{"type":"ErrorResponse","error":{"id":"0f2a3b4c-aaaa-bbbb-cccc-1234567890ab","status":422,"code":"SUBSCRIPTION_TOKEN_INVALID","detail":"The provided subscription token is invalid.","meta":{"token":"notarealkey0123456789abcdefgh"}}}"#,
        );
        let error = tauri::async_runtime::block_on(search(&engine, "x")).unwrap_err();
        assert!(error.starts_with(BRAVE_KEY_REJECTED), "{error}");
        assert!(error.contains("subscribed to a Search plan"), "{error}");
        assert!(error.contains("SUBSCRIPTION_TOKEN_INVALID"), "{error}");
        assert!(error.contains("HTTP 422"), "{error}");
        // Nothing key-like from the body is repeated.
        assert!(!error.contains("notarealkey0123456789abcdefgh"));

        let engine = serve_brave_error("401 Unauthorized", "");
        let error = tauri::async_runtime::block_on(search(&engine, "x")).unwrap_err();
        assert!(error.starts_with(BRAVE_KEY_REJECTED), "{error}");
    }

    #[test]
    fn other_brave_errors_show_what_brave_said() {
        let engine = serve_brave_error(
            "429 Too Many Requests",
            r#"{"error":{"code":"RATE_LIMITED","detail":"Request rate limit exceeded for plan."}}"#,
        );
        let error = tauri::async_runtime::block_on(search(&engine, "x")).unwrap_err();
        assert!(
            error.contains("too many searches") && error.contains("RATE_LIMITED"),
            "{error}"
        );
        let engine = serve_brave_error("500 Internal Server Error", "oops");
        let error = tauri::async_runtime::block_on(search(&engine, "x")).unwrap_err();
        assert_eq!(error, "Brave answered HTTP 500, oops");
        // An answer that isn't the expected JSON says so, with its start.
        let engine = serve_brave_error("200 OK", "<html>maintenance</html>");
        let error = tauri::async_runtime::block_on(search(&engine, "x")).unwrap_err();
        assert!(
            error.contains("couldn't read") && error.contains("maintenance"),
            "{error}"
        );
    }

    #[test]
    fn server_text_is_redacted() {
        assert_eq!(
            redact("token fakekey-0123456789abcdefXYZ is bad"),
            "token [redacted] is bad"
        );
        assert_eq!(
            brave_error_detail(r#"{"message":"Invalid API key"}"#).as_deref(),
            Some("Invalid API key")
        );
        assert_eq!(brave_error_detail("{}"), None);
    }

    /// One real search with the key in `BRAVE_API_KEY`, exactly as Kusema
    /// sends it. Skipped (passes) when the variable isn't set; the key is
    /// never printed.
    #[test]
    fn live_brave_search_when_a_key_is_set() {
        let Ok(key) = std::env::var("BRAVE_API_KEY") else {
            eprintln!("BRAVE_API_KEY not set: live Brave test skipped");
            return;
        };
        let engine = Engine::Brave {
            endpoint: BRAVE_URL.to_string(),
            key: key.trim().to_string(),
        };
        match tauri::async_runtime::block_on(search(&engine, "Kusema live test search")) {
            Ok(results) => {
                eprintln!("Brave live search: {} results", results.len());
                assert!(results.len() <= MAX_RESULTS);
                assert!(results.iter().all(|r| r.url.starts_with("http")));
            }
            Err(error) => panic!("Brave live search failed: {error}"),
        }
    }

    #[test]
    fn engines_are_named_for_the_ledger() {
        assert_eq!(Engine::brave("k").label(), "Brave Search");
        assert_eq!(
            Engine::Searxng {
                server: "https://search.firm.example".into()
            }
            .label(),
            "SearXNG (search.firm.example)"
        );
    }
}
