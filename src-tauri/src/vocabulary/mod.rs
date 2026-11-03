//! Kusema vocabulary: names, terms and the user's own way of speaking.
//!
//! The speech models Kusema recommends (Parakeet, Nemotron, Canary) can't be
//! biased toward words in transcribe-cpp 0.4: its `vocabulary` option only
//! works for Whisper, Qwen3-ASR, FunASR, Granite and MOSS. So vocabulary is
//! applied after transcription, for every model, and also handed to the
//! engine where the model takes it:
//!
//! 1. **Sounds like:** each spoken form ("habeus corpus", "Canela") becomes
//!    its written form, as whole words, ignoring case (`spoken`).
//! 2. **Written only:** Handy's fuzzy custom-word matching (edit distance and
//!    Soundex) for the user's entries without spoken forms, and for long,
//!    distinctive pack words.
//! 3. **Citations:** the legal pack's rules (`legal`).
//!
//! The limit: a word the model writes as something far from both its spoken
//! forms and its written form isn't caught. Corrections ("correct that")
//! teach exactly those cases (`learn`).

pub mod files;
pub mod learn;
mod legal;
mod spoken;

use log::info;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Emitter};

use crate::settings::{get_settings, write_settings, AppSettings};

/// One vocabulary entry.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Type)]
pub struct VocabEntry {
    /// How it's written: "Kanella", "habeas corpus".
    pub written: String,
    /// What the model writes when it's said, lowercase ("canela").
    #[serde(default)]
    pub sounds_like: Vec<String>,
    /// Learned from a correction, rather than added by the user.
    #[serde(default)]
    pub learned: bool,
}

impl VocabEntry {
    pub fn new(written: &str, sounds_like: Vec<String>, learned: bool) -> Self {
        let written = written.trim().to_string();
        let mut spoken: Vec<String> = Vec::new();
        for s in sounds_like {
            let s = s.trim().to_lowercase();
            if !s.is_empty() && s != written.to_lowercase() && !spoken.contains(&s) {
                spoken.push(s);
            }
        }
        Self {
            written,
            sounds_like: spoken,
            learned,
        }
    }
}

/// Which built-in packs are on. Kusema's own name is always on.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Type)]
pub struct VocabPacks {
    #[serde(default = "on")]
    pub legal: bool,
    #[serde(default)]
    pub medical: bool,
}

fn on() -> bool {
    true
}

