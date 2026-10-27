//! English text → Kokoro phonemes.
//!
//! A small Rust take on misaki, Kokoro's own phonemizer: words are looked up
//! in misaki's "gold" then "silver" dictionaries (which use Kokoro's phoneme
//! set), with inflections (-s, -ed, -ing, …), compounds and acronyms derived
//! from dictionary words. Anything left falls back to letter-to-sound rules
//! (`rules.rs`). No espeak-ng, so nothing GPL.

use super::numbers;
use super::rules;
use regex::Regex;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;
use std::sync::LazyLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Dialect {
    Us,
    Gb,
}

impl Dialect {
    /// Kokoro voice names start with the language: `a` American, `b` British.
    pub fn for_voice(voice: &str) -> Self {
        if voice.starts_with('b') {
            Dialect::Gb
        } else {
            Dialect::Us
        }
    }

    fn prefix(self) -> &'static str {
        match self {
            Dialect::Us => "us",
            Dialect::Gb => "gb",
        }
    }
}

/// Words that matter to Kusema but aren't in misaki's dictionaries.
const EXTRA_WORDS: &[(&str, &str)] = &[("Kusema", "kusˈɛmə"), ("Neulinc", "nˈulɪŋk")];

/// A dictionary value: one pronunciation, or one per part of speech.
#[derive(Deserialize)]
#[serde(untagged)]
enum Entry {
    One(String),
    Tagged(HashMap<String, Option<String>>),
}

impl Entry {
    fn into_default(self) -> Option<String> {
        match self {
            Entry::One(p) => Some(p),
            // Without a tagger, the usual reading ("read" as in "I read books").
            Entry::Tagged(mut tags) => tags.remove("DEFAULT").flatten(),
        }
    }
}

pub struct Lexicon {
    words: HashMap<String, String>,
    dialect: Dialect,
}

impl Lexicon {
    pub fn load(dir: &Path, dialect: Dialect) -> Result<Self, String> {
        let mut words = HashMap::new();
        // Silver first so gold overrides it.
        for tier in ["silver", "gold"] {
            let path = dir.join(format!("{}_{tier}.json", dialect.prefix()));
            let text =
                std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            let entries: HashMap<String, Entry> =
                serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
            words.extend(
                entries
                    .into_iter()
                    .filter_map(|(word, entry)| Some((word, entry.into_default()?))),
            );
        }
        Ok(Self::from_words(words, dialect))
    }

    fn from_words(mut words: HashMap<String, String>, dialect: Dialect) -> Self {
        for (word, phonemes) in EXTRA_WORDS {
            words.insert(word.to_string(), phonemes.to_string());
        }
        Self { words, dialect }
    }

    fn get(&self, word: &str) -> Option<String> {
        if let Some(p) = self.words.get(word) {
            return Some(p.clone());
        }
        let lower = word.to_lowercase();
        if lower != word {
            if let Some(p) = self.words.get(&lower) {
                return Some(p.clone());
            }
        }
        // "ghana" → "Ghana"
        let mut chars = lower.chars();
        let title: String = chars
            .next()
            .map(|c| c.to_uppercase().chain(chars).collect())?;
        self.words.get(&title).cloned()
    }

    /// Pronounce one word (letters, apostrophes, hyphens, periods).
    fn word(&self, word: &str) -> String {
        if let Some(p) = self.get(word).or_else(|| self.derived(word, 2)) {
            return p;
        }
        if word.chars().all(|c| c.is_ascii_digit()) {
            let words: Vec<String> = numbers::expand(word)
                .split_whitespace()
                .map(|w| self.word(w))
                .collect();
            return words.join(" ");
        }
        if word.contains(['-', '.']) {
            let parts: Vec<String> = word
                .split(['-', '.'])
                .filter(|p| !p.is_empty())
                .map(|p| self.word(p))
                .collect();
            return parts.join(" ");
        }
        if is_acronym(word) {
            return self.spell(word);
        }
        rules::pronounce(word)
    }

