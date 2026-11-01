//! Spoken punctuation: "comma", "period", "open quote" and so on become marks.
//!
//! The transcript has no reliable pauses, and models add their own punctuation
//! after a spoken mark, so the rule works from the surrounding words:
//! - "period" is a common word in legal writing. It only counts at the end of a
//!   phrase or before a capitalized word, and never after a determiner or a word
//!   that makes a time period ("the limitations period").
//! - Other marks count anywhere, unless a determiner comes before them ("the
//!   colon") or a word after them says they're a word ("colon cancer").
//!
//! The cost: "denied period the parties may appeal", all one lowercase run, stays
//! as words. "literal period" types the word; "scratch that" undoes a wrong guess.

/// How a mark sits against its neighbors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Spacing {
    /// `word, next`
    AfterWord,
    /// `word (next`
    Opening,
    /// `word-next`
    Joining,
    /// `word § next`
    Separate,
}

/// When a mark's words count as the mark.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Rule {
    /// Unless a determiner comes before it.
    Anywhere,
    /// The strict rule for "period".
    PhraseEnd,
}

struct Mark {
    words: &'static [&'static str],
    output: &'static str,
    spacing: Spacing,
    rule: Rule,
    /// The next word starts a sentence.
    ends_sentence: bool,
    /// Following words that make this the ordinary word.
    not_before: &'static [&'static str],
    /// Preceding words that make this the ordinary word.
    not_after: &'static [&'static str],
}

const fn mark(words: &'static [&'static str], output: &'static str, spacing: Spacing) -> Mark {
    Mark {
        words,
        output,
        spacing,
        rule: Rule::Anywhere,
        ends_sentence: false,
        not_before: &[],
        not_after: &[],
    }
}

const fn sentence_end(words: &'static [&'static str], output: &'static str) -> Mark {
    Mark {
        ends_sentence: true,
        ..mark(words, output, Spacing::AfterWord)
    }
}

/// Every spoken mark. The one place to translate or extend them.
const MARKS: &[Mark] = &[
    Mark {
        rule: Rule::PhraseEnd,
        not_before: &["of"],
        ..sentence_end(&["period"], ".")
    },
    sentence_end(&["full", "stop"], "."),
    sentence_end(&["question", "mark"], "?"),
    sentence_end(&["exclamation", "mark"], "!"),
    sentence_end(&["exclamation", "point"], "!"),
    Mark {
        not_before: &["separated", "delimited", "splice"],
        not_after: &["oxford", "serial"],
        ..mark(&["comma"], ",", Spacing::AfterWord)
    },
    Mark {
        not_before: &["cancer", "surgery", "polyp", "polyps", "screening"],
        not_after: &[
            "sigmoid",
            "ascending",
            "descending",
            "transverse",
            "spastic",
        ],
        ..mark(&["colon"], ":", Spacing::AfterWord)
    },
    mark(&["semicolon"], ";", Spacing::AfterWord),
    mark(&["semi", "colon"], ";", Spacing::AfterWord),
    mark(&["ellipsis"], "…", Spacing::AfterWord),
    mark(&["open", "quote"], "“", Spacing::Opening),
    mark(&["close", "quote"], "”", Spacing::AfterWord),
    mark(&["open", "paren"], "(", Spacing::Opening),
    mark(&["close", "paren"], ")", Spacing::AfterWord),
    mark(&["open", "parenthesis"], "(", Spacing::Opening),
    mark(&["close", "parenthesis"], ")", Spacing::AfterWord),
    mark(&["hyphen"], "-", Spacing::Joining),
    Mark {
        not_before: &["cam", "board", "of", "to", "through"],
        not_after: &["mad", "quick", "last", "final", "hundred", "yard", "meter"],
        ..mark(&["dash"], "—", Spacing::Joining)
    },
    mark(&["section", "symbol"], "§", Spacing::Separate),
    mark(&["paragraph", "symbol"], "¶", Spacing::Separate),
];

/// Before any mark's words, these make them ordinary words ("the comma").
const DETERMINERS: &[&str] = &[
    "a", "an", "the", "this", "that", "these", "those", "each", "every", "any", "some", "no",
    "another", "my", "your", "his", "her", "its", "our", "their", "which", "what", "such",
];

