//! Joins dictated segments so they read as one text: spacing between them, a
//! capital after a sentence ends, and a period when a model left one off.
//!
//! The period is only added when both sides are clear: this segment could finish
//! a sentence, and the next opens with a word that starts one. Names never qualify,
//! so "We spoke to John" + "Smith called" is left alone. A wrong guess adds a stray
//! period, which is worse than a missing one, so it stays strict.

/// Abbreviations whose period doesn't end a sentence, common in legal writing.
/// Compared lowercase against the last word of a segment.
const ABBREVIATIONS: &[&str] = &[
    "v.", "vs.", "mr.", "mrs.", "ms.", "dr.", "prof.", "hon.", "jr.", "sr.", "inc.", "corp.",
    "co.", "ltd.", "llc.", "llp.", "sec.", "art.", "para.", "fig.", "cf.", "e.g.", "i.e.", "st.",
    "ave.", "dept.", "gov.", "u.s.", "u.k.", "u.s.a.",
];

/// A segment ending in one of these (lowercase, no punctuation) isn't finished:
/// the sentence carries on, or the word is an abbreviation waiting for a name.
const OPEN_ENDED: &[&str] = &[
    "a",
    "an",
    "the",
    "this",
    "that",
    "these",
    "those",
    "my",
    "your",
    "our",
    "his",
    "her",
    "its",
    "their",
    "of",
    "to",
    "and",
    "or",
    "but",
    "nor",
    "for",
    "in",
    "on",
    "at",
    "by",
    "with",
    "from",
    "as",
    "into",
    "over",
    "under",
    "between",
    "against",
    "during",
    "before",
    "after",
    "until",
    "since",
    "per",
    "via",
    "about",
    "including",
    "regarding",
    "concerning",
    "is",
    "are",
    "was",
    "were",
    "be",
    "been",
    "am",
    "will",
    "shall",
    "would",
    "should",
    "could",
    "can",
    "may",
    "might",
    "must",
    "do",
    "does",
    "did",
    "has",
    "have",
    "had",
    "not",
    "if",
    "then",
    "than",
    "so",
    "because",
    "although",
    "though",
    "while",
    "when",
    "where",
    "whether",
    "which",
    "who",
    "whom",
    "whose",
    "what",
    "very",
    "more",
    "most",
    "such",
    "said",
    "dear",
    "mr",
    "mrs",
    "ms",
    "dr",
    "prof",
    "hon",
    "jr",
    "sr",
    "inc",
    "corp",
    "co",
    "ltd",
    "llc",
    "llp",
    "lp",
    "u.s",
    "u.k",
    "u.s.a",
    "v",
    "vs",
    "no",
    "nos",
    "st",
    "ave",
    "dept",
    "gov",
    "sec",
    "art",
    "para",
    "fig",
    "cf",
    "e.g",
    "i.e",
    "et",
    "esq",
];

/// Words that start a sentence, exactly as a model capitalizes them. A list, not
/// "any capital", because a capital could be a name. "That" and "Which" are left
/// out: models capitalize them when a pause splits "held that".
const SENTENCE_OPENERS: &[&str] = &[
    "The",
    "This",
    "These",
    "Those",
    "We",
    "I",
    "You",
    "He",
    "She",
    "They",
    "It",
    "Our",
    "My",
    "Your",
    "His",
    "Her",
    "Their",
    "There",
    "Here",
    "Please",
    "However",
    "Therefore",
    "Thus",
    "Accordingly",
    "Moreover",
    "Furthermore",
    "Additionally",
    "Finally",
    "First",
    "Second",
    "Third",
    "Next",
    "Also",
    "In",
    "On",
    "At",
    "If",
    "When",
    "After",
    "Before",
    "As",
    "For",
    "A",
    "An",
    "Since",
    "Because",
    "Although",
    "While",
    "Thank",
    "Dear",
    "Sincerely",
    "Regards",
    "Let",
];

/// A segment needs this many words to read as a finished sentence.
const MIN_FINISHED_WORDS: usize = 3;

