//! Speech with the operating system's built-in voices.
//!
//! Each sentence is synthesized to WAV and queued on a rodio player, so
//! playback goes to the output device chosen in Kusema's settings, sentences
//! play back to back, and `stop()` silences everything at once.
//!
//! - Windows: WinRT `Windows.Media.SpeechSynthesis`, i.e. the OneCore voices,
//!   which sound more natural than the legacy SAPI desktop voices.
//! - macOS: the `say` command.
//! - Linux: `espeak-ng` (or `espeak`), if installed.

use super::{TtsEngine, Voice};
use crate::audio_toolkit::audio::output_device_name;
use log::{debug, warn};
use rodio::cpal::traits::HostTrait;
use rodio::{Decoder, DeviceSinkBuilder, MixerDeviceSink, Player};
use std::io::Cursor;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

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

pub struct SystemTts {
    tx: Mutex<Sender<Job>>,
    shared: Arc<Shared>,
}

impl SystemTts {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        let shared = Arc::new(Shared::default());
        let worker_shared = Arc::clone(&shared);
        std::thread::Builder::new()
            .name("tts".into())
            .spawn(move || worker(rx, worker_shared))
            .expect("failed to start the text-to-speech thread");
        Self {
            tx: Mutex::new(tx),
            shared,
        }
    }
}

impl TtsEngine for SystemTts {
    fn list_voices(&self) -> Result<Vec<Voice>, String> {
        platform::voices()
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
fn worker(rx: Receiver<Job>, shared: Arc<Shared>) {
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
                continue;
            }
            Err(RecvTimeoutError::Disconnected) => return,
        };

        let current = |job: &Job| job.epoch == shared.epoch.load(Ordering::SeqCst);
        if current(&job) {
            match platform::synthesize(&job.text, job.voice_id.as_deref(), job.rate) {
                Ok(wav) if current(&job) => {
                    if let Err(e) = play(&shared, &mut sink, wav) {
                        warn!("Failed to play synthesized speech: {e}");
                    }
                }
                Ok(_) => {}
                Err(e) => warn!("Speech synthesis failed: {e}"),
            }
        }
        shared.pending.fetch_sub(1, Ordering::SeqCst);
    }
}

fn play(
    shared: &Shared,
    sink: &mut Option<(Option<String>, MixerDeviceSink)>,
    wav: Vec<u8>,
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

    let source = Decoder::new_wav(Cursor::new(wav)).map_err(|e| e.to_string())?;
    player.append(source);
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

/// Words per minute at rate 1.0 for the command-line engines.
#[cfg_attr(windows, allow(dead_code))]
const BASE_WPM: f32 = 175.0;

#[cfg_attr(windows, allow(dead_code))]
fn words_per_minute(rate: f32) -> u32 {
    (BASE_WPM * rate.clamp(0.5, 2.0)).round() as u32
}

/// Parse `say -v ?` lines such as `Samantha    en_US    # Hello, my name is Samantha.`
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn parse_say_voices(output: &str) -> Vec<Voice> {
    output
        .lines()
        .filter_map(|line| {
            let left = line.split('#').next()?.trim_end();
            let (name, language) = left.rsplit_once(char::is_whitespace)?;
            let name = name.trim();
            (!name.is_empty()).then(|| Voice {
                id: name.to_string(),
                name: name.to_string(),
                language: language.replace('_', "-"),
            })
        })
        .collect()
}

/// Parse `espeak-ng --voices`: `Pty Language Age/Gender VoiceName File Other`.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn parse_espeak_voices(output: &str) -> Vec<Voice> {
    output
        .lines()
        .skip(1)
        .filter_map(|line| {
            let mut cols = line.split_whitespace();
            let _priority = cols.next()?;
            let language = cols.next()?;
            let _age_gender = cols.next()?;
            let name = cols.next()?;
            Some(Voice {
                id: language.to_string(),
                name: name.replace('_', " "),
                language: language.to_string(),
            })
        })
        .collect()
}

