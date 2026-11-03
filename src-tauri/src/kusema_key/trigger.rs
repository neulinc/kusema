//! Spotting the trigger word at the start of what was said, as speech
//! recognition writes it: "Kusema", but also "Kosema", "Kosima", "Kuzma",
//! "Ku sema"... Only the first word (or the first two, run together, when the
//! first is short) is ever looked at, so the same words later in a sentence,
//! as the names they often are, are left alone.
//!
//! Loose matching compares how the word sounds, roughly: its consonants with
//! c/k/q and s/z merged ("ksm" for all of the above), the kind of vowel it
//! ends on, and its length. "Cosmetics", "Cosmos" and "Cause" have other
//! consonants; "Cosmo" ends on another vowel, so they stay dictation.

/// How the start of an utterance compares with the trigger word.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TriggerMatch {
    /// It starts with the trigger word; `rest` is what follows. `exact`: as
    /// one of its known spellings, not just sounding like it.
    Found {
        rest: String,
        exact: bool,
        heard: String,
    },
    /// It starts with something close, but not close enough to route: typed
    /// as dictation, with a hint about the reliable way.
    Near {
        heard: String,
    },
    None,
}

fn is_vowel(c: char) -> bool {
    matches!(c, 'a' | 'e' | 'i' | 'o' | 'u' | 'y')
}

/// Lowercase letters only ("Ku-sema," → "kusema").
fn letters(word: &str) -> String {
    word.chars()
        .filter(|c| c.is_alphabetic())
        .flat_map(char::to_lowercase)
        .collect()
}

/// The consonants, as they sound: c, k, q and ck are one sound, so are s and
/// z; x is "ks"; h is dropped; doubled sounds count once.
fn skeleton(word: &str) -> String {
    let word = letters(word).replace("ph", "f").replace("ck", "k");
    let mut out = String::new();
    for c in word.chars() {
        let mapped: &str = match c {
            'c' | 'k' | 'q' => "k",
            's' | 'z' => "s",
            'x' => "ks",
            'h' => "",
            c if is_vowel(c) => "",
            _ => {
                out.push(c);
                continue;
            }
        };
        out.push_str(mapped);
    }
    let mut collapsed = String::new();
    for c in out.chars() {
        if !collapsed.ends_with(c) {
            collapsed.push(c);
        }
    }
    collapsed
}

/// The kind of sound a word ends on: 'a' for a/e, 'o' for o/u, 'i' for i/y,
/// 'c' for a consonant.
fn ending(word: &str) -> char {
    match letters(word).chars().last() {
        Some('a' | 'e') => 'a',
        Some('o' | 'u') => 'o',
        Some('i' | 'y') => 'i',
        Some(_) => 'c',
        None => ' ',
    }
}

fn starts_with_vowel(word: &str) -> bool {
    letters(word).chars().next().is_some_and(is_vowel)
}

/// `heard` sounds like `form`.
fn sounds_like(heard: &str, form: &str) -> bool {
    let (h, f) = (letters(heard), letters(form));
    h.len() >= 3
        && skeleton(&h) == skeleton(&f)
        && !skeleton(&f).is_empty()
        && ending(&h) == ending(&f)
        && starts_with_vowel(&h) == starts_with_vowel(&f)
        && h.len().abs_diff(f.len()) <= 2
}

/// Close to `form`, but not enough to route.
fn nearly(heard: &str, form: &str) -> bool {
    let (h, f) = (letters(heard), letters(form));
    h.len() >= 3 && skeleton(&h) == skeleton(&f) && !skeleton(&f).is_empty()
}

/// The first two words of `text` and where each ends.
fn first_words(text: &str) -> Vec<(String, usize, usize)> {
    let mut words = Vec::new();
    let mut start = None;
    for (i, c) in text.char_indices() {
        let word_char = c.is_alphanumeric() || c == '\'' || c == '-';
        match (word_char, start) {
            (true, None) => start = Some(i),
            (false, Some(s)) => {
                words.push((text[s..i].to_string(), s, i));
                start = None;
                if words.len() == 2 {
                    return words;
                }
            }
            _ => {}
        }
    }
    if let Some(s) = start {
        words.push((text[s..].to_string(), s, text.len()));
    }
    words.truncate(2);
    words
}

