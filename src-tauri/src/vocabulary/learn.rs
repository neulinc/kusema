//! Learning from a correction: the words the user changed become personal
//! vocabulary ("Canela" heard, "Kanella" meant). Only small, word-level
//! fixes that sound alike are learned; rewording, added sentences and
//! changes to unrelated text aren't misrecognitions, so nothing is learned
//! from them. Everything here is local, in Privileged mode too.

/// One thing learned: what was heard, and how it's written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Correction {
    pub heard: String,
    pub written: String,
}

/// At most this many words on either side of one change.
const MAX_WORDS: usize = 4;
/// At most this many separate changes in one correction.
const MAX_CHANGES: usize = 3;
/// More than this share of the words changed is a rewrite, not a fix.
const MAX_CHANGED_SHARE: f64 = 0.5;
/// How different the letters may be (edit distance over the longer length)
/// for a change still to count as a mishearing.
const MAX_DISTANCE: f64 = 0.45;
/// A heard word this short ("all") is learned only with the word before it
/// ("et all" → "et al."), never alone.
const SHORT_LETTERS: usize = 4;

/// A word as compared: without the punctuation around it. Case counts, so a
/// capital added to a name is a change.
fn bare(word: &str) -> String {
    word.trim_matches(|c: char| !c.is_alphanumeric())
        .to_string()
}

