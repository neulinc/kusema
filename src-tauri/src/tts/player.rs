//! The speech queue shared by every engine.
//!
//! Each sentence is synthesized to audio and queued on a rodio player, so
//! playback goes to the output device chosen in Kusema's settings, sentences
//! play back to back, and `stop()` silences everything at once. Kokoro voice
//! ids (`kokoro:…`) go to Kokoro; everything else, and any Kokoro sentence
//! that can't be synthesized, goes to the system voices.

use super::kokoro::{self, Kokoro};
use super::system::SystemVoices;
use super::{Audio, Synthesizer, TtsEngine, Voice};
use crate::audio_toolkit::audio::output_device_name;
use log::{debug, warn};
use rodio::buffer::SamplesBuffer;
use rodio::cpal::traits::HostTrait;
use rodio::{Decoder, DeviceSinkBuilder, MixerDeviceSink, Player};
use std::io::Cursor;
use std::num::NonZero;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Close the output device after this long with nothing to say.
const IDLE_CLOSE: Duration = Duration::from_secs(2);

struct Job {
    text: String,
    voice_id: Option<String>,
    rate: f32,
    epoch: u64,
}

#[derive(Default)]
struct Shared {
    /// Bumped by `stop()`; queued sentences from an older epoch are dropped.
    epoch: AtomicU64,
    /// Sentences accepted by `speak()` but not yet queued for playback.
    pending: AtomicUsize,
    player: Mutex<Option<Arc<Player>>>,
    output_device: Mutex<Option<String>>,
}

impl Shared {
    fn player(&self) -> std::sync::MutexGuard<'_, Option<Arc<Player>>> {
        self.player.lock().unwrap_or_else(|e| e.into_inner())
    }
}

struct Engines {
    system: SystemVoices,
    kokoro: Arc<Kokoro>,
}

pub struct Speaker {
    tx: Mutex<Sender<Job>>,
    shared: Arc<Shared>,
    engines: Arc<Engines>,
}

impl Speaker {
    pub fn new(kokoro: Arc<Kokoro>) -> Self {
        let (tx, rx) = mpsc::channel();
        let shared = Arc::new(Shared::default());
        let engines = Arc::new(Engines {
            system: SystemVoices,
            kokoro,
        });
        let worker_shared = Arc::clone(&shared);
        let worker_engines = Arc::clone(&engines);
        std::thread::Builder::new()
            .name("tts".into())
            .spawn(move || worker(rx, worker_shared, worker_engines))
            .expect("failed to start the text-to-speech thread");
        Self {
            tx: Mutex::new(tx),
            shared,
            engines,
        }
    }
}

impl TtsEngine for Speaker {
    fn list_voices(&self) -> Result<Vec<Voice>, String> {
        let mut voices = self.engines.kokoro.voices().unwrap_or_default();
        match self.engines.system.voices() {
            Ok(system) => voices.extend(system),
            // Kokoro voices are still worth offering without the system ones.
            Err(e) if !voices.is_empty() => warn!("Could not list system voices: {e}"),
            Err(e) => return Err(e),
        }
        Ok(voices)
    }

    fn speak(&self, text: &str, voice_id: Option<&str>, rate: f32) -> Result<(), String> {
        let job = Job {
            text: text.to_string(),
            voice_id: voice_id.map(str::to_string),
            rate,
            epoch: self.shared.epoch.load(Ordering::SeqCst),
        };
        self.shared.pending.fetch_add(1, Ordering::SeqCst);
        let sent = self.tx.lock().unwrap_or_else(|e| e.into_inner()).send(job);
        if sent.is_err() {
            self.shared.pending.fetch_sub(1, Ordering::SeqCst);
            return Err("The text-to-speech thread has stopped".to_string());
        }
        Ok(())
    }

    fn stop(&self) {
        self.shared.epoch.fetch_add(1, Ordering::SeqCst);
        if let Some(player) = self.shared.player().take() {
            player.stop();
        }
    }

    fn is_speaking(&self) -> bool {
        self.shared.pending.load(Ordering::SeqCst) > 0
            || self.shared.player().as_ref().is_some_and(|p| !p.empty())
    }

    fn set_output_device(&self, device: Option<String>) {
        *self
            .shared
            .output_device
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = device;
    }
}