const SENTENCE_END: &[char] = &['.', '?', '!', '…', '。', '！', '？'];
/// May follow the end of a sentence: `He said "stop."`
const CLOSERS: &[char] = &['"', '\'', ')', ']', '}', '”', '’', '»'];
const OPENERS: &[char] = &['(', '[', '{', '"', '“', '‘', '«'];
/// Attach to the text after them. A straight quote is left out: at the end of a
/// segment it's far more likely closing than opening.
const NO_SPACE_AFTER: &[char] = &['(', '[', '{', '“', '‘', '«', '—', '-'];
/// Attach to the text before them, with no space.
const NO_SPACE_BEFORE: &[char] = &[
    '.', ',', ';', ':', '?', '!', '…', ')', ']', '}', '%', '”', '’', '»', '。', '，', '！', '？',
    '—', '-',
];

/// What has been written so far, as far as joining the next segment cares.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct JoinState {
    /// Nothing written yet this session. What's before the caret is unknown,
    /// so the first segment goes in exactly as transcribed.
    at_start: bool,
    capitalize_next: bool,
    last_char: Option<char>,
    /// Add a missing period between segments (part of local cleanup).
    insert_periods: bool,
    /// What's written so far could be a finished sentence with no period.
    may_end: bool,
}

impl Default for JoinState {
    fn default() -> Self {
        Self {
            at_start: true,
            capitalize_next: false,
            last_char: None,
            insert_periods: false,
            may_end: false,
        }
    }
}

impl JoinState {
    /// A state that also adds missing periods.
    pub fn with_period_insertion() -> Self {
        Self {
            insert_periods: true,
            ..Self::default()
        }
    }

