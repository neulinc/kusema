//! Kusema web search, for one question: carries out what `decide` chose.
//! Asks the AI which searches to run (Automatic mode, from the question
//! alone), shows the card when the searches must be confirmed or the
//! question has private context, then searches or reads the page.

use super::{
    admin, brave_key, clean_queries, decide, fetch, parse_planned, search, untrusted,
    BuiltinConcern, Decision, PrivateKind, Refusal, Situation, WebSource, PLANNER_PROMPT,
};
use crate::assistant::failure::Failure;
use crate::assistant::Target;
use crate::llm_client::{self, ChatRole, ChatTurn};
use crate::screen::card::{self, ScreenCard, WebCardAction};
use crate::screen::CapturedWindow;
use crate::settings::AppSettings;
use log::{info, warn};
use tauri::AppHandle;

/// What the question gets from the web.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Prepared {
    /// Nothing: ask as usual.
    None,
    /// Results or a page, to add to the question as untrusted data.
    Context {
        block: String,
        sources: Vec<WebSource>,
    },
    /// Ask with the provider's own search.
    Builtin,
    /// Don't ask: say why (`Some`), or the user cancelled (`None`).
    Stop(Option<Failure>),
}

impl From<Refusal> for Failure {
    fn from(refusal: Refusal) -> Self {
        match refusal {
            Refusal::Off => Failure::WebOff,
            Refusal::ByAdmin => Failure::WebByAdmin,
            Refusal::Privileged => Failure::WebPrivileged,
            Refusal::NoProvider => Failure::NoSearchProvider,
        }
    }
}

/// The searches the AI suggests for `question` (Automatic mode). It sees the
/// question alone. Any failure means no search: the question is still answered.
async fn plan(target: &Target, question: &str) -> Vec<String> {
    let turns = [ChatTurn::new(ChatRole::User, question)];
    match llm_client::send_conversation(
        &target.provider,
        target.api_key.clone(),
        &target.model,
        PLANNER_PROMPT,
        &turns,
        |_| {},
    )
    .await
    {
        Ok(Some(reply)) => {
            let planned = parse_planned(&crate::actions::strip_think_block(&reply));
            info!("Web search: {} search(es) chosen", planned.len());
            planned
        }
        Ok(None) => Vec::new(),
        Err(e) => {
            warn!("Web search: couldn't choose searches ({e}); answering without");
            Vec::new()
        }
    }
}

fn web_card(
    action: WebCardAction,
    queries: Vec<String>,
    editable: bool,
    url: Option<String>,
    provider: String,
    private: Option<PrivateKind>,
    concern: Option<BuiltinConcern>,
) -> ScreenCard {
    ScreenCard::Web {
        action,
        queries,
        editable,
        url,
        provider,
        private,
        concern,
        timeout_secs: card::CONFIRM_TIMEOUT.as_secs() as u32,
    }
}

