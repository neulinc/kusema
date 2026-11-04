//! Kusema live preview: the words in the pill while you speak, like Dragon's
//! results box. They come from the model's own stream (Parakeet Unified and
//! Nemotron Streaming have one), on this computer. The preview is only ever
//! shown: what's typed is a full run over the recording (hold-to-talk) or
//! the segment (continuous dictation), as before, for the best accuracy.

pub mod dictation;
pub mod display;
pub mod throttle;

use std::sync::{Arc, Mutex};
use std::time::Instant;

use log::{info, warn};
use tauri::{AppHandle, Emitter};
use tauri_specta::Event;

use crate::managers::transcription::StreamTextEvent;
use crate::settings::{get_settings, write_settings, AppSettings};
use throttle::{Throttle, Verdict};

/// Parakeet Unified's stream answers every 560 ms of audio with no lookahead,
/// instead of its default 1040 ms plus 1040 ms: the first words show after
/// about 1.1 s instead of 2. On a Ryzen 5 5600H's CPU that costs 0.57 s of
/// compute per second of speech instead of 0.35. Its own text is a little
/// less accurate, which doesn't matter: it's never typed.
const PARAKEET_UNIFIED_CHUNK_MS: i32 = 560;

/// How a model's preview stream is set up.
pub fn stream_options(model_id: &str) -> transcribe_cpp::StreamOptions {
    use transcribe_cpp::{ParakeetBufferedStreamOptions, StreamExtension, StreamOptions};
    if model_id.contains("parakeet-unified") {
        return StreamOptions {
            family: Some(StreamExtension::ParakeetBuffered(
                ParakeetBufferedStreamOptions {
                    left_ms: None,
                    chunk_ms: Some(PARAKEET_UNIFIED_CHUNK_MS),
                    right_ms: Some(0),
                },
            )),
            ..Default::default()
        };
    }
    // Nemotron Streaming shows its first words after about a second as it is.
    StreamOptions::default()
}

/// Told to the pill once, when the preview turns itself off.
const OFF_EVENT: &str = "live-preview-off";

pub fn enabled(settings: &AppSettings) -> bool {
    settings.live_preview
}

/// The preview's text for the pill: `committed` and `tentative` as the
/// stream gave them (after any Chinese script conversion).
pub fn show(app: &AppHandle, settings: &AppSettings, committed: &str, tentative: &str) {
    let (committed, tentative) = display::display(settings, committed, tentative);
    emit(app, committed, tentative);
}

/// Clears the preview.
pub fn clear(app: &AppHandle) {
    emit(app, String::new(), String::new());
}

fn emit(app: &AppHandle, committed: String, tentative: String) {
    if let Err(e) = (StreamTextEvent {
        committed,
        tentative,
    })
    .emit(app)
    {
        warn!("Failed to emit the live preview: {e}");
    }
}

/// The preview couldn't keep up even at its slowest: off from now on, and the
/// user is told once. They can turn it back on in Settings.
pub fn turn_off_for_speed(app: &AppHandle) {
    let mut settings = get_settings(app);
    if !settings.live_preview {
        return;
    }
    info!("Live preview turned off: this computer couldn't keep up");
    settings.live_preview = false;
    settings.live_preview_auto_off = true;
    write_settings(app, settings);
    clear(app);
    let _ = app.emit(OFF_EVENT, ());
    let _ = app.emit(
        "settings-changed",
        serde_json::json!({ "setting": "live_preview", "value": false }),
    );
}

/// What a stream's answers do: watch the pace, and show the text.
pub struct Progress {
    app: AppHandle,
    settings: AppSettings,
    throttle: Arc<Mutex<Throttle>>,
    started: Instant,
}

impl Progress {
    pub fn new(app: &AppHandle, throttle: Arc<Mutex<Throttle>>) -> Self {
        Self {
            app: app.clone(),
            settings: get_settings(app),
            throttle,
            started: Instant::now(),
        }
    }

    /// The engine took in `processed_ms` of audio so far; `text` is new
    /// text, if it changed. Runs on the engine's thread: quick, no waiting.
    pub fn answer(&mut self, processed_ms: i64, text: Option<(&str, &str)>) {
        let verdict = lock(&self.throttle).progress(processed_ms, self.started.elapsed());
        match verdict {
            Verdict::Off => {
                turn_off_for_speed(&self.app);
                return;
            }
            Verdict::Slower => info!(
                "Live preview fell behind; updating less often ({} ms of audio per update)",
                lock(&self.throttle).chunk_samples() / 16
            ),
            Verdict::Keep => {}
        }
        if let Some((committed, tentative)) = text {
            show(&self.app, &self.settings, committed, tentative);
        }
    }
}

pub fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

/// Gathers audio into the throttle's chunk size before each feed.
#[derive(Default)]
pub struct Feeder {
    pending: Vec<f32>,
}

impl Feeder {
    /// Adds `pcm`; returns the chunks to feed now, if any.
    pub fn push(&mut self, throttle: &Mutex<Throttle>, pcm: &[f32]) -> Option<Vec<f32>> {
        let mut throttle = lock(throttle);
        if throttle.is_off() {
            self.pending.clear();
            return None;
        }
        self.pending.extend_from_slice(pcm);
        if self.pending.len() < throttle.chunk_samples() {
            return None;
        }
        let chunk = std::mem::take(&mut self.pending);
        throttle.fed(chunk.len());
        Some(chunk)
    }

    pub fn clear(&mut self) {
        self.pending.clear();
    }
}

#[tauri::command]
#[specta::specta]
pub fn change_live_preview_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = get_settings(&app);
    settings.live_preview = enabled;
    // Turned back on: the note about turning off no longer applies.
    if enabled {
        settings.live_preview_auto_off = false;
    }
    write_settings(&app, settings);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parakeet_unified_answers_sooner_and_other_models_keep_their_own() {
        let unified = stream_options("parakeet-unified-en-0.6b");
        assert!(matches!(
            unified.family,
            Some(transcribe_cpp::StreamExtension::ParakeetBuffered(ref o))
                if o.chunk_ms == Some(560) && o.right_ms == Some(0)
        ));
        assert_eq!(
            stream_options("nemotron-3.5-asr-streaming-0.6b"),
            transcribe_cpp::StreamOptions::default()
        );
    }

    #[test]
    fn audio_is_fed_in_the_throttles_chunks() {
        let throttle = Mutex::new(Throttle::new());
        let mut feeder = Feeder::default();
        // 30 ms frames, as the recorder delivers them: the first feed waits
        // for 512 samples.
        assert!(feeder.push(&throttle, &[0.0; 480]).is_none());
        assert_eq!(
            feeder.push(&throttle, &[0.0; 480]).map(|c| c.len()),
            Some(960)
        );
        assert!(feeder.push(&throttle, &[0.0; 480]).is_none());
    }
}
