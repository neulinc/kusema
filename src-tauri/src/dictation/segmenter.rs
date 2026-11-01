//! Cuts a stream of voice-activity-filtered audio into segments at pauses.
//! Holds at most one segment, so a session's memory doesn't grow with its length.

use crate::audio_toolkit::constants::WHISPER_SAMPLE_RATE;

const RATE: usize = WHISPER_SAMPLE_RATE as usize;

/// No segment runs past this, even without a pause: models lose accuracy on
/// longer audio, and it caps what's held in memory.
const MAX_SEGMENT_SAMPLES: usize = 30 * RATE;
/// Past this, any pause the detector reports ends the segment, so someone who
/// never pauses for long rarely reaches the hard cut, which can split a word.
const EAGER_CUT_SAMPLES: usize = 20 * RATE;
/// Less audio than this is a click or a cough, not speech. Every segment has
/// the detector's pre-roll and tail (about 600 ms), so a one-syllable word passes.
const MIN_SEGMENT_SAMPLES: usize = 7 * RATE / 10;
/// Shorter segments are padded with silence to this: the same padding a short
/// recording gets, because models do poorly on under a second of audio.
const SHORT_SEGMENT_SAMPLES: usize = RATE;
const PADDED_SAMPLES: usize = 5 * RATE / 4;

pub struct SegmentCutter {
    /// Silence after the last speech frame that ends a segment.
    cut_after_silence: usize,
    buffer: Vec<f32>,
    silence: usize,
}

impl SegmentCutter {
    /// `pause_ms` is how long a pause ends a segment. The detector already
    /// passes `hangover_ms` of it along as speech, so that's not counted again.
    pub fn new(pause_ms: u64, hangover_ms: u64) -> Self {
        let silence_ms = pause_ms.saturating_sub(hangover_ms).max(1) as usize;
        Self {
            cut_after_silence: silence_ms * RATE / 1000,
            buffer: Vec::new(),
            silence: 0,
        }
    }

    /// Speech arrived. Returns a segment if this reached the length limit.
    pub fn push_speech(&mut self, frame: &[f32]) -> Option<Vec<f32>> {
        self.silence = 0;
        self.buffer.extend_from_slice(frame);
        (self.buffer.len() >= MAX_SEGMENT_SAMPLES)
            .then(|| self.cut())
            .flatten()
    }

    /// `samples` of silence arrived. Returns a segment if this ended one.
    pub fn push_silence(&mut self, samples: usize) -> Option<Vec<f32>> {
        if self.buffer.is_empty() {
            return None;
        }
        self.silence += samples;
        (self.silence >= self.cut_after_silence || self.buffer.len() >= EAGER_CUT_SAMPLES)
            .then(|| self.cut())
            .flatten()
    }

    /// The session ended: whatever is held, as a last segment.
    pub fn finish(&mut self) -> Option<Vec<f32>> {
        self.cut()
    }

    /// Whether a segment is being collected.
    pub fn is_collecting(&self) -> bool {
        !self.buffer.is_empty()
    }