fn rest_after(text: &str, end: usize) -> Option<String> {
    let rest = text[end..]
        .trim_start_matches(|c: char| {
            c.is_whitespace() || matches!(c, ',' | ':' | ';' | '.' | '!' | '-' | '\u{2014}')
        })
        .trim();
    rest.chars()
        .any(char::is_alphanumeric)
        .then(|| rest.to_string())
}

/// Looks for the trigger word at the start of `text`. `forms`: the word and
/// its known spellings (the vocabulary's, the user's added variants).
pub fn find(text: &str, forms: &[String]) -> TriggerMatch {
    // Known spellings, of any number of words ("hey kusema"), as said.
    for form in forms {
        if let Some(rest) = crate::voice_edit::commands::parse_trigger(text, form) {
            return TriggerMatch::Found {
                rest,
                exact: true,
                heard: form.clone(),
            };
        }
    }
    let words = first_words(text);
    let Some((w1, _, end1)) = words.first().cloned() else {
        return TriggerMatch::None;
    };
    // The first word, or the first two run together ("Ku sema") when the
    // first is short and only a space parts them.
    let mut candidates = vec![(w1.clone(), end1)];
    if let Some((w2, start2, end2)) = words.get(1) {
        let gap = &text[end1..*start2];
        if letters(&w1).len() <= 3 && gap.chars().all(char::is_whitespace) {
            candidates.push((format!("{w1}{w2}"), *end2));
        }
    }
    let wanted: Vec<String> = forms
        .iter()
        .map(|f| letters(f))
        .filter(|f| !f.is_empty())
        .collect();
    // Exact spellings first, the longer candidate first ("Ku sema" over "Ku").
    for exact in [true, false] {
        for (heard, end) in candidates.iter().rev() {
            let h = letters(heard);
            let hit = wanted
                .iter()
                .any(|f| if exact { h == *f } else { sounds_like(&h, f) });
            if hit {
                return match rest_after(text, *end) {
                    Some(rest) => TriggerMatch::Found {
                        rest,
                        exact,
                        heard: heard.clone(),
                    },
                    None => TriggerMatch::None,
                };
            }
        }
    }
    for (heard, _) in &candidates {
        if wanted.iter().any(|f| nearly(heard, f)) {
            return TriggerMatch::Near {
                heard: heard.clone(),
            };
        }
    }
    TriggerMatch::None
}

