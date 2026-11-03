//! Kusema voice editing: select text in any app, hold the editing hotkey and
//! say what to do with it. A command ("select last word", "undo that") runs
//! here, with standard keystrokes or UI Automation, and nothing leaves the
//! computer. Anything else is an instruction for the AI provider ("make this
//! more formal"): the selection and the instruction go to it, a preview card
//! shows the result, and Apply types it over the selection, so Ctrl+Z in the
//! app undoes it.
//!
//! With nothing selected, the instruction applies to the last text Kusema
//! typed in that window and control, if it can be selected back and read as
//! exactly what was typed (see `last_typed`).

pub mod commands;
mod keys;
pub mod last_typed;
mod platform;
pub mod prompt;
pub mod text;

use commands::EditCommand;
use log::{debug, info};
use platform::{Selection, UiaError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

use crate::actions::complete_unless_cancelled;
use crate::clipboard::{OnTarget, TypeOutcome};
use crate::dictation::FocusToken;
use crate::network::Purpose;
use crate::screen::card::{self, ScreenCard};
use crate::screen::{CapturedWindow, Confirmation};
use crate::settings::{get_settings, write_settings};

pub const BINDING: &str = "voice_edit";

/// More characters than this are selected by hand, not by Shift+Left.
const MAX_SELECT_BACK: usize = 3000;
/// How long the user has to let go of the hotkey's modifiers.
const KEYS_UP_WAIT: Duration = Duration::from_millis(1500);
/// How long focus has to come back after the preview card.
const FOCUS_BACK_WAIT: Duration = Duration::from_millis(1500);
/// Time an app needs to show a selection it was just asked for.
const SETTLE: Duration = Duration::from_millis(60);

/// Where the hotkey was pressed.
#[derive(Clone, Copy, Debug)]
struct Target {
    window: CapturedWindow,
    focus: FocusToken,
}

static PENDING: Mutex<Option<Target>> = Mutex::new(None);

fn pending() -> std::sync::MutexGuard<'static, Option<Target>> {
    PENDING.lock().unwrap_or_else(|e| e.into_inner())
}

/// Why nothing was changed. Each has a translated message on the card and
/// the English one that's spoken and logged.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Problem {
    NoWindow,
    Unsupported,
    SelectFirst,
    NothingSelected,
    NoTextPattern,
    NothingBefore,
    NotFound(String),
    WindowChanged,
    SelectionChanged,
    NoChange,
    EmptyReply(String),
    Failed,
    /// Already worded (AI provider errors, Privileged mode).
    Said(String),
}

impl Problem {
    fn key_and_english(&self) -> (Option<&'static str>, String) {
        let (key, english) = match self {
            Problem::NoWindow => (
                "noWindow",
                "Click in the text you want to change, then try again.".to_string(),
            ),
            Problem::Unsupported => (
                "unsupported",
                "Voice editing only works on Windows for now.".to_string(),
            ),
            Problem::SelectFirst => (
                "selectFirst",
                "Select the text you want to change first.".to_string(),
            ),
            Problem::NothingSelected => ("nothingSelected", "Nothing is selected.".to_string()),
            Problem::NoTextPattern => (
                "noTextPattern",
                "This app doesn't let Kusema find text by voice. Select it with the mouse or keyboard instead.".to_string(),
            ),
            Problem::NothingBefore => (
                "nothingBefore",
                "There's no text before the cursor.".to_string(),
            ),
            Problem::NotFound(phrase) => ("notFound", format!("Couldn't find \u{201C}{phrase}\u{201D}.")),
            Problem::WindowChanged => (
                "windowChanged",
                "The window changed, so nothing was changed.".to_string(),
            ),
            Problem::SelectionChanged => (
                "selectionChanged",
                "The selection changed, so nothing was replaced.".to_string(),
            ),
            Problem::NoChange => ("noChange", "The text didn't need changing.".to_string()),
            Problem::EmptyReply(provider) => {
                ("emptyReply", format!("{provider} returned an empty reply."))
            }
            Problem::Failed => ("failed", "Couldn't do that in this window.".to_string()),
            Problem::Said(message) => return (None, message.clone()),
        };
        (Some(key), english)
    }

    fn name(&self) -> Option<String> {
        match self {
            Problem::NotFound(name) | Problem::EmptyReply(name) => Some(name.clone()),
            _ => None,
        }
    }
}

fn tell(app: &AppHandle, problem: Problem, near: Option<CapturedWindow>) {
    info!("Voice edit: {problem:?}");
    let (key, english) = problem.key_and_english();
    card::notice_with_key(
        app,
        english,
        key.map(|k| format!("voiceEdit.notice.{k}")),
        problem.name(),
        near,
    );
}