#[cfg(windows)]
mod platform {
    use super::Voice;
    use windows::core::HSTRING;
    use windows::Media::SpeechSynthesis::{SpeechSynthesizer, VoiceInformation};
    use windows::Storage::Streams::DataReader;

    fn err(e: windows::core::Error) -> String {
        format!("Windows speech error: {}", e.message())
    }

    fn all_voices() -> windows::core::Result<Vec<VoiceInformation>> {
        let voices = SpeechSynthesizer::AllVoices()?;
        (0..voices.Size()?).map(|i| voices.GetAt(i)).collect()
    }

    pub fn voices() -> Result<Vec<Voice>, String> {
        all_voices()
            .and_then(|voices| {
                voices
                    .iter()
                    .map(|v| {
                        Ok(Voice {
                            id: v.Id()?.to_string(),
                            name: v.DisplayName()?.to_string(),
                            language: v.Language()?.to_string(),
                        })
                    })
                    .collect()
            })
            .map_err(err)
    }

    pub fn synthesize(text: &str, voice_id: Option<&str>, rate: f32) -> Result<Vec<u8>, String> {
        let synth = SpeechSynthesizer::new().map_err(err)?;
        if let Some(id) = voice_id {
            let wanted = HSTRING::from(id);
            match all_voices()
                .map_err(err)?
                .into_iter()
                .find(|v| v.Id().is_ok_and(|vid| vid == wanted))
            {
                Some(voice) => synth.SetVoice(&voice).map_err(err)?,
                None => log::warn!("Voice '{id}' is not installed; using the default voice"),
            }
        }
        // SpeakingRate needs Windows 10 1709+; speak at normal speed without it.
        if let Err(e) = synth
            .Options()
            .and_then(|o| o.SetSpeakingRate(f64::from(rate.clamp(0.5, 6.0))))
        {
            log::debug!("Could not set the speaking rate: {}", e.message());
        }

        let stream = synth
            .SynthesizeTextToStreamAsync(&HSTRING::from(text))
            .and_then(|op| op.get())
            .map_err(err)?;
        let size = u32::try_from(stream.Size().map_err(err)?)
            .map_err(|_| "Synthesized speech is too large".to_string())?;
        let reader =
            DataReader::CreateDataReader(&stream.GetInputStreamAt(0).map_err(err)?).map_err(err)?;
        reader
            .LoadAsync(size)
            .and_then(|op| op.get())
            .map_err(err)?;
        let mut wav = vec![0u8; size as usize];
        reader.ReadBytes(&mut wav).map_err(err)?;
        Ok(wav)
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use super::{parse_say_voices, words_per_minute, Voice};
    use std::io::Write;
    use std::process::{Command, Stdio};

    pub fn voices() -> Result<Vec<Voice>, String> {
        let output = Command::new("say")
            .args(["-v", "?"])
            .output()
            .map_err(|e| format!("Could not run say: {e}"))?;
        Ok(parse_say_voices(&String::from_utf8_lossy(&output.stdout)))
    }

    pub fn synthesize(text: &str, voice_id: Option<&str>, rate: f32) -> Result<Vec<u8>, String> {
        let path = std::env::temp_dir().join(format!(
            "kusema-tts-{}-{}.wav",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let mut command = Command::new("say");
        command
            .arg("-o")
            .arg(&path)
            .args(["--file-format=WAVE", "--data-format=LEI16@22050", "-f", "-"])
            .args(["-r", &words_per_minute(rate).to_string()]);
        if let Some(voice) = voice_id {
            command.args(["-v", voice]);
        }
        let mut child = command
            .stdin(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Could not run say: {e}"))?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(text.as_bytes())
                .map_err(|e| e.to_string())?;
        }
        let status = child.wait().map_err(|e| e.to_string())?;
        let wav = std::fs::read(&path);
        let _ = std::fs::remove_file(&path);
        if !status.success() {
            return Err(format!("say failed with {status}"));
        }
        wav.map_err(|e| e.to_string())
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use super::{parse_espeak_voices, words_per_minute, Voice};
    use std::io::Write;
    use std::process::{Command, Stdio};

    const PROGRAMS: [&str; 2] = ["espeak-ng", "espeak"];

    fn run(args: &[&str], input: Option<&str>) -> Result<Vec<u8>, String> {
        for program in PROGRAMS {
            let child = Command::new(program)
                .args(args)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn();
            let Ok(mut child) = child else { continue };
            if let (Some(mut stdin), Some(text)) = (child.stdin.take(), input) {
                stdin
                    .write_all(text.as_bytes())
                    .map_err(|e| e.to_string())?;
            }
            let output = child.wait_with_output().map_err(|e| e.to_string())?;
            if !output.status.success() {
                return Err(format!("{program} failed with {}", output.status));
            }
            return Ok(output.stdout);
        }
        Err("No speech engine found. Install espeak-ng to hear replies.".to_string())
    }

    pub fn voices() -> Result<Vec<Voice>, String> {
        run(&["--voices"], None).map(|out| parse_espeak_voices(&String::from_utf8_lossy(&out)))
    }

    pub fn synthesize(text: &str, voice_id: Option<&str>, rate: f32) -> Result<Vec<u8>, String> {
        let wpm = words_per_minute(rate).to_string();
        let mut args = vec!["--stdin", "--stdout", "-s", wpm.as_str()];
        if let Some(voice) = voice_id {
            args.extend(["-v", voice]);
        }
        run(&args, Some(text))
    }
}

#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
mod platform {
    use super::Voice;

    pub fn voices() -> Result<Vec<Voice>, String> {
        Ok(Vec::new())
    }

    pub fn synthesize(_: &str, _: Option<&str>, _: f32) -> Result<Vec<u8>, String> {
        Err("Text-to-speech is not supported on this platform".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn say_voice_list_is_parsed() {
        let voices = parse_say_voices(
            "Albert              en_US    # Hello! My name is Albert.\n\
             Amélie              fr_CA    # Bonjour, je m’appelle Amélie.\n\
             Eddy (English (UK)) en_GB    # Hello! My name is Eddy.\n",
        );
        assert_eq!(voices.len(), 3);
        assert_eq!(voices[0].id, "Albert");
        assert_eq!(voices[1].language, "fr-CA");
        assert_eq!(voices[2].name, "Eddy (English (UK))");
    }

    #[test]
    fn espeak_voice_list_is_parsed() {
        let voices = parse_espeak_voices(
            "Pty Language       Age/Gender VoiceName          File                 Other Languages\n \
             5  af              --/M      Afrikaans          gmw/af\n \
             2  en-gb           --/M      English_(Great_Britain) gmw/en\n",
        );
        assert_eq!(voices.len(), 2);
        assert_eq!(voices[1].id, "en-gb");
        assert_eq!(voices[1].name, "English (Great Britain)");
    }

    /// Uses the real OS voices: `cargo test system_voices -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn system_voices_synthesize_wav() {
        let voices = platform::voices().unwrap();
        for voice in &voices {
            println!("{} | {} | {}", voice.language, voice.name, voice.id);
        }
        let started = std::time::Instant::now();
        let wav = platform::synthesize("The capital of Ghana is Accra.", None, 1.0).unwrap();
        println!("synthesized {} bytes in {:?}", wav.len(), started.elapsed());
        assert_eq!(&wav[..4], b"RIFF");
        assert!(Decoder::new_wav(Cursor::new(wav)).is_ok());

        if let Some(voice) = voices.last() {
            let fast = platform::synthesize("Faster.", Some(&voice.id), 2.0).unwrap();
            assert_eq!(&fast[..4], b"RIFF");
        }
    }

    #[test]
    fn rate_maps_to_words_per_minute() {
        assert_eq!(words_per_minute(1.0), 175);
        assert_eq!(words_per_minute(2.0), 350);
        assert_eq!(words_per_minute(9.0), 350);
    }
}
