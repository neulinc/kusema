//! The single way out of Kusema. Every HTTP request in Rust goes through here,
//! which records it in the privacy ledger and enforces Privileged mode. A test
//! below scans the source so a new client can't appear elsewhere unnoticed.
//!
//! Two kinds of request can't be routed because a library makes them itself:
//! `hf_hub` model downloads and the updater plugin. They're logged and allowed
//! or refused at the last point before the library takes over (`announce`).
//!
//! This fails closed: if the gate isn't set up, nothing is sent.

mod ledger;
mod policy;
mod privileged;

pub use ledger::{DataKind, DestinationCount, LedgerEntry, LedgerFilter, Purpose, RequestResult};
pub use policy::Rules;
pub use privileged::*;

use std::fmt;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, OnceLock};

use log::{error, warn};
use reqwest::redirect;
use tauri::{AppHandle, Emitter};

pub use ledger::Ledger;
use policy::{classify, decide, Category, Reach, Verdict};

/// Starts every "couldn't send" error that means policy, not failure, so
/// callers can tell them apart through the string errors `llm_client` returns.
const BLOCKED_MARKER: &str = "Blocked by Privileged mode: ";
const PRUNE_EVERY_SECS: i64 = 60 * 60;
const SECS_PER_DAY: i64 = 24 * 60 * 60;

impl Purpose {
    fn category(self) -> Category {
        match self {
            Purpose::AssistantQuestion
            | Purpose::ScreenQuestion
            | Purpose::PostProcessing
            | Purpose::VoiceEdit
            | Purpose::ProviderModels => Category::Ai,
            Purpose::ModelDownload | Purpose::VoiceDownload | Purpose::UpdateCheck => {
                Category::Download
            }
        }
    }
}

/// What the gate needs to know, read fresh for each request.
#[derive(Clone, Copy, Debug)]
pub struct GateConfig {
    pub rules: Rules,
    pub retention_days: u32,
}

/// Enforces and records. Separate from the app so tests can drive it.
pub struct Gate {
    config: Box<dyn Fn() -> GateConfig + Send + Sync>,
    ledger: Option<Ledger>,
    on_change: Box<dyn Fn() + Send + Sync>,
    last_prune: AtomicI64,
}

/// A request Privileged mode refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Blocked {
    pub host: String,
    pub verdict: BlockReason,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockReason {
    CloudAi,
    LanAi,
    Download,
    /// The gate was never set up. Should not happen; nothing is sent if it does.
    NotReady,
}

impl fmt::Display for Blocked {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.verdict {
            BlockReason::Download => write!(
                f,
                "{BLOCKED_MARKER}downloads and update checks are blocked, so nothing was requested from {}.",
                self.host
            ),
            BlockReason::NotReady => write!(f, "{BLOCKED_MARKER}not ready yet."),
            _ => write!(f, "{BLOCKED_MARKER}nothing was sent to {}.", self.host),
        }
    }
}

impl std::error::Error for Blocked {}

/// Why a request didn't produce a response.
#[derive(Debug)]
pub enum SendError {
    Blocked(Blocked),
    Http(reqwest::Error),
}

impl fmt::Display for SendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SendError::Blocked(blocked) => blocked.fmt(f),
            SendError::Http(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for SendError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            SendError::Blocked(blocked) => Some(blocked),
            SendError::Http(error) => Some(error),
        }
    }
}

