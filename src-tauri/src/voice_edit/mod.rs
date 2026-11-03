//! Kusema voice editing: select text in any app, hold the editing hotkey and
//! say what to do with it. A command ("select last word", "undo that") runs
//! here, with standard keystrokes or UI Automation, and nothing leaves the
//! computer. Anything else is an instruction for the AI provider ("make this
//! more formal"): the selection and the instruction go to it, a preview card
//! shows the result, and Apply types it over the selection, so Ctrl+Z in the
//! app undoes it. In continuous dictation, a segment that starts with the
//! trigger word ("Kusema, shorten that") does the same.
//!
//! With nothing selected, the instruction applies to the last text Kusema
//! typed in that window and control, if it can be selected back and read as
//! exactly what was typed (see `last_typed`).
//!
//! Every way an edit can fail says so, in the overlay (or on a card when the
//! overlay is off), and aloud when replies are spoken.

pub mod commands;
mod correct;
mod keys;
pub mod last_typed;
mod platform;
pub mod prompt;
pub mod text;

use commands::EditCommand;
pub use correct::correct_from_tray;
use log::{debug, info};
use platform::{Selection, UiaError};
use serde::Serialize;
use specta::Type;
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
/// How many extra Ctrl+Shift+Left presses "select last word" may add when
/// the first only reached punctuation.
const MAX_EXTRA_WORD_STEPS: usize = 3;
/// How long a message stays in the overlay.
const NOTICE_SECS: u64 = 5;

/// Where the hotkey was pressed, and what the app there can do.
#[derive(Clone, Copy, Debug)]
struct Target {
    window: CapturedWindow,
    focus: FocusToken,
    /// Known not to show its text to UI Automation: don't ask, use keys and
    /// the clipboard.
    hides_text: bool,
    /// Keeps formatting a plain-text replacement loses.
    rich_text: bool,
    /// Started from continuous dictation, whose overlay stays up.
    in_dictation: bool,
}

impl Target {
    fn new(window: CapturedWindow, focus: FocusToken, in_dictation: bool) -> Self {
        let (process, class) = platform::app_of(window.handle(), focus.control);
        let title = platform::window_title(window.handle());
        let hides_text = text::hides_text(&process, &title);
        if hides_text {
            debug!("Voice edit: {process} doesn't show its text to UI Automation");
        }
        Self {
            window,
            focus,
            hides_text,
            rich_text: text::keeps_rich_text(&process, &class),
            in_dictation,
        }
    }
}

static PENDING: Mutex<Option<Target>> = Mutex::new(None);

fn pending() -> std::sync::MutexGuard<'static, Option<Target>> {
    PENDING.lock().unwrap_or_else(|e| e.into_inner())
}

