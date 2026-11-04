//! Kusema Assistant mode: a spoken question is answered by the LLM provider
//! configured for post-processing, keeping the conversation for follow-ups.
//! "Ask about my screen" adds a screenshot of the active window to the
//! question (see `crate::screen`).

pub mod answer;
mod conversation;
pub mod failure;
pub mod insert;

pub use answer::AnswerTo;
pub use conversation::Conversation;
use failure::Failure;

use crate::actions::{complete_unless_cancelled, strip_think_block};
use crate::llm_client::{self, ChatImage, ChatTurn};
use crate::screen::{self, Capture, CapturedWindow, Confirmation};
use crate::settings::{
    get_settings, write_settings, AppSettings, PostProcessProvider, APPLE_INTELLIGENCE_PROVIDER_ID,
};
use log::{debug, error, info};
use serde::Serialize;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, State};

/// Which Assistant hotkey started a question.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AskMode {
    /// Ask Kusema: the question alone.
    Plain,
    /// Ask about my screen: the question plus a screenshot of the window.
    Screen,
    /// Kusema voice editing: an instruction or command for the selected text
    /// (see `crate::voice_edit`).
    Edit,
}

/// Shown and spoken when a provider rejects an image.
pub const NO_VISION_MESSAGE: &str =
    "This AI can't see the screen. Choose one that can see images, \
like Claude or a vision model in Ollama.";

/// Emitted as `assistant-reply` when a question has been answered.
#[derive(Clone, Debug, Serialize)]
pub struct AssistantReply {
    pub question: String,
    pub answer: String,
    /// Kusema: the request it answers (see `AskOptions::id`).
    pub id: u64,
}

/// Emitted as `assistant-error` when a question couldn't be answered.
#[derive(Clone, Debug, Serialize)]
pub struct AssistantError {
    pub question: String,
    /// The English, as spoken and logged.
    pub message: String,
    pub id: u64,
    /// Kusema: shown translated: `key` with `name` for its placeholder.
    pub key: String,
    pub name: Option<String>,
    /// Where "Set up" goes, if settings can fix it.
    pub setup: Option<failure::SetupPage>,
}

/// Emitted as `assistant-stream` as an answer streams in.
#[derive(Clone, Debug, Serialize)]
struct AssistantStream {
    id: u64,
    text: String,
}

/// Where a question came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Source {
    /// Spoken with an Assistant hotkey.
    #[default]
    Spoken,
    /// Typed in the floating Ask box.
    AskBox,
    /// Typed on the Assistant page.
    Page,
}

/// How a question was asked.
#[derive(Debug, Clone, Copy, Default)]
pub struct AskOptions {
    pub source: Source,
    /// The window and control a "type it" answer goes into.
    pub target: Option<insert::DocTarget>,
    /// Tells this question's stream, reply or error from others'.
    pub id: u64,
    /// Asked inside continuous dictation, whose pill stays up.
    pub in_dictation: bool,
}

/// Answers kept for "Insert into document", with the window each was asked from.
const MAX_ANSWER_TARGETS: usize = 20;

#[derive(Default)]
struct Inner {
    conversation: Conversation,
    /// Bumped by "New conversation", so a reply to a question asked before the
    /// reset doesn't land in the fresh conversation.
    epoch: u64,
    /// Kusema paste-the-answer: recent answers and where they were asked.
    answer_targets: Vec<(String, insert::DocTarget)>,
    /// Where a spoken question's hotkey was pressed.
    pending_target: Option<insert::DocTarget>,
}

/// Tauri managed state for Assistant mode.
#[derive(Default)]
pub struct AssistantState {
    inner: Mutex<Inner>,
    in_flight: AtomicUsize,
    next_id: AtomicU64,
}

/// A fresh id for `AskOptions::id`.
pub fn next_id(app: &AppHandle) -> u64 {
    app.try_state::<AssistantState>()
        .map_or(0, |state| state.next_id.fetch_add(1, Ordering::SeqCst) + 1)
}

