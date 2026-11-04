//! Kusema microphone check, in setup and in Settings: a few seconds of quiet,
//! then a sentence read aloud, measured on this computer. The result says
//! what to change in plain words, and the room's noise level tunes the voice
//! detector for that microphone (`gate`). The audio stays in memory, only for
//! "Play my test back", and is dropped when the check closes; nothing is
//! saved to disk or sent anywhere.

mod analysis;
mod gate;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use handy_recorder::{AudioChunk, Recorder, Sink};
use log::{info, warn};
use tauri::{AppHandle, Emitter, Manager, State};

pub use analysis::{MicProfile, MicReport};

use crate::audio_toolkit::vad::VoiceActivityDetector;
use crate::managers::audio::AudioRecordingManager;
use crate::settings::{get_settings, write_settings, AppSettings};

/// The meter's event, shared with setup's level meter.
const LEVEL_EVENT: &str = "mic-test-level";
const EMIT_EVERY: Duration = Duration::from_millis(66);
/// A check never holds more than this much audio.
const MAX_SECONDS: usize = 20;
const RATE: u32 = 16_000;

/// The microphone a profile belongs to: the one chosen in Settings, or the
/// system default.
pub fn profile_key(settings: &AppSettings) -> String {
    settings
        .selected_microphone
        .clone()
        .filter(|name| !name.trim().is_empty() && !name.eq_ignore_ascii_case("default"))
        .unwrap_or_else(|| "default".to_string())
}

/// The gate for the current microphone, if it was measured and a gate helps.
pub fn gate_for(settings: &AppSettings) -> Option<f32> {
    if !settings.adaptive_vad {
        return None;
    }
    settings
        .mic_profiles
        .get(&profile_key(settings))
        .and_then(analysis::gate_db)
}

/// The voice detector, with the gate for the current microphone in front of
/// it when there is one.
pub fn gated(
    detector: Box<dyn VoiceActivityDetector>,
    app: &AppHandle,
) -> Box<dyn VoiceActivityDetector> {
    match gate_for(&get_settings(app)) {
        Some(gate_db) => {
            info!("Voice detection ignores sound under {gate_db:.1} dBFS (measured room noise)");
            Box::new(gate::LevelGate::new(detector, gate_db))
        }
        None => detector,
    }
}

/// Rebuilds the recorder's voice detector, so a new measurement, the switch
/// or another microphone takes effect from the next recording. Off the
/// calling thread: reopening an always-on microphone can block.
pub fn refresh_gate(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        let backend = get_settings(&app).vad_backend;
        let rm = Arc::clone(&*app.state::<Arc<AudioRecordingManager>>());
        if let Err(e) = rm.update_vad_backend(backend) {
            // Recording right now: the next recorder open picks it up anyway.
            warn!("Couldn't apply the measured room noise yet: {e}");
        }
    });
}

/// The audio a check is collecting, and where its quiet part ended.
#[derive(Default)]
struct Collected {
    samples: Vec<f32>,
    /// Where the sentence began, once the quiet part is over.
    split: Option<usize>,
    /// Samples since `split` that were at full scale, and in all.
    clipped: usize,
    counted: usize,
}

impl Collected {
    fn push(&mut self, samples: &[f32]) {
        let room =
            MAX_SECONDS * RATE as usize - self.samples.len().min(MAX_SECONDS * RATE as usize);
        let samples = &samples[..samples.len().min(room)];
        if self.split.is_some() {
            self.counted += samples.len();
            self.clipped += samples.iter().filter(|s| analysis::is_clipped(**s)).count();
        }
        self.samples.extend_from_slice(samples);
    }
}

struct CollectSink {
    app: AppHandle,
    collected: Arc<Mutex<Collected>>,
    last_emit: Instant,
    loudest: f32,
}

impl CollectSink {
    fn take(&mut self, samples: &[f32]) {
        self.loudest = self
            .loudest
            .max(crate::setup::mic_test::meter_level(samples));
        lock(&self.collected).push(samples);
        if self.last_emit.elapsed() >= EMIT_EVERY {
            let _ = self.app.emit(LEVEL_EVENT, self.loudest);
            self.loudest = 0.0;
            self.last_emit = Instant::now();
        }
    }
}

impl Sink for CollectSink {
    fn process_chunk(&mut self, chunk: AudioChunk<'_>) {
        let real = chunk.valid_frames * chunk.channels as usize;
        self.take(&chunk.samples[..real]);
    }
}

enum Source {
    Microphone(Recorder<CollectSink>),
    /// Debug builds: a WAV file played in real time instead of the microphone.
    #[cfg(debug_assertions)]
    File(Arc<AtomicBool>),
}

