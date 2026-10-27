//! Kusema text-to-speech: speaks assistant replies sentence by sentence.

pub mod kokoro;
mod player;
mod sentences;
mod system;

use crate::managers::audio::AudioRecordingManager;
use crate::settings::{self, AppSettings};
use crate::shortcut;
use crate::tray::{set_tray_state, TrayIconState};
use log::{debug, warn};
use sentences::SentenceSplitter;
use serde::Serialize;
use specta::Type;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Manager, State};

pub use player::Speaker;

/// Lowest and highest speaking rate offered (1.0 is the voice's normal speed).
pub const MIN_RATE: f32 = 0.5;
pub const MAX_RATE: f32 = 2.0;

const FINISH_POLL_INTERVAL: Duration = Duration::from_millis(50);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
pub struct Voice {
    pub id: String,
    pub name: String,
    /// BCP-47 tag, e.g. "en-US".
    pub language: String,
    /// Engine label for the voice picker: "Kokoro", "Windows", "macOS"...
    pub engine: String,
}

/// Synthesized speech for the player.
pub enum Audio {
    Wav(Vec<u8>),
    /// Mono samples in -1.0..=1.0.
    Mono {
        sample_rate: u32,
        samples: Vec<f32>,
    },
}

/// Turns text into audio; [`Speaker`] queues and plays it.
pub trait Synthesizer: Send + Sync {
    fn voices(&self) -> Result<Vec<Voice>, String>;
    /// Synthesize `text`, passing each piece to `out` as soon as it's ready
    /// (long sentences may come in several pieces). `out` returns `false` once
    /// speech was interrupted, so the rest can be skipped.
    fn synthesize(
        &self,
        text: &str,
        voice_id: Option<&str>,
        rate: f32,
        out: &mut dyn FnMut(Audio) -> bool,
    ) -> Result<(), String>;
}

/// A text-to-speech engine. `speak` queues text behind anything already
/// queued and returns without waiting for playback.
pub trait TtsEngine: Send + Sync {
    fn list_voices(&self) -> Result<Vec<Voice>, String>;
    /// `voice_id` is an id from `list_voices()`, or `None` for the default.
    fn speak(&self, text: &str, voice_id: Option<&str>, rate: f32) -> Result<(), String>;
    /// Silence playback and drop everything queued.
    fn stop(&self);
    /// Whether anything is playing or still queued.
    fn is_speaking(&self) -> bool;
    /// Where to play, by output device name. Engines that play through the OS
    /// may ignore it.
    fn set_output_device(&self, _device: Option<String>) {}
}

/// Tauri managed state.
pub struct SpeechState {
    engine: Arc<dyn TtsEngine>,
    /// Bumped by `stop()`, so a reply still streaming stops feeding sentences.
    epoch: AtomicU64,
    /// The overlay/tray show "speaking" for a reply.
    speaking: AtomicBool,
}

impl SpeechState {
    pub fn new(engine: Arc<dyn TtsEngine>) -> Self {
        Self {
            engine,
            epoch: AtomicU64::new(0),
            speaking: AtomicBool::new(false),
        }
    }
}

/// Set up speech: the Kokoro voices (files in the app data folder) and the
/// shared player. Called from `setup`, as Kokoro needs the app handle.
pub fn init(app: &AppHandle) {
    let kokoro = Arc::new(kokoro::Kokoro::new(app));
    app.manage(SpeechState::new(Arc::new(Speaker::new(Arc::clone(
        &kokoro,
    )))));
    app.manage(kokoro);
}

/// Get ready to speak a reply soon: loads Kokoro in the background when the
/// selected voice uses it, so the first sentence isn't held up by loading.
pub fn prepare(app: &AppHandle) {
    let settings = settings::get_settings(app);
    if !settings.assistant_speak_replies {
        return;
    }
    let is_kokoro = settings
        .assistant_voice
        .as_deref()
        .is_some_and(|v| v.starts_with(kokoro::VOICE_PREFIX));
    if let (true, Some(kokoro)) = (is_kokoro, app.try_state::<Arc<kokoro::Kokoro>>()) {
        kokoro.preload();
    }
}

fn state(app: &AppHandle) -> Option<State<'_, SpeechState>> {
    app.try_state::<SpeechState>()
}

/// Whether a reply is being spoken (the Cancel hotkey uses this).
pub fn is_speaking(app: &AppHandle) -> bool {
    state(app).is_some_and(|s| s.speaking.load(Ordering::SeqCst))
}

/// Stop speaking immediately. Leaves the overlay and tray alone: callers are
/// starting something that takes them over, or reset them themselves.
/// Returns whether a reply was being spoken.
pub fn stop(app: &AppHandle) -> bool {
    let Some(state) = state(app) else {
        return false;
    };
    state.epoch.fetch_add(1, Ordering::SeqCst);
    state.engine.stop();
    state.speaking.swap(false, Ordering::SeqCst)
}

/// Stop speaking and, if nothing else is using them, return the overlay and
/// tray to idle.
pub fn stop_and_idle(app: &AppHandle) {
    if stop(app) {
        return_to_idle(app);
    }
}