/// At an Assistant hotkey's press: remember where an answer could be typed.
pub fn note_target_at_press(app: &AppHandle) {
    if let Some(state) = app.try_state::<AssistantState>() {
        state.lock().pending_target = insert::at_press();
    }
}

impl AskOptions {
    /// For a spoken question: the window noted at the hotkey's press.
    pub fn spoken(app: &AppHandle) -> Self {
        Self {
            source: Source::Spoken,
            target: app
                .try_state::<AssistantState>()
                .and_then(|state| state.lock().pending_target.take()),
            id: next_id(app),
            in_dictation: false,
        }
    }
}

impl AssistantState {
    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// Counts a request as in flight for as long as it lives.
struct InFlight<'a>(&'a AtomicUsize);

impl<'a> InFlight<'a> {
    fn new(counter: &'a AtomicUsize) -> Self {
        counter.fetch_add(1, Ordering::SeqCst);
        Self(counter)
    }
}

impl Drop for InFlight<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Whether a question is waiting for its answer (the Cancel hotkey uses this).
pub fn is_busy(app: &AppHandle) -> bool {
    app.try_state::<AssistantState>()
        .is_some_and(|state| state.in_flight.load(Ordering::SeqCst) > 0)
}

/// Provider, model and key from the post-processing settings.
pub(crate) struct Target {
    pub provider: PostProcessProvider,
    pub model: String,
    pub api_key: String,
}

/// Which request a target is for, by whether it's about the screen.
fn would_be_refused(provider: &PostProcessProvider) -> bool {
    crate::network::would_block(
        crate::network::Purpose::AssistantQuestion,
        &provider.base_url,
    )
}

pub(crate) fn resolve_target(settings: &AppSettings) -> Result<Target, Failure> {
    let provider = settings
        .active_post_process_provider()
        .cloned()
        .ok_or(Failure::NoProvider)?;
    resolve_provider(settings, provider, None)
}

/// The provider for screen questions: the Assistant's, unless the user chose
/// another one (e.g. a local model for confidential documents).
fn resolve_screen_target(settings: &AppSettings) -> Result<Target, Failure> {
    let Some(id) = settings
        .assistant_screen_provider_id
        .as_deref()
        .filter(|id| !id.is_empty())
    else {
        return resolve_target(settings);
    };
    let provider = settings
        .post_process_providers
        .iter()
        .find(|p| p.id == id)
        .cloned()
        .ok_or(Failure::ScreenProviderGone)?;
    resolve_provider(
        settings,
        provider,
        settings.assistant_screen_model.as_deref(),
    )
}

/// `model` overrides the provider's Post Process model.
fn resolve_provider(
    settings: &AppSettings,
    provider: PostProcessProvider,
    model: Option<&str>,
) -> Result<Target, Failure> {
    if provider.id == APPLE_INTELLIGENCE_PROVIDER_ID {
        return Err(Failure::ProviderUnsupported(provider.label));
    }

    let model = model
        .map(str::trim)
        .filter(|m| !m.is_empty())
        .map(str::to_string)
        .or_else(|| {
            settings
                .post_process_models
                .get(&provider.id)
                .map(|model| model.trim().to_string())
        })
        .unwrap_or_default();
    let api_key = settings
        .post_process_api_keys
        .get(&provider.id)
        .map(|key| key.trim().to_string())
        .unwrap_or_default();
    // Privileged mode's refusal says more than a missing key or model would.
    if let Some(failure) =
        failure::missing_setup(&provider, &model, &api_key, would_be_refused(&provider))
    {
        return Err(failure);
    }

    Ok(Target {
        provider,
        model,
        api_key,
    })
}

/// Turn an llm_client error into something worth showing the user.
/// `with_image`: the request carried a screenshot.
#[cfg(test)]
pub(crate) fn describe_error(
    provider: &PostProcessProvider,
    error: &str,
    with_image: bool,
) -> String {
    failure::classify(provider, error, with_image).english()
}

