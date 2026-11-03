//! "Sounds like" replacement: a spoken form, matched as whole words without
//! regard to case, becomes its written form. Exact, so it can't touch words
//! that only resemble an entry; the loose matching is Handy's fuzzy custom
//! words, for entries without spoken forms.

use std::collections::HashMap;

/// A word in the text and where it is.
struct Word<'a> {
    text: &'a str,
    start: usize,
    end: usize,
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '\''
}

fn words(text: &str) -> Vec<Word<'_>> {
    let mut out = Vec::new();
    let mut start = None;
    for (i, c) in text.char_indices() {
        match (is_word_char(c), start) {
            (true, None) => start = Some(i),
            (false, Some(s)) => {
                out.push(Word {
                    text: &text[s..i],
                    start: s,
                    end: i,
                });
                start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = start {
        out.push(Word {
            text: &text[s..],
            start: s,
            end: text.len(),
        });
    }
    out
}

/// Lowercase words of a spoken form ("Et. al" → ["et", "al"]).
fn key(spoken: &str) -> Vec<String> {
    words(spoken)
        .iter()
        .map(|w| w.text.trim_matches('\'').to_lowercase())
        .filter(|w| !w.is_empty())
        .collect()
}

/// What may stand between the words of one spoken form: spaces, hyphens and
/// the periods of spelled-out letters ("c.o.p.d."), never a comma or a
/// sentence's end followed by a space.
fn joins(gap: &str) -> bool {
    !gap.is_empty()
        && gap.len() <= 3
        && gap.chars().all(|c| c == ' ' || c == '-' || c == '.')
        && !gap.contains(". ")
}

/// The spoken-form table, longest first under each first word.
pub struct Replacer {
    by_first: HashMap<String, Vec<(Vec<String>, String)>>,
}

impl Replacer {
    /// `pairs`: (spoken form, written form).
    pub fn new<'a>(pairs: impl IntoIterator<Item = (&'a str, &'a str)>) -> Self {
        let mut by_first: HashMap<String, Vec<(Vec<String>, String)>> = HashMap::new();
        for (spoken, written) in pairs {
            let key = key(spoken);
            let Some(first) = key.first().cloned() else {
                continue;
            };
            let list = by_first.entry(first).or_default();
            if !list.iter().any(|(k, _)| *k == key) {
                list.push((key, written.to_string()));
            }
        }
        for list in by_first.values_mut() {
            list.sort_by_key(|(key, _)| std::cmp::Reverse(key.len()));
        }
        Self { by_first }
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.by_first.is_empty()
    }

    /// One pass, left to right: replaced text is never matched again.
    pub fn apply(&self, text: &str) -> String {
        if self.by_first.is_empty() {
            return text.to_string();
        }
        let ws = words(text);
        let lower: Vec<String> = ws
            .iter()
            .map(|w| w.text.trim_matches('\'').to_lowercase())
            .collect();
        let mut out = String::with_capacity(text.len());
        let mut copied = 0; // bytes of `text` already in `out`
        let mut i = 0;
        while i < ws.len() {
            let found = self.by_first.get(&lower[i]).and_then(|candidates| {
                candidates.iter().find(|(key, _)| {
                    key.len() <= ws.len() - i
                        && key.iter().enumerate().all(|(k, part)| {
                            lower[i + k] == *part
                                && (k == 0 || joins(&text[ws[i + k - 1].end..ws[i + k].start]))
                        })
                })
            });
            let Some((key, written)) = found else {
                i += 1;
                continue;
            };
            let start = ws[i].start;
            let mut end = ws[i + key.len() - 1].end;
            // A dotted spelled-out form's own final period ("c.o.p.d."), but
            // not a sentence's ("C O P D.").
            let dotted = key.len() > 1
                && key.iter().all(|k| k.chars().count() == 1)
                && (1..key.len()).all(|k| text[ws[i + k - 1].end..ws[i + k].start].contains('.'));
            if dotted && text[end..].starts_with('.') {
                end += 1;
            }
            out.push_str(&text[copied..start]);
            // A capital the model gave the misheard words counts only at the
            // start of a sentence ("Bois Dyer" mid-sentence → "voir dire").
            let before = text[..start].trim_end_matches([' ', '"', '\u{201C}']);
            let sentence_start = before.is_empty() || before.ends_with(['.', '!', '?', '\n', ':']);
            if sentence_start {
                out.push_str(&capitalized_like(written, ws[i].text));
            } else {
                out.push_str(written);
            }
            // "et al." where the text already had "et al.": one period.
            let ends_with_punct = written.ends_with(['.', '!', '?']);
            if ends_with_punct && text[end..].starts_with('.') {
                end += 1;
            }
            copied = end;
            i += key.len();
        }
        out.push_str(&text[copied..]);
        out
    }
}

/// `written`, with a capital first letter if the words it replaced began
/// with one (a sentence start) and the written form is all lowercase.
fn capitalized_like(written: &str, heard: &str) -> String {
    let heard_capital = heard.chars().next().is_some_and(char::is_uppercase);
    let written_lower = !written.chars().any(char::is_uppercase);
    if heard_capital && written_lower {
        let mut chars = written.chars();
        match chars.next() {
            Some(first) => first.to_uppercase().chain(chars).collect(),
            None => String::new(),
        }
    } else {
        written.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn replacer(pairs: &[(&str, &str)]) -> Replacer {
        Replacer::new(pairs.iter().copied())
    }

    #[test]
    fn spoken_forms_become_written_forms_as_whole_words() {
        let r = replacer(&[("habeus corpus", "habeas corpus"), ("canela", "Kanella")]);
        assert_eq!(
            r.apply("A petition for habeus corpus was filed by Canela."),
            "A petition for habeas corpus was filed by Kanella."
        );
        // Part of a longer word is left alone.
        assert_eq!(r.apply("Canelas and canelated"), "Canelas and canelated");
    }

    #[test]
    fn a_sentence_start_keeps_its_capital_and_names_keep_theirs() {
        let r = replacer(&[("pro say", "pro se"), ("kusama", "Kusema")]);
        assert_eq!(r.apply("Pro say litigants."), "Pro se litigants.");
        assert_eq!(r.apply("Ask kusama now"), "Ask Kusema now");
        assert_eq!(r.apply("We need Pro Say help."), "We need pro se help.");
    }

    #[test]
    fn the_longest_spoken_form_wins_and_nothing_is_replaced_twice() {
        let r = replacer(&[
            ("subpena", "subpoena"),
            ("subpena duces tikum", "subpoena duces tecum"),
            ("kusemma", "Kusema"),
            ("kusema", "Kusema"),
        ]);
        assert_eq!(
            r.apply("Serve a subpena duces tikum and a subpena."),
            "Serve a subpoena duces tecum and a subpoena."
        );
        assert_eq!(r.apply("Kusemma"), "Kusema");
    }

    #[test]
    fn a_comma_or_a_sentence_end_breaks_a_phrase() {
        let r = replacer(&[("pro say", "pro se")]);
        assert_eq!(r.apply("I'm pro, say it."), "I'm pro, say it.");
        assert_eq!(r.apply("It was pro. Say it."), "It was pro. Say it.");
        assert_eq!(r.apply("pro-say"), "pro se");
    }

    #[test]
    fn written_periods_are_not_doubled_and_spelled_letters_match() {
        let r = replacer(&[("et al", "et al."), ("c o p d", "COPD")]);
        assert_eq!(r.apply("Smith et al. argued"), "Smith et al. argued");
        assert_eq!(r.apply("Smith et al argued"), "Smith et al. argued");
        assert_eq!(r.apply("History of c.o.p.d. and"), "History of COPD and");
        assert_eq!(r.apply("History of C O P D."), "History of COPD.");
    }

    #[test]
    fn an_empty_table_changes_nothing() {
        let r = replacer(&[]);
        assert!(r.is_empty());
        assert_eq!(r.apply("Anything."), "Anything.");
    }
}