/// Whether an error from `llm_client` means Privileged mode refused the request.
pub fn is_blocked(error: &str) -> bool {
    error.starts_with(BLOCKED_MARKER)
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

impl Gate {
    pub fn new(
        config: impl Fn() -> GateConfig + Send + Sync + 'static,
        ledger: Option<Ledger>,
        on_change: impl Fn() + Send + Sync + 'static,
    ) -> Self {
        Self {
            config: Box::new(config),
            ledger,
            on_change: Box::new(on_change),
            last_prune: AtomicI64::new(0),
        }
    }

    /// The rule for this request, without recording anything.
    fn verdict(&self, purpose: Purpose, host: &str) -> Option<BlockReason> {
        match decide((self.config)().rules, purpose.category(), classify(host)) {
            Verdict::Allow => None,
            Verdict::BlockCloudAi => Some(BlockReason::CloudAi),
            Verdict::BlockLanAi => Some(BlockReason::LanAi),
            Verdict::BlockDownload => Some(BlockReason::Download),
        }
    }

    fn record(
        &self,
        host: &str,
        purpose: Purpose,
        kind: DataKind,
        bytes: usize,
        result: RequestResult,
    ) -> Option<i64> {
        let ledger = self.ledger.as_ref()?;
        let id = match ledger.record(now(), host, purpose, kind, bytes, result) {
            Ok(id) => Some(id),
            Err(e) => {
                error!("Couldn't record a request in the privacy ledger: {e}");
                None
            }
        };
        self.prune_if_due();
        (self.on_change)();
        id
    }

    fn mark_failed(&self, id: Option<i64>) {
        if let (Some(ledger), Some(id)) = (self.ledger.as_ref(), id) {
            if let Err(e) = ledger.mark_failed(id) {
                error!("Couldn't update the privacy ledger: {e}");
            }
            (self.on_change)();
        }
    }

    fn prune_if_due(&self) {
        let time = now();
        if time - self.last_prune.load(Ordering::Relaxed) >= PRUNE_EVERY_SECS {
            self.prune();
        }
    }

    fn prune(&self) {
        self.last_prune.store(now(), Ordering::Relaxed);
        if let Some(ledger) = &self.ledger {
            let days = i64::from((self.config)().retention_days);
            if let Err(e) = ledger.prune(now() - days * SECS_PER_DAY) {
                warn!("Couldn't clean up the privacy ledger: {e}");
            }
        }
    }

    /// Allows or refuses a request, and records it either way. The record comes
    /// first, so a crash can't leave a request that went out unlisted.
    fn authorize(
        &self,
        purpose: Purpose,
        kind: DataKind,
        host: &str,
        bytes: usize,
    ) -> Result<Option<i64>, Blocked> {
        if let Some(verdict) = self.verdict(purpose, host) {
            self.record(host, purpose, kind, bytes, RequestResult::Blocked);
            return Err(Blocked {
                host: host.to_string(),
                verdict,
            });
        }
        Ok(self.record(host, purpose, kind, bytes, RequestResult::Sent))
    }
}

static GATE: OnceLock<Arc<Gate>> = OnceLock::new();

#[cfg(test)]
thread_local! {
    static TEST_GATE: std::cell::RefCell<Option<Arc<Gate>>> = const { std::cell::RefCell::new(None) };
}

/// Runs the rest of this test thread's requests through `gate`.
#[cfg(test)]
pub fn set_test_gate(gate: Arc<Gate>) {
    TEST_GATE.with(|slot| *slot.borrow_mut() = Some(gate));
}

fn gate() -> Option<Arc<Gate>> {
    #[cfg(test)]
    {
        // Tests that aren't about the gate get one that allows and records nothing.
        Some(
            TEST_GATE
                .with(|slot| slot.borrow().clone())
                .unwrap_or_else(|| {
                    Arc::new(Gate::new(
                        || GateConfig {
                            rules: Rules::default(),
                            retention_days: 30,
                        },
                        None,
                        || {},
                    ))
                }),
        )
    }
    #[cfg(not(test))]
    GATE.get().cloned()
}

fn not_ready(host: &str) -> Blocked {
    error!("A request to {host} was attempted before the network gate was set up");
    Blocked {
        host: host.to_string(),
        verdict: BlockReason::NotReady,
    }
}

/// The host of a URL, as the ledger shows it. Never the path or query.
fn host_of(url: &str) -> String {
    reqwest::Url::parse(url)
        .ok()
        .and_then(|url| url.host_str().map(str::to_string))
        .unwrap_or_default()
}

/// Sets up the gate. Call before anything can make a request.
pub fn init(app: &AppHandle) {
    let config_app = app.clone();
    let event_app = app.clone();
    let ledger = crate::portable::app_data_dir(app)
        .map_err(|e| e.to_string())
        .and_then(|dir| Ledger::open(&dir.join("privacy_ledger.db")).map_err(|e| e.to_string()));
    let ledger = match ledger {
        Ok(ledger) => Some(ledger),
        Err(e) => {
            error!("Couldn't open the privacy ledger: {e}");
            None
        }
    };
    let gate = Gate::new(
        move || {
            let settings = crate::settings::get_settings(&config_app);
            GateConfig {
                rules: Rules {
                    privileged: settings.privileged_mode,
                    allow_lan: settings.privileged_allow_lan,
                    block_downloads: settings.privileged_block_downloads,
                },
                retention_days: settings.ledger_retention_days,
            }
        },
        ledger,
        move || {
            let _ = event_app.emit("privacy-ledger-changed", ());
        },
    );
    gate.prune();
    let _ = GATE.set(Arc::new(gate));
}

/// Marks a redirect refused by policy, to tell it apart from a network error.
#[derive(Debug)]
struct RedirectBlocked(Blocked);

impl fmt::Display for RedirectBlocked {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl std::error::Error for RedirectBlocked {}

/// A client for requests to `url`, with two protections every request needs:
/// - Each redirect is checked like a request, so a local server can't bounce
///   text to the cloud (a 307 resends the body).
/// - No proxy for local addresses, where the system proxy would carry
///   "localhost" traffic off the computer.
pub fn client_builder(purpose: Purpose, url: &str) -> reqwest::ClientBuilder {
    let gate = gate();
    let policy = redirect::Policy::custom(move |attempt| {
        let host = attempt.url().host_str().unwrap_or_default().to_string();
        let Some(gate) = &gate else {
            return attempt.error(RedirectBlocked(not_ready(&host)));
        };
        if let Some(verdict) = gate.verdict(purpose, &host) {
            gate.record(&host, purpose, DataKind::None, 0, RequestResult::Blocked);
            return attempt.error(RedirectBlocked(Blocked { host, verdict }));
        }
        if attempt.previous().len() >= 10 {
            attempt.error("too many redirects")
        } else {
            attempt.follow()
        }
    });
    let builder = reqwest::Client::builder().redirect(policy);
    if classify(&host_of(url)) == Reach::Public {
        builder
    } else {
        builder.no_proxy()
    }
}

fn redirect_blocked(error: &reqwest::Error) -> Option<Blocked> {
    let mut source: Option<&(dyn std::error::Error + 'static)> = Some(error);
    while let Some(current) = source {
        if let Some(blocked) = current.downcast_ref::<RedirectBlocked>() {
            return Some(blocked.0.clone());
        }
        source = current.source();
    }
    None
}

/// Sends a request: the one place HTTP leaves Kusema. Records it, or refuses it.
pub async fn send(
    purpose: Purpose,
    kind: DataKind,
    request: reqwest::RequestBuilder,
) -> Result<reqwest::Response, SendError> {
    let (client, request) = request.build_split();
    let request = request.map_err(SendError::Http)?;
    let host = request.url().host_str().unwrap_or_default().to_string();
    let bytes = request
        .body()
        .and_then(reqwest::Body::as_bytes)
        .map_or(0, <[u8]>::len);

    let Some(gate) = gate() else {
        return Err(SendError::Blocked(not_ready(&host)));
    };
    let id = gate
        .authorize(purpose, kind, &host, bytes)
        .map_err(SendError::Blocked)?;
    match client.execute(request).await {
        Ok(response) => Ok(response),
        Err(error) => {
            gate.mark_failed(id);
            Err(match redirect_blocked(&error) {
                Some(blocked) => SendError::Blocked(blocked),
                None => SendError::Http(error),
            })
        }
    }
}

/// For a request a library will make itself: records it, or refuses it.
/// `url` is where it's going, as far as anyone here can know (the library may
/// follow redirects this can't see).
pub fn announce(purpose: Purpose, url: &str) -> Result<(), Blocked> {
    let host = host_of(url);
    let Some(gate) = gate() else {
        return Err(not_ready(&host));
    };
    gate.authorize(purpose, DataKind::None, &host, 0)
        .map(|_| ())
}

/// Whether a request would be refused right now. Records nothing: for skipping
/// steps that would be pointless (asking to confirm a screenshot that can't go).
pub fn would_block(purpose: Purpose, url: &str) -> bool {
    match gate() {
        Some(gate) => gate.verdict(purpose, &host_of(url)).is_some(),
        None => true,
    }
}

/// Whether `url` points at this computer, not a server elsewhere.
pub fn is_on_this_computer(url: &str) -> bool {
    classify(&host_of(url)) == Reach::Loopback
}

fn ledger() -> Result<Ledger, String> {
    gate()
        .and_then(|gate| gate.ledger.clone())
        .ok_or_else(|| "The privacy ledger isn't available.".to_string())
}

/// Applies the retention setting now, after it changed.
fn prune_now() {
    if let Some(gate) = gate() {
        gate.prune();
        (gate.on_change)();
    }
}

/// A ledger at `path`, for tests elsewhere that need to read what was recorded.
#[cfg(test)]
pub fn test_ledger(path: &std::path::Path) -> Ledger {
    Ledger::open(path).unwrap()
}

#[cfg(test)]
mod tests;