/// "Set up" in the pill or on a card: the main window, on that page.
#[tauri::command]
#[specta::specta]
pub fn open_setup_page(app: AppHandle, page: failure::SetupPage) {
    crate::overlay::hide_recording_overlay(&app);
    crate::show_main_window(&app);
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.emit("open-settings-page", page);
    }
}

/// How long a failure stays in the pill.
const FAILURE_SECS: u64 = 6;

/// Says what went wrong where the user is looking: in the pill (inside
/// continuous dictation's, with `in_dictation`), or on a card beside `near`
/// when the pill is off; spoken when replies are spoken. Both offer "Set up"
/// when settings can fix it. `true` if the pill shows it, so it mustn't be
/// hidden right away.
pub(crate) fn tell_failure(
    app: &AppHandle,
    failure: &Failure,
    in_dictation: bool,
    near: Option<CapturedWindow>,
) -> bool {
    let english = failure.english();
    let key = format!("overlay.ai.{}", failure.key());
    info!("Assistant: {}", failure.key());
    let notice = serde_json::json!({
        "key": key,
        "name": failure.name(),
        "message": english,
        "in_dictation": in_dictation,
        "setup": failure.setup(),
    });
    if crate::overlay::show_edit_notice(app, &notice, in_dictation, FAILURE_SECS) {
        crate::tts::say(app, &english);
        !in_dictation
    } else {
        screen::card::notice_with_setup(
            app,
            english,
            Some(key),
            failure.name(),
            failure.setup(),
            near,
        );
        false
    }
}

/// Reports a failed question: told where the user is (a spoken question), and
/// to the Ask box and the Assistant page, which show it themselves.
fn report(
    app: &AppHandle,
    question: &str,
    failure: Failure,
    options: &AskOptions,
    near: Option<CapturedWindow>,
) {
    error!("Assistant request failed: {}", failure.english());
    if options.source == Source::Spoken {
        tell_failure(app, &failure, options.in_dictation, near);
    }
    let _ = app.emit(
        "assistant-error",
        AssistantError {
            question: question.to_string(),
            message: failure.english(),
            id: options.id,
            key: format!("overlay.ai.{}", failure.key()),
            name: failure.name(),
            setup: failure.setup(),
        },
    );
}

/// What one question asks for, from the question and the settings.
struct Plan {
    question: String,
    delivery: answer::Delivery,
    tutor: bool,
    style: Option<crate::app_styles::Active>,
}

impl Plan {
    fn new(settings: &AppSettings, question: &str, options: &AskOptions) -> Self {
        let request = answer::parse(question);
        let delivery = answer::delivery(
            settings.assistant_answer_to,
            request.type_it,
            options.source != Source::Spoken,
            settings.assistant_speak_typed,
        );
        // The Assistant page has no document of its own to type into.
        let delivery = answer::Delivery {
            type_it: delivery.type_it && options.source != Source::Page,
            ..delivery
        };
        let style = options
            .target
            .filter(|_| delivery.type_it)
            .and_then(|target| {
                crate::app_styles::active(
                    settings,
                    &crate::app_styles::process_of(target.window.handle()),
                )
            });
        Self {
            tutor: answer::tutor(settings.assistant_tutor, &request),
            question: request.question,
            delivery,
            style,
        }
    }

    fn prompt(&self, base: &str) -> String {
        answer::compose(
            base,
            &answer::Extras {
                document: self.delivery.type_it,
                tutor: self.tutor,
                style: self.style.as_ref().map(|s| s.instruction.as_str()),
            },
        )
    }
}

