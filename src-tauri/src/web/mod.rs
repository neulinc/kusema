//! Kusema web search and link reading, off by default.
//!
//! A question can bring in the web in three ways: the user asks ("search the
//! web for…", or the search toggle in the Ask box), the AI decides
//! (Automatic mode), or the user names a page to read. What then happens is
//! decided here, in `decide`, from the settings and the question alone, so
//! every rule can be tested:
//! - Web search off, the administrator's switch, or Privileged mode: nothing
//!   goes to the web. A question that asked for it says why.
//! - Brave Search or a SearXNG server: Kusema searches (at most 3 searches,
//!   5 results each, snippets only) and gives the AI the results as untrusted
//!   data (`untrusted`).
//! - The AI's built-in search (Claude or OpenAI): the provider searches. It
//!   chooses its own searches, so they can't be confirmed first or kept away
//!   from a screenshot or document in the question: with "Confirm searches"
//!   on, or private context in the question, Kusema uses Brave or SearXNG if
//!   set up, and otherwise asks.
//! - Private context (a screenshot, selected or document text): Kusema asks
//!   before searching, and the searches are made from the question alone.
//!
//! Every search and page read is listed in the privacy ledger.

pub mod admin;
pub mod builtin;
pub mod fetch;
pub mod flow;
pub mod intent;
pub mod search;
pub mod untrusted;

use crate::settings::{get_settings, write_settings, AppSettings};
use intent::Intent;
use search::Engine;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Emitter};

/// Where the Brave Search key is kept: beside the AI providers' keys, in the
/// OS keychain (Windows Credential Manager), never in the settings file.
pub const BRAVE_KEY_ID: &str = "brave_search";

/// A page an answer drew on, shown as a link under it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct WebSource {
    pub title: String,
    pub url: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum WebSearchMode {
    /// Only when the question asks ("search the web for…") or the search
    /// toggle is on.
    #[default]
    WhenAsked,
    /// The AI decides whether a question needs the web.
    Automatic,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum WebSearchProvider {
    /// Brave Search API with the user's own key.
    #[default]
    Brave,
    /// A SearXNG server (the user's, or the firm's).
    Searxng,
    /// The AI provider's own search: Claude or OpenAI.
    Builtin,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(default)]
pub struct WebSearchSettings {
    pub enabled: bool,
    pub mode: WebSearchMode,
    pub provider: WebSearchProvider,
    /// The user's SearXNG server; the administrator's, if set, wins.
    pub searxng_url: String,
    /// Show each Brave/SearXNG search, editable, before it's sent.
    pub confirm_searches: bool,
    /// "Paste the answer" adds a short sources list.
    pub paste_sources: bool,
}

impl Default for WebSearchSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            mode: WebSearchMode::WhenAsked,
            provider: WebSearchProvider::Brave,
            searxng_url: String::new(),
            confirm_searches: false,
            paste_sources: true,
        }
    }
}

/// What in a question isn't for the web.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum PrivateKind {
    /// A screenshot is attached, or the conversation is about one.
    Screenshot,
    /// Selected text is attached ("Kusema, what does this mean?").
    Selection,
}

/// AI providers with a built-in web search Kusema can use.
pub fn builtin_label(provider_id: &str) -> Option<&'static str> {
    match provider_id {
        "anthropic" => Some("Claude"),
        "openai" => Some("OpenAI"),
        _ => None,
    }
}

/// The SearXNG server in force: the administrator's, else the user's.
pub fn searxng_server(settings: &WebSearchSettings, policy: &admin::AdminPolicy) -> Option<String> {
    policy.searxng_url.clone().or_else(|| {
        let url = settings.searxng_url.trim().trim_end_matches('/');
        (url.starts_with("http://") || url.starts_with("https://")).then(|| url.to_string())
    })
}

/// The engine the user chose, if it's set up.
fn chosen_engine(
    web: &WebSearchSettings,
    policy: &admin::AdminPolicy,
    brave_key: &str,
) -> Option<Engine> {
    match web.provider {
        WebSearchProvider::Brave if !brave_key.trim().is_empty() => {
            Some(Engine::brave(brave_key.trim()))
        }
        WebSearchProvider::Searxng => {
            searxng_server(web, policy).map(|server| Engine::Searxng { server })
        }
        _ => None,
    }
}