    /// Inflections, contractions and compounds of dictionary words.
    fn derived(&self, word: &str, depth: u8) -> Option<String> {
        if depth == 0 || word.chars().count() < 3 {
            return None;
        }
        let lookup = |w: &str| self.get(w).or_else(|| self.derived(w, depth - 1));
        let lower = word.to_lowercase();

        // Possessives and contractions.
        for (suffix, sound) in [
            ("n't", "nt"),
            ("'ll", "l"),
            ("'re", "ɹ"),
            ("'ve", "v"),
            ("'d", "d"),
            ("'m", "m"),
        ] {
            if let Some(stem) = strip(word, &lower, suffix) {
                return lookup(stem).map(|p| p + sound);
            }
        }
        if let Some(stem) = strip(word, &lower, "'s").or_else(|| strip(word, &lower, "s'")) {
            return lookup(stem).map(|p| add_s(&p));
        }

        if let Some(stem) = strip(word, &lower, "ing") {
            for candidate in stems(stem, true) {
                if let Some(p) = lookup(&candidate) {
                    return Some(p + "ɪŋ");
                }
            }
        }
        if let Some(stem) = strip(word, &lower, "ed") {
            for candidate in stems(stem, true) {
                if let Some(p) = lookup(&candidate) {
                    return Some(add_ed(&p, self.dialect));
                }
            }
        }
        if let Some(stem) = strip(word, &lower, "ies") {
            if let Some(p) = lookup(&format!("{stem}y")) {
                return Some(add_s(&p));
            }
        }
        if let Some(stem) = strip(word, &lower, "es") {
            if let Some(p) = lookup(stem) {
                return Some(add_s(&p));
            }
        }
        if let Some(stem) = strip(word, &lower, "s") {
            if !stem.ends_with('s') {
                if let Some(p) = lookup(stem) {
                    return Some(add_s(&p));
                }
            }
        }
        let r = if self.dialect == Dialect::Us {
            "ɹ"
        } else {
            ""
        };
        for (suffix, sound) in [
            ("ly", "li".to_string()),
            ("ness", "nəs".to_string()),
            ("less", "ləs".to_string()),
            ("ful", "fəl".to_string()),
            ("ment", "mənt".to_string()),
            ("er", format!("ə{r}")),
            ("est", "ɪst".to_string()),
        ] {
            if let Some(stem) = strip(word, &lower, suffix) {
                for candidate in stems(stem, suffix.starts_with('e')) {
                    if let Some(p) = lookup(&candidate) {
                        return Some(p + &sound);
                    }
                }
            }
        }

        // Compounds: "smartwatch" = "smart" + "watch".
        let chars: Vec<char> = lower.chars().collect();
        if chars.len() >= 6 && chars.iter().all(|c| c.is_alphabetic()) {
            for split in (3..=chars.len() - 3).rev() {
                let (a, b): (String, String) = (
                    chars[..split].iter().collect(),
                    chars[split..].iter().collect(),
                );
                if let (Some(pa), Some(pb)) = (self.get(&a), self.get(&b)) {
                    return Some(pa + &demote(&pb));
                }
            }
        }
        None
    }

    /// Letter by letter, stressing the last: "USA" → "jˌuˌɛsˈA".
    fn spell(&self, word: &str) -> String {
        let letters: Vec<String> = word
            .chars()
            .filter(|c| c.is_alphanumeric())
            .map(|c| {
                if c.is_ascii_digit() {
                    numbers::digit_word(c).map_or_else(String::new, |w| self.word(w))
                } else {
                    self.get(&c.to_uppercase().to_string()).unwrap_or_default()
                }
            })
            .collect();
        let last = letters.len().saturating_sub(1);
        letters
            .iter()
            .enumerate()
            .map(|(i, p)| if i < last { demote(p) } else { p.clone() })
            .collect()
    }
}

/// `word` without `suffix` (matched case-insensitively), if something's left.
fn strip<'a>(word: &'a str, lower: &str, suffix: &str) -> Option<&'a str> {
    if lower.len() > suffix.len() + 1 && lower.ends_with(suffix) {
        word.get(..word.len() - suffix.len())
    } else {
        None
    }
}

/// Spellings a stem may have had before a suffix: "mak" → "make",
/// "runn" → "run".
fn stems(stem: &str, silent_e: bool) -> Vec<String> {
    let mut out = vec![stem.to_string()];
    if silent_e {
        out.push(format!("{stem}e"));
    }
    let chars: Vec<char> = stem.chars().collect();
    if chars.len() >= 3 && chars[chars.len() - 1] == chars[chars.len() - 2] {
        out.push(chars[..chars.len() - 1].iter().collect());
    }
    if let Some(base) = stem.strip_suffix('i') {
        out.push(format!("{base}y"));
    }
    out
}