/// Ask the configured LLM `question` with the whole conversation as context,
/// then record the exchange and emit `assistant-reply`, or `assistant-error`
/// on failure. Returns early, emitting nothing, once `is_cancelled` is true.
pub async fn ask<C: Fn() -> bool>(
    app: &AppHandle,
    question: String,
    options: AskOptions,
    is_cancelled: C,
) {
    let Some(state) = app.try_state::<AssistantState>() else {
        error!("AssistantState is not initialized");
        return;
    };
    let _in_flight = InFlight::new(&state.in_flight);
    let settings = get_settings(app);
    let plan = Plan::new(&settings, &question, &options);
    let question = plan.question.clone();

    let (turns, epoch, about_screen) = {
        let inner = state.lock();
        (
            inner.conversation.with_question(&question),
            inner.epoch,
            inner.conversation.has_screenshot(),
        )
    };
    // Follow-ups about a screenshot stay with the screen-question provider:
    // the earlier answers describe what was on screen.
    let target = if about_screen {
        resolve_screen_target(&settings)
    } else {
        resolve_target(&settings)
    };
    let target = match target {
        Ok(target) => target,
        Err(failure) => return report(app, &question, failure, &options, None),
    };
    let turns = conversation::keep_images_for(turns, &target.provider.id);
    let system_prompt = plan.prompt(&state.lock().conversation.prompt_for(&turns));

    let Some(answer) = get_answer(
        app,
        &question,
        &target,
        &system_prompt,
        &turns,
        &is_cancelled,
        None,
        &plan,
        &options,
    )
    .await
    else {
        return;
    };
    if !record(
        app,
        &state,
        epoch,
        question,
        answer.clone(),
        &options,
        |c, q, a| c.push_exchange(q, a),
    ) {
        return;
    }
    deliver(app, &plan, &options, answer, &is_cancelled).await;
}

/// Ask `question` about the window in `capture`: prepare the screenshot, get
/// the user's go-ahead (unless they turned that off), then ask as above with
/// the image attached. Nothing leaves the computer before Send.
pub async fn ask_about_screen<C: Fn() -> bool>(
    app: &AppHandle,
    question: String,
    capture: Option<Capture>,
    options: AskOptions,
    is_cancelled: C,
) {
    let Some(state) = app.try_state::<AssistantState>() else {
        error!("AssistantState is not initialized");
        return;
    };
    let _in_flight = InFlight::new(&state.in_flight);
    let id = options.id;

    let Some(capture) = capture else {
        let failure = Failure::Other(
            "Kusema".to_string(),
            "The screenshot is missing. Press the shortcut again.".to_string(),
        );
        return report(app, &question, failure, &options, None);
    };
    let window = capture.window;
    let settings = get_settings(app);
    let plan = Plan::new(&settings, &question, &options);
    let question = plan.question.clone();
    let target = match resolve_screen_target(&settings) {
        Ok(target) => target,
        Err(failure) => return report(app, &question, failure, &options, Some(window)),
    };

    let prepared =
        tauri::async_runtime::spawn_blocking(move || screen::image::prepare(&capture.image))
            .await
            .map_err(|e| e.to_string())
            .and_then(|result| result);
    let prepared = match prepared {
        Ok(prepared) => prepared,
        Err(e) => {
            let failure = Failure::Other(
                "Kusema".to_string(),
                format!("Couldn't prepare the screenshot ({e})."),
            );
            return report(app, &question, failure, &options, Some(window));
        }
    };
    debug!(
        "Prepared screenshot: {}x{} {}, {} KB",
        prepared.width,
        prepared.height,
        prepared.media_type,
        prepared.data.len() / 1024
    );
    let image = ChatImage {
        media_type: prepared.media_type,
        data: Arc::from(prepared.data),
        approved_for: target.provider.id.clone(),
    };

    // A screenshot that can't go needs no confirming: the refusal comes below.
    if settings.assistant_screen_confirm && !would_be_refused(&target.provider) {
        crate::utils::hide_recording_overlay(app);
        let confirmation = screen::card::confirm(
            app,
            window,
            &image,
            &question,
            &target.provider.label,
            &target.model,
            &is_cancelled,
        )
        .await;
        if confirmation != Confirmation::Send {
            info!("Screenshot not sent ({confirmation:?})");
            crate::ask_box::question_ended(app, id);
            return;
        }
        if options.source == Source::Spoken {
            crate::overlay::show_processing_overlay(app);
        }
    }

    let (turns, epoch) = {
        let inner = state.lock();
        (
            inner
                .conversation
                .with_screen_question(&question, image.clone()),
            inner.epoch,
        )
    };
    let system_prompt = plan.prompt(&state.lock().conversation.prompt_for(&turns));

    let Some(answer) = get_answer(
        app,
        &question,
        &target,
        &system_prompt,
        &turns,
        &is_cancelled,
        Some(window),
        &plan,
        &options,
    )
    .await
    else {
        return;
    };
    if !record(
        app,
        &state,
        epoch,
        question,
        answer.clone(),
        &options,
        move |c, q, a| c.push_screen_exchange(q, a, image),
    ) {
        return;
    }
    deliver(app, &plan, &options, answer, &is_cancelled).await;
}