    /// The text to write for `segment`, and the state after writing it. A
    /// blank segment writes nothing.
    pub fn join(&self, segment: &str) -> (String, JoinState) {
        let segment = segment.trim();
        let Some(first) = segment.chars().next() else {
            return (String::new(), *self);
        };

        let mut out = String::with_capacity(segment.len() + 2);
        if self.insert_periods && self.may_end && opens_a_sentence(segment) {
            out.push('.');
        }
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
            may_end: could_be_finished(segment),
            ..*self
        };
        (out, next)
    }

    /// `line_break` as the next thing written; what follows starts a sentence.
    pub fn join_break(&self, line_break: &str) -> (String, JoinState) {
        let next = JoinState {
            at_start: false,
            capitalize_next: true,
            last_char: None,
            may_end: false,
            ..*self
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

/// Could this segment be a whole sentence that's just missing its period?
fn could_be_finished(segment: &str) -> bool {
    if !segment.ends_with(char::is_alphanumeric) {
        return false;
    }
    let words: Vec<&str> = segment.split_whitespace().collect();
    if words.len() < MIN_FINISHED_WORDS {
        return false;
    }
    let last = words[words.len() - 1].to_lowercase();
    // A lone letter is an initial: "John Q".
    last.chars().count() > 1 && !OPEN_ENDED.contains(&last.as_str())
}

/// Does this segment open with a word that starts a sentence?
fn opens_a_sentence(segment: &str) -> bool {
    segment
        .split_whitespace()
        .next()
        .map(|word| word.trim_end_matches(|c: char| !c.is_alphanumeric()))
        .is_some_and(|word| SENTENCE_OPENERS.contains(&word))
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

    fn written_with_periods(segments: &[&str]) -> String {
        let mut state = JoinState::with_period_insertion();
        let mut text = String::new();
        for segment in segments {
            let (out, next) = state.join(segment);
            text.push_str(&out);
            state = next;
        }
        text
    }

    #[test]
    fn adds_a_period_between_a_finished_sentence_and_a_new_one() {
        assert_eq!(
            written_with_periods(&["The court denied the motion", "The parties may appeal."]),
            "The court denied the motion. The parties may appeal."
        );
        assert_eq!(
            written_with_periods(&["We filed the complaint", "However, it was late"]),
            "We filed the complaint. However, it was late"
        );
        assert_eq!(
            written_with_periods(&["We spoke to John", "I told him"]),
            "We spoke to John. I told him"
        );
    }

    #[test]
    fn no_period_unless_asked_for() {
        assert_eq!(
            written(&["The court denied the motion", "The parties may appeal."]),
            "The court denied the motion The parties may appeal."
        );
    }

    #[test]
    fn no_period_before_a_name() {
        for segments in [
            ["We spoke to John", "Smith called later"],
            ["The letter went to", "Jones and Day"],
            ["The buyer met with counsel", "Acme agreed"],
        ] {
            assert_eq!(
                written_with_periods(&segments),
                format!("{} {}", segments[0], segments[1]),
                "{segments:?}"
            );
        }
    }

    #[test]
    fn no_period_after_an_abbreviation() {
        for last in [
            "We wrote to Mr",
            "We wrote to Mrs",
            "We wrote to Dr",
            "It was sold by Acme Inc",
            "The law of the U.S",
            "The case was Smith v",
            "See also U.S.A",
            "It went to John Q",
        ] {
            assert_eq!(
                written_with_periods(&[last, "The parties agreed"]),
                format!("{last} The parties agreed"),
                "{last}"
            );
        }
    }

    #[test]
    fn abbreviations_with_their_periods_do_not_end_a_sentence() {
        assert_eq!(
            written(&["We wrote to Mr.", "jones"]),
            "We wrote to Mr. jones"
        );
        assert_eq!(
            written(&["In the U.S.", "courts differ"]),
            "In the U.S. courts differ"
        );
        assert_eq!(
            written(&["It was Acme Inc.", "that sued"]),
            "It was Acme Inc. that sued"
        );
    }

    #[test]
    fn no_period_when_the_sentence_carries_on() {
        for (first, second) in [
            ("The court held that", "The defendant was liable"),
            ("We sent it to the", "The other party"),
            ("The motion was filed and", "I signed it"),
            ("We need the documents because", "They are late"),
            ("The court held", "That the defendant was liable"),
        ] {
            assert_eq!(
                written_with_periods(&[first, second]),
                format!("{first} {second}"),
                "{first}"
            );
        }
    }

    #[test]
    fn no_period_after_a_short_fragment_or_existing_punctuation() {
        assert_eq!(
            written_with_periods(&["Thank you", "The end"]),
            "Thank you The end"
        );
        assert_eq!(
            written_with_periods(&["We are done.", "The end"]),
            "We are done. The end"
        );
        assert_eq!(
            written_with_periods(&["We are done,", "The end"]),
            "We are done, The end"
        );
        assert_eq!(
            written_with_periods(&["Are we done?", "The end"]),
            "Are we done? The end"
        );
    }

    #[test]
    fn no_period_before_a_lowercase_word() {
        assert_eq!(
            written_with_periods(&["We filed the motion", "the parties agreed"]),
            "We filed the motion the parties agreed"
        );
    }

    #[test]
    fn no_period_at_a_line_break() {
        let (first, state) = JoinState::with_period_insertion().join("123 Main Street Springfield");
        let (line_break, state) = state.join_break("\n");
        let (next, _) = state.join("The tenant");
        assert_eq!(
            format!("{first}{line_break}{next}"),
            "123 Main Street Springfield\nThe tenant"
        );
    }

    #[test]
    fn nothing_is_lowercased_when_a_period_goes_in() {
        assert_eq!(
            written_with_periods(&["The CEO of Acme resigned", "The Board Met"]),
            "The CEO of Acme resigned. The Board Met"
        );
    }

    #[test]
    fn a_saved_state_undoes_the_period_too() {
        let (_, before) = JoinState::with_period_insertion().join("We filed the motion");
        let (_, after) = before.join("The court agreed");
        assert!(after.may_end);
        // After scratching that segment, the next one still gets its period.
        let (out, _) = before.join("The clerk stamped it");
        assert_eq!(out, ". The clerk stamped it");
    }

    #[test]
    fn spoken_marks_arriving_as_their_own_segment_attach() {
        assert_eq!(
            written(&["Dear Sir", ",", "thank you"]),
            "Dear Sir, thank you"
        );
        assert_eq!(written(&["The deal", "—", "signed"]), "The deal—signed");
        assert_eq!(written(&["We won", ".", "the end"]), "We won. The end");
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