const VOWELS: &str = "AIOWYaeiouæɑɒɔəɛɜɪʊʌᵻᵊɐ";

fn last_sound(p: &str) -> Option<char> {
    p.chars().rev().find(|c| !matches!(c, 'ˈ' | 'ˌ'))
}

/// Plural/possessive/3rd person: "cats" s, "dogs" z, "buses" ɪz.
fn add_s(p: &str) -> String {
    match last_sound(p) {
        Some('s' | 'z' | 'ʃ' | 'ʒ' | 'ʧ' | 'ʤ') => format!("{p}ɪz"),
        Some('p' | 't' | 'k' | 'f' | 'θ') => format!("{p}s"),
        _ => format!("{p}z"),
    }
}

/// Past tense: "walked" t, "played" d, "wanted" ɪd.
fn add_ed(p: &str, dialect: Dialect) -> String {
    match last_sound(p) {
        Some('t' | 'd') => format!(
            "{p}{}",
            if dialect == Dialect::Us {
                "ᵻd"
            } else {
                "ɪd"
            }
        ),
        Some('p' | 'k' | 'f' | 'θ' | 's' | 'ʃ' | 'ʧ') => format!("{p}t"),
        _ => format!("{p}d"),
    }
}

/// Primary stress → secondary, for the non-final parts of a compound.
fn demote(p: &str) -> String {
    p.replace('ˈ', "ˌ")
}

fn is_acronym(word: &str) -> bool {
    let letters = word.chars().filter(|c| c.is_alphabetic()).count();
    (2..=6).contains(&letters)
        && word
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
}

fn starts_with_vowel(p: &str) -> bool {
    p.chars()
        .find(|c| !matches!(c, 'ˈ' | 'ˌ'))
        .is_some_and(|c| VOWELS.contains(c))
}

#[derive(Debug, PartialEq)]
enum Token {
    Word(String),
    /// Kokoro punctuation, already in its own symbols.
    Punct(char),
}

static TOKEN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?x)
        (?P<time>\b\d{1,2}:\d{2}\b)
      | (?P<num>[$£€]?\d[\d,]*(?:\.\d+)?(?:st|nd|rd|th|s|%)?)
      | (?P<word>[\p{L}](?:[\p{L}\d'.\-]*[\p{L}\d])?\.?)
      | (?P<punct>[.,!?;:—…()])
      | (?P<sym>[&@+=%/])",
    )
    .expect("valid token regex")
});

/// Undo typography and Markdown the model has no sounds for.
fn normalize(text: &str) -> String {
    text.replace(['’', '‘', 'ʼ'], "'")
        .replace("...", "…")
        .replace(" - ", " — ")
        .replace(['–', '―'], "—")
        .replace(
            ['*', '_', '#', '`', '~', '|', '>', '[', ']', '"', '“', '”'],
            " ",
        )
}

fn tokenize(text: &str, lexicon: &Lexicon) -> Vec<Token> {
    let text = normalize(text);
    let mut tokens = Vec::new();
    let words = |s: &str, tokens: &mut Vec<Token>| {
        tokens.extend(s.split_whitespace().map(|w| Token::Word(w.to_string())));
    };
    for caps in TOKEN.captures_iter(&text) {
        if let Some(m) = caps.name("time") {
            words(&numbers::time(m.as_str()), &mut tokens);
        } else if let Some(m) = caps.name("num") {
            words(&numbers::expand(m.as_str()), &mut tokens);
        } else if let Some(m) = caps.name("word") {
            let w = m.as_str();
            // Keep the period of "Mr." or "e.g." only when the dictionary
            // knows it; otherwise it ends the sentence.
            match w.strip_suffix('.') {
                Some(bare) if lexicon.get(w).is_none() => {
                    tokens.push(Token::Word(bare.to_string()));
                    tokens.push(Token::Punct('.'));
                }
                _ => tokens.push(Token::Word(w.to_string())),
            }
        } else if let Some(m) = caps.name("punct") {
            let c = m.as_str().chars().next().unwrap_or('.');
            tokens.push(Token::Punct(c));
        } else if let Some(m) = caps.name("sym") {
            let word = match m.as_str() {
                "&" => "and",
                "@" => "at",
                "+" => "plus",
                "=" => "equals",
                "%" => "percent",
                _ => continue, // "/" as in "and/or": a short pause is enough
            };
            tokens.push(Token::Word(word.to_string()));
        }
    }
    tokens
}

