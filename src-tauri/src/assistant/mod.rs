//! Kusema Assistant mode: a spoken question is answered by the LLM provider
//! configured for post-processing, keeping the conversation for follow-ups.
//! "Ask about my screen" adds a screenshot of the active window to the
//! question (see `crate::screen`).

mod conversation;

pub use conversation::Conversation;

use crate::actions::{complete_unless_cancelled, strip_think_block};
use crate::llm_client::{self, ChatImage, ChatTurn};
use crate::screen::{self, Capture, CapturedWindow, Confirmation};
use crate::settings::{
    get_settings, write_settings, AppSettings, PostProcessProvider, APPLE_INTELLIGENCE_PROVIDER_ID,
};
use log::{debug, error, info};
use serde::Serialize;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, State};

/// Which Assistant hotkey started a question.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AskMode {
    /// Ask Kusema: the question alone.
    Plain,
    /// Ask about my screen: the question plus a screenshot of the window.
    Screen,
}

/// Shown and spoken when a provider rejects an image.
pub const NO_VISION_MESSAGE: &str = "This model can't see images. Choose a vision model like \
Claude, or a local vision model in Ollama.";

/// Emitted as `assistant-reply` when a question has been answered.
#[derive(Clone, Debug, Serialize)]
pub struct AssistantReply {
    pub question: String,
    pub answer: String,
}

/// Emitted as `assistant-error` when a question couldn't be answered.
#[derive(Clone, Debug, Serialize)]
pub struct AssistantError {
    pub question: String,
    pub message: String,
}

#[derive(Default)]
struct Inner {
    conversation: Conversation,
    /// Bumped by "New conversation", so a reply to a question asked before the
    /// reset doesn't land in the fresh conversation.
    epoch: u64,
}

