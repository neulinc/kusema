//! Debug builds only: feeds a session from a WAV file instead of the microphone,
//! through the real voice activity detector, so the whole pipeline can be tested
//! without anyone speaking. Set `KUSEMA_DEBUG_DICTATION_WAV` to a 16 kHz mono
//! file, and optionally `KUSEMA_DEBUG_DICTATION_SPEED` to run faster than real
//! time. The session stops itself when the file ends; with no file there, it
//! uses the microphone.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use log::{error, info};

use super::{request_stop, Message, SegmentTap, Shared, StopReason};
use crate::audio_toolkit::audio::FrameTap;
use crate::audio_toolkit::read_wav_samples;
use crate::audio_toolkit::vad::{
    frames_for_duration_ms, VadFrame, VoiceActivityDetector, VAD_CONTINUOUS_HANGOVER_MS,
};
use crate::managers::audio::create_smoothed_detector;
use crate::settings::get_settings;

pub fn configured_wav() -> Option<PathBuf> {
    std::env::var_os("KUSEMA_DEBUG_DICTATION_WAV")
        .map(PathBuf::from)
        .filter(|path| path.exists())
}

pub struct DebugFeed {
    stop: Arc<AtomicBool>,
    thread: JoinHandle<()>,
}

impl DebugFeed {
    pub fn start(wav: PathBuf, pause_ms: u64, shared: &Arc<Shared>, tx: &Sender<Message>) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let speed = std::env::var("KUSEMA_DEBUG_DICTATION_SPEED")
            .ok()
            .and_then(|s| s.parse::<f64>().ok())
            .filter(|s| *s > 0.0)
            .unwrap_or(1.0);
        let mut tap = SegmentTap::new(pause_ms, Arc::clone(shared), tx.clone());
        let shared = Arc::clone(shared);
        let stop_flag = Arc::clone(&stop);
        let thread = thread::spawn(move || {
            let backend = get_settings(&shared.app).vad_backend;
            let result = read_wav_samples(&wav).and_then(|samples| {
                create_smoothed_detector(backend, &shared.app).map(|vad| (samples, vad))
            });
            let (samples, mut vad) = match result {
                Ok(loaded) => loaded,
                Err(e) => {
                    error!("Couldn't start the debug feed: {e:#}");
                    return;
                }
            };
            let frame_samples = vad.frame_samples();
            vad.set_hangover_frames(frames_for_duration_ms(
                VAD_CONTINUOUS_HANGOVER_MS,
                frame_samples,
            ));
            let frame_time = Duration::from_secs_f64(frame_samples as f64 / 16_000.0 / speed);
            info!(
                "Debug feed: {:.0} s of audio at {speed}x",
                samples.len() as f64 / 16_000.0
            );
            for frame in samples.chunks_exact(frame_samples) {
                if stop_flag.load(Ordering::Acquire) {
                    break;
                }
                match vad.push_frame(frame) {
                    Ok(VadFrame::Speech(audio)) => tap.speech(audio),
                    Ok(VadFrame::Noise) => tap.silence(frame_samples),
                    Err(e) => error!("Debug feed VAD error: {e}"),
                }
                thread::sleep(frame_time);
            }
            tap.finish();
            if !stop_flag.load(Ordering::Acquire) {
                request_stop(&shared.app.clone(), &shared, StopReason::User);
            }
        });
        Self { stop, thread }
    }

    pub fn finish(self) {
        self.stop.store(true, Ordering::Release);
        let _ = self.thread.join();
    }
}