/// Phonemes for a sentence, words separated by spaces, punctuation kept.
pub fn phonemize(text: &str, lexicon: &Lexicon) -> String {
    let tokens = tokenize(text, lexicon);
    let pronounced: Vec<Option<String>> = tokens
        .iter()
        .map(|t| match t {
            Token::Word(w) => Some(lexicon.word(w)),
            Token::Punct(_) => None,
        })
        .collect();

    let mut out = String::new();
    for (i, token) in tokens.iter().enumerate() {
        match token {
            Token::Punct(c) => {
                let opening = *c == '(';
                if opening && !out.is_empty() && !out.ends_with(' ') {
                    out.push(' ');
                }
                if !opening {
                    // "word ," → "word,"
                    while out.ends_with(' ') {
                        out.pop();
                    }
                }
                out.push(*c);
                if !opening {
                    out.push(' ');
                }
            }
            Token::Word(w) => {
                let next = pronounced[i + 1..].iter().flatten().next();
                let next_vowel = next.is_some_and(|p| starts_with_vowel(p));
                let p = match w.as_str() {
                    "the" | "The" => if next_vowel { "ði" } else { "ðə" }.to_string(),
                    // The article, not the letter (misaki does the same).
                    "a" => "ɐ".to_string(),
                    "A" if next.is_some()
                        && !matches!(tokens.get(i + 1), Some(Token::Punct(_))) =>
                    {
                        "ɐ".to_string()
                    }
                    "to" | "To" if next.is_some() => {
                        if next_vowel { "tʊ" } else { "tə" }.to_string()
                    }
                    _ => pronounced[i].clone().unwrap_or_default(),
                };
                if p.is_empty() {
                    continue;
                }
                if !out.is_empty() && !out.ends_with([' ', '(']) {
                    out.push(' ');
                }
                out.push_str(&p);
            }
        }
    }
    out.trim_end().to_string()
}

/// Split phonemes into pieces to synthesize one after another: at clause
/// punctuation, so the first words play while the rest is synthesized (which
/// runs ~3× faster than playback), merging clauses shorter than `min`
/// characters into the next. No piece is longer than `max`.
pub fn chunks(phonemes: &str, min: usize, max: usize) -> Vec<String> {
    let mut pieces: Vec<String> = Vec::new();
    let mut current = String::new();
    for word in phonemes.split(' ') {
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
        let ends_clause = word.ends_with([',', ';', ':', '—', '…', '.', '!', '?']);
        if ends_clause && current.chars().count() >= min {
            pieces.push(std::mem::take(&mut current));
        }
    }
    if !current.trim().is_empty() {
        pieces.push(current);
    }
    pieces.iter().flat_map(|p| split_long(p, max)).collect()
}