    fn cut(&mut self) -> Option<Vec<f32>> {
        self.silence = 0;
        let mut segment = std::mem::take(&mut self.buffer);
        if segment.len() < MIN_SEGMENT_SAMPLES {
            return None;
        }
        if segment.len() < SHORT_SEGMENT_SAMPLES {
            segment.resize(PADDED_SAMPLES, 0.0);
        }
        Some(segment)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAUSE_MS: u64 = 700;
    const HANGOVER_MS: u64 = 150;

    fn ms(n: usize) -> usize {
        n * RATE / 1000
    }

    fn speech(n: usize) -> Vec<f32> {
        vec![0.1; ms(n)]
    }

    #[test]
    fn a_pause_of_the_set_length_ends_the_segment() {
        let mut cutter = SegmentCutter::new(PAUSE_MS, HANGOVER_MS);
        assert!(cutter.push_speech(&speech(2000)).is_none());
        // 550 ms of real silence + 150 ms of hangover the detector passed as speech.
        assert!(cutter.push_silence(ms(549)).is_none());
        let segment = cutter.push_silence(ms(1)).expect("the pause ended it");
        assert_eq!(segment.len(), ms(2000));
    }

    #[test]
    fn a_short_pause_does_not_end_the_segment() {
        let mut cutter = SegmentCutter::new(PAUSE_MS, HANGOVER_MS);
        assert!(cutter.push_speech(&speech(2000)).is_none());
        assert!(cutter.push_silence(ms(300)).is_none());
        assert!(cutter.push_speech(&speech(2000)).is_none());
        // The earlier silence no longer counts toward the next pause.
        assert!(cutter.push_silence(ms(300)).is_none());
        assert_eq!(cutter.finish().unwrap().len(), ms(4000));
    }

    #[test]
    fn silence_with_nothing_collected_does_nothing() {
        let mut cutter = SegmentCutter::new(PAUSE_MS, HANGOVER_MS);
        assert!(cutter.push_silence(ms(60_000)).is_none());
        assert!(!cutter.is_collecting());
    }

    #[test]
    fn a_pause_setting_below_the_hangover_still_works() {
        let mut cutter = SegmentCutter::new(100, HANGOVER_MS);
        cutter.push_speech(&speech(2000));
        assert!(cutter.push_silence(ms(1)).is_some());
    }

    #[test]
    fn long_speech_is_cut_at_the_limit() {
        let mut cutter = SegmentCutter::new(PAUSE_MS, HANGOVER_MS);
        let mut cut = None;
        for _ in 0..30 {
            cut = cut.or(cutter.push_speech(&speech(1000)));
        }
        assert_eq!(cut.expect("cut at 30 s").len(), MAX_SEGMENT_SAMPLES);
        assert!(!cutter.is_collecting());
    }

    #[test]
    fn past_the_eager_length_any_pause_ends_the_segment() {
        let mut cutter = SegmentCutter::new(PAUSE_MS, HANGOVER_MS);
        for _ in 0..21 {
            assert!(cutter.push_speech(&speech(1000)).is_none());
        }
        assert!(cutter.push_silence(ms(32)).is_some());
    }

    #[test]
    fn a_click_is_dropped_and_a_short_word_is_padded() {
        let mut cutter = SegmentCutter::new(PAUSE_MS, HANGOVER_MS);
        cutter.push_speech(&speech(100));
        assert!(cutter.push_silence(ms(1000)).is_none());
        assert!(!cutter.is_collecting());

        cutter.push_speech(&speech(800));
        let padded = cutter.push_silence(ms(1000)).expect("a short word is kept");
        assert_eq!(padded.len(), PADDED_SAMPLES);
    }

    #[test]
    fn finish_hands_over_what_is_held() {
        let mut cutter = SegmentCutter::new(PAUSE_MS, HANGOVER_MS);
        assert!(cutter.finish().is_none());
        cutter.push_speech(&speech(1500));
        assert_eq!(cutter.finish().unwrap().len(), ms(1500));
        assert!(cutter.finish().is_none());
    }

    /// The memory bound: however long the session, the cutter holds no more
    /// than one maximum-length segment.
    #[test]
    fn holds_at_most_one_segment_over_a_long_session() {
        let mut cutter = SegmentCutter::new(PAUSE_MS, HANGOVER_MS);
        let frame = vec![0.1; 512];
        let mut peak = 0;
        let mut segments = 0;
        // 30 minutes: 8 s of speech, 1 s of silence.
        for second in 0..1800 {
            for _ in 0..(RATE / 512) {
                let cut = if second % 9 == 8 {
                    cutter.push_silence(512)
                } else {
                    cutter.push_speech(&frame)
                };
                segments += usize::from(cut.is_some());
                peak = peak.max(cutter.buffer.capacity());
            }
        }
        assert_eq!(segments, 200);
        assert!(peak <= 2 * MAX_SEGMENT_SAMPLES, "peak capacity {peak}");
    }
}
