//! The live level meter on setup's Microphone screen. Nothing is kept: each
//! chunk becomes a number for the meter, and the audio is dropped.

use crate::managers::audio::AudioRecordingManager;
use handy_recorder::{AudioChunk, Recorder, Sink};
use log::warn;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, State};

const LEVEL_EVENT: &str = "mic-test-level";
const EMIT_EVERY: Duration = Duration::from_millis(66);
/// Quieter than this reads as an empty meter.
const FLOOR_DB: f32 = -60.0;

/// How full the meter is for these samples, from 0 to 1, on a decibel scale
/// so ordinary speech moves it.
pub fn meter_level(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let power = samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32;
    if power <= 0.0 {
        return 0.0;
    }
    let db = 10.0 * power.log10();
    ((db - FLOOR_DB) / -FLOOR_DB).clamp(0.0, 1.0)
}

struct LevelSink {
    app: AppHandle,
    last_emit: Instant,
    /// The loudest chunk since the last emit, so a short sound still shows.
    loudest: f32,
}

impl Sink for LevelSink {
    fn process_chunk(&mut self, chunk: AudioChunk<'_>) {
        let real = chunk.valid_frames * chunk.channels as usize;
        self.loudest = self.loudest.max(meter_level(&chunk.samples[..real]));
        if self.last_emit.elapsed() >= EMIT_EVERY {
            let _ = self.app.emit(LEVEL_EVENT, self.loudest);
            self.loudest = 0.0;
            self.last_emit = Instant::now();
        }
    }
}

#[derive(Default)]
pub struct MicTestState(Mutex<Option<Recorder<LevelSink>>>);

impl MicTestState {
    fn close_running(&self) {
        let running = self.0.lock().unwrap_or_else(|e| e.into_inner()).take();
        if let Some(recorder) = running {
            if let Err(e) = recorder.close() {
                warn!("Failed to close the microphone test: {e}");
            }
        }
    }
}

/// Starts the meter on the selected microphone, replacing any meter running.
#[tauri::command]
#[specta::specta]
pub fn start_mic_test(
    app: AppHandle,
    state: State<'_, MicTestState>,
    manager: State<'_, Arc<AudioRecordingManager>>,
) -> Result<(), String> {
    state.close_running();
    let mut recorder = Recorder::open(manager.mic_test_config()).map_err(|e| e.to_string())?;
    recorder
        .start(LevelSink {
            app,
            last_emit: Instant::now(),
            loudest: 0.0,
        })
        .map_err(|e| e.to_string())?;
    *state.0.lock().unwrap_or_else(|e| e.into_inner()) = Some(recorder);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn stop_mic_test(state: State<'_, MicTestState>) {
    state.close_running();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silence_leaves_the_meter_empty() {
        assert_eq!(meter_level(&[]), 0.0);
        assert_eq!(meter_level(&[0.0; 480]), 0.0);
        assert_eq!(meter_level(&[0.0001; 480]), 0.0, "below the floor");
    }

    #[test]
    fn speech_levels_land_mid_meter_and_a_full_signal_fills_it() {
        let speech = meter_level(&[0.05; 480]); // about -26 dB
        assert!((0.4..0.7).contains(&speech), "{speech}");
        assert_eq!(meter_level(&[1.0; 480]), 1.0);
    }
}