/// Brave or SearXNG, whichever is set up, for when the built-in search can't
/// be used. The firm's SearXNG server comes first.
fn fallback_engine(
    web: &WebSearchSettings,
    policy: &admin::AdminPolicy,
    brave_key: &str,
) -> Option<Engine> {
    searxng_server(web, policy)
        .map(|server| Engine::Searxng { server })
        .or_else(|| (!brave_key.trim().is_empty()).then(|| Engine::brave(brave_key.trim())))
}

/// Why the built-in search needs the user's go-ahead.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum BuiltinConcern {
    /// "Confirm searches" is on, and it can't show its searches first.
    Unconfirmable,
    /// The question has private context the provider could search with.
    PrivateContext,
}

/// Why nothing goes to the web for a question that asked for it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refusal {
    Off,
    ByAdmin,
    Privileged,
    /// No search provider set up (no Brave key, no SearXNG server, and no
    /// AI with a built-in search).
    NoProvider,
}

/// What to do about the web for one question.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Decision {
    /// Nothing: answer as usual.
    None,
    /// The question asked for the web, which can't be used: say why.
    Refuse(Refusal),
    /// Read this page (`None`: the copied link).
    ReadLink(Option<String>),
    /// Search with `engine`. `queries` empty: the AI chooses them first
    /// (Automatic mode).
    Search {
        engine: Engine,
        queries: Vec<String>,
    },
    /// Let the AI provider search (Claude or OpenAI).
    Builtin,
    /// Only the built-in search is available and it can't honour the
    /// settings: ask before using it.
    AskBuiltin(BuiltinConcern),
}

/// Everything `decide` looks at.
pub struct Situation<'a> {
    pub web: &'a WebSearchSettings,
    pub policy: &'a admin::AdminPolicy,
    pub privileged: bool,
    pub brave_key: &'a str,
    /// The AI provider answering.
    pub ai_provider: &'a str,
    /// The question, without any private context attached to it.
    pub question: &'a str,
    /// The search toggle was on for this question.
    pub forced: bool,
    pub private: Option<PrivateKind>,
}

/// The rules, in one place.
pub fn decide(s: &Situation) -> Decision {
    let intent = if s.forced {
        match intent::detect(s.question) {
            Intent::None => Intent::Search(s.question.trim().to_string()),
            other => other,
        }
    } else {
        intent::detect(s.question)
    };
    let asked = intent != Intent::None;
    let automatic = s.web.mode == WebSearchMode::Automatic;
    let refuse = |why: Refusal| {
        if asked {
            Decision::Refuse(why)
        } else {
            Decision::None
        }
    };
    if s.policy.disable_web_access {
        return refuse(Refusal::ByAdmin);
    }
    if !s.web.enabled {
        return refuse(Refusal::Off);
    }
    if s.privileged {
        return refuse(Refusal::Privileged);
    }
    let queries = match intent {
        Intent::ReadLink(url) => return Decision::ReadLink(url),
        Intent::Search(query) => vec![query],
        Intent::None if automatic => Vec::new(),
        Intent::None => return Decision::None,
    };
    let search = |engine: Engine| Decision::Search {
        engine,
        queries: queries.clone(),
    };
    if s.web.provider == WebSearchProvider::Builtin {
        if builtin_label(s.ai_provider).is_none() {
            return match fallback_engine(s.web, s.policy, s.brave_key) {
                Some(engine) => search(engine),
                None => refuse(Refusal::NoProvider),
            };
        }
        let concern = if s.private.is_some() {
            Some(BuiltinConcern::PrivateContext)
        } else if s.web.confirm_searches {
            Some(BuiltinConcern::Unconfirmable)
        } else {
            None
        };
        return match (concern, fallback_engine(s.web, s.policy, s.brave_key)) {
            (None, _) => Decision::Builtin,
            (Some(_), Some(engine)) => search(engine),
            (Some(concern), None) => Decision::AskBuiltin(concern),
        };
    }
    match chosen_engine(s.web, s.policy, s.brave_key) {
        Some(engine) => search(engine),
        None => refuse(Refusal::NoProvider),
    }
}