/// At the hotkey press, before any overlay: remember the window and control
/// with the caret. `false`, after saying why, if there's nothing to edit.
pub fn at_press(app: &AppHandle) -> bool {
    pending().take();
    if !cfg!(target_os = "windows") {
        tell(app, Problem::Unsupported, None);
        return false;
    }
    let window = match crate::screen::foreground_target() {
        Ok(window) => window,
        Err(e) => {
            debug!("No window to edit in: {e:?}");
            tell(app, Problem::NoWindow, None);
            return false;
        }
    };
    match crate::dictation::current_focus() {
        Some(focus) if focus.window == window.handle() => {
            *pending() = Some(Target { window, focus });
            true
        }
        _ => {
            tell(app, Problem::NoWindow, Some(window));
            false
        }
    }
}

/// Keystrokes now would reach where the hotkey was pressed, as sent.
fn on_target(target: Target) -> OnTarget {
    Arc::new(move || {
        crate::dictation::current_focus() == Some(target.focus) && !platform::any_modifier_held()
    })
}

/// Waits for the hotkey's keys to be let go, and checks the caret is still
/// where it was.
fn ready(target: Target, wait_for_focus: Duration) -> bool {
    let deadline = Instant::now() + KEYS_UP_WAIT.max(wait_for_focus);
    loop {
        let here = crate::dictation::current_focus() == Some(target.focus);
        if here && !platform::any_modifier_held() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// The selection, by UI Automation, else by copying it. `None` if there's no
/// way to tell; `Some("")` if nothing is selected.
fn read_selection(app: &AppHandle, on: OnTarget) -> Option<String> {
    match platform::selected_text() {
        Ok(text) => Some(text),
        Err(_) => match crate::clipboard::copy_selection(app, on) {
            Ok(copied) => Some(copied.unwrap_or_default()),
            Err(e) => {
                debug!("Couldn't copy the selection: {e}");
                None
            }
        },
    }
}

/// What was said with the editing hotkey, once transcribed.
pub async fn handle<C: Fn() -> bool>(app: &AppHandle, said: String, is_cancelled: C) {
    let Some(target) = pending().take() else {
        return;
    };
    let said = said.trim().to_string();
    if said.is_empty() {
        return;
    }
    if let Some(command) = commands::parse_hotkey(&said) {
        info!("Voice edit command: {command:?}");
        let app = app.clone();
        let _ = tauri::async_runtime::spawn_blocking(move || {
            if let Err(problem) = run_command(&app, &command, target) {
                tell(&app, problem, Some(target.window));
            }
        })
        .await;
        return;
    }
    if let Err(problem) = ai_edit(app, said, target, is_cancelled).await {
        tell(app, problem, Some(target.window));
    }
}

/// A command said alone during continuous dictation, typing into `window`.
/// Runs only while that window is still in front.
pub fn run_in_dictation(app: &AppHandle, command: &EditCommand, window: isize) {
    let target = crate::screen::foreground_target()
        .ok()
        .filter(|w| w.handle() == window)
        .and_then(|window| {
            crate::dictation::current_focus()
                .filter(|focus| focus.window == window.handle())
                .map(|focus| Target { window, focus })
        });
    let Some(target) = target else {
        debug!("Not running {command:?}: the dictation window isn't in front");
        return;
    };
    info!("Voice edit command in dictation: {command:?}");
    if let Err(problem) = run_command(app, command, target) {
        tell(app, problem, Some(target.window));
    }
}

fn run_command(app: &AppHandle, command: &EditCommand, target: Target) -> Result<(), Problem> {
    if !ready(target, Duration::ZERO) {
        return Err(Problem::WindowChanged);
    }
    let on = on_target(target);
    match command {
        EditCommand::NewParagraph | EditCommand::NewLine => {
            let text = if *command == EditCommand::NewParagraph {
                "\n\n"
            } else {
                "\n"
            };
            last_typed::forget();
            match crate::clipboard::type_segment(text, app, on) {
                Ok(TypeOutcome::WindowChanged) => Err(Problem::WindowChanged),
                Ok(_) => Ok(()),
                Err(e) => {
                    debug!("Couldn't type a line break: {e}");
                    Err(Problem::Failed)
                }
            }
        }
        // With the text in reach, words keep their punctuation and a line
        // leaves its line break; otherwise the standard keys below.
        EditCommand::SelectLastWords(n) if select(Selection::LastWords(*n), None).is_ok() => Ok(()),
        EditCommand::SelectLine if select(Selection::Line, None).is_ok() => Ok(()),
        EditCommand::SelectLastSentence => select(Selection::LastSentence, None),
        EditCommand::SelectLastParagraph => select(Selection::LastParagraph, None),
        EditCommand::SelectPhrase(phrase) => {
            select(Selection::Phrase(phrase.clone()), Some(phrase.clone()))
        }
        _ => {
            // Delete and Cut with nothing selected would take a character, or
            // a whole line in some editors.
            if matches!(command, EditCommand::DeleteThat | EditCommand::CutThat) {
                match read_selection(app, on.clone()) {
                    Some(selected) if !selected.is_empty() => {}
                    _ => return Err(Problem::NothingSelected),
                }
            }
            let chords = keys::chords(command).ok_or(Problem::Failed)?;
            match keys::send(app, &chords, on) {
                Ok(true) => Ok(()),
                Ok(false) => Err(Problem::WindowChanged),
                Err(e) => {
                    debug!("Couldn't press the keys for {command:?}: {e}");
                    Err(Problem::Failed)
                }
            }
        }
    }
}

fn select(what: Selection, phrase: Option<String>) -> Result<(), Problem> {
    platform::select(what).map_err(|e| match e {
        UiaError::NoTextPattern => Problem::NoTextPattern,
        UiaError::NothingBefore => Problem::NothingBefore,
        UiaError::NotFound => match phrase {
            Some(phrase) => Problem::NotFound(phrase),
            None => Problem::NothingBefore,
        },
        UiaError::Failed(e) => {
            debug!("UI Automation failed: {e}");
            Problem::NoTextPattern
        }
    })
}

/// The text to edit: what's selected, or the last text Kusema typed here,
/// selected back and checked.
fn find_text(app: &AppHandle, target: Target) -> Result<String, Problem> {
    if !ready(target, Duration::ZERO) {
        return Err(Problem::WindowChanged);
    }
    let on = on_target(target);
    match read_selection(app, on.clone()) {
        Some(selected) if !selected.is_empty() => {
            debug!("Voice edit: using the selection");
            return Ok(selected);
        }
        _ => {}
    }

    let typed = last_typed::get();
    let typed = last_typed::check(
        typed.as_ref(),
        crate::dictation::current_focus(),
        Instant::now(),
    )
    .map_err(|refusal| {
        debug!("Not using the last typed text: {refusal:?}");
        Problem::SelectFirst
    })?
    .clone();
    let steps = text::caret_steps(&typed.text);
    if steps > MAX_SELECT_BACK {
        return Err(Problem::SelectFirst);
    }
    match keys::send(app, &[keys::select_back(steps)], on.clone()) {
        Ok(true) => {}
        Ok(false) => return Err(Problem::WindowChanged),
        Err(_) => return Err(Problem::Failed),
    }
    std::thread::sleep(SETTLE);
    match read_selection(app, on.clone()) {
        Some(back) if text::same_text(&typed.text, &back) => {
            debug!("Voice edit: using the last text Kusema typed");
            Ok(back)
        }
        _ => {
            // Not what was typed: put the caret back where it was.
            let _ = keys::send(app, &[keys::collapse_to_end()], on);
            Err(Problem::SelectFirst)
        }
    }
}

async fn ai_edit<C: Fn() -> bool>(
    app: &AppHandle,
    instruction: String,
    target: Target,
    is_cancelled: C,
) -> Result<(), Problem> {
    let settings = get_settings(app);
    let ai = crate::assistant::resolve_target(&settings).map_err(Problem::Said)?;

    let app_for_find = app.clone();
    let original = tauri::async_runtime::spawn_blocking(move || find_text(&app_for_find, target))
        .await
        .map_err(|_| Problem::Failed)??;
    info!(
        "Voice edit: {} characters, instruction of {} characters, to {}",
        original.chars().count(),
        instruction.chars().count(),
        ai.provider.id
    );

    let system = prompt::system_prompt(&instruction);
    let turns = prompt::turns(&original);
    let request = crate::llm_client::send_conversation_for(
        Some(Purpose::VoiceEdit),
        &ai.provider,
        ai.api_key.clone(),
        &ai.model,
        &system,
        &turns,
        |_| {},
    );
    let Some(result) = complete_unless_cancelled(request, &is_cancelled).await else {
        info!("Voice edit cancelled");
        return Ok(());
    };
    let reply = match result {
        Ok(reply) => reply.unwrap_or_default(),
        Err(e) => {
            return Err(Problem::Said(crate::assistant::describe_error(
                &ai.provider,
                &e,
                false,
            )))
        }
    };
    let replacement =
        prompt::finish(&original, &reply).ok_or(Problem::EmptyReply(ai.provider.label.clone()))?;
    if text::normalize(&replacement) == text::normalize(&original) {
        return Err(Problem::NoChange);
    }

    let previewed = settings.voice_edit_preview;
    if previewed {
        let (process, class) = platform::app_of(target.window.handle(), target.focus.control);
        let card = ScreenCard::Edit {
            original: original.clone(),
            replacement: replacement.clone(),
            instruction: instruction.clone(),
            provider: format!("{} \u{b7} {}", ai.provider.label, ai.model),
            rich_text: text::keeps_rich_text(&process, &class),
            timeout_secs: card::EDIT_TIMEOUT.as_secs() as u32,
        };
        crate::utils::hide_recording_overlay(app);
        let answer = card::confirm_edit(app, target.window, card, &is_cancelled).await;
        if answer != Confirmation::Send {
            info!("Voice edit not applied ({answer:?})");
            return Ok(());
        }
    }

    let app_for_apply = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        apply(&app_for_apply, target, &original, &replacement, previewed)
    })
    .await
    .map_err(|_| Problem::Failed)?
}

