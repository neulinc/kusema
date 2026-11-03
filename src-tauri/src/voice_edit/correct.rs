//! "Correct that": the last dictated text in a small box, to fix by hand.
//! Kusema replaces it in the document under the same rules as "scratch
//! that" (the same window and control, and the text exactly as typed, so a
//! click or more typing since refuses), then learns the words that changed
//! as personal vocabulary. The text can also be corrected from History,
//! which changes History only.

use log::{debug, info};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};

use super::{
    keys, last_typed, on_target, read_selection, ready, text, Problem, Selected, Target,
    FOCUS_BACK_WAIT, MAX_SELECT_BACK, SETTLE,
};
use crate::clipboard::TypeOutcome;
use crate::managers::history::HistoryManager;

/// What a correction learned, for the overlay: "Canela → Kanella".
fn learned_list(learned: &[crate::vocabulary::learn::Correction]) -> String {
    learned
        .iter()
        .map(|c| format!("{} \u{2192} {}", c.heard, c.written))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The last text Kusema typed where the caret is, selected and checked, or
/// why not.
fn select_last_typed(app: &AppHandle, target: Target) -> Result<String, Problem> {
    if !ready(target, Duration::ZERO) {
        return Err(Problem::WindowChanged);
    }
    let typed = last_typed::get();
    let typed = last_typed::check(
        typed.as_ref(),
        crate::dictation::current_focus(),
        Instant::now(),
    )
    .map_err(|refusal| {
        debug!("Not correcting: {refusal:?}");
        Problem::CorrectNotFound
    })?
    .clone();
    let steps = text::caret_steps(&typed.text);
    if steps > MAX_SELECT_BACK {
        return Err(Problem::CorrectChanged);
    }
    let on = on_target(target);
    match keys::send(app, &[keys::select_back(steps)], on.clone()) {
        Ok(true) => {}
        Ok(false) => return Err(Problem::WindowChanged),
        Err(_) => return Err(Problem::Failed),
    }
    std::thread::sleep(SETTLE);
    match read_selection(app, target) {
        Selected::Text(back) if text::same_text(&typed.text, &back) => Ok(typed.text),
        other => {
            let _ = keys::send(app, &[keys::collapse_to_end()], on);
            Err(match other {
                Selected::Unknown => Problem::CantRead,
                _ => Problem::CorrectChanged,
            })
        }
    }
}

/// Puts the caret back after the selection Kusema made.
fn unselect(app: &AppHandle, target: Target) {
    if ready(target, FOCUS_BACK_WAIT) {
        let _ = keys::send(app, &[keys::collapse_to_end()], on_target(target));
    }
}

/// Runs "correct that" where `target` is. Blocks until the box is answered.
pub(super) fn correct_last(app: &AppHandle, target: Target) -> Result<(), Problem> {
    let original = select_last_typed(app, target)?;
    let shown = original.trim_end().to_string();
    if !target.in_dictation {
        crate::utils::hide_recording_overlay(app);
    }
    let corrected = tauri::async_runtime::block_on(crate::screen::card::confirm_correction(
        app,
        target.window,
        &shown,
    ));
    let Some(corrected) = corrected
        .map(|c| c.trim().to_string())
        .filter(|c| !c.is_empty())
    else {
        info!("Correction cancelled");
        unselect(app, target);
        return Ok(());
    };
    if corrected == shown.trim() {
        unselect(app, target);
        return Ok(());
    }

    // Focus came back from the box; the selection must still be the text.
    if !ready(target, FOCUS_BACK_WAIT) {
        crate::screen::activate(&target.window);
        if !ready(target, FOCUS_BACK_WAIT) {
            return Err(Problem::WindowChanged);
        }
    }
    let on = on_target(target);
    match read_selection(app, target) {
        Selected::Text(now) if text::same_text(&original, &now) => {}
        Selected::Unknown => return Err(Problem::CantRead),
        _ => return Err(Problem::SelectionChanged),
    }
    // What followed the dictated words (a trailing space) stays.
    let trailing = &original[original.trim_end().len()..];
    let replacement = format!("{corrected}{trailing}");
    match crate::clipboard::type_segment(&replacement, app, on) {
        Ok(TypeOutcome::Typed) => {}
        Ok(TypeOutcome::NotTyping) => {}
        Ok(TypeOutcome::WindowChanged) => return Err(Problem::WindowChanged),
        Err(e) => {
            debug!("Couldn't type the correction: {e}");
            return Err(Problem::Failed);
        }
    }
    last_typed::record(&replacement, crate::dictation::current_focus());
    update_history(app, &shown, &corrected);
    let learned = crate::vocabulary::learn_from(app, &shown, &corrected);
    info!("Corrected the last dictation; learned {}", learned.len());
    super::inform(
        app,
        if learned.is_empty() {
            "corrected"
        } else {
            "learned"
        },
        Some(learned_list(&learned)),
        if learned.is_empty() {
            "Corrected.".to_string()
        } else {
            format!("Corrected. Kusema learned: {}", learned_list(&learned))
        },
        Some(target),
    );
    Ok(())
}

/// The latest History entry, if it's the text just corrected.
fn update_history(app: &AppHandle, original: &str, corrected: &str) {
    let Some(hm) = app.try_state::<Arc<HistoryManager>>() else {
        return;
    };
    match hm.get_latest_completed_entry() {
        Ok(Some(entry)) if entry.transcription_text.trim().ends_with(original.trim()) => {
            let text = entry.transcription_text.trim_end();
            let kept = &text[..text.len() - original.trim().len()];
            if let Err(e) = hm.set_text(entry.id, &format!("{kept}{corrected}")) {
                debug!("Couldn't correct History: {e}");
            }
        }
        _ => debug!("The corrected text isn't the latest History entry; History left as is"),
    }
}

/// "Correct last dictation" from the tray: back to the window it was typed
/// in, then as said.
pub fn correct_from_tray(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        let Some(typed) = last_typed::get() else {
            super::tell(&app, Problem::CorrectNotFound, None);
            return;
        };
        crate::screen::activate_handle(typed.focus.window);
        std::thread::sleep(Duration::from_millis(300));
        let Some(target) = super::window_target(typed.focus.window, false) else {
            super::tell(&app, Problem::CorrectNotFound, None);
            return;
        };
        if let Err(problem) = correct_last(&app, target) {
            super::tell(&app, problem, Some(target));
        }
    });
}