/// Kusema paste-the-answer: type the answer into the document, if asked to.
async fn deliver<C: Fn() -> bool>(
    app: &AppHandle,
    plan: &Plan,
    options: &AskOptions,
    answer: String,
    is_cancelled: &C,
) {
    if !plan.delivery.type_it || is_cancelled() {
        return;
    }
    let Some(target) = options.target else {
        insert::tell(app, &insert::InsertProblem::NoTarget, None);
        return;
    };
    if let Some(style) = &plan.style {
        crate::app_styles::announce(app, style);
    }
    // From the Ask box: it closes, handing focus back if it still had it.
    // Nothing is brought to the front here: if the user went elsewhere, the
    // answer waits for them to come back.
    if options.source == Source::AskBox {
        crate::ask_box::close_for_typing(app);
    }
    let app = app.clone();
    let _ =
        tauri::async_runtime::spawn_blocking(move || insert::insert(&app, target, &answer, false))
            .await;
}

/// Send the conversation and speak the reply as it streams in. Returns the
/// answer, or `None` after reporting a failure (or on cancel). `screen`: the
/// question is about a screenshot of this window, so failures are also shown
/// beside it.
#[allow(clippy::too_many_arguments)]
async fn get_answer<C: Fn() -> bool>(
    app: &AppHandle,
    question: &str,
    target: &Target,
    system_prompt: &str,
    turns: &[ChatTurn],
    is_cancelled: &C,
    screen: Option<CapturedWindow>,
    plan: &Plan,
    options: &AskOptions,
) -> Option<String> {
    let id = options.id;
    let with_image = turns.iter().any(|t| t.image.is_some());
    debug!(
        "Asking assistant via provider '{}' (model: {}, {} messages{}{}{})",
        target.provider.id,
        target.model,
        turns.len(),
        if with_image {
            ", with a screenshot"
        } else {
            ""
        },
        if plan.tutor { ", tutor style" } else { "" },
        if plan.delivery.type_it {
            ", to type"
        } else {
            ""
        },
    );
    let fail = |failure: Failure| report(app, question, failure, options, screen);

    // Speak each sentence as soon as it has streamed in.
    let mut speech = plan
        .delivery
        .speak
        .then(|| crate::tts::begin_reply(app))
        .flatten();
    let request = llm_client::send_conversation(
        &target.provider,
        target.api_key.clone(),
        &target.model,
        system_prompt,
        turns,
        |text| {
            if let Some(speech) = speech.as_mut() {
                speech.push(text);
            }
            let _ = app.emit(
                "assistant-stream",
                AssistantStream {
                    id,
                    text: text.to_string(),
                },
            );
        },
    );
    let Some(result) = complete_unless_cancelled(request, is_cancelled).await else {
        info!("Assistant request cancelled");
        crate::ask_box::question_ended(app, id);
        return None;
    };

    let answer = match result {
        Ok(Some(text)) => strip_think_block(&text).trim().to_string(),
        Ok(None) => String::new(),
        Err(e) => {
            crate::tts::stop(app);
            fail(failure::classify(&target.provider, &e, with_image));
            return None;
        }
    };
    if let Some(speech) = speech {
        speech.finish();
    }
    if answer.is_empty() {
        fail(Failure::EmptyReply(target.provider.label.clone()));
        return None;
    }
    Some(answer)
}