struct Running {
    source: Source,
    collected: Arc<Mutex<Collected>>,
    device_name: String,
    device_rate: Option<u32>,
}

/// The check in progress, and the last one's audio for "Play my test back".
#[derive(Default)]
pub struct MicCheckState {
    running: Mutex<Option<Running>>,
    last: Mutex<Option<Vec<f32>>>,
    playing: Arc<AtomicBool>,
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

impl MicCheckState {
    fn stop_running(&self) -> Option<Running> {
        let running = lock(&self.running).take()?;
        match &running.source {
            Source::Microphone(_) => {}
            #[cfg(debug_assertions)]
            Source::File(stop) => stop.store(true, Ordering::Release),
        }
        Some(running)
    }
}

fn close(running: Running) -> (Arc<Mutex<Collected>>, String, Option<u32>) {
    if let Source::Microphone(recorder) = running.source {
        if let Err(e) = recorder.close() {
            warn!("Failed to close the microphone check: {e}");
        }
    }
    (running.collected, running.device_name, running.device_rate)
}

/// Debug builds: `KUSEMA_DEBUG_MIC_CHECK_WAV` plays a 16 kHz mono file into
/// the check instead of the microphone, so it can be tested without anyone
/// speaking.
#[cfg(debug_assertions)]
fn debug_wav() -> Option<std::path::PathBuf> {
    std::env::var_os("KUSEMA_DEBUG_MIC_CHECK_WAV")
        .map(std::path::PathBuf::from)
        .filter(|path| path.exists())
}

/// Starts a check on the selected microphone: the quiet part begins now.
#[tauri::command]
#[specta::specta]
pub fn start_mic_check(
    app: AppHandle,
    state: State<'_, MicCheckState>,
    manager: State<'_, Arc<AudioRecordingManager>>,
) -> Result<(), String> {
    if let Some(running) = state.stop_running() {
        close(running);
    }
    // The setup screen's meter would hold the microphone too.
    if let Some(meter) = app.try_state::<crate::setup::mic_test::MicTestState>() {
        crate::setup::mic_test::stop_mic_test(meter);
    }
    *lock(&state.last) = None;
    let collected = Arc::new(Mutex::new(Collected::default()));
    let sink = CollectSink {
        app: app.clone(),
        collected: Arc::clone(&collected),
        last_emit: Instant::now(),
        loudest: 0.0,
    };

    #[cfg(debug_assertions)]
    if let Some(path) = debug_wav() {
        let samples = crate::audio_toolkit::read_wav_samples(&path).map_err(|e| e.to_string())?;
        let stop = Arc::new(AtomicBool::new(false));
        let stop_flag = Arc::clone(&stop);
        std::thread::spawn(move || {
            let mut sink = sink;
            for chunk in samples.chunks(480) {
                if stop_flag.load(Ordering::Acquire) {
                    break;
                }
                sink.take(chunk);
                std::thread::sleep(Duration::from_millis(30));
            }
        });
        *lock(&state.running) = Some(Running {
            source: Source::File(stop),
            collected,
            device_name: "Test recording".to_string(),
            device_rate: Some(RATE),
        });
        return Ok(());
    }

    let recorder = Recorder::open(manager.mic_test_config()).map_err(|e| e.to_string())?;
    let device_name = recorder.info().device.name.clone();
    let device_rate = Some(recorder.info().device_format.sample_rate);
    recorder.start(sink).map_err(|e| e.error.to_string())?;
    info!("Microphone check started on {device_name}");
    *lock(&state.running) = Some(Running {
        source: Source::Microphone(recorder),
        collected,
        device_name,
        device_rate,
    });
    Ok(())
}

/// The quiet part is over: the sentence starts now.
#[tauri::command]
#[specta::specta]
pub fn mark_mic_check_speech(state: State<'_, MicCheckState>) {
    if let Some(running) = lock(&state.running).as_ref() {
        let mut collected = lock(&running.collected);
        if collected.split.is_none() {
            collected.split = Some(collected.samples.len());
        }
    }
}

/// Ends the check and says what it found. The room's noise is kept for this
/// microphone, so the voice detector can ignore it.
#[tauri::command]
#[specta::specta]
pub fn finish_mic_check(
    app: AppHandle,
    state: State<'_, MicCheckState>,
) -> Result<MicReport, String> {
    let running = state
        .stop_running()
        .ok_or("The microphone check isn't running")?;
    let (collected, device_name, device_rate) = close(running);
    let collected = std::mem::take(&mut *lock(&collected));
    let split = collected
        .split
        .ok_or("The microphone check ended before the sentence")?
        .min(collected.samples.len());
    let clipped = if collected.counted == 0 {
        0.0
    } else {
        100.0 * collected.clipped as f32 / collected.counted as f32
    };
    let (quiet, speech) = collected.samples.split_at(split);
    let report = analysis::analyze(quiet, speech, clipped, RATE, &device_name, device_rate);
    info!(
        "Microphone check: noise {:.1} dBFS, speech {:.1} dBFS, {:.1} dB apart, {:.3}% clipped, tips {:?}",
        report.noise_db, report.speech_db, report.snr_db, report.clipped_percent, report.tips
    );
    let mut settings = get_settings(&app);
    let key = profile_key(&settings);
    match report.profile() {
        Some(profile) => {
            settings.mic_profiles.insert(key, profile);
        }
        None => {
            settings.mic_profiles.remove(&key);
        }
    }
    write_settings(&app, settings);
    refresh_gate(&app);
    *lock(&state.last) = Some(collected.samples);
    Ok(report)
}

/// Stops a check without measuring, and forgets its audio.
#[tauri::command]
#[specta::specta]
pub fn cancel_mic_check(state: State<'_, MicCheckState>) {
    if let Some(running) = state.stop_running() {
        close(running);
    }
    *lock(&state.last) = None;
}

/// Plays the last check back, from memory, on the chosen output device.
#[tauri::command]
#[specta::specta]
pub fn play_mic_check(app: AppHandle, state: State<'_, MicCheckState>) -> Result<(), String> {
    let samples = lock(&state.last)
        .clone()
        .ok_or("There's no test to play back")?;
    if state.playing.swap(true, Ordering::AcqRel) {
        return Ok(());
    }
    let playing = Arc::clone(&state.playing);
    let device = get_settings(&app).selected_output_device;
    std::thread::spawn(move || {
        if let Err(e) = crate::tts::play_samples(device, RATE, samples) {
            warn!("Couldn't play the microphone check back: {e}");
        }
        playing.store(false, Ordering::Release);
    });
    Ok(())
}

/// The measured profile of the current microphone, and the level the voice
/// detector ignores sound under (`None` when no gate applies).
#[derive(Clone, Debug, serde::Serialize, specta::Type)]
pub struct MicProfileStatus {
    pub profile: Option<MicProfile>,
    pub gate_db: Option<f32>,
}

#[tauri::command]
#[specta::specta]
pub fn get_mic_profile(app: AppHandle) -> MicProfileStatus {
    let settings = get_settings(&app);
    MicProfileStatus {
        profile: settings.mic_profiles.get(&profile_key(&settings)).copied(),
        gate_db: gate_for(&settings),
    }
}

#[tauri::command]
#[specta::specta]
pub fn change_adaptive_vad_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = get_settings(&app);
    settings.adaptive_vad = enabled;
    write_settings(&app, settings);
    refresh_gate(&app);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::get_default_settings;

