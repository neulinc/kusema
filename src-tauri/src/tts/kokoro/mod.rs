//! Kokoro-82M voices (Apache-2.0), run locally with ONNX Runtime.
//!
//! Text → phonemes (`g2p`, misaki's dictionaries plus rules) → token ids →
//! the ONNX model, with a per-voice style vector → 24 kHz audio. Files are
//! downloaded on demand (`assets`); until then Kokoro lists no voices and the
//! player falls back to the system voices.

pub mod assets;
mod g2p;
mod numbers;
mod rules;

use super::{Audio, Synthesizer, Voice};
use assets::{Downloads, MODEL_FILE, VOCAB_FILE, VOICES, VOICES_DIR};
use g2p::{Dialect, Lexicon};
use log::{debug, info, warn};
use ort::environment::Environment;
use ort::session::Session;
use ort::value::Tensor;
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};
use tauri::AppHandle;

/// Voice ids are `kokoro:<name>`, e.g. `kokoro:af_heart`.
pub const VOICE_PREFIX: &str = "kokoro:";
pub const ENGINE: &str = "Kokoro";

const SAMPLE_RATE: u32 = 24_000;
/// The model's context is 512 tokens, two of which are the padding.
const MAX_TOKENS: usize = 510;
/// Shortest clause synthesized on its own (≈1.5 s of speech): long enough to
/// cover synthesizing the next one.
const MIN_CHUNK: usize = 40;
const STYLE_DIM: usize = 256;
/// Free the model's memory after this long without speaking.
const UNLOAD_AFTER: Duration = Duration::from_secs(10 * 60);

/// The ONNX Runtime environment: one per process, and the library can only
/// be loaded once.
static ENVIRONMENT: OnceLock<Result<Environment, String>> = OnceLock::new();

fn environment(dir: &Path) -> Result<&'static Environment, String> {
    ENVIRONMENT
        .get_or_init(|| {
            let library = assets::runtime_library(dir).ok_or("Unsupported platform")?;
            ort::init_from(&library)
                .map_err(|e| format!("Could not load ONNX Runtime: {e}"))?
                .with_name("kusema-kokoro")
                .build()
                .map_err(|e| format!("Could not start ONNX Runtime: {e}"))
        })
        .as_ref()
        .map_err(Clone::clone)
}

struct Loaded {
    session: Session,
    vocab: HashMap<char, i64>,
    lexicons: HashMap<Dialect, Lexicon>,
    styles: HashMap<String, Arc<Vec<f32>>>,
    last_used: Instant,
}

pub struct Kokoro {
    app: AppHandle,
    dir: PathBuf,
    loaded: Mutex<Option<Loaded>>,
    downloads: Downloads,
}

impl Kokoro {
    pub fn new(app: &AppHandle) -> Self {
        let dir = crate::portable::app_data_dir(app)
            .map(|d| d.join("kokoro"))
            .unwrap_or_else(|_| std::env::temp_dir().join("kusema-kokoro"));
        Self {
            app: app.clone(),
            dir,
            loaded: Mutex::new(None),
            downloads: Downloads::default(),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Option<Loaded>> {
        self.loaded.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Load in the background so the next sentence starts sooner.
    pub fn preload(self: &Arc<Self>) {
        if !assets::is_ready(&self.dir) || self.lock().is_some() {
            return;
        }
        let kokoro = Arc::clone(self);
        std::thread::spawn(move || {
            let mut loaded = kokoro.lock();
            if let Err(e) = kokoro.ensure_loaded(&mut loaded, None) {
                warn!("Could not load Kokoro: {e}");
            }
        });
    }

    pub fn unload(&self) {
        if self.lock().take().is_some() {
            info!("Unloaded Kokoro");
        }
    }

    /// Called by the player while idle.
    pub fn unload_if_idle(&self) {
        // Don't wait behind a sentence being synthesized.
        let Ok(mut loaded) = self.loaded.try_lock() else {
            return;
        };
        if loaded
            .as_ref()
            .is_some_and(|l| l.last_used.elapsed() >= UNLOAD_AFTER)
        {
            *loaded = None;
            info!(
                "Unloaded Kokoro after {} idle minutes",
                UNLOAD_AFTER.as_secs() / 60
            );
        }
    }

    fn ensure_loaded<'a>(
        &self,
        loaded: &'a mut Option<Loaded>,
        dialect: Option<Dialect>,
    ) -> Result<&'a mut Loaded, String> {
        if loaded.is_none() {
            if !assets::is_ready(&self.dir) {
                return Err("Kokoro voices aren't downloaded".to_string());
            }
            let started = Instant::now();
            let session = Session::builder(environment(&self.dir)?)
                .and_then(|mut b| b.commit_from_file(self.dir.join(MODEL_FILE)))
                .map_err(|e| format!("Could not load the Kokoro model: {e}"))?;
            let vocab = load_vocab(&self.dir.join(VOCAB_FILE))?;
            info!("Loaded Kokoro in {:?}", started.elapsed());
            *loaded = Some(Loaded {
                session,
                vocab,
                lexicons: HashMap::new(),
                styles: HashMap::new(),
                last_used: Instant::now(),
            });
        }
        let state = loaded.as_mut().expect("loaded above");
        let dialect = dialect.unwrap_or_else(|| self.selected_dialect());
        if let Entry::Vacant(slot) = state.lexicons.entry(dialect) {
            let started = Instant::now();
            slot.insert(Lexicon::load(&self.dir, dialect)?);
            debug!("Loaded the {dialect:?} lexicon in {:?}", started.elapsed());
        }
        Ok(state)
    }

    fn selected_dialect(&self) -> Dialect {
        crate::settings::get_settings(&self.app)
            .assistant_voice
            .as_deref()
            .and_then(|v| v.strip_prefix(VOICE_PREFIX))
            .map(Dialect::for_voice)
            .unwrap_or(Dialect::Us)
    }

    fn style(&self, state: &mut Loaded, voice: &str) -> Result<Arc<Vec<f32>>, String> {
        if let Some(style) = state.styles.get(voice) {
            return Ok(Arc::clone(style));
        }
        let style = Arc::new(load_style(&self.dir, voice)?);
        state.styles.insert(voice.to_string(), Arc::clone(&style));
        Ok(style)
    }
}

/// A voice file: one 256-float style row per input length.
fn load_style(dir: &Path, voice: &str) -> Result<Vec<f32>, String> {
    if !VOICES.iter().any(|(name, _)| *name == voice) {
        return Err(format!("Unknown Kokoro voice '{voice}'"));
    }
    let path = dir.join(VOICES_DIR).join(format!("{voice}.bin"));
    let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let style: Vec<f32> = bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|b| f32::from_le_bytes(*b))
        .collect();
    if style.len() < STYLE_DIM {
        return Err(format!("{} is not a voice file", path.display()));
    }
    Ok(style)
}