/// Types `replacement` over the selection, if it's still `original`.
fn apply(
    app: &AppHandle,
    target: Target,
    original: &str,
    replacement: &str,
    previewed: bool,
) -> Result<(), Problem> {
    // The preview card had the focus; it went back as the card closed.
    let wait = if previewed {
        FOCUS_BACK_WAIT
    } else {
        Duration::ZERO
    };
    if !ready(target, wait) {
        crate::screen::activate(&target.window);
        if !ready(target, wait) {
            return Err(Problem::WindowChanged);
        }
    }
    let on = on_target(target);
    match read_selection(app, on.clone()) {
        Some(now) if text::same_text(original, &now) => {}
        _ => return Err(Problem::SelectionChanged),
    }
    match crate::clipboard::type_segment(replacement, app, on) {
        Ok(TypeOutcome::Typed) => {
            last_typed::record(replacement, crate::dictation::current_focus());
            info!("Voice edit applied");
            let _ = app.emit("voice-edit-applied", ());
            Ok(())
        }
        Ok(TypeOutcome::NotTyping) => Ok(()),
        Ok(TypeOutcome::WindowChanged) => Err(Problem::WindowChanged),
        Err(e) => {
            debug!("Couldn't type the edit: {e}");
            Err(Problem::Failed)
        }
    }
}

