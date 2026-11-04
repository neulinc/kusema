//! The adaptive voice detector threshold: sound quieter than a level set from
//! the microphone check's room noise is never speech, whatever the detector
//! thinks. Distant voices, a television or a fan stop turning into stray
//! words, and speech into the microphone still passes.

use anyhow::Result;

use super::analysis::level_db;
use crate::audio_toolkit::vad::{VadFrame, VadTailReport, VoiceActivityDetector};

pub struct LevelGate {
    inner: Box<dyn VoiceActivityDetector>,
    gate_db: f32,
}

impl LevelGate {
    pub fn new(inner: Box<dyn VoiceActivityDetector>, gate_db: f32) -> Self {
        Self { inner, gate_db }
    }
}

impl VoiceActivityDetector for LevelGate {
    fn push_frame<'a>(&'a mut self, frame: &'a [f32]) -> Result<VadFrame<'a>> {
        // The detector sees every frame, so its own state stays as it would be.
        let voiced = self.inner.push_frame(frame)?.is_speech();
        if voiced && level_db(frame) >= self.gate_db {
            Ok(VadFrame::Speech(frame))
        } else {
            Ok(VadFrame::Noise)
        }
    }

    fn frame_samples(&self) -> usize {
        self.inner.frame_samples()
    }

    fn set_hangover_frames(&mut self, frames: usize) {
        self.inner.set_hangover_frames(frames);
    }

    fn tail_report(&self) -> Option<VadTailReport> {
        self.inner.tail_report()
    }

    fn reset(&mut self) {
        self.inner.reset();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Says every frame is speech, like Silero does for a distant voice.
    struct AlwaysVoice;

    impl VoiceActivityDetector for AlwaysVoice {
        fn push_frame<'a>(&'a mut self, frame: &'a [f32]) -> Result<VadFrame<'a>> {
            Ok(VadFrame::Speech(frame))
        }
        fn frame_samples(&self) -> usize {
            512
        }
    }

    fn at(db: f32) -> Vec<f32> {
        vec![10f32.powf(db / 20.0); 512]
    }

    #[test]
    fn quieter_than_the_gate_is_noise_and_louder_is_speech() {
        let mut gate = LevelGate::new(Box::new(AlwaysVoice), -45.0);
        assert!(!gate.push_frame(&at(-50.0)).unwrap().is_speech());
        assert!(gate.push_frame(&at(-30.0)).unwrap().is_speech());
        assert!(gate.push_frame(&at(-45.0)).unwrap().is_speech());
        assert_eq!(gate.frame_samples(), 512);
    }

    #[test]
    fn loud_sound_the_detector_calls_noise_stays_noise() {
        struct NeverVoice;
        impl VoiceActivityDetector for NeverVoice {
            fn push_frame<'a>(&'a mut self, _frame: &'a [f32]) -> Result<VadFrame<'a>> {
                Ok(VadFrame::Noise)
            }
            fn frame_samples(&self) -> usize {
                512
            }
        }
        let mut gate = LevelGate::new(Box::new(NeverVoice), -45.0);
        assert!(!gate.push_frame(&at(-10.0)).unwrap().is_speech());
    }
}