impl Default for VocabPacks {
    fn default() -> Self {
        Self {
            legal: true,
            medical: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum Pack {
    Legal,
    Medical,
}

const KUSEMA_PACK: &str = include_str!("packs/kusema.tsv");
const LEGAL_PACK: &str = include_str!("packs/legal.tsv");
const MEDICAL_PACK: &str = include_str!("packs/medical.tsv");
/// Pack words matched loosely must be at least this long, so short ordinary
/// words never get pulled toward them.
const PACK_FUZZY_MIN_CHARS: usize = 7;
/// The most terms handed to a model that takes a vocabulary.
const MAX_ENGINE_TERMS: usize = 100;

/// A pack file: `written<TAB>spoken|spoken`, `#` comments.
fn parse_pack(text: &str) -> Vec<VocabEntry> {
    text.lines()
        .map(str::trim_end)
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
        .map(|line| match line.split_once('\t') {
            Some((written, spoken)) => VocabEntry::new(
                written,
                spoken.split('|').map(str::to_string).collect(),
                false,
            ),
            None => VocabEntry::new(line, Vec::new(), false),
        })
        .collect()
}

fn pack(which: Pack) -> Vec<VocabEntry> {
    parse_pack(match which {
        Pack::Legal => LEGAL_PACK,
        Pack::Medical => MEDICAL_PACK,
    })
}

/// Vocabulary in English only: the packs and rules are English words.
fn is_english(language: Option<&str>) -> bool {
    language.is_none_or(|l| l.to_lowercase().starts_with("en"))
}

/// The entries in use: the user's and learned ones, Kusema's name, and the
/// packs that are on (English only).
fn active(settings: &AppSettings, language: Option<&str>) -> (Vec<VocabEntry>, Vec<VocabEntry>) {
    let own: Vec<VocabEntry> = settings.vocabulary.clone();
    let mut packs = parse_pack(KUSEMA_PACK);
    if is_english(language) {
        if settings.vocabulary_packs.legal {
            packs.extend(pack(Pack::Legal));
        }
        if settings.vocabulary_packs.medical {
            packs.extend(pack(Pack::Medical));
        }
    }
    (own, packs)
}

/// Applies the vocabulary to transcribed text. `prompted`: the model was
/// already given the terms, so the loose matching is left out.
pub fn apply(text: &str, settings: &AppSettings, language: Option<&str>, prompted: bool) -> String {
    let (own, packs) = active(settings, language);
    // The user's own entries first: they override a pack's. A pack term in
    // lowercase also fixes its own casing ("pro SE" → "pro se").
    let pairs: Vec<(&str, &str)> = own
        .iter()
        .chain(packs.iter())
        .flat_map(|e| {
            e.sounds_like
                .iter()
                .map(move |s| (s.as_str(), e.written.as_str()))
        })
        .chain(
            packs
                .iter()
                .filter(|e| e.written.contains(' ') && !e.written.chars().any(char::is_uppercase))
                .map(|e| (e.written.as_str(), e.written.as_str())),
        )
        .collect();
    let mut text = spoken::Replacer::new(pairs).apply(text);

    if !prompted {
        let mut loose: Vec<String> = settings.custom_words.clone();
        loose.extend(own.iter().map(|e| e.written.clone()));
        loose.extend(
            packs
                .iter()
                .filter(|e| {
                    e.sounds_like.is_empty() && e.written.chars().count() >= PACK_FUZZY_MIN_CHARS
                })
                .map(|e| e.written.clone()),
        );
        if !loose.is_empty() {
            text = crate::audio_toolkit::apply_custom_words(
                &text,
                &loose,
                settings.word_correction_threshold,
            );
        }
    }

    if settings.vocabulary_packs.legal && is_english(language) {
        text = legal::apply(&text);
    }
    text
}

/// Terms for a model that takes a vocabulary, the user's first.
pub fn engine_terms(settings: &AppSettings) -> Vec<String> {
    let (own, packs) = active(settings, None);
    let mut terms: Vec<String> = settings.custom_words.clone();
    for e in own.iter().chain(packs.iter()) {
        if !terms.contains(&e.written) {
            terms.push(e.written.clone());
        }
    }
    terms.truncate(MAX_ENGINE_TERMS);
    terms
}

/// Handy's custom words become vocabulary entries, once.
pub fn migrate(settings: &mut AppSettings) -> bool {
    if settings.custom_words.is_empty() {
        return false;
    }
    for word in std::mem::take(&mut settings.custom_words) {
        add_entry(
            &mut settings.vocabulary,
            VocabEntry::new(&word, Vec::new(), false),
        );
    }
    info!("Moved custom words into the vocabulary");
    true
}

/// Adds `entry`, merging it into one with the same written form.
fn add_entry(list: &mut Vec<VocabEntry>, entry: VocabEntry) -> bool {
    if entry.written.is_empty() {
        return false;
    }
    if let Some(existing) = list
        .iter_mut()
        .find(|e| e.written.eq_ignore_ascii_case(&entry.written))
    {
        let mut changed = false;
        for s in entry.sounds_like {
            if !existing.sounds_like.contains(&s) {
                existing.sounds_like.push(s);
                changed = true;
            }
        }
        // Something the user added stays theirs.
        existing.learned &= entry.learned;
        return changed;
    }
    list.push(entry);
    true
}

/// Learns from a correction of `original` to `corrected`: adds what was
/// misheard, and returns it, to show the user.
pub fn learn_from(app: &AppHandle, original: &str, corrected: &str) -> Vec<learn::Correction> {
    let learned = learn::learn(original, corrected);
    if learned.is_empty() {
        return learned;
    }
    let mut settings = get_settings(app);
    for c in &learned {
        add_entry(
            &mut settings.vocabulary,
            VocabEntry::new(&c.written, vec![c.heard.clone()], true),
        );
    }
    info!("Learned {} word(s) from a correction", learned.len());
    write_settings(app, settings);
    changed(app);
    learned
}

fn changed(app: &AppHandle) {
    let _ = app.emit(
        "settings-changed",
        serde_json::json!({ "setting": "vocabulary" }),
    );
}

/// What the Vocabulary page shows.
#[derive(Serialize, Clone, Debug, Type)]
pub struct VocabularyView {
    pub entries: Vec<VocabEntry>,
    pub packs: VocabPacks,
    pub legal_count: u32,
    pub medical_count: u32,
}

#[tauri::command]
#[specta::specta]
pub fn get_vocabulary(app: AppHandle) -> VocabularyView {
    let settings = get_settings(&app);
    VocabularyView {
        entries: settings.vocabulary,
        packs: settings.vocabulary_packs,
        legal_count: pack(Pack::Legal).len() as u32,
        medical_count: pack(Pack::Medical).len() as u32,
    }
}

#[tauri::command]
#[specta::specta]
pub fn add_vocabulary_entry(
    app: AppHandle,
    written: String,
    sounds_like: Vec<String>,
) -> Result<(), String> {
    let entry = VocabEntry::new(&written, sounds_like, false);
    if entry.written.is_empty() {
        return Err("Type how the word is written.".to_string());
    }
    let mut settings = get_settings(&app);
    add_entry(&mut settings.vocabulary, entry);
    write_settings(&app, settings);
    changed(&app);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn remove_vocabulary_entry(app: AppHandle, written: String) -> Result<(), String> {
    let mut settings = get_settings(&app);
    settings.vocabulary.retain(|e| e.written != written);
    write_settings(&app, settings);
    changed(&app);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn set_vocabulary_pack(app: AppHandle, pack: Pack, enabled: bool) -> Result<(), String> {
    let mut settings = get_settings(&app);
    match pack {
        Pack::Legal => settings.vocabulary_packs.legal = enabled,
        Pack::Medical => settings.vocabulary_packs.medical = enabled,
    }
    write_settings(&app, settings);
    changed(&app);
    Ok(())
}

/// How an import went.
#[derive(Serialize, Clone, Debug, Type)]
pub struct ImportResult {
    pub added: u32,
    pub already_there: u32,
}

/// Imports a Kusema JSON or CSV file, or a Dragon word list.
#[tauri::command]
#[specta::specta]
pub fn import_vocabulary(app: AppHandle, path: String) -> Result<ImportResult, String> {
    let bytes = std::fs::read(&path).map_err(|e| format!("Couldn't open the file ({e})."))?;
    let entries = files::parse(&files::decode(&bytes)?)?;
    if entries.is_empty() {
        return Err("The file has no words in it.".to_string());
    }
    let mut settings = get_settings(&app);
    let mut result = ImportResult {
        added: 0,
        already_there: 0,
    };
    for entry in entries {
        let new = !settings
            .vocabulary
            .iter()
            .any(|e| e.written.eq_ignore_ascii_case(&entry.written));
        add_entry(&mut settings.vocabulary, entry);
        if new {
            result.added += 1;
        } else {
            result.already_there += 1;
        }
    }
    info!(
        "Imported vocabulary: {} added, {} already there",
        result.added, result.already_there
    );
    write_settings(&app, settings);
    changed(&app);
    Ok(result)
}

/// Writes the user's and learned entries (not the packs) to `path`.
#[tauri::command]
#[specta::specta]
pub fn export_vocabulary(
    app: AppHandle,
    path: String,
    format: files::ExportFormat,
) -> Result<u32, String> {
    let settings = get_settings(&app);
    std::fs::write(&path, files::write(&settings.vocabulary, format))
        .map_err(|e| format!("Couldn't save the file ({e})."))?;
    Ok(settings.vocabulary.len() as u32)
}

/// What was learned from a correction, for the History page to show.
#[derive(Serialize, Clone, Debug, Type)]
pub struct LearnedWord {
    pub heard: String,
    pub written: String,
}

/// Corrects a History entry's text (History only, not the document it was
/// typed into) and learns from the fix.
#[tauri::command]
#[specta::specta]
pub fn correct_history_entry(
    app: AppHandle,
    history: tauri::State<'_, std::sync::Arc<crate::managers::history::HistoryManager>>,
    id: i64,
    text: String,
) -> Result<Vec<LearnedWord>, String> {
    let original = history
        .entry_text(id)
        .map_err(|e| e.to_string())?
        .ok_or("That History entry is gone.")?;
    let corrected = text.trim().to_string();
    if corrected.is_empty() || corrected == original.trim() {
        return Ok(Vec::new());
    }
    history
        .set_text(id, &corrected)
        .map_err(|e| e.to_string())?;
    Ok(learn_from(&app, &original, &corrected)
        .into_iter()
        .map(|c| LearnedWord {
            heard: c.heard,
            written: c.written,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::get_default_settings;

    fn settings() -> AppSettings {
        get_default_settings()
    }

    #[test]
    fn the_packs_parse_and_have_no_duplicates() {
        for which in [Pack::Legal, Pack::Medical] {
            let entries = pack(which);
            assert!(entries.len() > 20, "{which:?}");
            let mut written: Vec<String> =
                entries.iter().map(|e| e.written.to_lowercase()).collect();
            written.sort();
            let before = written.len();
            written.dedup();
            assert_eq!(before, written.len(), "{which:?} has a written form twice");
            // No spoken form may be a written form of the same pack: that
            // would turn one right word into another.
            for e in &entries {
                for s in &e.sounds_like {
                    assert!(
                        !written.contains(s),
                        "{which:?}: {s} is also a written form"
                    );
                }
            }
        }
    }

    #[test]
    fn kusema_is_always_recognized() {
        let mut s = settings();
        s.vocabulary_packs = VocabPacks {
            legal: false,
            medical: false,
        };
        for heard in ["Kusama", "Cusema", "Kasema", "Cosima"] {
            assert_eq!(
                apply(
                    &format!("{heard}, make this more formal."),
                    &s,
                    Some("en"),
                    false
                ),
                "Kusema, make this more formal.",
                "{heard}"
            );
        }
        // In other languages too: it's a name.
        assert_eq!(
            apply("Kusama, hazlo formal", &s, Some("es"), false),
            "Kusema, hazlo formal"
        );
    }

    #[test]
    fn the_legal_pack_switch() {
        let text = "The writ of habeus corpus in Smith v Jones, 123 F 3d 456.";
        let mut s = settings();
        assert!(s.vocabulary_packs.legal, "on by default");
        assert_eq!(
            apply(text, &s, Some("en"), false),
            "The writ of habeas corpus in Smith v. Jones, 123 F.3d 456."
        );
        s.vocabulary_packs.legal = false;
        assert_eq!(apply(text, &s, Some("en"), false), text);
        // English words only: off in other languages.
        s.vocabulary_packs.legal = true;
        assert_eq!(apply(text, &s, Some("de"), false), text);
    }

    #[test]
    fn the_medical_pack_switch() {
        let text = "Started lysine april and met for min for dyspnea.";
        let mut s = settings();
        assert!(!s.vocabulary_packs.medical, "off by default");
        assert_eq!(apply(text, &s, Some("en"), false), text);
        s.vocabulary_packs.medical = true;
        assert_eq!(
            apply(text, &s, Some("en"), false),
            "Started lisinopril and metformin for dyspnea."
        );
    }

    #[test]
    fn the_users_own_and_learned_words() {
        let mut s = settings();
        s.vocabulary = vec![
            VocabEntry::new("Kanella", vec!["Canela".into()], true),
            VocabEntry::new("Brightwater", vec![], false),
        ];
        assert_eq!(
            apply("Canela met Brightwatter.", &s, Some("en"), false),
            "Kanella met Brightwater."
        );
        // With the terms already given to the model, no loose matching.
        assert_eq!(
            apply("Canela met Brightwatter.", &s, Some("en"), true),
            "Kanella met Brightwatter."
        );
        let terms = engine_terms(&s);
        assert_eq!(&terms[..2], ["Kanella", "Brightwater"]);
        assert!(terms.contains(&"Kusema".to_string()));
        assert!(terms.len() <= MAX_ENGINE_TERMS);
    }

    #[test]
    fn ordinary_sentences_pass_through_the_default_vocabulary() {
        let s = settings();
        for text in [
            "Tell us what happened on Tuesday.",
            "The parties may appeal within ten days.",
            "We all agreed to meet at the bus stop.",
            "Please prosecute the claim promptly.",
        ] {
            assert_eq!(apply(text, &s, Some("en"), false), text, "{text}");
        }
    }

    #[test]
    fn custom_words_move_into_the_vocabulary_once() {
        let mut s = settings();
        s.custom_words = vec!["Kanella".into(), "kanella".into(), "Acme".into()];
        assert!(migrate(&mut s));
        assert!(s.custom_words.is_empty());
        let written: Vec<&str> = s.vocabulary.iter().map(|e| e.written.as_str()).collect();
        assert_eq!(written, ["Kanella", "Acme"]);
        assert!(!migrate(&mut s));
    }

    #[test]
    fn adding_merges_spoken_forms_and_keeps_the_users_entry() {
        let mut list = vec![VocabEntry::new("Kanella", vec!["canela".into()], false)];
        add_entry(
            &mut list,
            VocabEntry::new("kanella", vec!["canella".into()], true),
        );
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].sounds_like, ["canela", "canella"]);
        assert!(!list[0].learned);
        assert!(!add_entry(&mut list, VocabEntry::new("  ", vec![], false)));
    }
}