/// The style row for an input of `tokens` phonemes.
fn style_row(style: &[f32], tokens: usize) -> Vec<f32> {
    let row = tokens.min(style.len() / STYLE_DIM - 1);
    style[row * STYLE_DIM..(row + 1) * STYLE_DIM].to_vec()
}

/// Token ids for a phoneme string, padded with 0 on both ends.
fn token_ids(phonemes: &str, vocab: &HashMap<char, i64>) -> Vec<i64> {
    let mut ids = Vec::with_capacity(phonemes.len() + 2);
    ids.push(0);
    ids.extend(phonemes.chars().filter_map(|c| vocab.get(&c).copied()));
    ids.push(0);
    ids
}

impl Synthesizer for Kokoro {
    fn voices(&self) -> Result<Vec<Voice>, String> {
        if !assets::is_ready(&self.dir) {
            return Ok(Vec::new());
        }
        Ok(VOICES.iter().map(|(name, _)| voice_info(name)).collect())
    }

    fn synthesize(
        &self,
        text: &str,
        voice_id: Option<&str>,
        rate: f32,
        out: &mut dyn FnMut(Audio) -> bool,
    ) -> Result<(), String> {
        let voice = voice_id.unwrap_or(assets::DEFAULT_VOICE);
        let dialect = Dialect::for_voice(voice);
        let mut guard = self.lock();
        let state = self.ensure_loaded(&mut guard, Some(dialect))?;
        state.last_used = Instant::now();
        let style = self.style(state, voice)?;
        let phonemes = g2p::phonemize(text, &state.lexicons[&dialect]);
        debug!("Kokoro phonemes: {phonemes}");

        for chunk in g2p::chunks(&phonemes, MIN_CHUNK, MAX_TOKENS) {
            let ids = token_ids(&chunk, &state.vocab);
            if ids.len() <= 2 {
                continue;
            }
            let row = style_row(&style, ids.len() - 2);
            let samples = run(&mut state.session, ids, row, rate)?;
            state.last_used = Instant::now();
            if !out(Audio::Mono {
                sample_rate: SAMPLE_RATE,
                samples,
            }) {
                break;
            }
        }
        Ok(())
    }
}

fn run(
    session: &mut Session,
    ids: Vec<i64>,
    style: Vec<f32>,
    rate: f32,
) -> Result<Vec<f32>, String> {
    let err = |e: ort::Error| format!("Kokoro failed: {e}");
    let len = ids.len();
    let inputs = ort::inputs![
        "input_ids" => Tensor::from_array(([1, len], ids)).map_err(err)?,
        "style" => Tensor::from_array(([1, STYLE_DIM], style)).map_err(err)?,
        "speed" => Tensor::from_array(([1], vec![rate.clamp(0.5, 2.0)])).map_err(err)?,
    ];
    let outputs = session.run(inputs).map_err(err)?;
    let (_, samples) = outputs[0].try_extract_tensor::<f32>().map_err(err)?;
    Ok(samples.to_vec())
}