/// Why nothing was changed. Each has a translated message for the overlay
/// and the English one that's spoken and logged.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Problem {
    NoWindow,
    Unsupported,
    NoInstruction,
    SelectFirst,
    NothingSelected,
    /// The app didn't show the selection, and copying it didn't work either.
    CantRead,
    /// "delete that" couldn't confirm a selection either way.
    CantConfirm,
    NoTextPattern,
    NothingBefore,
    NotFound(String),
    WindowChanged,
    SelectionChanged,
    NoChange,
    EmptyReply(String),
    TimedOut,
    Failed,
    /// "Correct that" with nothing Kusema typed here to correct.
    CorrectNotFound,
    /// What Kusema typed has been changed or moved since.
    CorrectChanged,
    /// Already worded (no provider, AI provider errors, Privileged mode).
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
            Problem::NoInstruction => (
                "noInstruction",
                "Kusema didn't catch an instruction. Hold the shortcut while you speak.".to_string(),
            ),
            Problem::SelectFirst => (
                "selectFirst",
                "Select the text you want to change first.".to_string(),
            ),
            Problem::NothingSelected => ("nothingSelected", "Nothing is selected.".to_string()),
            Problem::CantRead => (
                "cantRead",
                "Couldn't read the selected text in this app.".to_string(),
            ),
            Problem::CantConfirm => (
                "cantConfirm",
                "Couldn't tell whether anything is selected in this app, so nothing was deleted."
                    .to_string(),
            ),
            Problem::NoTextPattern => (
                "noTextPattern",
                "This app doesn't let Kusema find text by voice. Select it with the mouse or keyboard instead.".to_string(),
            ),
            Problem::NothingBefore => (
                "nothingBefore",
                "There's no text before the cursor.".to_string(),
            ),
            Problem::NotFound(phrase) => (
                "notFound",
                format!("Couldn't find \u{201C}{phrase}\u{201D}."),
            ),
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
            Problem::TimedOut => (
                "timedOut",
                "The preview timed out, so nothing was replaced.".to_string(),
            ),
            Problem::Failed => ("failed", "Couldn't do that in this window.".to_string()),
            Problem::CorrectNotFound => (
                "correctNotFound",
                "There's nothing Kusema typed here to correct. Correct it from History instead."
                    .to_string(),
            ),
            Problem::CorrectChanged => (
                "correctChanged",
                "The text Kusema typed has changed since, so it can't be corrected here. Correct it from History instead.".to_string(),
            ),
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

/// A message for the overlay, emitted as `voice-edit-notice`.
#[derive(Clone, Debug, Serialize, Type)]
pub struct EditNotice {
    /// Translation key, if it has one; else `message` is shown as is.
    pub key: Option<String>,
    /// For the key's `{{name}}` placeholder.
    pub name: Option<String>,
    /// The English, as spoken and logged.
    pub message: String,
    /// Shown inside continuous dictation's overlay rather than on its own.
    pub in_dictation: bool,
}

/// Says what went wrong where the user is looking: in the overlay, or on a
/// card beside the window when the overlay is off. `true` if the overlay is
/// showing it, so it mustn't be hidden right away.
fn tell(app: &AppHandle, problem: Problem, target: Option<Target>) -> bool {
    info!("Voice edit: {problem:?}");
    let (key, english) = problem.key_and_english();
    let key = key.map(|k| format!("voiceEdit.notice.{k}"));
    let in_dictation = target.is_some_and(|t| t.in_dictation);
    let notice = EditNotice {
        key: key.clone(),
        name: problem.name(),
        message: english.clone(),
        in_dictation,
    };
    if crate::overlay::show_edit_notice(app, &notice, in_dictation, NOTICE_SECS) {
        crate::tts::say(app, &english);
        !in_dictation
    } else {
        card::notice_with_key(app, english, key, problem.name(), target.map(|t| t.window));
        false
    }
}

/// Something that went right, in the overlay like a problem would be.
fn inform(
    app: &AppHandle,
    key: &str,
    name: Option<String>,
    english: String,
    target: Option<Target>,
) {
    info!("Voice edit: {english}");
    let in_dictation = target.is_some_and(|t| t.in_dictation);
    let key = format!("voiceEdit.notice.{key}");
    let notice = EditNotice {
        key: Some(key.clone()),
        name: name.clone(),
        message: english.clone(),
        in_dictation,
    };
    if !crate::overlay::show_edit_notice(app, &notice, in_dictation, NOTICE_SECS) {
        card::notice_with_key(app, english, Some(key), name, target.map(|t| t.window));
    }
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
            *pending() = Some(Target::new(window, focus, false));
            true
        }
        _ => {
            tell(app, Problem::NoWindow, None);
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

/// What's selected, as far as Kusema can tell.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Selected {
    Text(String),
    /// Really nothing: UI Automation shows the document's text and no
    /// selection, or copying put nothing on the clipboard.
    Nothing,
    /// The app doesn't show its text and the clipboard couldn't be used.
    Unknown,
}