fn letters(words: &[&str]) -> String {
    words
        .iter()
        .flat_map(|w| w.chars())
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// The written form of `words`: as typed, without the sentence punctuation
/// around the change ("Kanella," → "Kanella"), keeping inner periods ("et al.").
fn written(words: &[&str]) -> String {
    let joined = words.join(" ");
    let trimmed = joined
        .trim_start_matches(|c: char| !c.is_alphanumeric() && c != '§')
        .trim_end_matches([',', ';', ':', '!', '?', '"', '\u{201D}', ')']);
    // A sentence's period goes; an abbreviation's stays ("et al.", "Inc.",
    // "U.S."): a short last word, or one with dots inside.
    match trimmed.strip_suffix('.') {
        Some(rest) => {
            let last = rest.rsplit(' ').next().unwrap_or(rest);
            if last.contains('.') || last.chars().filter(|c| c.is_alphanumeric()).count() <= 3 {
                trimmed.to_string()
            } else {
                rest.to_string()
            }
        }
        None => trimmed.to_string(),
    }
}

/// Soundex digits without the first letter: "Canila" and "Kanella" are
/// both 540, so a mishearing that changes the first sound still matches.
fn sound_digits(letters: &str) -> String {
    let code = |c: char| match c {
        'b' | 'f' | 'p' | 'v' => '1',
        'c' | 'g' | 'j' | 'k' | 'q' | 's' | 'x' | 'z' => '2',
        'd' | 't' => '3',
        'l' => '4',
        'm' | 'n' => '5',
        'r' => '6',
        _ => '0',
    };
    let letters: Vec<char> = letters
        .chars()
        .filter(|c| c.is_ascii_alphabetic())
        .collect();
    let mut out = String::new();
    let mut last = letters.first().map(|&c| code(c)).unwrap_or('0');
    for &c in letters.iter().skip(1) {
        let d = code(c);
        if d != '0' && d != last {
            out.push(d);
        }
        if c != 'h' && c != 'w' {
            last = d;
        }
    }
    out.truncate(3);
    out
}

/// One word only inflected into the other ("move" → "moved", "claim" →
/// "claims"): grammar, not a mishearing.
fn inflected(a: &str, b: &str) -> bool {
    let (short, long) = if a.len() <= b.len() { (a, b) } else { (b, a) };
    const ENDINGS: &[&str] = &["s", "es", "d", "ed", "ing", "'s", "ly", "er", "ers"];
    long.strip_prefix(short)
        .is_some_and(|rest| ENDINGS.contains(&rest))
        || short
            .strip_suffix('e')
            .and_then(|stem| long.strip_prefix(stem))
            .is_some_and(|rest| ENDINGS.contains(&rest))
}

/// The aligned changes between two word lists: (old range, new range) of
/// each run of words that differ, from a longest-common-subsequence match.
fn changes(
    old: &[String],
    new: &[String],
) -> Vec<(std::ops::Range<usize>, std::ops::Range<usize>)> {
    let (n, m) = (old.len(), new.len());
    let mut lcs = vec![vec![0usize; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lcs[i][j] = if old[i] == new[j] {
                lcs[i + 1][j + 1] + 1
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }
    let (mut i, mut j) = (0, 0);
    let mut out = Vec::new();
    let mut open: Option<(usize, usize)> = None;
    while i < n || j < m {
        if i < n && j < m && old[i] == new[j] {
            if let Some((oi, nj)) = open.take() {
                out.push((oi..i, nj..j));
            }
            i += 1;
            j += 1;
        } else {
            open.get_or_insert((i, j));
            if j < m && (i == n || lcs[i][j + 1] >= lcs[i + 1][j]) {
                j += 1;
            } else {
                i += 1;
            }
        }
    }
    if let Some((oi, nj)) = open {
        out.push((oi..n, nj..m));
    }
    out
}

/// What to learn from `original` (as dictated) corrected to `corrected`.
pub fn learn(original: &str, corrected: &str) -> Vec<Correction> {
    let old_words: Vec<&str> = original.split_whitespace().collect();
    let new_words: Vec<&str> = corrected.split_whitespace().collect();
    let old: Vec<String> = old_words.iter().map(|w| bare(w)).collect();
    let new: Vec<String> = new_words.iter().map(|w| bare(w)).collect();
    if old.is_empty() || new.is_empty() {
        return Vec::new();
    }
    let found = changes(&old, &new);
    if found.is_empty() || found.len() > MAX_CHANGES {
        return Vec::new();
    }
    let changed: usize = found.iter().map(|(o, _)| o.len()).sum();
    if changed as f64 > old.len() as f64 * MAX_CHANGED_SHARE && old.len() > 1 {
        return Vec::new();
    }
    let mut out = Vec::new();
    for (mut o, mut n) in found {
        // Added or removed words aren't a mishearing of anything.
        if o.is_empty() || n.is_empty() || o.len() > MAX_WORDS || n.len() > MAX_WORDS {
            continue;
        }
        // Numbers aren't vocabulary.
        if old_words[o.clone()]
            .iter()
            .chain(&new_words[n.clone()])
            .any(|w| w.chars().any(|c| c.is_ascii_digit()))
        {
            continue;
        }
        // Where the changed words themselves start, before any widening.
        let sentence_start = o.start == 0 || old_words[o.start - 1].ends_with(['.', '!', '?']);
        // A short word is too common to learn alone: take the word before.
        if letters(&old_words[o.clone()]).chars().count() < SHORT_LETTERS {
            if o.start == 0 || n.start == 0 || old[o.start - 1] != new[n.start - 1] {
                continue;
            }
            o = o.start - 1..o.end;
            n = n.start - 1..n.end;
        }
        let case_only =
            old[o.clone()].join(" ").to_lowercase() == new[n.clone()].join(" ").to_lowercase();
        // A capital at the start of a sentence is grammar, not a name; and
        // only a capital added (a name) is worth learning, not one taken away.
        let adds_capital = new_words[n.clone()]
            .iter()
            .any(|w| w.chars().any(char::is_uppercase));
        if case_only && (sentence_start || !adds_capital) {
            continue;
        }
        let heard_letters = letters(&old_words[o.clone()]);
        let written_letters = letters(&new_words[n.clone()]);
        if heard_letters.is_empty()
            || written_letters.is_empty()
            || heard_letters.chars().all(|c| c.is_ascii_digit())
        {
            continue;
        }
        let distance = strsim::levenshtein(&heard_letters, &written_letters) as f64
            / heard_letters
                .chars()
                .count()
                .max(written_letters.chars().count()) as f64;
        // Different words that neither look nor sound alike aren't a
        // mishearing ("Monday" → "Friday" is a change of mind).
        let sounds_alike = sound_digits(&heard_letters) == sound_digits(&written_letters)
            && !sound_digits(&heard_letters).is_empty();
        if distance > MAX_DISTANCE && !sounds_alike {
            continue;
        }
        if o.len() == 1 && n.len() == 1 && inflected(&heard_letters, &written_letters) {
            continue;
        }
        let heard = old[o]
            .iter()
            .filter(|w| !w.is_empty())
            .map(|w| w.to_lowercase())
            .collect::<Vec<_>>()
            .join(" ");
        let written = written(&new_words[n]);
        if heard.is_empty() || written.is_empty() || heard == written.to_lowercase() && !case_only {
            continue;
        }
        out.push(Correction { heard, written });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(original: &str, corrected: &str) -> Vec<(String, String)> {
        learn(original, corrected)
            .into_iter()
            .map(|c| (c.heard, c.written))
            .collect()
    }

    #[test]
    fn a_misheard_name_is_learned() {
        assert_eq!(
            pairs(
                "I spoke with Canela yesterday.",
                "I spoke with Kanella yesterday."
            ),
            vec![("canela".into(), "Kanella".into())]
        );
        // Punctuation next to it isn't part of the written form.
        assert_eq!(
            pairs("Thanks, Canela.", "Thanks, Kanella."),
            vec![("canela".into(), "Kanella".into())]
        );
    }

    #[test]
    fn a_misheard_phrase_and_a_capital_are_learned() {
        assert_eq!(
            pairs(
                "File the habeus corpus petition",
                "File the habeas corpus petition"
            ),
            vec![("habeus".into(), "habeas".into())]
        );
        assert_eq!(
            pairs(
                "Meet at brightwater partners",
                "Meet at Brightwater Partners"
            ),
            vec![("brightwater partners".into(), "Brightwater Partners".into())]
        );
        assert_eq!(
            pairs(
                "as decided in the case et all",
                "as decided in the case et al."
            ),
            vec![("et all".into(), "et al.".into())]
        );
    }

    #[test]
    fn rewording_and_added_text_teach_nothing() {
        // A rewrite, not a fix.
        assert!(pairs("We should meet soon.", "Let's schedule a call next week.").is_empty());
        // Words only added or removed.
        assert!(pairs("Send the draft.", "Send the final draft today.").is_empty());
        assert!(pairs("Send the final draft.", "Send the draft.").is_empty());
        // A different word that doesn't sound alike: a change of mind.
        assert!(pairs(
            "Meet on Monday at noon please.",
            "Meet on Friday at noon please."
        )
        .is_empty());
        // Numbers aren't vocabulary.
        assert!(pairs("Pay 300 dollars", "Pay 400 dollars").is_empty());
        // Nothing changed, or nothing at all.
        assert!(pairs("Same text.", "Same text.").is_empty());
        assert!(pairs("", "Something").is_empty());
    }

    #[test]
    fn what_parakeet_actually_misheard_is_learned() {
        assert_eq!(
            pairs(
                "Ms. Canila will attend the deposition",
                "Ms. Kanella will attend the deposition."
            ),
            vec![("canila".into(), "Kanella".into())]
        );
        assert_eq!(
            pairs(
                "The parties stipulated to Anunk Pro Tunk order",
                "The parties stipulated to a nunc pro tunc order."
            ),
            vec![("anunk pro tunk".into(), "a nunc pro tunc".into())]
        );
        assert_eq!(
            pairs(
                "Plaintiffs filed a motion in Limonay.",
                "Plaintiffs filed a motion in limine."
            ),
            vec![("limonay".into(), "limine".into())]
        );
    }

    #[test]
    fn lowering_a_capital_teaches_nothing() {
        assert!(pairs(
            "Counsel conducted Voir dire on Monday",
            "Counsel conducted voir dire on Monday"
        )
        .is_empty());
    }

    #[test]
    fn grammar_fixes_teach_nothing() {
        assert!(pairs(
            "Defendants move to dismiss now.",
            "Defendants moved to dismiss now."
        )
        .is_empty());
        assert!(pairs(
            "The claim were barred today.",
            "The claims were barred today."
        )
        .is_empty());
        assert!(pairs(
            "She file it yesterday morning.",
            "She filed it yesterday morning."
        )
        .is_empty());
    }

    #[test]
    fn a_short_word_alone_or_a_sentence_capital_teaches_nothing() {
        // "all" → "al." would change every "all"; with no word before, nothing.
        assert!(pairs("all agreed", "al. agreed").is_empty());
        // Capitalizing a sentence start isn't a name.
        assert!(pairs(
            "the court agreed. the clerk filed it.",
            "The court agreed. The clerk filed it."
        )
        .is_empty());
    }

    #[test]
    fn unrelated_text_is_never_learned() {
        assert!(pairs(
            "The motion is denied.",
            "Dear Ms. Kanella, please find the agreement attached."
        )
        .is_empty());
    }

    #[test]
    fn several_small_fixes_in_one_correction() {
        assert_eq!(
            pairs(
                "Canela and Brightwatter signed the lease with Acme today in Boston.",
                "Kanella and Brightwater signed the lease with Akmee today in Boston."
            ),
            vec![
                ("canela".into(), "Kanella".into()),
                ("brightwatter".into(), "Brightwater".into()),
                ("acme".into(), "Akmee".into()),
            ]
        );
    }
}
