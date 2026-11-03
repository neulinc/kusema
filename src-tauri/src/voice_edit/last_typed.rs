//! The last text Kusema typed, so "make this more formal" with nothing
//! selected can work on it. The same rules as "scratch that" decide whether
//! it can be found again: the same window and control, nothing changed since.
//! Pressing the editing hotkey is itself keyboard input, so instead of
//! Windows' last-input time, the text is selected back and read: if the
//! selection isn't exactly what was typed (a click moved the caret, the user
//! typed more), it's put back and the user is asked to select the text.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::dictation::FocusToken;

/// Older than this, the user has probably moved on.
pub const MAX_AGE: Duration = Duration::from_secs(10 * 60);

#[derive(Clone, Debug)]
pub struct Typed {
    pub text: String,
    pub focus: FocusToken,
    pub at: Instant,
}

/// Why the last typed text can't be used.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    Nothing,
    WindowChanged,
    TooOld,
}

static LAST: Mutex<Option<Typed>> = Mutex::new(None);

fn last() -> std::sync::MutexGuard<'static, Option<Typed>> {
    LAST.lock().unwrap_or_else(|e| e.into_inner())
}

/// Kusema just typed `text` where `focus` is.
pub fn record(text: &str, focus: Option<FocusToken>) {
    *last() = match focus {
        Some(focus) if !text.is_empty() => Some(Typed {
            text: text.to_string(),
            focus,
            at: Instant::now(),
        }),
        // Somewhere unknown: nothing that can be found again.
        _ => None,
    };
}

/// What Kusema typed is gone ("scratch that").
pub fn forget() {
    *last() = None;
}

pub fn get() -> Option<Typed> {
    last().clone()
}

/// Whether `typed` can be selected back where focus is now.
pub fn check(
    typed: Option<&Typed>,
    focus_now: Option<FocusToken>,
    now: Instant,
) -> Result<&Typed, Refusal> {
    let typed = typed
        .filter(|t| !t.text.trim().is_empty())
        .ok_or(Refusal::Nothing)?;
    if focus_now != Some(typed.focus) {
        return Err(Refusal::WindowChanged);
    }
    if now.duration_since(typed.at) > MAX_AGE {
        return Err(Refusal::TooOld);
    }
    Ok(typed)
}

#[cfg(test)]
mod tests {
    use super::*;

    const HERE: FocusToken = FocusToken {
        window: 1,
        control: 10,
    };

    fn typed(text: &str) -> Typed {
        Typed {
            text: text.to_string(),
            focus: HERE,
            at: Instant::now(),
        }
    }

    #[test]
    fn found_again_in_the_same_window_and_control() {
        let t = typed("We agree.");
        assert_eq!(check(Some(&t), Some(HERE), t.at).unwrap().text, "We agree.");
    }

    #[test]
    fn nothing_typed_or_only_whitespace_is_refused() {
        assert_eq!(
            check(None, Some(HERE), Instant::now()).unwrap_err(),
            Refusal::Nothing
        );
        let t = typed("  \n");
        assert_eq!(
            check(Some(&t), Some(HERE), t.at).unwrap_err(),
            Refusal::Nothing
        );
    }

    #[test]
    fn another_window_or_control_is_refused() {
        let t = typed("We agree.");
        for focus in [
            Some(FocusToken {
                window: 2,
                control: 10,
            }),
            Some(FocusToken {
                window: 1,
                control: 11,
            }),
            None,
        ] {
            assert_eq!(
                check(Some(&t), focus, t.at).unwrap_err(),
                Refusal::WindowChanged
            );
        }
    }

    #[test]
    fn old_text_is_refused() {
        let t = typed("We agree.");
        let later = t.at + MAX_AGE + Duration::from_secs(1);
        assert_eq!(
            check(Some(&t), Some(HERE), later).unwrap_err(),
            Refusal::TooOld
        );
    }

    #[test]
    fn recording_somewhere_unknown_forgets() {
        record("text", Some(HERE));
        assert!(get().is_some());
        record("text", None);
        assert!(get().is_none());
        record("text", Some(HERE));
        forget();
        assert!(get().is_none());
    }
}