/// Tauri managed state for Assistant mode.
#[derive(Default)]
pub struct AssistantState {
    inner: Mutex<Inner>,
    in_flight: AtomicUsize,
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
struct Target {
    provider: PostProcessProvider,
    model: String,
    api_key: String,
}

fn resolve_target(settings: &AppSettings) -> Result<Target, String> {
    let provider = settings.active_post_process_provider().cloned().ok_or(
        "No AI provider selected. Choose one in the Post Processing settings.".to_string(),
    )?;
    resolve_provider(settings, provider, None)
}

/// The provider for screen questions: the Assistant's, unless the user chose
/// another one (e.g. a local model for confidential documents).
fn resolve_screen_target(settings: &AppSettings) -> Result<Target, String> {
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
        .ok_or("The provider chosen for screen questions no longer exists. Choose another on the Assistant page.".to_string())?;
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
) -> Result<Target, String> {
    if provider.id == APPLE_INTELLIGENCE_PROVIDER_ID {
        return Err("Apple Intelligence can't be used for Assistant mode yet. Choose another provider in the Post Processing settings.".to_string());
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
    if model.is_empty() {
        return Err(format!("No model selected for {}", provider.label));
    }

    let api_key = settings
        .post_process_api_keys
        .get(&provider.id)
        .map(|key| key.trim().to_string())
        .unwrap_or_default();
    // A custom endpoint (e.g. a local Ollama server) may not need a key.
    if api_key.is_empty() && provider.id != "custom" {
        return Err(format!("No API key set for {}", provider.label));
    }

    Ok(Target {
        provider,
        model,
        api_key,
    })
}

/// Whether an error says the model doesn't take images. Providers word this
/// differently ("image_url is only supported by certain models", "unknown
/// variant `image_url`, expected `text`", "does not support image input"...).
fn is_image_rejection(error: &str) -> bool {
    let error = error.to_lowercase();
    [
        "image",
        "vision",
        "multimodal",
        "multi-modal",
        "unknown variant",
        "expected `text`",
        "must be a string",
        "content type",
    ]
    .iter()
    .any(|hint| error.contains(hint))
}

/// Turn an llm_client error into something worth showing the user.
/// `with_image`: the request carried a screenshot.
fn describe_error(provider: &PostProcessProvider, error: &str, with_image: bool) -> String {
    let label = &provider.label;
    if error.contains("status 401") || error.contains("status 403") {
        format!("{label} rejected the API key. Check it in the Post Processing settings.")
    } else if with_image && !error.contains("status 429") && is_image_rejection(error) {
        NO_VISION_MESSAGE.to_string()
    } else if error.contains("status 404") {
        format!("{label} couldn't find that model or endpoint. Check the Post Processing settings.")
    } else if error.contains("status 429") {
        format!("{label} is rate limiting requests. Try again in a moment.")
    } else if error.contains("kind: connect") || error.contains("kind: timeout") {
        format!("Couldn't reach {label}. Check your connection and the provider's URL.")
    } else {
        format!("{label}: {error}")
    }
}

fn emit_error(app: &AppHandle, question: &str, message: String) {
    error!("Assistant request failed: {message}");
    let _ = app.emit(
        "assistant-error",
        AssistantError {
            question: question.to_string(),
            message,
        },
    );
}

/// Report a failed screen question where the user is looking: on a card
/// beside the window, and spoken, as well as on the Assistant page.
fn emit_screen_error(
    app: &AppHandle,
    question: &str,
    message: String,
    near: Option<CapturedWindow>,
) {
    screen::card::notice(app, message.clone(), near);
    emit_error(app, question, message);
}

/// Ask the configured LLM `question` with the whole conversation as context,
/// then record the exchange and emit `assistant-reply`, or `assistant-error`
/// on failure. Returns early, emitting nothing, once `is_cancelled` is true.
pub async fn ask<C: Fn() -> bool>(app: &AppHandle, question: String, is_cancelled: C) {
    let Some(state) = app.try_state::<AssistantState>() else {
        error!("AssistantState is not initialized");
        return;
    };
    let _in_flight = InFlight::new(&state.in_flight);

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
    let settings = get_settings(app);
    let target = if about_screen {
        resolve_screen_target(&settings)
    } else {
        resolve_target(&settings)
    };
    let target = match target {
        Ok(target) => target,
        Err(message) => return emit_error(app, &question, message),
    };
    let turns = conversation::keep_images_for(turns, &target.provider.id);
    let system_prompt = state.lock().conversation.prompt_for(&turns);

    let Some(answer) = get_answer(
        app,
        &question,
        &target,
        &system_prompt,
        &turns,
        &is_cancelled,
        None,
    )
    .await
    else {
        return;
    };
    record(app, &state, epoch, question, answer, |c, q, a| {
        c.push_exchange(q, a)
    });
}

/// Ask `question` about the window in `capture`: prepare the screenshot, get
/// the user's go-ahead (unless they turned that off), then ask as above with
/// the image attached. Nothing leaves the computer before Send.
pub async fn ask_about_screen<C: Fn() -> bool>(
    app: &AppHandle,
    question: String,
    capture: Option<Capture>,
    is_cancelled: C,
) {
    let Some(state) = app.try_state::<AssistantState>() else {
        error!("AssistantState is not initialized");
        return;
    };
    let _in_flight = InFlight::new(&state.in_flight);

    let Some(capture) = capture else {
        let message = "The screenshot is missing. Press the shortcut again.".to_string();
        return emit_screen_error(app, &question, message, None);
    };
    let window = capture.window;
    let settings = get_settings(app);
    let target = match resolve_screen_target(&settings) {
        Ok(target) => target,
        Err(message) => return emit_screen_error(app, &question, message, Some(window)),
    };

    let prepared =
        tauri::async_runtime::spawn_blocking(move || screen::image::prepare(&capture.image))
            .await
            .map_err(|e| e.to_string())
            .and_then(|result| result);
    let prepared = match prepared {
        Ok(prepared) => prepared,
        Err(e) => {
            let message = format!("Couldn't prepare the screenshot ({e}).");
            return emit_screen_error(app, &question, message, Some(window));
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

    if settings.assistant_screen_confirm {
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
            return;
        }
        crate::overlay::show_processing_overlay(app);
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
    let system_prompt = state.lock().conversation.prompt_for(&turns);

    let Some(answer) = get_answer(
        app,
        &question,
        &target,
        &system_prompt,
        &turns,
        &is_cancelled,
        Some(window),
    )
    .await
    else {
        return;
    };
    record(app, &state, epoch, question, answer, move |c, q, a| {
        c.push_screen_exchange(q, a, image)
    });
}

/// Send the conversation and speak the reply as it streams in. Returns the
/// answer, or `None` after reporting a failure (or on cancel). `screen`: the
/// question is about a screenshot of this window, so failures are also shown
/// beside it.
async fn get_answer<C: Fn() -> bool>(
    app: &AppHandle,
    question: &str,
    target: &Target,
    system_prompt: &str,
    turns: &[ChatTurn],
    is_cancelled: &C,
    screen: Option<CapturedWindow>,
) -> Option<String> {
    let with_image = turns.iter().any(|t| t.image.is_some());
    debug!(
        "Asking assistant via provider '{}' (model: {}, {} messages{})",
        target.provider.id,
        target.model,
        turns.len(),
        if with_image {
            ", with a screenshot"
        } else {
            ""
        }
    );
    let fail = |message: String| {
        if screen.is_some() || with_image {
            emit_screen_error(app, question, message, screen);
        } else {
            emit_error(app, question, message);
        }
    };

    // Speak each sentence as soon as it has streamed in.
    let mut speech = crate::tts::begin_reply(app);
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
        },
    );
    let Some(result) = complete_unless_cancelled(request, is_cancelled).await else {
        info!("Assistant request cancelled");
        return None;
    };

    let answer = match result {
        Ok(Some(text)) => strip_think_block(&text).trim().to_string(),
        Ok(None) => String::new(),
        Err(e) => {
            crate::tts::stop(app);
            fail(describe_error(&target.provider, &e, with_image));
            return None;
        }
    };
    if let Some(speech) = speech {
        speech.finish();
    }
    if answer.is_empty() {
        fail(format!("{} returned an empty reply", target.provider.label));
        return None;
    }
    Some(answer)
}

/// Add a finished exchange to the conversation (unless it was cleared while
/// the question was out) and emit `assistant-reply`.
fn record(
    app: &AppHandle,
    state: &AssistantState,
    epoch: u64,
    question: String,
    answer: String,
    push: impl FnOnce(&mut Conversation, String, String),
) {
    {
        let mut inner = state.lock();
        if inner.epoch != epoch {
            debug!("Dropping assistant reply for a conversation that was cleared");
            drop(inner);
            crate::tts::stop(app);
            return;
        }
        push(&mut inner.conversation, question.clone(), answer.clone());
    }

    let _ = app.emit("assistant-reply", AssistantReply { question, answer });
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

#[cfg(test)]
mod tests {
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
            Err(e) => e,
        }
    }

    #[test]
    fn missing_key_names_the_provider() {
        let settings = settings_with("anthropic", "claude-haiku", "");
        assert_eq!(error_of(&settings), "No API key set for Anthropic");
    }

    #[test]
    fn missing_model_names_the_provider() {
        let settings = settings_with("anthropic", " ", "key");
        assert_eq!(error_of(&settings), "No model selected for Anthropic");
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
        assert!(message.starts_with("Anthropic rejected the API key"));
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
        assert!(describe_error(provider, auth, true).contains("rejected the API key"));
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
