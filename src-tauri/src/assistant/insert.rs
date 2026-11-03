//! Kusema paste-the-answer: an answer is typed only into the window and
//! control that had the caret when the question was asked, under continuous
//! dictation's rules. While focus is elsewhere it waits; it never types into
//! another window, and each keystroke is checked as it's sent. The answer goes
//! in as one paste (with the clipboard paste method), so one Ctrl+Z takes it
//! back out.

use log::{debug, info};
use serde::Serialize;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::AppHandle;

use crate::clipboard::{OnTarget, TypeOutcome};
use crate::dictation::FocusToken;
use crate::screen::CapturedWindow;

/// How long an answer waits for its window to come back to the front.
pub const WAIT_FOR_FOCUS: Duration = Duration::from_secs(20);
const POLL: Duration = Duration::from_millis(50);
/// Seconds a message about it stays in the overlay.
const NOTICE_SECS: u64 = 6;

/// Where an answer may be typed.
#[derive(Debug, Clone, Copy)]
pub struct DocTarget {
    pub window: CapturedWindow,
    pub focus: FocusToken,
}

/// The window in front and its focused control, unless it's one of Kusema's
/// own or there's no caret to type at.
pub fn at_press() -> Option<DocTarget> {
    let window = crate::screen::foreground_target().ok()?;
    let focus = crate::dictation::current_focus().filter(|f| f.window == window.handle())?;
    Some(DocTarget { window, focus })
}

/// Why an answer wasn't typed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InsertProblem {
    /// Asked from Kusema's own window, or with no caret anywhere.
    NoTarget,
    /// The window didn't come back in time.
    WindowChanged,
    /// The paste method is "None".
    NotTyping,
    Failed(String),
}

impl InsertProblem {
    fn key_and_english(&self) -> (&'static str, &'static str) {
        match self {
            InsertProblem::NoTarget => (
                "noTarget",
                "Kusema doesn't know which document this answer is for. Click in the document and ask again.",
            ),
            InsertProblem::WindowChanged => (
                "windowChanged",
                "The window changed, so the answer wasn't typed. Insert it from the Assistant page.",
            ),
            InsertProblem::NotTyping => (
                "notTyping",
                "The paste method is set to None, so the answer wasn't typed.",
            ),
            InsertProblem::Failed(_) => ("failed", "Couldn't type the answer into this window."),
        }
    }
}

/// Keystrokes now would reach the target, as sent: the same window and
/// control, with no modifier key still held from a hotkey.
pub fn may_type(now: Option<FocusToken>, target: FocusToken, modifiers_held: bool) -> bool {
    now == Some(target) && !modifiers_held
}

/// Waits (up to `wait`) until `ready()`, then types with `type_it`, handing
/// it `ready` to check again right before the keystroke. `waiting` is called
/// once if it has to wait at all.
pub fn type_when_ready(
    ready: OnTarget,
    wait: Duration,
    mut waiting: impl FnMut(),
    type_it: impl FnOnce(OnTarget) -> Result<TypeOutcome, String>,
) -> Result<(), InsertProblem> {
    let deadline = Instant::now() + wait;
    let mut told = false;
    while !ready() {
        if Instant::now() >= deadline {
            return Err(InsertProblem::WindowChanged);
        }
        if !told {
            told = true;
            waiting();
        }
        std::thread::sleep(POLL);
    }
    match type_it(ready) {
        Ok(TypeOutcome::Typed) => Ok(()),
        Ok(TypeOutcome::NotTyping) => Err(InsertProblem::NotTyping),
        Ok(TypeOutcome::WindowChanged) => Err(InsertProblem::WindowChanged),
        Err(e) => Err(InsertProblem::Failed(e)),
    }
}

/// A message for the overlay (or a card when it's off), as `voice-edit-notice`.
#[derive(Clone, Debug, Serialize)]
struct Notice {
    key: Option<String>,
    name: Option<String>,
    message: String,
    in_dictation: bool,
}

fn show(app: &AppHandle, key: &str, english: &str, near: Option<CapturedWindow>) {
    info!("Paste the answer: {english}");
    let key = format!("overlay.answer.{key}");
    let notice = Notice {
        key: Some(key.clone()),
        name: None,
        message: english.to_string(),
        in_dictation: false,
    };
    if !crate::overlay::show_edit_notice(app, &notice, false, NOTICE_SECS) {
        crate::screen::card::notice_with_key(app, english.to_string(), Some(key), None, near);
    }
}

pub fn tell(app: &AppHandle, problem: &InsertProblem, near: Option<CapturedWindow>) {
    if let InsertProblem::Failed(e) = problem {
        debug!("Couldn't type the answer: {e}");
    }
    let (key, english) = problem.key_and_english();
    show(app, key, english, near);
}

