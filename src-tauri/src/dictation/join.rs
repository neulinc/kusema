//! Joins dictated segments so they read as one text: spacing between them, and
//! a capital after a sentence ends.

/// Abbreviations whose period doesn't end a sentence, common in legal writing.
/// Compared lowercase against the last word of a segment.
const ABBREVIATIONS: &[&str] = &[
    "v.", "vs.", "mr.", "mrs.", "ms.", "dr.", "prof.", "hon.", "jr.", "sr.", "inc.", "corp.",
    "co.", "ltd.", "llc.", "llp.", "sec.", "art.", "para.", "fig.", "cf.", "e.g.", "i.e.", "st.",
    "ave.", "dept.", "gov.",
];

const SENTENCE_END: &[char] = &['.', '?', '!', '…', '。', '！', '？'];
/// May follow the end of a sentence: `He said "stop."`
const CLOSERS: &[char] = &['"', '\'', ')', ']', '}', '”', '’', '»'];
const OPENERS: &[char] = &['(', '[', '{', '"', '“', '‘', '«'];
/// Attach to the text after them. A straight quote is left out: at the end of a
/// segment it's far more likely closing than opening.
const NO_SPACE_AFTER: &[char] = &['(', '[', '{', '“', '‘', '«'];
/// Attach to the text before them, with no space.
const NO_SPACE_BEFORE: &[char] = &[
    '.', ',', ';', ':', '?', '!', '…', ')', ']', '}', '%', '”', '’', '»', '。', '，', '！', '？',
];

/// What has been written so far, as far as joining the next segment cares.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct JoinState {
    /// Nothing written yet this session. What's before the caret is unknown,
    /// so the first segment goes in exactly as transcribed.
    at_start: bool,
    capitalize_next: bool,
    last_char: Option<char>,
}

impl Default for JoinState {
    fn default() -> Self {
        Self {
            at_start: true,
            capitalize_next: false,
            last_char: None,
        }
    }
}

impl JoinState {
    /// The text to write for `segment`, and the state after writing it. A
    /// blank segment writes nothing.
    pub fn join(&self, segment: &str) -> (String, JoinState) {
        let segment = segment.trim();
        let Some(first) = segment.chars().next() else {
            return (String::new(), *self);
        };

        let mut out = String::with_capacity(segment.len() + 1);
        if self.needs_space_before(first) {
            out.push(' ');
        }
        if self.capitalize_next {
            out.push_str(&capitalize_first_word(segment));
        } else {
            out.push_str(segment);
        }

        let next = JoinState {
            at_start: false,
            capitalize_next: ends_sentence(segment),
            last_char: segment.chars().last(),
        };
        (out, next)
    }

    /// `line_break` as the next thing written; what follows starts a sentence.
    pub fn join_break(&self, line_break: &str) -> (String, JoinState) {
        let next = JoinState {
            at_start: false,
            capitalize_next: true,
            last_char: None,
        };
        (line_break.to_string(), next)
    }

    fn needs_space_before(&self, first: char) -> bool {
        if self.at_start {
            return false;
        }
        let Some(last) = self.last_char else {
            return false; // straight after a line break
        };
        if NO_SPACE_BEFORE.contains(&first) || NO_SPACE_AFTER.contains(&last) {
            return false;
        }
        !(is_cjk(last) || is_cjk(first))
    }
}

fn is_cjk(c: char) -> bool {
    matches!(c,
        '\u{3000}'..='\u{30FF}' | '\u{3400}'..='\u{4DBF}' | '\u{4E00}'..='\u{9FFF}'
        | '\u{F900}'..='\u{FAFF}' | '\u{FF00}'..='\u{FFEF}')
}

fn ends_sentence(segment: &str) -> bool {
    let trimmed = segment.trim_end_matches(CLOSERS);
    if !trimmed.ends_with(SENTENCE_END) {
        return false;
    }
    let last_word = trimmed
        .rsplit(char::is_whitespace)
        .next()
        .unwrap_or_default()
        .trim_start_matches(OPENERS)
        .to_lowercase();
    !ABBREVIATIONS.contains(&last_word.as_str())
}

