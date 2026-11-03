//! Hold and double-tap on the Kusema key, as a pure state machine fed key
//! edges and clock ticks.
//!
//! Dictation starts on the key-down itself, with no wait to see what the
//! press turns into: waiting would cut off the first word. A release within
//! `TAP_MAX` was a tap, so that recording is thrown away unheard. A second
//! press within `DOUBLE_TAP_GAP` of a tap's release is a double-tap, which
//! starts or stops continuous dictation; a tap with no second press does
//! nothing.

use std::time::{Duration, Instant};

/// A release sooner than this after the press is a tap, not a hold. Short
/// enough that a held "yes" still counts; a deliberate tap is 50–150 ms.
pub const TAP_MAX: Duration = Duration::from_millis(250);
/// The longest gap between a tap's release and the second press. Most
/// double-taps are 80–200 ms apart; past this a press is a new gesture.
pub const DOUBLE_TAP_GAP: Duration = Duration::from_millis(350);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    /// Start push-to-talk dictation.
    StartHold,
    /// The key was held: finish that take as usual. `assistant`: Space was
    /// tapped during it, so it goes to the Assistant.
    FinishHold { assistant: bool },
    /// Space was tapped while holding: this take is for the Assistant.
    SwitchToAssistant,
    /// It was a tap: drop the recording the press started.
    DiscardHold,
    /// A double-tap: start continuous dictation, or stop it.
    ToggleContinuous,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Idle,
    /// The key is down. `recording`: the press started dictation (it doesn't
    /// while continuous dictation runs).
    Down {
        at: Instant,
        recording: bool,
        assistant: bool,
    },
    /// A tap ended at `at`; a press soon is a double-tap.
    Tapped {
        at: Instant,
    },
    /// The second press of a double-tap is down; its release means nothing.
    SecondDown,
}

#[derive(Debug)]
pub struct Gesture {
    state: State,
}

impl Default for Gesture {
    fn default() -> Self {
        Self::new()
    }
}

impl Gesture {
    pub const fn new() -> Self {
        Self { state: State::Idle }
    }

    /// The key went down. `continuous`: continuous dictation is running, so
    /// a press doesn't start push-to-talk (only a double-tap stops it).
    pub fn down(&mut self, now: Instant, continuous: bool) -> Option<Effect> {
        match self.state {
            // Auto-repeat while held.
            State::Down { .. } | State::SecondDown => None,
            State::Tapped { at } if now.duration_since(at) <= DOUBLE_TAP_GAP => {
                self.state = State::SecondDown;
                Some(Effect::ToggleContinuous)
            }
            State::Idle | State::Tapped { .. } => {
                let recording = !continuous;
                self.state = State::Down {
                    at: now,
                    recording,
                    assistant: false,
                };
                recording.then_some(Effect::StartHold)
            }
        }
    }

    pub fn up(&mut self, now: Instant) -> Option<Effect> {
        match self.state {
            State::Down {
                at,
                recording,
                assistant,
            } => {
                // A take switched to the Assistant is never a tap.
                if now.duration_since(at) < TAP_MAX && !assistant {
                    self.state = State::Tapped { at: now };
                    recording.then_some(Effect::DiscardHold)
                } else {
                    self.state = State::Idle;
                    recording.then_some(Effect::FinishHold { assistant })
                }
            }
            State::SecondDown => {
                self.state = State::Idle;
                None
            }
            State::Idle | State::Tapped { .. } => None,
        }
    }

    /// Space was tapped. Only a take that's recording can switch, once.
    pub fn space(&mut self) -> Option<Effect> {
        match &mut self.state {
            State::Down {
                recording: true,
                assistant,
                ..
            } if !*assistant => {
                *assistant = true;
                Some(Effect::SwitchToAssistant)
            }
            _ => None,
        }
    }

    /// Time passes: a tap with no second press is forgotten.
    pub fn tick(&mut self, now: Instant) {
        if let State::Tapped { at } = self.state {
            if now.duration_since(at) > DOUBLE_TAP_GAP {
                self.state = State::Idle;
            }
        }
    }

    /// When `tick` next needs calling, if at all.
    pub fn deadline(&self) -> Option<Instant> {
        match self.state {
            State::Tapped { at } => Some(at + DOUBLE_TAP_GAP + Duration::from_millis(1)),
            _ => None,
        }
    }