/// Types `text` into `target`. Blocks while it waits for the window, so call
/// it off the async runtime. `activate`: bring the window to the front first
/// (the user asked from Kusema's own window or the Ask box).
pub fn insert(
    app: &AppHandle,
    target: DocTarget,
    text: &str,
    activate: bool,
) -> Result<(), InsertProblem> {
    if activate {
        crate::screen::activate(&target.window);
    }
    let focus = target.focus;
    let ready: OnTarget = Arc::new(move || {
        may_type(
            crate::dictation::current_focus(),
            focus,
            crate::voice_edit::modifiers_held(),
        )
    });
    let app_for_wait = app.clone();
    let waiting = move || {
        show(
            &app_for_wait,
            "waiting",
            "Click back into your document to type the answer.",
            Some(target.window),
        )
    };
    let text = text.trim();
    let result = type_when_ready(ready, WAIT_FOR_FOCUS, waiting, |on_target| {
        crate::clipboard::type_segment(text, app, on_target)
    });
    match &result {
        Ok(()) => {
            crate::voice_edit::last_typed::record(text, crate::dictation::current_focus());
            info!("Typed the answer into the document");
        }
        Err(problem) => tell(app, problem, Some(target.window)),
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    const DOC: FocusToken = FocusToken {
        window: 10,
        control: 11,
    };
    const OTHER: FocusToken = FocusToken {
        window: 20,
        control: 21,
    };

    #[test]
    fn only_the_same_window_and_control_with_keys_up() {
        assert!(may_type(Some(DOC), DOC, false));
        assert!(!may_type(Some(OTHER), DOC, false));
        // Same window, another field (the search box): not where it was asked.
        assert!(!may_type(
            Some(FocusToken {
                window: 10,
                control: 99
            }),
            DOC,
            false
        ));
        assert!(!may_type(None, DOC, false));
        // Ctrl still held from a hotkey would turn the text into shortcuts.
        assert!(!may_type(Some(DOC), DOC, true));
    }

    fn probe(sequence: Vec<Option<FocusToken>>) -> (OnTarget, Arc<AtomicUsize>) {
        let calls = Arc::new(AtomicUsize::new(0));
        let seen = Arc::clone(&calls);
        let sequence = Mutex::new(sequence);
        let ready: OnTarget = Arc::new(move || {
            seen.fetch_add(1, Ordering::SeqCst);
            let mut sequence = sequence.lock().unwrap();
            let now = if sequence.len() > 1 {
                sequence.remove(0)
            } else {
                sequence[0]
            };
            may_type(now, DOC, false)
        });
        (ready, calls)
    }

    #[test]
    fn never_types_into_another_window() {
        let (ready, _) = probe(vec![Some(OTHER)]);
        let mut typed = false;
        let result = type_when_ready(
            ready,
            Duration::from_millis(120),
            || {},
            |_| {
                typed = true;
                Ok(TypeOutcome::Typed)
            },
        );
        assert_eq!(result, Err(InsertProblem::WindowChanged));
        assert!(!typed);
    }

    #[test]
    fn waits_while_focus_is_elsewhere_then_types() {
        let (ready, _) = probe(vec![Some(OTHER), Some(OTHER), Some(DOC)]);
        let mut waited = 0;
        let mut typed = false;
        let result = type_when_ready(
            ready,
            Duration::from_secs(2),
            || waited += 1,
            |_| {
                typed = true;
                Ok(TypeOutcome::Typed)
            },
        );
        assert_eq!(result, Ok(()));
        assert!(typed);
        assert_eq!(waited, 1, "said once, not on every poll");
    }

    #[test]
    fn the_keystroke_check_is_handed_to_the_typing() {
        // Focus moves away between the wait and the keystroke: the typing
        // path's own check refuses, and nothing is reported as typed.
        let (ready, _) = probe(vec![Some(DOC), Some(OTHER)]);
        let result = type_when_ready(
            ready,
            Duration::from_secs(1),
            || {},
            |on_target| {
                Ok(if on_target() {
                    TypeOutcome::Typed
                } else {
                    TypeOutcome::WindowChanged
                })
            },
        );
        assert_eq!(result, Err(InsertProblem::WindowChanged));
    }

    #[test]
    fn paste_method_none_and_failures_are_reported() {
        let (ready, _) = probe(vec![Some(DOC)]);
        let none = type_when_ready(
            ready.clone(),
            Duration::ZERO,
            || {},
            |_| Ok(TypeOutcome::NotTyping),
        );
        assert_eq!(none, Err(InsertProblem::NotTyping));
        let failed = type_when_ready(ready, Duration::ZERO, || {}, |_| Err("enigo".into()));
        assert_eq!(failed, Err(InsertProblem::Failed("enigo".into())));
    }
}