/// Synthesizes queued sentences and appends them to the player. Owns the
/// output device, which is opened on demand and closed again when idle.
fn worker(rx: Receiver<Job>, shared: Arc<Shared>, engines: Arc<Engines>) {
    let mut sink: Option<(Option<String>, MixerDeviceSink)> = None;
    loop {
        let job = match rx.recv_timeout(IDLE_CLOSE) {
            Ok(job) => job,
            Err(RecvTimeoutError::Timeout) => {
                let mut player = shared.player();
                if sink.is_some() && player.as_ref().is_none_or(|p| p.empty()) {
                    debug!("Closing text-to-speech output device");
                    *player = None;
                    sink = None;
                }
                drop(player);
                engines.kokoro.unload_if_idle();
                continue;
            }
            Err(RecvTimeoutError::Disconnected) => return,
        };

        let current = || job.epoch == shared.epoch.load(Ordering::SeqCst);
        if current() {
            let started = Instant::now();
            let mut first = true;
            let mut out = |audio: Audio| {
                if !current() {
                    return false;
                }
                if first {
                    first = false;
                    debug!("First audio of the sentence after {:?}", started.elapsed());
                }
                if let Err(e) = play(&shared, &mut sink, audio) {
                    warn!("Failed to play synthesized speech: {e}");
                }
                true
            };
            if let Err(e) = synthesize(&engines, &job, &mut out) {
                warn!("Speech synthesis failed: {e}");
            }
        }
        shared.pending.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Route a sentence to its engine. A Kokoro voice that can't speak (not
/// downloaded, or failing) falls back to the default system voice, so a reply
/// is never silently dropped.
fn synthesize(
    engines: &Engines,
    job: &Job,
    out: &mut dyn FnMut(Audio) -> bool,
) -> Result<(), String> {
    let kokoro_voice = job
        .voice_id
        .as_deref()
        .and_then(|id| id.strip_prefix(kokoro::VOICE_PREFIX));
    let Some(voice) = kokoro_voice else {
        return engines
            .system
            .synthesize(&job.text, job.voice_id.as_deref(), job.rate, out);
    };
    let mut produced = false;
    let result = engines
        .kokoro
        .synthesize(&job.text, Some(voice), job.rate, &mut |audio| {
            produced = true;
            out(audio)
        });
    match result {
        Ok(()) => Ok(()),
        Err(e) if !produced => {
            warn!("Kokoro could not speak ({e}); using the system voice");
            engines.system.synthesize(&job.text, None, job.rate, out)
        }
        Err(e) => Err(e),
    }
}

fn play(
    shared: &Shared,
    sink: &mut Option<(Option<String>, MixerDeviceSink)>,
    audio: Audio,
) -> Result<(), String> {
    let device = shared
        .output_device
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    let mut player = shared.player();

    // Reopen when the output device setting changed since the last sentence.
    if sink.as_ref().is_some_and(|(open, _)| *open != device) && player.is_none() {
        *sink = None;
    }
    if sink.is_none() {
        *sink = Some((device.clone(), open_output(device)?));
    }
    let mixer = sink.as_ref().map(|(_, s)| s.mixer()).expect("opened above");
    let player = player.get_or_insert_with(|| Arc::new(Player::connect_new(mixer)));

    match audio {
        Audio::Wav(wav) => {
            let source = Decoder::new_wav(Cursor::new(wav)).map_err(|e| e.to_string())?;
            player.append(source);
        }
        Audio::Mono {
            sample_rate,
            samples,
        } => {
            let rate = NonZero::new(sample_rate).ok_or("Invalid sample rate")?;
            player.append(SamplesBuffer::new(NonZero::<u16>::MIN, rate, samples));
        }
    }
    Ok(())
}

/// Open the selected output device, falling back to the default one. Mirrors
/// `audio_feedback::play_audio_file`.
fn open_output(selected_device: Option<String>) -> Result<MixerDeviceSink, String> {
    let builder = match selected_device.filter(|name| name != "Default") {
        Some(name) => {
            let found = crate::audio_toolkit::get_cpal_host()
                .output_devices()
                .map_err(|e| e.to_string())?
                .find(|device| output_device_name(device).as_deref() == Some(&name));
            match found {
                Some(device) => DeviceSinkBuilder::from_device(device),
                None => {
                    warn!("Device '{name}' not found, using default device");
                    DeviceSinkBuilder::from_default_device()
                }
            }
        }
        None => DeviceSinkBuilder::from_default_device(),
    };
    let mut sink = builder
        .and_then(|b| b.open_stream())
        .map_err(|e| e.to_string())?;
    sink.log_on_drop(false);
    Ok(sink)
}
