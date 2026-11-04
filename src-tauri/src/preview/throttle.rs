//! Keeps the live preview from falling behind. The stream is fed in chunks;
//! when the engine's answers trail the audio by too long, the chunks get
//! bigger (fewer, cheaper updates), and when even the biggest can't keep up,
//! the preview turns off. Pure: time is passed in, so every rule is tested.

use std::time::Duration;

/// Samples per feed at each level: 32, 160 and 480 ms of 16 kHz audio.
const LEVELS: [usize; 3] = [512, 2560, 7680];
/// The preview trails the audio by more than this...
const BEHIND_MS: i64 = 1200;
/// ...for this long, and it slows down (or, at the last level, stops).
const SUSTAIN: Duration = Duration::from_millis(1000);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Keep,
    /// Feed bigger chunks from now on.
    Slower,
    /// Turn the preview off: this computer can't keep up.
    Off,
}

#[derive(Debug)]
pub struct Throttle {
    level: usize,
    /// Samples fed to the current stream.
    fed: usize,
    /// Audio the stream started with in one go (a backlog), which it may
    /// take a moment to get through without being behind.
    catch_up_ms: i64,
    behind_since: Option<Duration>,
    off: bool,
    /// Debug builds: added to the measured lag, to test the slowdown.
    extra_lag_ms: i64,
}

impl Default for Throttle {
    fn default() -> Self {
        Self::new()
    }
}

impl Throttle {
    pub fn new() -> Self {
        Self {
            level: 0,
            fed: 0,
            catch_up_ms: 0,
            behind_since: None,
            off: false,
            extra_lag_ms: debug_extra_lag_ms(),
        }
    }

    /// How many samples to gather before each feed.
    pub fn chunk_samples(&self) -> usize {
        LEVELS[self.level]
    }

    pub fn is_off(&self) -> bool {
        self.off
    }

    /// A new stream begins; `backlog` samples are fed to it at once. The
    /// level stays: the computer is no faster than a moment ago.
    pub fn begin(&mut self, backlog: usize) {
        self.fed = 0;
        self.catch_up_ms = samples_ms(backlog);
        self.behind_since = None;
    }

    pub fn fed(&mut self, samples: usize) {
        self.fed += samples;
    }

    /// The engine answered a feed, having taken in `processed_ms` of audio
    /// since the stream began, at time `now`.
    pub fn progress(&mut self, processed_ms: i64, now: Duration) -> Verdict {
        if self.off {
            return Verdict::Off;
        }
        let lag = samples_ms(self.fed) - processed_ms + self.extra_lag_ms;
        if processed_ms < self.catch_up_ms || lag <= BEHIND_MS {
            self.behind_since = None;
            return Verdict::Keep;
        }
        let since = *self.behind_since.get_or_insert(now);
        if now.saturating_sub(since) < SUSTAIN {
            return Verdict::Keep;
        }
        self.behind_since = None;
        if self.level + 1 < LEVELS.len() {
            self.level += 1;
            Verdict::Slower
        } else {
            self.off = true;
            Verdict::Off
        }
    }
}

fn samples_ms(samples: usize) -> i64 {
    (samples as i64 * 1000) / 16_000
}

#[cfg(debug_assertions)]
fn debug_extra_lag_ms() -> i64 {
    std::env::var("KUSEMA_DEBUG_PREVIEW_LAG_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0)
}

#[cfg(not(debug_assertions))]
fn debug_extra_lag_ms() -> i64 {
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    /// Feeds 32 ms frames for `seconds`, the engine trailing by `lag_ms`;
    /// returns every verdict that wasn't Keep, with its time.
    fn run(throttle: &mut Throttle, seconds: u64, lag_ms: i64, start: u64) -> Vec<(u64, Verdict)> {
        let mut out = Vec::new();
        let mut t = start;
        while t < start + seconds * 1000 {
            throttle.fed(512);
            let processed = samples_ms(throttle.fed) - lag_ms;
            let verdict = throttle.progress(processed.max(0), ms(t));
            if verdict != Verdict::Keep {
                out.push((t, verdict));
            }
            t += 32;
        }
        out
    }

    #[test]
    fn a_fast_computer_keeps_the_finest_updates() {
        let mut throttle = Throttle::new();
        assert!(run(&mut throttle, 60, 200, 0).is_empty());
        assert_eq!(throttle.chunk_samples(), 512);
    }

    #[test]
    fn falling_behind_slows_the_updates_then_turns_the_preview_off() {
        let mut throttle = Throttle::new();
        let verdicts = run(&mut throttle, 10, 2000, 0);
        let kinds: Vec<Verdict> = verdicts.iter().map(|(_, v)| *v).collect();
        assert_eq!(kinds[..3], [Verdict::Slower, Verdict::Slower, Verdict::Off]);
        // Each step waits for the lag to last a second.
        assert!(verdicts[0].0 >= 1000 && verdicts[1].0 - verdicts[0].0 >= 1000);
        assert!(throttle.is_off());
        assert_eq!(throttle.progress(0, ms(20_000)), Verdict::Off);
    }

    #[test]
    fn a_short_stall_changes_nothing() {
        let mut throttle = Throttle::new();
        let mut t = 0;
        for i in 0..200 {
            throttle.fed(512);
            // Behind for half a second, then caught up again.
            let lag = if (50..65).contains(&i) { 2000 } else { 100 };
            assert_eq!(
                throttle.progress(samples_ms(throttle.fed) - lag, ms(t)),
                Verdict::Keep
            );
            t += 32;
        }
        assert_eq!(throttle.chunk_samples(), 512);
    }

    #[test]
    fn a_backlog_fed_at_once_is_not_falling_behind() {
        let mut throttle = Throttle::new();
        // Four seconds heard while the engine was busy, fed in one go.
        throttle.begin(64_000);
        throttle.fed(64_000);
        // The engine works through it over 2 s: far behind, but catching up.
        for step in 0..20 {
            let processed = step * 200;
            assert_eq!(
                throttle.progress(processed, ms(step as u64 * 100)),
                Verdict::Keep
            );
        }
        assert_eq!(throttle.chunk_samples(), 512);
    }

    #[test]
    fn the_level_carries_over_to_the_next_stream() {
        let mut throttle = Throttle::new();
        // The lag builds to 2 s, then lasts a second: one step down by 3 s.
        run(&mut throttle, 3, 2000, 0);
        assert_eq!(throttle.chunk_samples(), 2560);
        throttle.begin(0);
        assert_eq!(throttle.chunk_samples(), 2560);
        assert!(run(&mut throttle, 30, 300, 10_000).is_empty());
    }
}