/// The phoneme → token id table from the model's `tokenizer.json`.
fn load_vocab(path: &Path) -> Result<HashMap<char, i64>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let json: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let vocab = json["model"]["vocab"]
        .as_object()
        .ok_or("tokenizer.json has no vocabulary")?;
    Ok(vocab
        .iter()
        .filter_map(|(k, v)| {
            let mut chars = k.chars();
            match (chars.next(), chars.next(), v.as_i64()) {
                (Some(c), None, Some(id)) => Some((c, id)),
                _ => None,
            }
        })
        .collect())
}

/// `af_heart` → "Heart", American English.
fn voice_info(name: &str) -> Voice {
    let display = name.split_once('_').map_or(name, |(_, n)| n);
    let mut chars = display.chars();
    let display = chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect())
        .unwrap_or_default();
    Voice {
        id: format!("{VOICE_PREFIX}{name}"),
        name: display,
        language: match Dialect::for_voice(name) {
            Dialect::Us => "en-US",
            Dialect::Gb => "en-GB",
        }
        .to_string(),
        engine: ENGINE.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn voices_are_named_and_tagged() {
        let heart = voice_info("af_heart");
        assert_eq!(heart.id, "kokoro:af_heart");
        assert_eq!(heart.name, "Heart");
        assert_eq!(heart.language, "en-US");
        assert_eq!(voice_info("bm_george").language, "en-GB");
    }

    /// Runs the real model. Needs the downloaded files in a folder:
    /// `KUSEMA_KOKORO_DIR=... cargo test --release kokoro_speaks -- --ignored --nocapture`
    /// Writes `kokoro-test.wav` there.
    #[test]
    #[ignore]
    fn kokoro_speaks() {
        let dir = PathBuf::from(std::env::var("KUSEMA_KOKORO_DIR").expect("KUSEMA_KOKORO_DIR"));
        let started = Instant::now();
        let mut session = Session::builder(environment(&dir).unwrap())
            .and_then(|mut b| b.commit_from_file(dir.join(MODEL_FILE)))
            .unwrap();
        let vocab = load_vocab(&dir.join(VOCAB_FILE)).unwrap();
        println!("model loaded in {:?}", started.elapsed());
        let started = Instant::now();
        let us = Lexicon::load(&dir, Dialect::Us).unwrap();
        println!("US lexicon loaded in {:?}", started.elapsed());

        let mut all = Vec::new();
        for (voice, lexicon, text) in [
            ("af_heart", &us, "The capital of Ghana is Accra."),
            ("af_heart", &us, "It has about 2.5 million people, and it sits on the Atlantic coast, near the Gulf of Guinea."),
            ("af_heart", &us, "Hi, I'm Kusema. Meet me at 3:30 on March 21st, 2027; it costs $12.50."),
        ] {
            let style = load_style(&dir, voice).unwrap();
            let phonemes = g2p::phonemize(text, lexicon);
            println!("{text}\n  {phonemes}");
            // When each chunk is ready vs. when the previous one stops playing.
            let started = Instant::now();
            let mut playing_until = 0.0f32;
            for (i, chunk) in g2p::chunks(&phonemes, MIN_CHUNK, MAX_TOKENS)
                .iter()
                .enumerate()
            {
                let ids = token_ids(chunk, &vocab);
                let row = style_row(&style, ids.len() - 2);
                let chunk_started = Instant::now();
                let samples = run(&mut session, ids, row, 1.0).unwrap();
                let took = chunk_started.elapsed().as_secs_f32();
                let ready = started.elapsed().as_secs_f32();
                let secs = samples.len() as f32 / SAMPLE_RATE as f32;
                let gap = if i == 0 { 0.0 } else { (ready - playing_until).max(0.0) };
                if i == 0 {
                    println!("  time to first audio: {ready:.2}s");
                }
                println!(
                    "  chunk {i}: {took:.2}s for {secs:.2}s of audio (RTF {:.2}), gap {gap:.2}s",
                    took / secs
                );
                playing_until = playing_until.max(ready) + secs;
                assert!(secs > 0.3);
                all.extend(samples);
            }
        }
        if dir.join(VOICES_DIR).join("bm_george.bin").exists() {
            let gb = Lexicon::load(&dir, Dialect::Gb).unwrap();
            let style = load_style(&dir, "bm_george").unwrap();
            let phonemes = g2p::phonemize("Lovely weather we're having, isn't it?", &gb);
            println!("GB: {phonemes}");
            let ids = token_ids(&phonemes, &vocab);
            let row = style_row(&style, ids.len() - 2);
            all.extend(run(&mut session, ids, row, 1.0).unwrap());
        }

        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: SAMPLE_RATE,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut wav = hound::WavWriter::create(dir.join("kokoro-test.wav"), spec).unwrap();
        for s in all {
            wav.write_sample((s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)
                .unwrap();
        }
        wav.finalize().unwrap();
    }
}