/// "Correct that" on an utterance whose first word was the trigger word
/// misheard: that spelling, to offer as a variant. `None` when the first
/// word wasn't changed to the trigger, or is already recognized.
pub fn misheard_first_word(
    original: &str,
    corrected: &str,
    forms: &[String],
    trigger: &str,
) -> Option<String> {
    let first = |text: &str| first_words(text).first().map(|(w, _, _)| w.clone());
    let (heard, fixed) = (first(original)?, first(corrected)?);
    if letters(&fixed) != letters(trigger) || letters(&heard) == letters(trigger) {
        return None;
    }
    let probe = format!("{heard}, x");
    if matches!(find(&probe, forms), TriggerMatch::Found { .. }) {
        return None;
    }
    Some(
        heard
            .trim_matches(|c: char| !c.is_alphanumeric())
            .to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn forms() -> Vec<String> {
        // As the app builds them for "Kusema": the word, the vocabulary's
        // spellings and the built-in mishearings.
        [
            "Kusema", "Cusema", "Kasema", "Kuzema", "Kusemma", "Ku sema", "Coosema", "Kuzma",
            "Kyzma",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect()
    }

    fn found(text: &str) -> Option<String> {
        match find(text, &forms()) {
            TriggerMatch::Found { rest, .. } => Some(rest),
            _ => None,
        }
    }

    #[test]
    fn the_trigger_is_heard_in_its_many_spellings() {
        for said in [
            "Kusema, what time is it?",
            "Kosema, what time is it?",
            "Kosima, what time is it?",
            "Kusama, what time is it?",
            "Cosima, what time is it?",
            "Kuzma what time is it?",
            "Kyzma, what time is it?",
            "Ku sema, what time is it?",
            "Ko sema what time is it?",
            "KOSEMA. What time is it?",
            "kusemma: what time is it",
        ] {
            let rest = found(said).map(|r| r.to_lowercase());
            assert_eq!(
                rest.as_deref().map(|r| r.trim_end_matches('?')),
                Some("what time is it"),
                "{said}"
            );
        }
    }

    #[test]
    fn ordinary_sentences_are_not_routed() {
        for said in [
            "Cosmetics are expensive.",
            "Cosmos is a great series.",
            "Cause I said so.",
            "Kismet brought us together.",
            "Customer service called back.",
            "Cinema tonight?",
            "Casa Blanca is closed.",
            "Kosovo declared independence.",
            "Coosemans will join the call.",
            "Kasem called this morning.",
            "Cosmo, what time is it?",
            "Ko, Sema said she'd call.",
            "Kusemas are great",
            "Kosema",
            "",
        ] {
            assert_eq!(found(said), None, "{said}");
        }
    }

    #[test]
    fn the_names_later_in_a_sentence_are_untouched() {
        for said in [
            "Thanks to Kuzma, the letter went out.",
            "I met Cosima, who runs the lab.",
            "The artist Yayoi Kusama painted dots.",
        ] {
            assert_eq!(find(said, &forms()), TriggerMatch::None, "{said}");
        }
    }

    #[test]
    fn exact_spellings_are_marked_so() {
        assert!(matches!(
            find("Kusema, hi", &forms()),
            TriggerMatch::Found { exact: true, .. }
        ));
        assert!(matches!(
            find("Kosima, hi", &forms()),
            TriggerMatch::Found { exact: false, .. }
        ));
    }

    #[test]
    fn near_misses_are_noted_for_the_hint() {
        for said in [
            "Cosmo, what time is it?",
            "Kasem, what time is it?",
            "Kusemi what is this",
        ] {
            assert!(
                matches!(find(said, &forms()), TriggerMatch::Near { .. }),
                "{said}"
            );
        }
        for said in [
            "Cause I said so.",
            "Cosmetics are expensive.",
            "Hello there",
        ] {
            assert_eq!(find(said, &forms()), TriggerMatch::None, "{said}");
        }
    }

    #[test]
    fn another_trigger_word_matches_by_the_same_rules() {
        let forms = vec!["Computer".to_string()];
        assert!(matches!(
            find("Computer, hi", &forms),
            TriggerMatch::Found { exact: true, .. }
        ));
        assert!(matches!(
            find("Kompyuter, hi", &forms),
            TriggerMatch::Found { exact: false, .. }
        ));
        assert_eq!(find("Commuter trains are late", &forms), TriggerMatch::None);
    }

    #[test]
    fn a_corrected_first_word_is_offered_as_a_variant() {
        let f = forms();
        assert_eq!(
            misheard_first_word("Koozeymah, what's on?", "Kusema, what's on?", &f, "Kusema")
                .as_deref(),
            Some("Koozeymah")
        );
        // Already recognized: nothing to add.
        assert_eq!(
            misheard_first_word("Kosima, what's on?", "Kusema, what's on?", &f, "Kusema"),
            None
        );
        // The first word wasn't changed to the trigger.
        assert_eq!(
            misheard_first_word("Hello there", "Hullo there", &f, "Kusema"),
            None
        );
        assert_eq!(
            misheard_first_word("Kusema, hi", "Kusema, hello", &f, "Kusema"),
            None
        );
    }
}