/// Gets what `question` needs from the web. `question` is the user's own
/// words, without any selection or document attached; `private` says what
/// else the question carries; `forced`: the search toggle was on.
#[allow(clippy::too_many_arguments)]
pub async fn prepare<C: Fn() -> bool>(
    app: &AppHandle,
    settings: &AppSettings,
    target: &Target,
    question: &str,
    forced: bool,
    private: Option<PrivateKind>,
    near: Option<CapturedWindow>,
    is_cancelled: &C,
) -> Prepared {
    let policy = admin::policy();
    let key = brave_key(settings).unwrap_or_default();
    let decision = decide(&Situation {
        web: &settings.web_search,
        policy: &policy,
        privileged: settings.privileged_mode,
        brave_key: &key,
        ai_provider: &target.provider.id,
        question,
        forced,
        private,
    });
    match decision {
        Decision::None => Prepared::None,
        Decision::Refuse(refusal) => Prepared::Stop(Some(refusal.into())),
        Decision::Builtin => Prepared::Builtin,
        Decision::AskBuiltin(concern) => {
            let label = super::builtin_label(&target.provider.id)
                .unwrap_or("The AI")
                .to_string();
            let card = web_card(
                WebCardAction::Builtin,
                Vec::new(),
                false,
                None,
                label,
                private,
                Some(concern),
            );
            match card::confirm_web(app, near, card, is_cancelled).await {
                Some(_) => Prepared::Builtin,
                None => Prepared::Stop(None),
            }
        }
        Decision::ReadLink(url) => {
            let url = match url {
                Some(url) => url,
                None => match crate::clipboard::copied_link(app) {
                    Ok(Some(url)) => url,
                    Ok(None) => return Prepared::Stop(Some(Failure::NoLink)),
                    Err(e) => return Prepared::Stop(Some(Failure::LinkFailed(e))),
                },
            };
            if private.is_some() {
                let card = web_card(
                    WebCardAction::ReadLink,
                    Vec::new(),
                    false,
                    Some(url.clone()),
                    String::new(),
                    private,
                    None,
                );
                if card::confirm_web(app, near, card, is_cancelled)
                    .await
                    .is_none()
                {
                    return Prepared::Stop(None);
                }
            }
            match fetch::read(&url).await {
                Ok(page) => {
                    let (block, sources) =
                        untrusted::wrap_page(&page.url, &page.title, &page.text, page.truncated);
                    Prepared::Context { block, sources }
                }
                Err(e) if crate::network::is_blocked(&e) => {
                    Prepared::Stop(Some(Failure::WebPrivileged))
                }
                Err(e) if crate::network::is_blocked_by_admin(&e) => {
                    Prepared::Stop(Some(Failure::WebByAdmin))
                }
                Err(e) => Prepared::Stop(Some(Failure::LinkFailed(e))),
            }
        }
        Decision::Search { engine, queries } => {
            let mut queries = queries;
            if queries.is_empty() {
                queries = plan(target, question).await;
                if queries.is_empty() || is_cancelled() {
                    return Prepared::None;
                }
            }
            let confirm = settings.web_search.confirm_searches;
            if confirm || private.is_some() {
                let card = web_card(
                    WebCardAction::Search,
                    queries.clone(),
                    confirm,
                    None,
                    engine.label(),
                    private,
                    None,
                );
                match card::confirm_web(app, near, card, is_cancelled).await {
                    Some(edited) if confirm => queries = clean_queries(edited),
                    Some(_) => {}
                    None => return Prepared::Stop(None),
                }
                if queries.is_empty() {
                    return Prepared::Stop(None);
                }
            }
            run_searches(&engine, queries, is_cancelled).await
        }
    }
}

async fn run_searches<C: Fn() -> bool>(
    engine: &search::Engine,
    queries: Vec<String>,
    is_cancelled: &C,
) -> Prepared {
    let label = engine.label();
    let mut done: Vec<(String, Vec<search::SearchResult>)> = Vec::new();
    let mut last_error = None;
    for query in queries.into_iter().take(search::MAX_SEARCHES) {
        if is_cancelled() {
            return Prepared::Stop(None);
        }
        match search::search(engine, &query).await {
            Ok(results) => done.push((query, results)),
            Err(e) if crate::network::is_blocked(&e) => {
                return Prepared::Stop(Some(Failure::WebPrivileged))
            }
            Err(e) if crate::network::is_blocked_by_admin(&e) => {
                return Prepared::Stop(Some(Failure::WebByAdmin))
            }
            Err(e) if e.starts_with(search::BRAVE_KEY_REJECTED) => {
                warn!("Web search failed: {e}");
                return Prepared::Stop(Some(Failure::BraveKeyRejected));
            }
            Err(e) => {
                warn!("Web search failed: {e}");
                last_error = Some(e);
            }
        }
    }
    if done.is_empty() {
        let error = last_error.unwrap_or_else(|| "no search ran".to_string());
        return Prepared::Stop(Some(Failure::SearchFailed(format!("{label}: {error}"))));
    }
    let searched: Vec<untrusted::Searched> = done
        .iter()
        .map(|(query, results)| untrusted::Searched {
            query,
            provider: &label,
            results,
        })
        .collect();
    let (block, sources) = untrusted::wrap_searches(&searched);
    Prepared::Context { block, sources }
}
