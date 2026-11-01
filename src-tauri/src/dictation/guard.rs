//! The rules that keep dictated text out of the wrong place: which window a
//! session types into, what happens when that changes, and when "scratch that"
//! may delete anything.

use std::time::Instant;

use super::join::JoinState;

/// The window in front.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Foreground {
    pub window: isize,
    /// One of Kusema's own windows, which is never typed into.
    pub own: bool,
}

/// A change the session has to act on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GuardEvent {
    /// The first window that can be typed into; the session started in Kusema.
    TargetChosen,
    Paused,
    Resumed,
}

/// What to do with a finished segment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Disposition {
    Type,
    /// Speech that began before the window changed: typed on return.
    Hold,
    /// Speech that began after the window changed: it might be meant for
    /// someone in the room, so it only goes to History.
    HistoryOnly,
}

/// Tracks the window recorded at the start of the session.
#[derive(Debug, Default)]
pub struct WindowGuard {
    target: Option<isize>,
    paused_since: Option<Instant>,
}

impl WindowGuard {
    /// Records the window the session types into. Starting in one of Kusema's
    /// own windows leaves it open: the next other window becomes the target.
    pub fn new(foreground: Option<Foreground>) -> Self {
        Self {
            target: foreground.filter(|f| !f.own).map(|f| f.window),
            paused_since: None,
        }
    }

    pub fn target(&self) -> Option<isize> {
        self.target
    }

    pub fn is_paused(&self) -> bool {
        self.paused_since.is_some()
    }

    /// Pauses if `foreground` isn't the target. Only ever adds a pause: noticing
    /// the return, and acting on it, stays with `observe`.
    pub fn pause_if_away(&mut self, foreground: Option<Foreground>, now: Instant) {
        let here = foreground.filter(|f| !f.own).map(|f| f.window);
        if self.target.is_some() && here != self.target && self.paused_since.is_none() {
            self.paused_since = Some(now);
        }
    }

    /// Takes in the window now in front.
    pub fn observe(&mut self, foreground: Option<Foreground>, now: Instant) -> Option<GuardEvent> {
        let here = foreground.filter(|f| !f.own).map(|f| f.window);
        let Some(target) = self.target else {
            self.target = here;
            return here.map(|_| GuardEvent::TargetChosen);
        };
        match (here == Some(target), self.paused_since) {
            (true, Some(_)) => {
                self.paused_since = None;
                Some(GuardEvent::Resumed)
            }
            (false, None) => {
                self.paused_since = Some(now);
                Some(GuardEvent::Paused)
            }
            _ => None,
        }
    }

    /// What to do with a segment whose speech began at `started_at`.
    pub fn disposition(&self, started_at: Instant) -> Disposition {
        if self.target.is_none() {
            return Disposition::HistoryOnly;
        }
        match self.paused_since {
            None => Disposition::Type,
            Some(since) if started_at < since => Disposition::Hold,
            Some(_) => Disposition::HistoryOnly,
        }
    }
}

/// Where the caret was: the top-level window and the control focused inside it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FocusToken {
    pub window: isize,
    pub control: isize,
}

/// The last thing typed, with what's needed to take it back.
#[derive(Clone, Copy, Debug)]
pub struct LastTyped {
    /// Characters to delete (a line break counts as one).
    pub chars: usize,
    pub focus: FocusToken,
    /// The system's last-input time right after typing it.
    pub input_tick: u32,
    pub join_before: JoinState,
    /// How to take it out of History, while it's still History's tail.
    pub history: Option<HistoryTail>,
}

/// The end of a session's History entry, and how to take it back.
#[derive(Clone, Copy, Debug)]
pub struct HistoryTail {
    pub chars: usize,
    pub join_before: JoinState,
}

/// Why "scratch that" did nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScratchRefusal {
    NothingToScratch,
    WindowChanged,
    OtherInput,
}