/// The decision behind `read_selection`. `uia` is `None` when the app is known
/// to hide its text, else what UI Automation said: the selected text and
/// whether it can see any text at all. An empty answer from a control that
/// shows no text proves nothing (apps that draw their own text give UI
/// Automation an empty stand-in), so then the clipboard is asked.
fn decide_selection(
    uia: Option<Result<(String, bool), UiaError>>,
    copy: impl FnOnce() -> Result<Option<String>, String>,
) -> Selected {
    match uia {
        Some(Ok((text, _))) if !text.is_empty() => return Selected::Text(text),
        Some(Ok((_, true))) => return Selected::Nothing,
        _ => {}
    }
    match copy() {
        Ok(Some(text)) => Selected::Text(text),
        Ok(None) => Selected::Nothing,
        Err(e) => {
            debug!("Couldn't copy the selection: {e}");
            Selected::Unknown
        }
    }
}

fn read_selection(app: &AppHandle, target: Target) -> Selected {
    let uia = (!target.hides_text).then(platform::selection);
    decide_selection(uia, || {
        crate::clipboard::copy_selection(app, on_target(target))
    })
}

/// What was said with the editing hotkey, once transcribed. `true` if the
/// overlay is showing a message about it, which hides itself.
pub async fn handle<C: Fn() -> bool>(app: &AppHandle, said: String, is_cancelled: C) -> bool {
    let Some(target) = pending().take() else {
        // The press already said why it couldn't start.
        return false;
    };
    let said = said.trim().to_string();
    if said.is_empty() {
        return tell(app, Problem::NoInstruction, Some(target));
    }
    if let Some(command) = commands::parse_hotkey(&said) {
        info!("Voice edit command: {command:?}");
        let app = app.clone();
        return tauri::async_runtime::spawn_blocking(move || {
            match run_command(&app, &command, target) {
                Ok(()) => false,
                Err(problem) => tell(&app, problem, Some(target)),
            }
        })
        .await
        .unwrap_or(false);
    }
    match ai_edit(app, said, target, is_cancelled).await {
        Ok(()) => false,
        Err(problem) => tell(app, problem, Some(target)),
    }
}

/// `window`, if it's in front, with the control focused in it.
fn window_target(window: isize, in_dictation: bool) -> Option<Target> {
    let window = crate::screen::foreground_target()
        .ok()
        .filter(|w| w.handle() == window)?;
    crate::dictation::current_focus()
        .filter(|focus| focus.window == window.handle())
        .map(|focus| Target::new(window, focus, in_dictation))
}

/// The window continuous dictation types into, if it's still in front.
fn dictation_target(window: isize) -> Option<Target> {
    window_target(window, true)
}

/// A command said alone during continuous dictation, typing into `window`.
/// Runs only while that window is still in front.
pub fn run_in_dictation(app: &AppHandle, command: &EditCommand, window: isize) {
    let Some(target) = dictation_target(window) else {
        debug!("Not running {command:?}: the dictation window isn't in front");
        return;
    };
    info!("Voice edit command in dictation: {command:?}");
    if let Err(problem) = run_command(app, command, target) {
        tell(app, problem, Some(target));
    }
}

/// What followed the trigger word in continuous dictation: a command, or an
/// instruction for the AI with the usual preview. Blocks until it's done, so
/// the session types nothing while the preview is open and carries on after.
pub fn run_trigger_in_dictation(app: &AppHandle, said: &str, window: isize) {
    let Some(target) = dictation_target(window) else {
        debug!("Not running a voice edit: the dictation window isn't in front");
        return;
    };
    if let Some(command) = commands::parse_hotkey(said) {
        info!("Voice edit command after the trigger word: {command:?}");
        if let Err(problem) = run_command(app, &command, target) {
            tell(app, problem, Some(target));
        }
        return;
    }
    info!("Voice edit in dictation");
    let notice = EditNotice {
        key: Some("voiceEdit.notice.working".to_string()),
        name: None,
        message: "Editing…".to_string(),
        in_dictation: true,
    };
    crate::overlay::show_edit_notice(app, &notice, true, NOTICE_SECS);
    let result = tauri::async_runtime::block_on(ai_edit(app, said.to_string(), target, || false));
    if let Err(problem) = result {
        tell(app, problem, Some(target));
    }
}

