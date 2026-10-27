//! Kusema Assistant mode: a spoken question is answered by the LLM provider
//! configured for post-processing, keeping the conversation for follow-ups.

mod conversation;

pub use conversation::Conversation;

use crate::actions::{complete_unless_cancelled, strip_think_block};
use crate::llm_client::{self, ChatTurn};
use crate::settings::{
    get_settings, AppSettings, PostProcessProvider, APPLE_INTELLIGENCE_PROVIDER_ID,
};
use log::{debug, error, info};
use serde::Serialize;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, State};

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
    if provider.id == APPLE_INTELLIGENCE_PROVIDER_ID {
        return Err("Apple Intelligence can't be used for Assistant mode yet. Choose another provider in the Post Processing settings.".to_string());
    }

    let model = settings
        .post_process_models
        .get(&provider.id)
        .map(|model| model.trim().to_string())
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

/// Turn an llm_client error into something worth showing the user.
fn describe_error(provider: &PostProcessProvider, error: &str) -> String {
    let label = &provider.label;
    if error.contains("status 401") || error.contains("status 403") {
        format!("{label} rejected the API key. Check it in the Post Processing settings.")
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

/// Ask the configured LLM `question` with the whole conversation as context,
/// then record the exchange and emit `assistant-reply`, or `assistant-error`
/// on failure. Returns early, emitting nothing, once `is_cancelled` is true.
pub async fn ask<C: Fn() -> bool>(app: &AppHandle, question: String, is_cancelled: C) {
    let Some(state) = app.try_state::<AssistantState>() else {
        error!("AssistantState is not initialized");
        return;
    };
    let _in_flight = InFlight::new(&state.in_flight);

    let target = match resolve_target(&get_settings(app)) {
        Ok(target) => target,
        Err(message) => return emit_error(app, &question, message),
    };

    let (system_prompt, turns, epoch) = {
        let inner = state.lock();
        (
            inner.conversation.system_prompt().to_string(),
            inner.conversation.with_question(&question),
            inner.epoch,
        )
    };

    debug!(
        "Asking assistant via provider '{}' (model: {}, {} messages)",
        target.provider.id,
        target.model,
        turns.len()
    );

    // Speak each sentence as soon as it has streamed in.
    let mut speech = crate::tts::begin_reply(app);
    let request = llm_client::send_conversation(
        &target.provider,
        target.api_key,
        &target.model,
        &system_prompt,
        &turns,
        |text| {
            if let Some(speech) = speech.as_mut() {
                speech.push(text);
            }
        },
    );
    let Some(result) = complete_unless_cancelled(request, is_cancelled).await else {
        info!("Assistant request cancelled");
        return;
    };

    let answer = match result {
        Ok(Some(text)) => strip_think_block(&text).trim().to_string(),
        Ok(None) => String::new(),
        Err(e) => {
            crate::tts::stop(app);
            return emit_error(app, &question, describe_error(&target.provider, &e));
        }
    };
    if let Some(speech) = speech {
        speech.finish();
    }
    if answer.is_empty() {
        return emit_error(
            app,
            &question,
            format!("{} returned an empty reply", target.provider.label),
        );
    }

    {
        let mut inner = state.lock();
        if inner.epoch != epoch {
            debug!("Dropping assistant reply for a conversation that was cleared");
            drop(inner);
            crate::tts::stop(app);
            return;
        }
        inner
            .conversation
            .push_exchange(question.clone(), answer.clone());
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
/// discarded.
#[tauri::command]
#[specta::specta]
pub fn clear_assistant_conversation(app: AppHandle, state: State<'_, AssistantState>) {
    {
        let mut inner = state.lock();
        inner.conversation.clear();
        inner.epoch += 1;
    }
    crate::tts::stop_and_idle(&app);
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
        );
        assert!(message.starts_with("Anthropic rejected the API key"));
    }
}