/// Add a finished exchange to the conversation (unless it was cleared while
/// the question was out) and emit `assistant-reply`. `false` if it was
/// dropped.
fn record(
    app: &AppHandle,
    state: &AssistantState,
    epoch: u64,
    question: String,
    answer: String,
    options: &AskOptions,
    push: impl FnOnce(&mut Conversation, String, String),
) -> bool {
    {
        let mut inner = state.lock();
        if inner.epoch != epoch {
            debug!("Dropping assistant reply for a conversation that was cleared");
            drop(inner);
            crate::tts::stop(app);
            crate::ask_box::question_ended(app, options.id);
            return false;
        }
        push(&mut inner.conversation, question.clone(), answer.clone());
        if let Some(target) = options.target {
            inner.answer_targets.push((answer.clone(), target));
            let excess = inner
                .answer_targets
                .len()
                .saturating_sub(MAX_ANSWER_TARGETS);
            inner.answer_targets.drain(..excess);
        }
    }
    crate::setup::first_wins::record(app, crate::setup::first_wins::Win::Asked);

    let _ = app.emit(
        "assistant-reply",
        AssistantReply {
            question,
            answer,
            id: options.id,
        },
    );
    true
}

/// The window `answer` was asked from, if it's one Kusema could type into.
fn target_of(app: &AppHandle, answer: &str) -> Option<insert::DocTarget> {
    let state = app.try_state::<AssistantState>()?;
    let inner = state.lock();
    inner
        .answer_targets
        .iter()
        .rev()
        .find(|(text, _)| text == answer)
        .map(|(_, target)| *target)
}

/// Kusema paste-the-answer: type an earlier answer into the window it was
/// asked from, bringing that window to the front first.
pub async fn insert_answer(app: &AppHandle, answer: String) -> Result<(), String> {
    let Some(target) = target_of(app, &answer) else {
        insert::tell(app, &insert::InsertProblem::NoTarget, None);
        return Err("noTarget".to_string());
    };
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || insert::insert(&app, target, &answer, true))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|problem| format!("{problem:?}"))
}

/// Type an answer from the Assistant page into the document it was asked from.
#[tauri::command]
#[specta::specta]
pub async fn insert_assistant_answer(app: AppHandle, answer: String) -> Result<(), String> {
    insert_answer(&app, answer).await
}

/// The answers that know which document they're for.
#[tauri::command]
#[specta::specta]
pub fn get_insertable_answers(state: State<'_, AssistantState>) -> Vec<String> {
    state
        .lock()
        .answer_targets
        .iter()
        .map(|(answer, _)| answer.clone())
        .collect()
}

/// A question typed on the Assistant page. Returns its id at once; the answer
/// comes as `assistant-stream`, then `assistant-reply` or `assistant-error`.
#[tauri::command]
#[specta::specta]
pub fn ask_typed(app: AppHandle, question: String) -> Result<u64, String> {
    let question = question.trim().to_string();
    if question.is_empty() {
        return Err("empty".to_string());
    }
    let options = AskOptions {
        source: Source::Page,
        target: None,
        id: next_id(&app),
        in_dictation: false,
    };
    let id = options.id;
    tauri::async_runtime::spawn(async move {
        let cancelled = crate::ask_box::cancel_flag(&app, id);
        ask(&app, question, options, || cancelled.is_set()).await;
    });
    Ok(id)
}