    #[test]
    fn the_gate_follows_the_chosen_microphone_and_the_switch() {
        let mut settings = get_default_settings();
        assert_eq!(gate_for(&settings), None, "nothing measured");
        settings.mic_profiles.insert(
            "default".into(),
            MicProfile {
                noise_db: -45.0,
                speech_db: -18.0,
            },
        );
        assert_eq!(gate_for(&settings), Some(-37.0));
        // Setup names the default device "Default".
        settings.selected_microphone = Some("Default".into());
        assert_eq!(gate_for(&settings), Some(-37.0));
        settings.selected_microphone = None;
        settings.adaptive_vad = false;
        assert_eq!(gate_for(&settings), None);
        settings.adaptive_vad = true;
        // Another microphone hasn't been measured.
        settings.selected_microphone = Some("USB Microphone".into());
        assert_eq!(gate_for(&settings), None);
    }

    #[test]
    fn a_check_holds_at_most_twenty_seconds_and_counts_clipping_in_the_sentence() {
        let mut collected = Collected::default();
        collected.push(&[1.0; 16_000]);
        // Clipping in the quiet part isn't counted.
        assert_eq!(collected.clipped, 0);
        collected.split = Some(collected.samples.len());
        collected.push(&[0.5; 15_000]);
        collected.push(&[1.0; 1_000]);
        assert_eq!((collected.clipped, collected.counted), (1_000, 16_000));
        for _ in 0..30 {
            collected.push(&[0.1; 16_000]);
        }
        assert_eq!(collected.samples.len(), MAX_SECONDS * RATE as usize);
    }
}