/// Normal dictation typed `text` (before any trailing space the paste adds).
pub fn note_pasted(app: &AppHandle, text: &str) {
    let settings = get_settings(app);
    if settings.paste_method == crate::settings::PasteMethod::None {
        return;
    }
    let typed = if settings.append_trailing_space && !text.ends_with('\n') {
        format!("{text} ")
    } else {
        text.to_string()
    };
    last_typed::record(&typed, crate::dictation::current_focus());
}

/// Every command and its phrases, for the "What can I say?" list.
#[tauri::command]
#[specta::specta]
pub fn get_voice_commands() -> Vec<commands::CommandInfo> {
    commands::list()
}

#[tauri::command]
#[specta::specta]
pub fn change_voice_edit_preview_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = get_settings(&app);
    settings.voice_edit_preview = enabled;
    write_settings(&app, settings);
    let _ = app.emit(
        "settings-changed",
        serde_json::json!({ "setting": "voice_edit_preview" }),
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_problem_has_a_translation_key_except_worded_ones() {
        for problem in [
            Problem::NoWindow,
            Problem::Unsupported,
            Problem::SelectFirst,
            Problem::NothingSelected,
            Problem::NoTextPattern,
            Problem::NothingBefore,
            Problem::NotFound("x".into()),
            Problem::WindowChanged,
            Problem::SelectionChanged,
            Problem::NoChange,
            Problem::EmptyReply("x".into()),
            Problem::Failed,
        ] {
            let (key, english) = problem.key_and_english();
            let key = key.unwrap();
            assert!(!english.is_empty());
            // The key is in the English translation file.
            let en: serde_json::Value = serde_json::from_str(include_str!(
                "../../../src/i18n/locales/en/translation.json"
            ))
            .unwrap();
            assert!(
                en["voiceEdit"]["notice"][key].is_string(),
                "voiceEdit.notice.{key} is missing"
            );
        }
        assert_eq!(
            Problem::Said("Privileged mode is on".into()).key_and_english(),
            (None, "Privileged mode is on".to_string())
        );
    }
}