fn return_to_idle(app: &AppHandle) {
    let recording = app
        .try_state::<Arc<AudioRecordingManager>>()
        .is_some_and(|rm| rm.is_recording());
    // The recording, or the assistant pipeline still finishing up, owns the
    // UI and resets it itself.
    if recording || crate::assistant::is_busy(app) {
        return;
    }
    shortcut::unregister_cancel_shortcut(app);
    crate::utils::hide_recording_overlay(app);
    set_tray_state(app, TrayIconState::Idle);
}

fn configure(engine: &dyn TtsEngine, settings: &AppSettings) -> (Option<String>, f32) {
    engine.set_output_device(settings.selected_output_device.clone());
    (
        settings.assistant_voice.clone(),
        settings.assistant_speech_rate.clamp(MIN_RATE, MAX_RATE),
    )
}

/// Speaks one streamed reply: feed it text as it arrives, then `finish()`.
pub struct ReplySpeech {
    app: AppHandle,
    epoch: u64,
    splitter: SentenceSplitter,
    voice: Option<String>,
    rate: f32,
    started: bool,
}

/// Start speaking a reply, or `None` when "Speak replies" is off.
pub fn begin_reply(app: &AppHandle) -> Option<ReplySpeech> {
    let settings = settings::get_settings(app);
    if !settings.assistant_speak_replies {
        return None;
    }
    let state = state(app)?;
    let (voice, rate) = configure(state.engine.as_ref(), &settings);
    Some(ReplySpeech {
        app: app.clone(),
        epoch: state.epoch.load(Ordering::SeqCst),
        splitter: SentenceSplitter::default(),
        voice,
        rate,
        started: false,
    })
}

impl ReplySpeech {
    /// Add streamed reply text; complete sentences start speaking right away.
    pub fn push(&mut self, delta: &str) {
        for sentence in self.splitter.push(delta) {
            self.say(&sentence);
        }
    }

    /// The reply is complete: speak what's left, then return the overlay and
    /// tray to idle once playback ends (unless interrupted first).
    pub fn finish(mut self) {
        if let Some(rest) = self.splitter.finish() {
            self.say(&rest);
        }
        if !self.started {
            return;
        }
        let app = self.app.clone();
        let epoch = self.epoch;
        std::thread::spawn(move || loop {
            std::thread::sleep(FINISH_POLL_INTERVAL);
            let Some(state) = state(&app) else { return };
            if state.epoch.load(Ordering::SeqCst) != epoch {
                return; // interrupted; whoever stopped it owns the UI now
            }
            if !state.engine.is_speaking() {
                if state.speaking.swap(false, Ordering::SeqCst) {
                    debug!("Finished speaking the reply");
                    return_to_idle(&app);
                }
                return;
            }
        });
    }

    fn say(&mut self, sentence: &str) {
        let Some(state) = state(&self.app) else {
            return;
        };
        if state.epoch.load(Ordering::SeqCst) != self.epoch {
            return;
        }
        if let Err(e) = state
            .engine
            .speak(sentence, self.voice.as_deref(), self.rate)
        {
            warn!("Could not speak the reply: {e}");
            return;
        }
        if !self.started {
            self.started = true;
            state.speaking.store(true, Ordering::SeqCst);
            crate::overlay::show_speaking_overlay(&self.app);
            set_tray_state(&self.app, TrayIconState::Speaking);
            // Escape must be able to interrupt.
            shortcut::register_cancel_shortcut(&self.app);
        }
    }
}

#[tauri::command]
#[specta::specta]
pub async fn get_tts_voices(state: State<'_, SpeechState>) -> Result<Vec<Voice>, String> {
    let mut voices = state.engine.list_voices()?;
    // Kokoro first, then the system voices.
    voices.sort_by(|a, b| {
        (b.engine == kokoro::ENGINE)
            .cmp(&(a.engine == kokoro::ENGINE))
            .then_with(|| a.language.cmp(&b.language))
            .then_with(|| a.name.cmp(&b.name))
    });
    Ok(voices)
}

/// Speak `text` with the given voice and rate, interrupting anything playing.
#[tauri::command]
#[specta::specta]
pub async fn preview_tts_voice(
    app: AppHandle,
    text: String,
    voice_id: Option<String>,
    rate: f32,
) -> Result<(), String> {
    stop_and_idle(&app);
    let state = state(&app).ok_or("Text-to-speech is not initialized")?;
    state
        .engine
        .set_output_device(settings::get_settings(&app).selected_output_device);
    state
        .engine
        .speak(&text, voice_id.as_deref(), rate.clamp(MIN_RATE, MAX_RATE))
}

#[tauri::command]
#[specta::specta]
pub fn change_assistant_speak_replies_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.assistant_speak_replies = enabled;
    settings::write_settings(&app, settings);
    if !enabled {
        stop_and_idle(&app);
    }
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_assistant_voice_setting(app: AppHandle, voice: Option<String>) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.assistant_voice = voice.filter(|v| !v.is_empty());
    settings::write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_assistant_speech_rate_setting(app: AppHandle, rate: f32) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.assistant_speech_rate = rate.clamp(MIN_RATE, MAX_RATE);
    settings::write_settings(&app, settings);
    Ok(())
}