/// Split at most `max` characters at a time, preferring punctuation, then
/// spaces.
fn split_long(phonemes: &str, max: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest: Vec<char> = phonemes.chars().collect();
    while rest.len() > max {
        let window = &rest[..max];
        let cut = window
            .iter()
            .rposition(|c| matches!(c, '.' | '!' | '?' | ';' | ':' | ',' | '—' | '…'))
            .map(|i| i + 1)
            .filter(|&i| i > max / 3)
            .or_else(|| window.iter().rposition(|c| *c == ' ').filter(|&i| i > 0))
            .unwrap_or(max);
        let piece: String = rest[..cut].iter().collect();
        out.push(piece.trim().to_string());
        rest.drain(..cut);
    }
    let last: String = rest.iter().collect();
    if !last.trim().is_empty() {
        out.push(last.trim().to_string());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lexicon() -> Lexicon {
        let words = [
            ("hello", "həlˈO"),
            ("world", "wˈɜɹld"),
            ("the", "ði"),
            ("a", "ˈA"),
            ("A", "ˈA"),
            ("U", "jˈu"),
            ("S", "ˈɛs"),
            ("capital", "kˈæpəɾᵊl"),
            ("of", "ʌv"),
            ("Ghana", "ɡˈɑnə"),
            ("is", "ɪz"),
            ("Accra", "əkɹˈɑ"),
            ("cat", "kˈæt"),
            ("dog", "dˈɔɡ"),
            ("bus", "bˈʌs"),
            ("walk", "wˈɔk"),
            ("want", "wˈɑnt"),
            ("make", "mˈAk"),
            ("run", "ɹˈʌn"),
            ("smart", "smˈɑɹt"),
            ("watch", "wˈɑʧ"),
            ("Mr.", "mˈɪstəɹ"),
            ("Smith", "smˈɪθ"),
            ("apple", "ˈæpᵊl"),
            ("twenty", "twˈɛnti"),
            ("four", "fˈɔɹ"),
            ("percent", "pəɹsˈɛnt"),
            ("to", "tu"),
            ("go", "ɡˈO"),
        ];
        Lexicon::from_words(
            words
                .iter()
                .map(|(w, p)| (w.to_string(), p.to_string()))
                .collect(),
            Dialect::Us,
        )
    }

    #[test]
    fn sentence_with_punctuation() {
        let lex = lexicon();
        assert_eq!(
            phonemize("The capital of Ghana is Accra.", &lex),
            "ðə kˈæpəɾᵊl ʌv ɡˈɑnə ɪz əkɹˈɑ."
        );
        assert_eq!(phonemize("Hello, world!", &lex), "həlˈO, wˈɜɹld!");
    }

    #[test]
    fn articles_depend_on_the_next_word() {
        let lex = lexicon();
        assert_eq!(phonemize("the apple", &lex), "ði ˈæpᵊl");
        assert_eq!(phonemize("a cat", &lex), "ɐ kˈæt");
        assert_eq!(phonemize("to go", &lex), "tə ɡˈO");
    }

    #[test]
    fn inflections_come_from_the_stem() {
        let lex = lexicon();
        assert_eq!(lex.word("cats"), "kˈæts");
        assert_eq!(lex.word("dogs"), "dˈɔɡz");
        assert_eq!(lex.word("buses"), "bˈʌsɪz");
        assert_eq!(lex.word("walked"), "wˈɔkt");
        assert_eq!(lex.word("wanted"), "wˈɑntᵻd");
        assert_eq!(lex.word("making"), "mˈAkɪŋ");
        assert_eq!(lex.word("running"), "ɹˈʌnɪŋ");
        assert_eq!(lex.word("cat's"), "kˈæts");
        assert_eq!(lex.word("smartwatch"), "smˈɑɹtwˌɑʧ");
    }

    #[test]
    fn case_numbers_acronyms_and_abbreviations() {
        let lex = lexicon();
        assert_eq!(lex.word("GHANA"), "ɡˈɑnə");
        assert_eq!(lex.word("US"), "jˌuˈɛs");
        assert_eq!(phonemize("Mr. Smith", &lex), "mˈɪstəɹ smˈɪθ");
        assert_eq!(phonemize("24%", &lex), "twˈɛnti fˈɔɹ pəɹsˈɛnt");
        assert_eq!(lex.word("Kusema"), "kusˈɛmə");
    }

    #[test]
    fn markdown_and_quotes_are_cleaned_up() {
        let lex = lexicon();
        assert_eq!(phonemize("**Hello** world", &lex), "həlˈO wˈɜɹld");
        assert_eq!(phonemize("“Hello” (world)", &lex), "həlˈO (wˈɜɹld)");
    }

    #[test]
    fn unknown_words_are_still_pronounced() {
        let lex = lexicon();
        assert!(!lex.word("Zorblax").is_empty());
    }

    #[test]
    fn clauses_become_chunks() {
        let text = "ab cd, ef gh. ij kl";
        assert_eq!(chunks(text, 100, 500), vec![text.to_string()]);
        assert_eq!(chunks(text, 5, 500), vec!["ab cd,", "ef gh.", "ij kl"]);
        // "ab," is too short to stand alone.
        assert_eq!(chunks("ab, cd ef, gh", 5, 500), vec!["ab, cd ef,", "gh"]);
    }

    #[test]
    fn long_phonemes_split_at_punctuation() {
        let text = "ab cd, ef gh. ij kl";
        assert_eq!(split_long(text, 10), vec!["ab cd,", "ef gh.", "ij kl"]);
        let pieces = split_long(&"x".repeat(25), 10);
        assert_eq!(pieces.iter().map(String::len).sum::<usize>(), 25);
        assert!(pieces.iter().all(|p| p.chars().count() <= 10));
    }
}