/// Stop a typed question waiting for its answer, and any reply being spoken.
#[tauri::command]
#[specta::specta]
pub fn cancel_typed_question(app: AppHandle, id: u64) {
    crate::ask_box::cancel(&app, id);
    crate::tts::stop(&app);
}

/// The current conversation, oldest message first.
#[tauri::command]
#[specta::specta]
pub fn get_assistant_conversation(state: State<'_, AssistantState>) -> Vec<ChatTurn> {
    state.lock().conversation.messages().to_vec()
}

/// Start a new conversation. A reply still in flight, or being spoken, is
/// discarded, and so is any screenshot.
#[tauri::command]
#[specta::specta]
pub fn clear_assistant_conversation(app: AppHandle, state: State<'_, AssistantState>) {
    {
        let mut inner = state.lock();
        inner.conversation.clear();
        inner.answer_targets.clear();
        inner.epoch += 1;
    }
    screen::discard(&app);
    crate::tts::stop_and_idle(&app);
}

#[tauri::command]
#[specta::specta]
pub fn change_assistant_screen_confirm_setting(
    app: AppHandle,
    enabled: bool,
) -> Result<(), String> {
    let mut settings = get_settings(&app);
    settings.assistant_screen_confirm = enabled;
    write_settings(&app, settings);
    Ok(())
}

/// `None` (or empty) uses the Assistant's provider for screen questions.
#[tauri::command]
#[specta::specta]
pub fn change_assistant_screen_provider_setting(
    app: AppHandle,
    provider_id: Option<String>,
) -> Result<(), String> {
    let mut settings = get_settings(&app);
    settings.assistant_screen_provider_id = provider_id.filter(|id| !id.is_empty());
    write_settings(&app, settings);
    Ok(())
}

/// `None` (or empty) uses the provider's Post Process model.
#[tauri::command]
#[specta::specta]
pub fn change_assistant_screen_model_setting(
    app: AppHandle,
    model: Option<String>,
) -> Result<(), String> {
    let mut settings = get_settings(&app);
    settings.assistant_screen_model = model
        .map(|m| m.trim().to_string())
        .filter(|m| !m.is_empty());
    write_settings(&app, settings);
    Ok(())
}

fn assistant_setting_changed(app: &AppHandle, settings: AppSettings, name: &str) {
    write_settings(app, settings);
    let _ = app.emit("settings-changed", serde_json::json!({ "setting": name }));
}

#[tauri::command]
#[specta::specta]
pub fn change_assistant_speak_typed_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = get_settings(&app);
    settings.assistant_speak_typed = enabled;
    assistant_setting_changed(&app, settings, "assistant_speak_typed");
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_assistant_answer_to_setting(
    app: AppHandle,
    answer_to: AnswerTo,
) -> Result<(), String> {
    let mut settings = get_settings(&app);
    settings.assistant_answer_to = answer_to;
    assistant_setting_changed(&app, settings, "assistant_answer_to");
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_assistant_tutor_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = get_settings(&app);
    settings.assistant_tutor = enabled;
    assistant_setting_changed(&app, settings, "assistant_tutor");
    Ok(())
}

#[cfg(test)]
mod tests {
    /// The words a lawyer sees and hears, for any AI feature that's refused.
    #[test]
    fn a_refusal_is_described_with_the_exact_message() {
        let provider = PostProcessProvider {
            label: "Anthropic".to_string(),
            ..crate::settings::get_default_settings()
                .post_process_providers
                .into_iter()
                .next()
                .unwrap()
        };
        let blocked = crate::network::Blocked {
            host: "api.anthropic.com".to_string(),
            verdict: crate::network::BlockReason::CloudAi,
        }
        .to_string();
        for with_image in [false, true] {
            assert_eq!(
                describe_error(&provider, &blocked, with_image),
                "Privileged mode is on, so cloud AI is blocked. Use a local AI, or turn Privileged mode off."
            );
        }
    }

