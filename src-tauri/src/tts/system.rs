//! Speech with the operating system's built-in voices.
//!
//! - Windows: WinRT `Windows.Media.SpeechSynthesis`, i.e. the OneCore voices,
//!   which sound more natural than the legacy SAPI desktop voices.
//! - macOS: the `say` command.
//! - Linux: `espeak-ng` (or `espeak`), if installed.
//!
//! Each sentence comes back as WAV for the shared player (`player.rs`).

use super::{Audio, Synthesizer, Voice};

/// How the voice picker labels these voices.
const ENGINE: &str = if cfg!(windows) {
    "Windows"
} else if cfg!(target_os = "macos") {
    "macOS"
} else {
    "System"
};

fn voice(id: String, name: String, language: String) -> Voice {
    Voice {
        id,
        name,
        language,
        engine: ENGINE.to_string(),
    }
}

pub struct SystemVoices;

impl Synthesizer for SystemVoices {
    fn voices(&self) -> Result<Vec<Voice>, String> {
        platform::voices()
    }

    fn synthesize(
        &self,
        text: &str,
        voice_id: Option<&str>,
        rate: f32,
        out: &mut dyn FnMut(Audio) -> bool,
    ) -> Result<(), String> {
        out(Audio::Wav(platform::synthesize(text, voice_id, rate)?));
        Ok(())
    }
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
            (!name.is_empty()).then(|| {
                voice(
                    name.to_string(),
                    name.to_string(),
                    language.replace('_', "-"),
                )
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
            Some(voice(
                language.to_string(),
                name.replace('_', " "),
                language.to_string(),
            ))
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
                        Ok(super::voice(
                            v.Id()?.to_string(),
                            v.DisplayName()?.to_string(),
                            v.Language()?.to_string(),
                        ))
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
        assert!(rodio::Decoder::new_wav(std::io::Cursor::new(wav)).is_ok());

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