/// Uppercases the first letter, past any opening punctuation. Only ever adds a
/// capital: a capital the model chose (a name, "I") is never taken away.
fn capitalize_first_word(segment: &str) -> String {
    let start = segment
        .char_indices()
        .find(|(_, c)| !OPENERS.contains(c))
        .map_or(segment.len(), |(i, _)| i);
    let (head, rest) = segment.split_at(start);
    let mut chars = rest.chars();
    match chars.next() {
        Some(first) => format!("{head}{}{}", first.to_uppercase(), chars.as_str()),
        None => segment.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Everything `segments` would write, in order.
    fn written(segments: &[&str]) -> String {
        let mut state = JoinState::default();
        let mut text = String::new();
        for segment in segments {
            let (out, next) = state.join(segment);
            text.push_str(&out);
            state = next;
        }
        text
    }

    #[test]
    fn first_segment_goes_in_as_transcribed() {
        assert_eq!(written(&["the court"]), "the court");
    }

    #[test]
    fn segments_are_separated_by_one_space() {
        assert_eq!(
            written(&["The court", "  held that "]),
            "The court held that"
        );
    }

    #[test]
    fn capitalizes_after_a_sentence_ends() {
        assert_eq!(
            written(&["The motion is denied.", "the parties may appeal"]),
            "The motion is denied. The parties may appeal"
        );
        assert_eq!(written(&["Is it final?", "yes"]), "Is it final? Yes");
        assert_eq!(written(&["Stop!", "now"]), "Stop! Now");
        assert_eq!(
            written(&["He said \"no.\"", "then left"]),
            "He said \"no.\" Then left"
        );
    }

    #[test]
    fn does_not_capitalize_mid_sentence() {
        assert_eq!(
            written(&["In the case of", "the plaintiff"]),
            "In the case of the plaintiff"
        );
    }

    #[test]
    fn keeps_a_capital_the_model_chose() {
        assert_eq!(
            written(&["We spoke to", "Ms. Jones"]),
            "We spoke to Ms. Jones"
        );
        assert_eq!(written(&["and then", "I left"]), "and then I left");
    }

    #[test]
    fn abbreviations_do_not_end_a_sentence() {
        assert_eq!(written(&["Smith v.", "jones"]), "Smith v. jones");
        assert_eq!(
            written(&["See Acme Inc.", "and others"]),
            "See Acme Inc. and others"
        );
        assert_eq!(written(&["(e.g.", "a lease)"]), "(e.g. a lease)");
    }

    #[test]
    fn punctuation_attaches_to_the_previous_segment() {
        assert_eq!(written(&["Dear Sir", ", thank you"]), "Dear Sir, thank you");
        assert_eq!(written(&["The end", "."]), "The end.");
    }

    #[test]
    fn no_space_inside_opening_punctuation() {
        assert_eq!(
            written(&["the party (", "the buyer)"]),
            "the party (the buyer)"
        );
    }

    #[test]
    fn capitalizes_past_an_opening_quote() {
        assert_eq!(
            written(&["Done.", "\"yes\" he said"]),
            "Done. \"Yes\" he said"
        );
    }

    #[test]
    fn chinese_and_japanese_take_no_spaces() {
        assert_eq!(written(&["今天天气", "很好"]), "今天天气很好");
    }

    #[test]
    fn capitalizes_non_ascii_letters() {
        assert_eq!(written(&["Fini.", "été"]), "Fini. Été");
    }

    #[test]
    fn a_blank_segment_writes_nothing_and_changes_nothing() {
        let state = JoinState::default();
        let (out, next) = state.join("   ");
        assert_eq!(out, "");
        assert_eq!(next, state);
    }

    #[test]
    fn after_a_line_break_there_is_no_space_and_a_capital() {
        let (first, state) = JoinState::default().join("First paragraph");
        let (line_break, state) = state.join_break("\n\n");
        let (next, _) = state.join("second paragraph");
        assert_eq!(
            format!("{first}{line_break}{next}"),
            "First paragraph\n\nSecond paragraph"
        );
    }

    #[test]
    fn a_saved_state_undoes_the_join() {
        let (_, before) = JoinState::default().join("One.");
        let (_, after) = before.join("two");
        assert!(!after.capitalize_next);
        // After scratching "two", the next segment joins as if it never happened.
        let (out, _) = before.join("three");
        assert_eq!(out, " Three");
    }
}