/// One step of selecting words with the keyboard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WordKey {
    /// Ctrl+Shift+Left.
    WordLeft,
    /// Right: back to where the caret was.
    Collapse,
    /// Shift+Home.
    LineStart,
}

/// Selects `n` words before the caret with the keyboard, then checks what
/// that took with `read` (the clipboard), as far as it can: Ctrl+Shift+Left
/// stops at punctuation in most apps, so "Hello." first gives ".", and on a
/// line with one word it can reach into the line before. Without a way to
/// read, the keys alone have to do. `false` if a key couldn't be sent.
fn select_words_by_keys(
    n: u32,
    mut press: impl FnMut(WordKey, u32) -> bool,
    mut read: impl FnMut() -> Option<String>,
) -> bool {
    if !press(WordKey::WordLeft, n) {
        return false;
    }
    for _ in 0..MAX_EXTRA_WORD_STEPS {
        let Some(selected) = read() else {
            break;
        };
        match text::check_word_selection(&selected, n as usize) {
            text::WordSelection::Done => break,
            text::WordSelection::Extend => {
                if !press(WordKey::WordLeft, 1) {
                    return false;
                }
            }
            text::WordSelection::LineStart => {
                return press(WordKey::Collapse, 1) && press(WordKey::LineStart, 1);
            }
        }
    }
    true
}