/// Before "period", these make it a time period.
const BEFORE_TIME_PERIOD: &[&str] = &[
    "limitations",
    "limitation",
    "grace",
    "waiting",
    "notice",
    "trial",
    "probation",
    "probationary",
    "cooling",
    "redemption",
    "discovery",
    "comment",
    "review",
    "warranty",
    "rental",
    "tax",
    "accounting",
    "billing",
    "pay",
    "payroll",
    "vesting",
    "lease",
    "rent",
    "loan",
    "time",
    "statutory",
    "relevant",
    "applicable",
    "same",
    "entire",
    "whole",
    "given",
    "certain",
    "short",
    "long",
    "brief",
    "initial",
    "final",
    "additional",
    "extended",
    "prior",
    "previous",
    "current",
    "next",
    "last",
    "first",
    "second",
    "third",
    "day",
    "days",
    "week",
    "weeks",
    "month",
    "months",
    "year",
    "years",
    "hour",
    "hours",
    "minute",
    "minutes",
    "quarter",
    "reporting",
    "fiscal",
    "transition",
    "extension",
    "renewal",
    "remedial",
    "cure",
    "objection",
    "appeal",
    "assessment",
    "said",
];

/// Models add these after a word; a spoken mark replaces them.
const ADDED_PUNCTUATION: &[char] = &[',', '.', ';', ':', '?', '!', '…'];
const SENTENCE_STOP: &[char] = &['.', '?', '!'];

/// A token's word, lowercased and without punctuation the model added around it.
/// `None` if something else is attached, like a quote: then it's not a bare word.
fn bare_word(token: &str) -> Option<String> {
    let core = token.trim_end_matches(ADDED_PUNCTUATION);
    if core.is_empty() || !core.chars().all(|c| c.is_alphanumeric() || c == '-') {
        return None;
    }
    Some(core.to_lowercase())
}

fn plain_word(token: &str) -> String {
    token
        .trim_matches(|c: char| !c.is_alphanumeric())
        .to_lowercase()
}

/// Whether `tokens[at..]` starts with `words`. Only the last may be followed by
/// punctuation, which is what separates "open quote" from "open, quote".
fn starts_with_words(tokens: &[&str], at: usize, words: &[&str]) -> bool {
    if at + words.len() > tokens.len() {
        return false;
    }
    words.iter().enumerate().all(|(offset, word)| {
        let token = tokens[at + offset];
        let is_last = offset + 1 == words.len();
        match bare_word(token) {
            Some(bare) => {
                bare == *word
                    && (is_last || token.len() == token.trim_end_matches(ADDED_PUNCTUATION).len())
            }
            None => false,
        }
    })
}

fn is_mark_here(mark: &Mark, tokens: &[&str], at: usize) -> bool {
    if !starts_with_words(tokens, at, mark.words) {
        return false;
    }
    let previous = at.checked_sub(1).map(|i| plain_word(tokens[i]));
    if previous
        .as_deref()
        .is_some_and(|p| DETERMINERS.contains(&p))
    {
        return false;
    }
    if previous
        .as_deref()
        .is_some_and(|p| mark.not_after.contains(&p))
    {
        return false;
    }
    let last = tokens[at + mark.words.len() - 1];
    let next = tokens.get(at + mark.words.len()).copied();
    if next.is_some_and(|n| mark.not_before.contains(&plain_word(n).as_str())) {
        return false;
    }
    match mark.rule {
        Rule::Anywhere => true,
        Rule::PhraseEnd => {
            let before_time_word = previous.as_deref().is_some_and(|p| {
                BEFORE_TIME_PERIOD.contains(&p)
                    || p.chars().all(|c| c.is_ascii_digit())
                    || p.ends_with("-day")
                    || p.ends_with("-week")
                    || p.ends_with("-month")
                    || p.ends_with("-year")
            });
            if before_time_word {
                return false;
            }
            let before_closing_mark = [
                ["close", "quote"],
                ["close", "paren"],
                ["close", "parenthesis"],
            ]
            .iter()
            .any(|words| starts_with_words(tokens, at + mark.words.len(), words));
            let at_phrase_end =
                next.is_none() || last.ends_with(SENTENCE_STOP) || before_closing_mark;
            let before_capital = next
                .and_then(|n| n.chars().find(|c| c.is_alphabetic()))
                .is_some_and(char::is_uppercase);
            at_phrase_end || before_capital
        }
    }
}

fn mark_at(tokens: &[&str], at: usize) -> Option<&'static Mark> {
    MARKS.iter().find(|mark| is_mark_here(mark, tokens, at))
}

/// How many tokens after "literal" it protects: a spoken mark, or a voice command.
fn literal_length(tokens: &[&str], at: usize) -> usize {
    let phrases = MARKS
        .iter()
        .map(|mark| mark.words.to_vec())
        .chain(super::COMMAND_WORDS.iter().map(|words| words.to_vec()));
    phrases
        .filter(|words| starts_with_words(tokens, at, words))
        .map(|words| words.len())
        .max()
        .unwrap_or(0)
}

fn capitalize(word: &str) -> String {
    let mut out = String::with_capacity(word.len());
    let mut done = false;
    for c in word.chars() {
        if !done && c.is_alphabetic() {
            out.extend(c.to_uppercase());
            done = true;
        } else {
            out.push(c);
        }
    }
    out
}