/// How many characters "scratch that" may delete right now. Deleting blindly
/// could eat the user's own work, so any doubt is a refusal: the keyboard or
/// mouse having been used at all (a click can move the caret) is enough.
pub fn check_scratch(
    last: Option<&LastTyped>,
    paused: bool,
    focus_now: Option<FocusToken>,
    input_tick_now: u32,
) -> Result<usize, ScratchRefusal> {
    let last = last
        .filter(|last| last.chars > 0)
        .ok_or(ScratchRefusal::NothingToScratch)?;
    if paused || focus_now != Some(last.focus) {
        return Err(ScratchRefusal::WindowChanged);
    }
    if input_tick_now != last.input_tick {
        return Err(ScratchRefusal::OtherInput);
    }
    Ok(last.chars)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    const DOC: isize = 1;
    const BROWSER: isize = 2;
    const KUSEMA: isize = 3;

    fn app(window: isize) -> Option<Foreground> {
        Some(Foreground { window, own: false })
    }

    fn kusema() -> Option<Foreground> {
        Some(Foreground {
            window: KUSEMA,
            own: true,
        })
    }

    #[test]
    fn records_the_window_at_the_start_and_types_into_it() {
        let guard = WindowGuard::new(app(DOC));
        assert_eq!(guard.target(), Some(DOC));
        assert!(!guard.is_paused());
        assert_eq!(guard.disposition(Instant::now()), Disposition::Type);
    }

    #[test]
    fn staying_in_the_window_changes_nothing() {
        let mut guard = WindowGuard::new(app(DOC));
        assert_eq!(guard.observe(app(DOC), Instant::now()), None);
        assert!(!guard.is_paused());
    }

    #[test]
    fn another_window_pauses_and_returning_resumes() {
        let mut guard = WindowGuard::new(app(DOC));
        let now = Instant::now();
        assert_eq!(guard.observe(app(BROWSER), now), Some(GuardEvent::Paused));
        assert!(guard.is_paused());
        // Still away, even in yet another window: no new event.
        assert_eq!(guard.observe(app(99), now), None);
        assert_eq!(guard.observe(app(DOC), now), Some(GuardEvent::Resumed));
        assert!(!guard.is_paused());
        assert_eq!(guard.target(), Some(DOC));
    }

    #[test]
    fn no_foreground_window_or_kusema_itself_also_pauses() {
        let now = Instant::now();
        let mut guard = WindowGuard::new(app(DOC));
        assert_eq!(guard.observe(None, now), Some(GuardEvent::Paused));
        assert_eq!(guard.observe(app(DOC), now), Some(GuardEvent::Resumed));
        assert_eq!(guard.observe(kusema(), now), Some(GuardEvent::Paused));
    }

    #[test]
    fn starting_in_kusema_waits_for_a_real_window() {
        let now = Instant::now();
        let mut guard = WindowGuard::new(kusema());
        assert_eq!(guard.target(), None);
        assert_eq!(guard.disposition(now), Disposition::HistoryOnly);
        assert_eq!(guard.observe(kusema(), now), None);
        assert_eq!(guard.observe(None, now), None);
        assert_eq!(guard.observe(app(DOC), now), Some(GuardEvent::TargetChosen));
        assert_eq!(guard.target(), Some(DOC));
        assert_eq!(guard.disposition(now), Disposition::Type);
    }

    #[test]
    fn speech_begun_before_the_change_is_held_and_speech_after_is_not() {
        let t0 = Instant::now();
        let paused_at = t0 + Duration::from_secs(10);
        let mut guard = WindowGuard::new(app(DOC));
        guard.observe(app(BROWSER), paused_at);
        assert_eq!(guard.disposition(t0), Disposition::Hold);
        assert_eq!(
            guard.disposition(paused_at + Duration::from_secs(1)),
            Disposition::HistoryOnly
        );
    }

    #[test]
    fn checking_before_typing_can_pause_but_never_resume() {
        let t0 = Instant::now();
        let mut guard = WindowGuard::new(app(DOC));
        guard.pause_if_away(app(DOC), t0);
        assert!(!guard.is_paused());

        guard.pause_if_away(app(BROWSER), t0);
        assert!(guard.is_paused());
        guard.pause_if_away(app(DOC), t0);
        assert!(guard.is_paused(), "only the monitor notices the return");
        assert_eq!(guard.observe(app(DOC), t0), Some(GuardEvent::Resumed));
    }

    #[test]
    fn checking_before_typing_does_not_pause_with_no_target_yet() {
        let t0 = Instant::now();
        let mut guard = WindowGuard::new(kusema());
        guard.pause_if_away(app(DOC), t0);
        assert!(!guard.is_paused());
        assert_eq!(guard.target(), None);
    }

    #[test]
    fn after_returning_segments_are_typed_again() {
        let t0 = Instant::now();
        let mut guard = WindowGuard::new(app(DOC));
        guard.observe(app(BROWSER), t0);
        guard.observe(app(DOC), t0 + Duration::from_secs(5));
        assert_eq!(
            guard.disposition(t0 + Duration::from_secs(6)),
            Disposition::Type
        );
    }

    const FOCUS: FocusToken = FocusToken {
        window: DOC,
        control: 10,
    };
    const TICK: u32 = 1000;

    fn typed(chars: usize) -> LastTyped {
        LastTyped {
            chars,
            focus: FOCUS,
            input_tick: TICK,
            join_before: JoinState::default(),
            history: None,
        }
    }

    #[test]
    fn scratch_deletes_when_nothing_has_changed() {
        assert_eq!(
            check_scratch(Some(&typed(12)), false, Some(FOCUS), TICK),
            Ok(12)
        );
    }

    #[test]
    fn scratch_with_nothing_typed_is_refused() {
        assert_eq!(
            check_scratch(None, false, Some(FOCUS), TICK),
            Err(ScratchRefusal::NothingToScratch)
        );
        assert_eq!(
            check_scratch(Some(&typed(0)), false, Some(FOCUS), TICK),
            Err(ScratchRefusal::NothingToScratch)
        );
    }

    #[test]
    fn scratch_after_the_focus_changed_is_refused() {
        let other_window = FocusToken {
            window: BROWSER,
            control: 10,
        };
        let other_control = FocusToken {
            window: DOC,
            control: 11,
        };
        for focus in [Some(other_window), Some(other_control), None] {
            assert_eq!(
                check_scratch(Some(&typed(12)), false, focus, TICK),
                Err(ScratchRefusal::WindowChanged)
            );
        }
    }

    #[test]
    fn scratch_while_paused_is_refused() {
        assert_eq!(
            check_scratch(Some(&typed(12)), true, Some(FOCUS), TICK),
            Err(ScratchRefusal::WindowChanged)
        );
    }

    #[test]
    fn scratch_after_any_keyboard_or_mouse_input_is_refused() {
        assert_eq!(
            check_scratch(Some(&typed(12)), false, Some(FOCUS), TICK + 1),
            Err(ScratchRefusal::OtherInput)
        );
    }

    #[test]
    fn a_window_change_is_reported_before_other_input() {
        assert_eq!(
            check_scratch(Some(&typed(12)), true, Some(FOCUS), TICK + 1),
            Err(ScratchRefusal::WindowChanged)
        );
    }
}