fn run_command(app: &AppHandle, command: &EditCommand, target: Target) -> Result<(), Problem> {
    if !ready(target, Duration::ZERO) {
        return Err(Problem::WindowChanged);
    }
    let on = on_target(target);
    let send = |chords: &[keys::Chord]| match keys::send(app, chords, on.clone()) {
        Ok(true) => Ok(()),
        Ok(false) => Err(Problem::WindowChanged),
        Err(e) => {
            debug!("Couldn't press the keys for {command:?}: {e}");
            Err(Problem::Failed)
        }
    };
    let uia = !target.hides_text;
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
        // leaves its line break; otherwise the standard keys, checked.
        EditCommand::SelectLastWords(n) => {
            if uia && select(Selection::LastWords(*n), None).is_ok() {
                return Ok(());
            }
            let mut failed = false;
            let sent = select_words_by_keys(
                *n,
                |key, times| {
                    let chord = match key {
                        WordKey::WordLeft => keys::words_left(times),
                        WordKey::Collapse => keys::collapse_to_end(),
                        WordKey::LineStart => keys::select_to_line_start(),
                    };
                    let ok = send(&[chord]).is_ok();
                    failed |= !ok;
                    ok
                },
                || {
                    std::thread::sleep(SETTLE);
                    crate::clipboard::copy_selection(app, on.clone())
                        .ok()
                        .flatten()
                },
            );
            if sent && !failed {
                Ok(())
            } else {
                Err(Problem::WindowChanged)
            }
        }
        EditCommand::SelectLine if uia && select(Selection::Line, None).is_ok() => Ok(()),
        EditCommand::CorrectThat => correct::correct_last(app, target),
        EditCommand::SelectLastSentence | EditCommand::SelectLastParagraph if !uia => {
            Err(Problem::NoTextPattern)
        }
        EditCommand::SelectPhrase(_) if !uia => Err(Problem::NoTextPattern),
        EditCommand::SelectLastSentence => select(Selection::LastSentence, None),
        EditCommand::SelectLastParagraph => select(Selection::LastParagraph, None),
        EditCommand::SelectPhrase(phrase) => {
            select(Selection::Phrase(phrase.clone()), Some(phrase.clone()))
        }
        _ => {
            // Delete and Cut with nothing selected would take a character, or
            // a whole line in some editors: only with a selection confirmed.
            if matches!(command, EditCommand::DeleteThat | EditCommand::CutThat) {
                match read_selection(app, target) {
                    Selected::Text(_) => {}
                    Selected::Nothing => return Err(Problem::NothingSelected),
                    Selected::Unknown => return Err(Problem::CantConfirm),
                }
            }
            let chords = keys::chords(command).ok_or(Problem::Failed)?;
            send(&chords)
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
/// selected back and checked. The flag says Kusema made that selection.
fn find_text(app: &AppHandle, target: Target) -> Result<(String, bool), Problem> {
    if !ready(target, Duration::ZERO) {
        return Err(Problem::WindowChanged);
    }
    let on = on_target(target);
    let unreadable = match read_selection(app, target) {
        Selected::Text(selected) => {
            debug!("Voice edit: using the selection");
            return Ok((selected, false));
        }
        Selected::Nothing => false,
        Selected::Unknown => true,
    };
    // With nothing to read the selection by, the last typed text can't be
    // checked either.
    let cant_read = if unreadable {
        Problem::CantRead
    } else {
        Problem::SelectFirst
    };

    let typed = last_typed::get();
    let typed = last_typed::check(
        typed.as_ref(),
        crate::dictation::current_focus(),
        Instant::now(),
    )
    .map_err(|refusal| {
        debug!("Not using the last typed text: {refusal:?}");
        cant_read.clone()
    })?
    .clone();
    if unreadable {
        return Err(cant_read);
    }
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
    match read_selection(app, target) {
        Selected::Text(back) if text::same_text(&typed.text, &back) => {
            debug!("Voice edit: using the last text Kusema typed");
            Ok((back, true))
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
    let (original, selected_by_us) =
        tauri::async_runtime::spawn_blocking(move || find_text(&app_for_find, target))
            .await
            .map_err(|_| Problem::Failed)??;
    let outcome = edit_text(
        app,
        &settings,
        &ai,
        instruction,
        target,
        original,
        &is_cancelled,
    )
    .await;
    // The text Kusema selected back stays selected only while it's being
    // edited: left selected, the next dictated words would replace it.
    if selected_by_us
        && !matches!(
            outcome,
            Ok(Edited::Applied) | Err(Problem::SelectionChanged)
        )
    {
        let app = app.clone();
        let _ = tauri::async_runtime::spawn_blocking(move || {
            if ready(target, FOCUS_BACK_WAIT) {
                let _ = keys::send(&app, &[keys::collapse_to_end()], on_target(target));
            }
        })
        .await;
    }
    outcome.map(|_| ())
}

/// How an edit ended, when it didn't fail.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Edited {
    Applied,
    /// Cancelled, by the user or the Cancel hotkey.
    NotApplied,
}

async fn edit_text<C: Fn() -> bool>(
    app: &AppHandle,
    settings: &crate::settings::AppSettings,
    ai: &crate::assistant::Target,
    instruction: String,
    target: Target,
    original: String,
    is_cancelled: &C,
) -> Result<Edited, Problem> {
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
    let Some(result) = complete_unless_cancelled(request, is_cancelled).await else {
        info!("Voice edit cancelled");
        return Ok(Edited::NotApplied);
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
        let card = ScreenCard::Edit {
            original: original.clone(),
            replacement: replacement.clone(),
            instruction: instruction.clone(),
            provider: format!("{} \u{b7} {}", ai.provider.label, ai.model),
            rich_text: target.rich_text,
            timeout_secs: card::EDIT_TIMEOUT.as_secs() as u32,
        };
        // Continuous dictation keeps its own overlay up.
        if !target.in_dictation {
            crate::utils::hide_recording_overlay(app);
        }
        let answer = card::confirm_edit(app, target.window, card, is_cancelled).await;
        match answer {
            Confirmation::Send => {}
            Confirmation::TimedOut => return Err(Problem::TimedOut),
            _ => {
                info!("Voice edit not applied ({answer:?})");
                return Ok(Edited::NotApplied);
            }
        }
    }

    let app_for_apply = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        apply(&app_for_apply, target, &original, &replacement, previewed)
    })
    .await
    .map_err(|_| Problem::Failed)?
    .map(|()| Edited::Applied)
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
    match read_selection(app, target) {
        Selected::Text(now) if text::same_text(original, &now) => {}
        Selected::Unknown => return Err(Problem::CantRead),
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

/// The word that starts an AI edit in continuous dictation. Empty turns that
/// off; surrounding punctuation is dropped.
#[tauri::command]
#[specta::specta]
pub fn change_voice_edit_trigger_setting(app: AppHandle, trigger: String) -> Result<(), String> {
    let trigger = trigger
        .trim()
        .trim_matches(|c: char| !c.is_alphanumeric())
        .to_string();
    let mut settings = get_settings(&app);
    settings.voice_edit_trigger = trigger;
    write_settings(&app, settings);
    let _ = app.emit(
        "settings-changed",
        serde_json::json!({ "setting": "voice_edit_trigger" }),
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_problem_has_a_translation_key_except_worded_ones() {
        let en: serde_json::Value = serde_json::from_str(include_str!(
            "../../../src/i18n/locales/en/translation.json"
        ))
        .unwrap();
        for problem in [
            Problem::NoWindow,
            Problem::Unsupported,
            Problem::NoInstruction,
            Problem::SelectFirst,
            Problem::NothingSelected,
            Problem::CantRead,
            Problem::CantConfirm,
            Problem::NoTextPattern,
            Problem::NothingBefore,
            Problem::NotFound("x".into()),
            Problem::WindowChanged,
            Problem::SelectionChanged,
            Problem::NoChange,
            Problem::EmptyReply("x".into()),
            Problem::TimedOut,
            Problem::Failed,
            Problem::CorrectNotFound,
            Problem::CorrectChanged,
        ] {
            let (key, english) = problem.key_and_english();
            let key = key.unwrap();
            assert!(!english.is_empty());
            assert!(
                en["voiceEdit"]["notice"][key].is_string(),
                "voiceEdit.notice.{key} is missing"
            );
        }
        for key in ["working", "corrected", "learned"] {
            assert!(en["voiceEdit"]["notice"][key].is_string(), "{key}");
        }
        assert_eq!(
            Problem::Said("Privileged mode is on".into()).key_and_english(),
            (None, "Privileged mode is on".to_string())
        );
    }

    fn copied(text: Option<&str>) -> impl FnOnce() -> Result<Option<String>, String> {
        let text = text.map(str::to_string);
        move || Ok(text)
    }

    fn never() -> Result<Option<String>, String> {
        panic!("the clipboard shouldn't be asked")
    }

    fn unavailable() -> Result<Option<String>, String> {
        Err("The clipboard is off in an isolated test run".to_string())
    }

    #[test]
    fn a_selection_ui_automation_shows_is_used_as_is() {
        assert_eq!(
            decide_selection(Some(Ok(("We agree.".into(), true))), never),
            Selected::Text("We agree.".into())
        );
    }

    #[test]
    fn nothing_selected_in_text_ui_automation_can_see_is_really_nothing() {
        assert_eq!(
            decide_selection(Some(Ok((String::new(), true))), never),
            Selected::Nothing
        );
    }

    #[test]
    fn an_app_without_text_for_ui_automation_falls_back_to_the_clipboard() {
        // ONLYOFFICE: no text pattern at all.
        assert_eq!(
            decide_selection(Some(Err(UiaError::NoTextPattern)), copied(Some("Hello."))),
            Selected::Text("Hello.".into())
        );
        // An empty stand-in (a hidden input box): empty, and no text either.
        assert_eq!(
            decide_selection(Some(Ok((String::new(), false))), copied(Some("Hello."))),
            Selected::Text("Hello.".into())
        );
        // Known to hide its text: UI Automation isn't asked.
        assert_eq!(
            decide_selection(None, copied(Some("Hello."))),
            Selected::Text("Hello.".into())
        );
    }

    #[test]
    fn the_clipboard_tells_nothing_selected_from_cant_tell() {
        assert_eq!(decide_selection(None, copied(None)), Selected::Nothing);
        assert_eq!(decide_selection(None, unavailable), Selected::Unknown);
        assert_eq!(
            decide_selection(Some(Err(UiaError::NoTextPattern)), unavailable),
            Selected::Unknown
        );
    }

    /// A text box with no UI Automation, whose Ctrl+Shift+Left stops at
    /// punctuation and at line breaks, as ONLYOFFICE's and Word's do.
    struct FakeEditor {
        text: Vec<char>,
        anchor: usize,
        caret: usize,
    }

    impl FakeEditor {
        fn new(text: &str) -> Self {
            let text: Vec<char> = text.chars().collect();
            let end = text.len();
            Self {
                text,
                anchor: end,
                caret: end,
            }
        }

        fn class(c: char) -> u8 {
            if c == '\n' {
                0
            } else if c.is_whitespace() {
                1
            } else if c.is_alphanumeric() {
                2
            } else {
                3
            }
        }

        fn press(&mut self, key: WordKey) {
            match key {
                WordKey::WordLeft => {
                    let mut i = self.caret;
                    while i > 0 && Self::class(self.text[i - 1]) == 1 {
                        i -= 1;
                    }
                    if i > 0 {
                        let kind = Self::class(self.text[i - 1]);
                        i -= 1;
                        if kind != 0 {
                            while i > 0 && Self::class(self.text[i - 1]) == kind {
                                i -= 1;
                            }
                        }
                    }
                    self.caret = i;
                }
                WordKey::Collapse => {
                    let end = self.caret.max(self.anchor);
                    self.caret = end;
                    self.anchor = end;
                }
                WordKey::LineStart => {
                    let mut i = self.caret;
                    while i > 0 && self.text[i - 1] != '\n' {
                        i -= 1;
                    }
                    self.caret = i;
                }
            }
        }

        fn selected(&self) -> String {
            let (a, b) = (self.caret.min(self.anchor), self.caret.max(self.anchor));
            self.text[a..b].iter().collect()
        }
    }

    fn select_in(text: &str, n: u32, can_read: bool) -> String {
        let editor = std::cell::RefCell::new(FakeEditor::new(text));
        let done = select_words_by_keys(
            n,
            |key, times| {
                for _ in 0..times {
                    editor.borrow_mut().press(key);
                }
                true
            },
            || can_read.then(|| editor.borrow().selected()),
        );
        assert!(done);
        let selected = editor.borrow().selected();
        selected
    }

    #[test]
    fn select_last_word_takes_the_word_with_its_punctuation() {
        assert_eq!(select_in("Dear Sir,\nHello.", 1, true), "Hello.");
        assert_eq!(select_in("The parties may appeal.", 1, true), "appeal.");
        assert_eq!(select_in("within ten days", 2, true), "ten days");
    }

    #[test]
    fn a_single_word_line_never_reaches_into_the_line_before() {
        assert_eq!(select_in("Dear Sir,\nHello.", 2, true), "Hello.");
        assert_eq!(select_in("Dear Sir,\nHello", 1, true), "Hello");
    }

    #[test]
    fn without_the_clipboard_the_keys_alone_have_to_do() {
        // What the plain keystroke gives, and why the check matters.
        assert_eq!(select_in("Dear Sir,\nHello.", 1, false), ".");
    }
}