/// What was last written, for deciding the space before the next thing.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Last {
    Nothing,
    Word,
    /// Nothing may follow with a space.
    Glued,
    /// A mark that wants a space after it.
    Spaced,
}

/// Turns spoken punctuation in `text` into marks. Never lowercases anything, and
/// only uppercases the word after a mark that ends a sentence.
pub fn apply(text: &str) -> String {
    let tokens: Vec<&str> = text.split_whitespace().collect();
    let mut out = String::with_capacity(text.len());
    let mut last = Last::Nothing;
    let mut capitalize_next = false;
    let mut at = 0;

    while at < tokens.len() {
        // "literal" only means something before a mark or a command, so that
        // "a literal reading" stays as it was said.
        if plain_word(tokens[at]) == "literal" && tokens[at].len() == "literal".len() {
            let protected = literal_length(&tokens, at + 1);
            if protected > 0 {
                let promote = tokens[at].starts_with(char::is_uppercase);
                for offset in 0..protected {
                    let token = tokens[at + 1 + offset];
                    let word = if offset == 0 && (promote || capitalize_next) {
                        capitalize(token)
                    } else {
                        token.to_string()
                    };
                    if last != Last::Nothing && last != Last::Glued {
                        out.push(' ');
                    }
                    out.push_str(&word);
                    last = Last::Word;
                    capitalize_next = false;
                }
                at += 1 + protected;
                continue;
            }
        }

        if let Some(mark) = mark_at(&tokens, at) {
            match mark.spacing {
                Spacing::AfterWord | Spacing::Joining => {
                    // The mark replaces whatever the model put there.
                    let kept = out.trim_end_matches(ADDED_PUNCTUATION).len();
                    if mark.output != "”" && mark.output != ")" {
                        out.truncate(kept);
                    }
                }
                Spacing::Opening | Spacing::Separate => {
                    if last != Last::Nothing && last != Last::Glued {
                        out.push(' ');
                    }
                }
            }
            // "close quote." : the model heard a sentence end there, so it stays,
            // inside a closing quote and after a closing paren, as US style has it.
            let spoken_stop = tokens[at + mark.words.len() - 1]
                .chars()
                .last()
                .filter(|c| SENTENCE_STOP.contains(c));
            let mut ends_sentence = mark.ends_sentence;
            match spoken_stop {
                Some(stop) if mark.output == "”" => {
                    out.push(stop);
                    out.push_str(mark.output);
                    ends_sentence = true;
                }
                Some(stop) if mark.output == ")" => {
                    out.push_str(mark.output);
                    out.push(stop);
                    ends_sentence = true;
                }
                _ => out.push_str(mark.output),
            }
            last = match mark.spacing {
                Spacing::AfterWord | Spacing::Separate => Last::Spaced,
                Spacing::Opening | Spacing::Joining => Last::Glued,
            };
            capitalize_next = ends_sentence;
            at += mark.words.len();
            continue;
        }

        if last != Last::Nothing && last != Last::Glued {
            out.push(' ');
        }
        if capitalize_next {
            out.push_str(&capitalize(tokens[at]));
        } else {
            out.push_str(tokens[at]);
        }
        last = Last::Word;
        capitalize_next = false;
        at += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_mark_and_its_spacing() {
        for (spoken, written) in [
            ("Dear Sir comma thank you", "Dear Sir, thank you"),
            ("The motion is denied period", "The motion is denied."),
            ("The motion is denied full stop", "The motion is denied."),
            ("Is it final question mark", "Is it final?"),
            ("We won exclamation mark", "We won!"),
            ("We won exclamation point", "We won!"),
            ("Re colon the Smith matter", "Re: the Smith matter"),
            ("We agree semicolon they do not", "We agree; they do not"),
            ("We agree semi colon they do not", "We agree; they do not"),
            (
                "He said open quote stop close quote today",
                "He said “stop” today",
            ),
            (
                "the buyer open paren Acme close paren agrees",
                "the buyer (Acme) agrees",
            ),
            (
                "the buyer open parenthesis Acme close parenthesis agrees",
                "the buyer (Acme) agrees",
            ),
            ("a well hyphen known rule", "a well-known rule"),
            (
                "the deal dash signed yesterday dash closed",
                "the deal—signed yesterday—closed",
            ),
            ("and so on ellipsis", "and so on…"),
            ("under section symbol 1983", "under § 1983"),
            ("see paragraph symbol 12", "see ¶ 12"),
        ] {
            assert_eq!(apply(spoken), written, "{spoken}");
        }
    }

    #[test]
    fn a_mark_replaces_the_punctuation_the_model_added() {
        assert_eq!(
            apply("The motion is denied, period."),
            "The motion is denied."
        );
        assert_eq!(apply("Dear Sir, comma, thank you"), "Dear Sir, thank you");
        assert_eq!(apply("Is it final, question mark?"), "Is it final?");
        assert_eq!(apply("Period."), ".");
        assert_eq!(apply("Comma."), ",");
    }

    #[test]
    fn capitals_are_case_insensitive_on_the_way_in() {
        assert_eq!(apply("Done. Comma, then more"), "Done, then more");
        assert_eq!(apply("Question Mark"), "?");
    }

    #[test]
    fn a_capital_follows_a_sentence_end_only() {
        assert_eq!(
            apply("Is it final question mark yes it is"),
            "Is it final? Yes it is"
        );
        assert_eq!(apply("We won exclamation point the end"), "We won! The end");
        assert_eq!(apply("Dear Sir comma thank you"), "Dear Sir, thank you");
        assert_eq!(apply("Re colon the matter"), "Re: the matter");
    }

    #[test]
    fn nothing_is_ever_lowercased() {
        assert_eq!(
            apply("Denied period The Parties Appeal"),
            "Denied. The Parties Appeal"
        );
        assert_eq!(apply("We met Ms. Jones comma Esq"), "We met Ms. Jones, Esq");
    }

    #[test]
    fn period_before_a_capital_counts() {
        assert_eq!(
            apply("The motion is denied period The parties may appeal"),
            "The motion is denied. The parties may appeal"
        );
    }

    #[test]
    fn words_used_normally_stay_words() {
        for sentence in [
            "the limitations period expired",
            "We are within the limitations period.",
            "the grace period ends Friday",
            "The period of limitations has run.",
            "a period of time",
            "during the period",
            "over a 30-day period.",
            "over a 30 day period.",
            "within the statutory period.",
            "the waiting period",
            "the colon was removed",
            "She has colon cancer",
            "a dash of salt",
            "a mad dash",
            "the comma after the clause",
            "a comma separated list",
            "the Oxford comma",
            "use a hyphen here",
            "put an ellipsis there",
            "the question mark",
            "a full stop",
            "an open quote",
            "the section symbol",
            "that paragraph symbol",
            "a literal reading of the statute",
            "the literal meaning",
            "Take it literally",
            "Please open the quote from the deposition",
            "the section on symbols",
        ] {
            assert_eq!(apply(sentence), sentence, "{sentence}");
        }
    }

    #[test]
    fn the_known_tradeoff_a_lowercase_run_stays_words() {
        assert_eq!(
            apply("the motion is denied period the parties may appeal"),
            "the motion is denied period the parties may appeal"
        );
    }

    #[test]
    fn literal_types_the_word_itself() {
        assert_eq!(
            apply("the word literal period here"),
            "the word period here"
        );
        assert_eq!(apply("put a literal comma there"), "put a comma there");
        assert_eq!(
            apply("say literal question mark now"),
            "say question mark now"
        );
        assert_eq!(apply("literal section symbol"), "section symbol");
        assert_eq!(apply("Literal period."), "Period.");
    }

    #[test]
    fn literal_also_protects_voice_commands() {
        assert_eq!(apply("literal new paragraph"), "new paragraph");
        assert_eq!(apply("Literal scratch that."), "Scratch that.");
    }

    #[test]
    fn literal_before_anything_else_stays_a_word() {
        assert_eq!(apply("literal truth"), "literal truth");
        assert_eq!(
            apply("a literal interpretation"),
            "a literal interpretation"
        );
    }

    #[test]
    fn legal_symbols_read_correctly_in_context() {
        assert_eq!(
            apply("see 42 U.S.C. section symbol 1983 comma paragraph symbol 12"),
            "see 42 U.S.C. § 1983, ¶ 12"
        );
    }

    #[test]
    fn marks_next_to_each_other() {
        assert_eq!(
            apply("open paren see below close paren period"),
            "(see below)."
        );
        assert_eq!(
            apply("he said open quote denied period close quote"),
            "he said “denied.”"
        );
    }

    #[test]
    fn a_sentence_end_the_model_heard_after_a_closing_mark_stays() {
        assert_eq!(
            apply("He said open quote we agree close quote. the end"),
            "He said “we agree.” The end"
        );
        assert_eq!(
            apply("the buyer open paren Acme close paren. the end"),
            "the buyer (Acme). The end"
        );
        assert_eq!(
            apply("He said open quote stop close quote, today"),
            "He said “stop” today"
        );
    }

    #[test]
    fn plain_text_is_unchanged_apart_from_spacing() {
        assert_eq!(apply("  The court   held that. "), "The court held that.");
        assert_eq!(apply(""), "");
    }
}