    use super::*;
    use crate::settings::get_default_settings;

    fn settings_with(provider_id: &str, model: &str, key: &str) -> AppSettings {
        let mut settings = get_default_settings();
        settings.post_process_provider_id = provider_id.to_string();
        settings
            .post_process_models
            .insert(provider_id.to_string(), model.to_string());
        settings
            .post_process_api_keys
            .insert(provider_id.to_string(), key.to_string());
        settings
    }

    fn error_of(settings: &AppSettings) -> String {
        match resolve_target(settings) {
            Ok(_) => panic!("expected an error"),
            Err(e) => e.english(),
        }
    }

    #[test]
    fn missing_key_names_the_provider() {
        let settings = settings_with("anthropic", "claude-haiku", "");
        assert_eq!(
            error_of(&settings),
            "No API key for Anthropic. Add one in Assistant → Settings."
        );
    }

    #[test]
    fn missing_model_names_the_provider() {
        let settings = settings_with("anthropic", " ", "key");
        assert_eq!(
            error_of(&settings),
            "No model chosen for Anthropic. Choose one in Assistant → Settings."
        );
    }

    #[test]
    fn custom_provider_needs_no_key() {
        let settings = settings_with("custom", "llama3", "");
        let target = resolve_target(&settings).ok().unwrap();
        assert_eq!(target.provider.id, "custom");
        assert_eq!(target.model, "llama3");
    }

    #[test]
    fn configured_provider_resolves() {
        let settings = settings_with("anthropic", "claude-haiku", "sk-test");
        let target = resolve_target(&settings).ok().unwrap();
        assert_eq!(target.model, "claude-haiku");
        assert_eq!(target.api_key, "sk-test");
    }

    #[test]
    fn auth_errors_are_explained() {
        let settings = settings_with("anthropic", "m", "k");
        let provider = settings.active_post_process_provider().unwrap();
        let message = describe_error(
            provider,
            "API request failed with status 401 Unauthorized: {}",
            false,
        );
        assert!(message.starts_with("Anthropic didn't accept the API key"));
    }

    #[test]
    fn image_rejections_get_the_vision_message() {
        let settings = settings_with("openai", "m", "k");
        let provider = settings.active_post_process_provider().unwrap();
        for error in [
            r#"API request failed with status 400 Bad Request: {"error":{"message":"Invalid content type. image_url is only supported by certain models."}}"#,
            "API request failed with status 422 Unprocessable Entity: unknown variant `image_url`, expected `text`",
            "API request failed with status 400 Bad Request: model does not support image input",
        ] {
            assert_eq!(describe_error(provider, error, true), NO_VISION_MESSAGE);
            // Without an image the same words mean something else.
            assert_ne!(describe_error(provider, error, false), NO_VISION_MESSAGE);
        }
        let auth = "API request failed with status 401 Unauthorized: image";
        assert!(describe_error(provider, auth, true).contains("didn't accept the API key"));
    }

    #[test]
    fn screen_questions_can_use_their_own_provider() {
        let mut settings = settings_with("anthropic", "claude-haiku", "sk-test");
        // Same as the Assistant by default.
        let target = resolve_screen_target(&settings).ok().unwrap();
        assert_eq!(target.provider.id, "anthropic");

        settings.assistant_screen_provider_id = Some("custom".to_string());
        settings
            .post_process_models
            .insert("custom".to_string(), "llama3".to_string());
        let target = resolve_screen_target(&settings).ok().unwrap();
        assert_eq!(target.provider.id, "custom");
        assert_eq!(target.model, "llama3");

        settings.assistant_screen_model = Some("qwen2.5vl".to_string());
        let target = resolve_screen_target(&settings).ok().unwrap();
        assert_eq!(target.model, "qwen2.5vl");

        settings.assistant_screen_provider_id = Some("gone".to_string());
        assert!(resolve_screen_target(&settings).is_err());
    }
}