    pub fn is_holding(&self) -> bool {
        matches!(
            self.state,
            State::Down {
                recording: true,
                ..
            }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn a_hold_starts_on_the_press_and_finishes_on_the_release() {
        let t = Instant::now();
        let mut g = Gesture::default();
        // No added delay: the press itself starts dictation.
        assert_eq!(g.down(t, false), Some(Effect::StartHold));
        assert!(g.is_holding());
        // Auto-repeat changes nothing.
        assert_eq!(g.down(t + ms(500), false), None);
        assert_eq!(
            g.up(t + ms(1800),),
            Some(Effect::FinishHold { assistant: false })
        );
        assert!(!g.is_holding());
        // A short hold just past the tap limit still counts.
        assert_eq!(g.down(t + ms(3000), false), Some(Effect::StartHold));
        assert_eq!(
            g.up(t + ms(3000) + TAP_MAX),
            Some(Effect::FinishHold { assistant: false })
        );
    }

    #[test]
    fn a_single_tap_does_nothing() {
        let t = Instant::now();
        let mut g = Gesture::default();
        assert_eq!(g.down(t, false), Some(Effect::StartHold));
        // The recording the press started is dropped, not transcribed.
        assert_eq!(g.up(t + ms(90)), Some(Effect::DiscardHold));
        g.tick(t + ms(90) + DOUBLE_TAP_GAP + ms(5));
        assert_eq!(g.deadline(), None);
        // A press after the window is a fresh hold, not a double-tap.
        assert_eq!(g.down(t + ms(800), false), Some(Effect::StartHold));
    }

    #[test]
    fn a_quick_double_tap_toggles_continuous_dictation() {
        let t = Instant::now();
        let mut g = Gesture::default();
        assert_eq!(g.down(t, false), Some(Effect::StartHold));
        assert_eq!(g.up(t + ms(80)), Some(Effect::DiscardHold));
        assert_eq!(g.down(t + ms(220), false), Some(Effect::ToggleContinuous));
        // The second release, and repeats while it's down, mean nothing.
        assert_eq!(g.down(t + ms(260), true), None);
        assert_eq!(g.up(t + ms(300)), None);
        // Continuous dictation is on: a press doesn't start push-to-talk...
        assert_eq!(g.down(t + ms(5000), true), None);
        assert_eq!(g.up(t + ms(5070)), None);
        // ...and a second tap stops it.
        assert_eq!(g.down(t + ms(5200), true), Some(Effect::ToggleContinuous));
        assert_eq!(g.up(t + ms(5260)), None);
    }

    #[test]
    fn the_double_tap_window_edges() {
        let t = Instant::now();
        let mut g = Gesture::default();
        g.down(t, false);
        g.up(t + ms(100));
        // At the very edge of the gap it still counts...
        assert_eq!(
            g.down(t + ms(100) + DOUBLE_TAP_GAP, false),
            Some(Effect::ToggleContinuous)
        );
        g.up(t + ms(600));
        // ...a moment later it doesn't.
        let t = t + ms(2000);
        g.down(t, false);
        g.up(t + ms(100));
        assert_eq!(
            g.down(t + ms(101) + DOUBLE_TAP_GAP, false),
            Some(Effect::StartHold)
        );
    }

    #[test]
    fn a_slow_first_press_is_a_hold_not_half_a_double_tap() {
        let t = Instant::now();
        let mut g = Gesture::default();
        g.down(t, false);
        assert_eq!(
            g.up(t + ms(400)),
            Some(Effect::FinishHold { assistant: false })
        );
        // A quick press right after a hold starts a new hold.
        assert_eq!(g.down(t + ms(500), false), Some(Effect::StartHold));
    }

    #[test]
    fn space_while_holding_switches_the_take_to_the_assistant() {
        let t = Instant::now();
        let mut g = Gesture::default();
        g.down(t, false);
        assert_eq!(g.space(), Some(Effect::SwitchToAssistant));
        // Once per take.
        assert_eq!(g.space(), None);
        assert_eq!(
            g.up(t + ms(2000)),
            Some(Effect::FinishHold { assistant: true })
        );
        // Even a quick one isn't thrown away as a tap.
        g.down(t + ms(3000), false);
        g.space();
        assert_eq!(
            g.up(t + ms(3100)),
            Some(Effect::FinishHold { assistant: true })
        );
        // The next take starts as dictation again.
        g.down(t + ms(5000), false);
        assert_eq!(
            g.up(t + ms(6000)),
            Some(Effect::FinishHold { assistant: false })
        );
        // Not held, or held during continuous dictation: nothing to switch.
        assert_eq!(g.space(), None);
        g.down(t + ms(8000), true);
        assert_eq!(g.space(), None);
    }

    #[test]
    fn a_tap_leaves_a_tick_deadline() {
        let t = Instant::now();
        let mut g = Gesture::default();
        assert_eq!(g.deadline(), None);
        g.down(t, false);
        g.up(t + ms(50));
        assert!(g.deadline().unwrap() > t + ms(50) + DOUBLE_TAP_GAP);
        g.tick(t + ms(50) + DOUBLE_TAP_GAP + ms(2));
        assert_eq!(g.deadline(), None);
    }
}