/// The system prompt for choosing searches (Automatic mode with Brave or
/// SearXNG). The AI sees the question alone, never private context.
pub const PLANNER_PROMPT: &str = "You decide whether a web search would help answer the user's question. Reply with JSON only, no other text: {\"searches\": [\"search text\", ...]}. Give at most 3 short searches, each a few keywords. Give an empty list when the question doesn't need current or outside information: greetings, writing or editing help, opinions, maths, general knowledge you're sure of, or questions about the user's own text.";

/// The searches the AI chose, at most `MAX_SEARCHES`. Anything unreadable
/// means no search.
pub fn parse_planned(reply: &str) -> Vec<String> {
    #[derive(Deserialize)]
    struct Planned {
        #[serde(default)]
        searches: Vec<String>,
    }
    let (Some(start), Some(end)) = (reply.find('{'), reply.rfind('}')) else {
        return Vec::new();
    };
    if end < start {
        return Vec::new();
    }
    serde_json::from_str::<Planned>(&reply[start..=end])
        .map(|p| p.searches)
        .unwrap_or_default()
        .into_iter()
        .map(|q| q.trim().chars().take(200).collect::<String>())
        .filter(|q| !q.is_empty())
        .take(search::MAX_SEARCHES)
        .collect()
}

/// Queries as the user left them on the confirmation card: trimmed, no
/// blanks, at most `MAX_SEARCHES`.
pub fn clean_queries(queries: Vec<String>) -> Vec<String> {
    queries
        .into_iter()
        .map(|q| q.trim().chars().take(200).collect::<String>())
        .filter(|q| !q.is_empty())
        .take(search::MAX_SEARCHES)
        .collect()
}

fn web_settings_changed(app: &AppHandle) {
    let _ = app.emit(
        "settings-changed",
        serde_json::json!({ "setting": "web_search" }),
    );
}

#[tauri::command]
#[specta::specta]
pub fn change_web_search_settings(app: AppHandle, web: WebSearchSettings) -> Result<(), String> {
    let mut settings = get_settings(&app);
    settings.web_search = web;
    write_settings(&app, settings);
    web_settings_changed(&app);
    Ok(())
}

/// Saves the Brave Search key (to the keychain, like the AI providers' keys).
#[tauri::command]
#[specta::specta]
pub fn set_brave_search_key(app: AppHandle, key: String) -> Result<(), String> {
    let mut settings = get_settings(&app);
    settings
        .post_process_api_keys
        .insert(BRAVE_KEY_ID.to_string(), key.trim().to_string());
    write_settings(&app, settings);
    web_settings_changed(&app);
    Ok(())
}

/// Whether a Brave Search key is saved. The key itself never goes to the page.
#[tauri::command]
#[specta::specta]
pub fn has_brave_search_key(app: AppHandle) -> bool {
    brave_key(&get_settings(&app)).is_some_and(|k| !k.is_empty())
}

pub fn brave_key(settings: &AppSettings) -> Option<String> {
    settings
        .post_process_api_keys
        .get(BRAVE_KEY_ID)
        .map(|k| k.trim().to_string())
}

/// "Test" beside the search settings: one real search, listed in the ledger
/// like any other. Returns how many results came back.
#[tauri::command]
#[specta::specta]
pub async fn test_web_search(app: AppHandle) -> Result<u32, String> {
    let settings = get_settings(&app);
    let policy = admin::policy();
    if policy.disable_web_access {
        return Err("Web access is turned off on this computer by your administrator.".into());
    }
    if settings.privileged_mode {
        return Err("Privileged mode is on, so Kusema doesn't search the web.".into());
    }
    let key = brave_key(&settings).unwrap_or_default();
    let engine = chosen_engine(&settings.web_search, &policy, &key)
        .ok_or("Add a Brave Search key or a SearXNG server first.")?;
    let results = search::search(&engine, "Kusema test search").await?;
    Ok(results.len() as u32)
}

/// Opens a source's link in the browser. Only http(s) addresses, and only
/// ones an answer listed (the page can't open anything else through this).
#[tauri::command]
#[specta::specta]
pub fn open_web_source(
    app: AppHandle,
    state: tauri::State<'_, crate::assistant::AssistantState>,
    url: String,
) -> Result<(), String> {
    let parsed = reqwest::Url::parse(&url).map_err(|e| e.to_string())?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err("Only web links can be opened.".into());
    }
    if !crate::assistant::is_listed_source(&state, &url) {
        return Err("That link isn't one of the answer's sources.".into());
    }
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_url(parsed.as_str(), None::<&str>)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn web(enabled: bool) -> WebSearchSettings {
        WebSearchSettings {
            enabled,
            ..Default::default()
        }
    }

    struct Case {
        web: WebSearchSettings,
        policy: admin::AdminPolicy,
        privileged: bool,
        brave_key: &'static str,
        ai: &'static str,
        question: &'static str,
        forced: bool,
        private: Option<PrivateKind>,
    }

    impl Case {
        fn new(question: &'static str) -> Self {
            Case {
                web: web(true),
                policy: admin::AdminPolicy::default(),
                privileged: false,
                brave_key: "key",
                ai: "anthropic",
                question,
                forced: false,
                private: None,
            }
        }

        fn decide(&self) -> Decision {
            decide(&Situation {
                web: &self.web,
                policy: &self.policy,
                privileged: self.privileged,
                brave_key: self.brave_key,
                ai_provider: self.ai,
                question: self.question,
                forced: self.forced,
                private: self.private,
            })
        }
    }

    fn brave_search(query: &str) -> Decision {
        Decision::Search {
            engine: Engine::brave("key"),
            queries: vec![query.to_string()],
        }
    }

    #[test]
    fn off_by_default() {
        assert!(!WebSearchSettings::default().enabled);
        assert_eq!(WebSearchSettings::default().mode, WebSearchMode::WhenAsked);
        assert!(!WebSearchSettings::default().confirm_searches);
        assert!(WebSearchSettings::default().paste_sources);
        let mut case = Case::new("Search the web for court holidays");
        case.web = WebSearchSettings::default();
        assert_eq!(case.decide(), Decision::Refuse(Refusal::Off));
        // A question that didn't ask just gets answered.
        case.question = "What is a tort?";
        assert_eq!(case.decide(), Decision::None);
    }

    #[test]
    fn only_when_asked_searches_only_when_asked() {
        let case = Case::new("What is a tort?");
        assert_eq!(case.decide(), Decision::None);
        let case = Case::new("Search the web for court holidays 2026");
        assert_eq!(case.decide(), brave_search("court holidays 2026"));
        // The toggle searches for the question itself.
        let mut case = Case::new("Latest filing fee in Ohio?");
        case.forced = true;
        assert_eq!(case.decide(), brave_search("Latest filing fee in Ohio?"));
    }

    #[test]
    fn automatic_lets_the_ai_choose() {
        let mut case = Case::new("What's new in the federal rules?");
        case.web.mode = WebSearchMode::Automatic;
        assert_eq!(
            case.decide(),
            Decision::Search {
                engine: Engine::brave("key"),
                queries: Vec::new()
            }
        );
        case.web.provider = WebSearchProvider::Builtin;
        assert_eq!(case.decide(), Decision::Builtin);
    }

    #[test]
    fn the_admin_switch_and_privileged_mode_stop_everything() {
        let mut case = Case::new("Search the web for x");
        case.policy.disable_web_access = true;
        assert_eq!(case.decide(), Decision::Refuse(Refusal::ByAdmin));
        case.question = "What does https://example.com say?";
        assert_eq!(case.decide(), Decision::Refuse(Refusal::ByAdmin));
        case.question = "Hello";
        case.web.mode = WebSearchMode::Automatic;
        assert_eq!(case.decide(), Decision::None);

        let mut case = Case::new("Search the web for x");
        case.privileged = true;
        assert_eq!(case.decide(), Decision::Refuse(Refusal::Privileged));
        case.question = "Summarize this link";
        assert_eq!(case.decide(), Decision::Refuse(Refusal::Privileged));
        case.web.provider = WebSearchProvider::Builtin;
        case.question = "Anything";
        case.web.mode = WebSearchMode::Automatic;
        assert_eq!(case.decide(), Decision::None);
    }

    #[test]
    fn links_are_read_whatever_the_search_provider() {
        let mut case = Case::new("Summarize https://example.com/a");
        case.brave_key = "";
        assert_eq!(
            case.decide(),
            Decision::ReadLink(Some("https://example.com/a".into()))
        );
        case.question = "Summarize this link";
        assert_eq!(case.decide(), Decision::ReadLink(None));
    }

    #[test]
    fn a_missing_search_provider_is_named() {
        let mut case = Case::new("Search the web for x");
        case.brave_key = "";
        assert_eq!(case.decide(), Decision::Refuse(Refusal::NoProvider));
        case.web.provider = WebSearchProvider::Searxng;
        assert_eq!(case.decide(), Decision::Refuse(Refusal::NoProvider));
        case.web.searxng_url = "https://search.example".into();
        assert_eq!(
            case.decide(),
            Decision::Search {
                engine: Engine::Searxng {
                    server: "https://search.example".into()
                },
                queries: vec!["x".into()]
            }
        );
    }

    #[test]
    fn the_firms_searxng_server_wins() {
        let mut case = Case::new("Search the web for x");
        case.web.provider = WebSearchProvider::Searxng;
        case.web.searxng_url = "https://mine.example".into();
        case.policy.searxng_url = Some("https://firm.example".into());
        assert_eq!(
            case.decide(),
            Decision::Search {
                engine: Engine::Searxng {
                    server: "https://firm.example".into()
                },
                queries: vec!["x".into()]
            }
        );
    }

    #[test]
    fn builtin_search_only_with_claude_or_openai() {
        let mut case = Case::new("Search the web for x");
        case.web.provider = WebSearchProvider::Builtin;
        for ai in ["anthropic", "openai"] {
            case.ai = ai;
            assert_eq!(case.decide(), Decision::Builtin, "{ai}");
        }
        // Other AIs: Brave or SearXNG if set up, else say what's missing.
        for ai in ["deepseek", "gemini", "ollama"] {
            case.ai = ai;
            case.brave_key = "key";
            assert_eq!(case.decide(), brave_search("x"), "{ai}");
            case.brave_key = "";
            assert_eq!(case.decide(), Decision::Refuse(Refusal::NoProvider), "{ai}");
        }
    }

    #[test]
    fn confirm_searches_falls_back_from_builtin_search_or_asks() {
        let mut case = Case::new("Search the web for x");
        case.web.provider = WebSearchProvider::Builtin;
        case.web.confirm_searches = true;
        assert_eq!(case.decide(), brave_search("x"));
        case.brave_key = "";
        assert_eq!(
            case.decide(),
            Decision::AskBuiltin(BuiltinConcern::Unconfirmable)
        );
        case.web.searxng_url = "https://s.example".into();
        assert!(matches!(
            case.decide(),
            Decision::Search {
                engine: Engine::Searxng { .. },
                ..
            }
        ));
    }

    #[test]
    fn private_context_keeps_builtin_search_away() {
        let mut case = Case::new("Search the web for x");
        case.web.provider = WebSearchProvider::Builtin;
        case.private = Some(PrivateKind::Screenshot);
        assert_eq!(case.decide(), brave_search("x"));
        case.brave_key = "";
        assert_eq!(
            case.decide(),
            Decision::AskBuiltin(BuiltinConcern::PrivateContext)
        );
    }

    #[test]
    fn planned_searches_are_capped_at_three() {
        assert_eq!(
            parse_planned(r#"{"searches": ["a", " b ", "", "c", "d"]}"#),
            ["a", "b", "c"]
        );
        assert_eq!(
            parse_planned("Sure! ```json\n{\"searches\": []}\n```"),
            Vec::<String>::new()
        );
        assert!(parse_planned("I don't think so").is_empty());
        assert!(parse_planned("} {").is_empty());
        assert_eq!(
            clean_queries(vec![
                "  one ".into(),
                " ".into(),
                "two".into(),
                "3".into(),
                "4".into()
            ]),
            ["one", "two", "3"]
        );
    }
}
