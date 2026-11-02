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
//! - "new paragraph" and "new line" count where "period" does, or said alone, and
//!   never after a word that makes them ordinary ("we added new paragraphs").
//!
//! The cost: "denied period the parties may appeal", all one lowercase run, stays
//! as words, and so does "denied new paragraph the parties may appeal". "literal
//! period" types the word; "scratch that" undoes a wrong guess.

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
    /// A line break, with no spaces around it.
    Break,
}

/// When a mark's words count as the mark.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Rule {
    /// Unless a determiner comes before it.
    Anywhere,
    /// The strict rule for "period".
    PhraseEnd,
    /// The same position rule, for "new paragraph" and "new line".
    LineCommand,
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

const fn line_command(
    words: &'static [&'static str],
    output: &'static str,
    capitalize_next: bool,
) -> Mark {
    Mark {
        rule: Rule::LineCommand,
        ends_sentence: capitalize_next,
        not_before: NOT_BEFORE_LINE_COMMAND,
        not_after: NOT_AFTER_LINE_COMMAND,
        ..mark(words, output, Spacing::Break)
    }
}

const PARAGRAPH_BREAK: &str = "\n\n";
const LINE_BREAK: &str = "\n";

/// Before a line command, these make it ordinary words ("insert new line").
const NOT_AFTER_LINE_COMMAND: &[&str] = &[
    "add",
    "adds",
    "added",
    "adding",
    "insert",
    "inserts",
    "inserted",
    "inserting",
    "start",
    "starts",
    "started",
    "starting",
    "begin",
    "begins",
    "began",
    "begun",
    "beginning",
    "write",
    "writes",
    "wrote",
    "written",
    "writing",
    "draft",
    "drafts",
    "drafted",
    "drafting",
    "create",
    "creates",
    "created",
    "creating",
    "include",
    "includes",
    "included",
    "including",
    "review",
    "reviews",
    "reviewed",
    "reviewing",
    "read",
    "reads",
    "reading",
    "see",
    "saw",
    "seen",
    "need",
    "needs",
    "needed",
    "want",
    "wants",
    "wanted",
    "draw",
    "draws",
    "drew",
    "drawn",
    "open",
    "opens",
    "opened",
    "launch",
    "launched",
    "in",
    "on",
    "of",
    "to",
    "for",
    "with",
    "by",
    "at",
    "from",
    "into",
    "per",
    "under",
    "and",
    "or",
    "as",
    "two",
    "three",
    "four",
    "five",
    "six",
    "seven",
    "eight",
    "nine",
    "ten",
    "several",
    "many",
    "few",
    "more",
    "both",
    "all",
    "brand",
    "whole",
    "entire",
    "own",
    "separate",
    "first",
    "second",
    "third",
    "next",
];

/// After a line command, these make it a name or a reference ("New Line Cinema").
const NOT_BEFORE_LINE_COMMAND: &[&str] = &[
    "cinema",
    "productions",
    "films",
    "entertainment",
    "communications",
    "one",
    "two",
    "three",
    "four",
    "five",
    "a",
    "b",
];

/// Every spoken mark. The one place to translate or extend them.
const MARKS: &[Mark] = &[
    line_command(&["new", "paragraph"], PARAGRAPH_BREAK, true),
    line_command(&["new", "paragraphs"], PARAGRAPH_BREAK, true),
    line_command(&["new", "para", "graph"], PARAGRAPH_BREAK, true),
    line_command(&["new-paragraph"], PARAGRAPH_BREAK, true),
    line_command(&["newparagraph"], PARAGRAPH_BREAK, true),
    line_command(&["new", "line"], LINE_BREAK, false),
    line_command(&["new", "lines"], LINE_BREAK, false),
    line_command(&["new-line"], LINE_BREAK, false),
    line_command(&["newline"], LINE_BREAK, false),
    line_command(&["newlines"], LINE_BREAK, false),
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
    mark(&["semi-colon"], ";", Spacing::AfterWord),
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
/// Models sometimes leave one of these after a word: `New paragraph"`.
const ADDED_QUOTES: &[char] = &['"', '\'', '”', '’'];

fn is_added_after_a_word(c: char) -> bool {
    ADDED_PUNCTUATION.contains(&c) || ADDED_QUOTES.contains(&c)
}

/// A token's word, lowercased and without punctuation the model added around it.
/// `None` if something else is attached, like a quote: then it's not a bare word.
fn bare_word(token: &str) -> Option<String> {
    let core = token.trim_end_matches(is_added_after_a_word);
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
                    && (is_last
                        || token.len() == token.trim_end_matches(is_added_after_a_word).len())
            }
            None => false,
        }
    })
}

/// A sentence ends here, even with a stray quote after the stop.
fn ends_with_stop(token: &str) -> bool {
    token
        .trim_end_matches(ADDED_QUOTES)
        .ends_with(SENTENCE_STOP)
}

fn starts_capitalized(token: &str) -> bool {
    token
        .chars()
        .find(|c| c.is_alphabetic())
        .is_some_and(char::is_uppercase)
}

/// Nothing but punctuation, like a lone "." the model wrote after a word.
fn is_only_added(token: &str) -> bool {
    token.chars().all(is_added_after_a_word)
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
    // Punctuation after the mark separates it from the word that would make it
    // ordinary: "new paragraph two" is a reference, "New paragraph. Two" isn't.
    let runs_on = last.len() == last.trim_end_matches(is_added_after_a_word).len();
    if runs_on && next.is_some_and(|n| mark.not_before.contains(&plain_word(n).as_str())) {
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
            // "denied period new paragraph" is how a paragraph ends.
            let before_line_command = MARKS
                .iter()
                .filter(|other| other.rule == Rule::LineCommand)
                .any(|other| is_mark_here(other, tokens, at + mark.words.len()));
            let at_phrase_end = next.is_none()
                || ends_with_stop(last)
                || before_closing_mark
                || before_line_command;
            let before_capital = next.is_some_and(starts_capitalized);
            at_phrase_end || before_capital
        }
        Rule::LineCommand => {
            if previous
                .as_deref()
                .is_some_and(|p| p.chars().all(|c| c.is_ascii_digit()))
            {
                return false;
            }
            // A comma counts here: the model heard a pause after the command.
            let at_phrase_end = next.is_none_or(is_only_added)
                || ends_with_stop(last)
                || last
                    .trim_end_matches(ADDED_QUOTES)
                    .ends_with([',', ';', ':']);
            at_phrase_end || next.is_some_and(starts_capitalized)
        }
    }
}

fn mark_at(tokens: &[&str], at: usize, line_commands: bool) -> Option<&'static Mark> {
    MARKS
        .iter()
        .filter(|mark| line_commands || mark.rule != Rule::LineCommand)
        .find(|mark| is_mark_here(mark, tokens, at))
}

/// The quote to put back after a mark that replaced `token`, if it closes one
/// that's open: `"denied period"` is `"denied."`. Otherwise it's a stray.
fn closing_quote(token: &str, written: &str) -> Option<char> {
    let quote = token.chars().last().filter(|c| matches!(c, '"' | '”'))?;
    let straight = written.matches('"').count();
    let open = written.matches('“').count() > written.matches('”').count();
    (straight % 2 == 1 || open).then_some(quote)
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
    /// A line break: no space follows, and a mark has nothing to attach to.
    Break,
}

impl Last {
    fn wants_space(self) -> bool {
        matches!(self, Last::Word | Last::Spaced)
    }
}

/// Turns spoken punctuation and line commands in `text` into marks and breaks.
pub fn apply(text: &str) -> String {
    convert(text, true)
}

/// The same, leaving "new paragraph" and "new line" as words.
pub fn apply_without_line_commands(text: &str) -> String {
    convert(text, false)
}

/// Never lowercases anything, and only uppercases the word after a mark that ends
/// a sentence or a new paragraph.
fn convert(text: &str, line_commands: bool) -> String {
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
                    if last.wants_space() {
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

        if let Some(mark) = mark_at(&tokens, at, line_commands) {
            if mark.spacing == Spacing::Break {
                out.push_str(mark.output);
                last = Last::Break;
                // After "denied period new line", a sentence still starts next.
                capitalize_next = capitalize_next || mark.ends_sentence;
                at += mark.words.len();
                // A stray "." the model wrote after the command goes with it.
                while tokens.get(at).copied().is_some_and(is_only_added) {
                    at += 1;
                }
                continue;
            }
            // "new paragraph period": there's no word left for the mark to follow.
            if last == Last::Break
                && matches!(mark.spacing, Spacing::AfterWord | Spacing::Joining)
                && mark.output != "”"
                && mark.output != ")"
            {
                at += mark.words.len();
                continue;
            }
            match mark.spacing {
                Spacing::AfterWord | Spacing::Joining => {
                    // The mark replaces whatever the model put there.
                    let kept = out.trim_end_matches(ADDED_PUNCTUATION).len();
                    if mark.output != "”" && mark.output != ")" {
                        out.truncate(kept);
                    }
                }
                Spacing::Opening | Spacing::Separate => {
                    if last.wants_space() {
                        out.push(' ');
                    }
                }
                Spacing::Break => unreachable!("handled above"),
            }
            // "close quote." : the model heard a sentence end there, so it stays,
            // inside a closing quote and after a closing paren, as US style has it.
            let last_token = tokens[at + mark.words.len() - 1];
            let spoken_stop = last_token
                .trim_end_matches(ADDED_QUOTES)
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
            if mark.output != "”" {
                let before = out.len() - mark.output.len();
                if let Some(quote) = closing_quote(last_token, &out[..before]) {
                    out.push(quote);
                }
            }
            last = match mark.spacing {
                Spacing::AfterWord | Spacing::Separate => Last::Spaced,
                Spacing::Opening | Spacing::Joining => Last::Glued,
                Spacing::Break => unreachable!("handled above"),
            };
            // An opening mark doesn't use up the capital a new paragraph is owed.
            capitalize_next = ends_sentence || (capitalize_next && last == Last::Glued);
            at += mark.words.len();
            continue;
        }

        if last.wants_space() {
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
    fn every_mark_tolerates_how_the_model_writes_it() {
        for (spoken, written) in [
            ("comma", ","),
            ("full stop", "."),
            ("period", "."),
            ("question mark", "?"),
            ("exclamation point", "!"),
            ("colon", ":"),
            ("semicolon", ";"),
            ("semi colon", ";"),
            ("semi-colon", ";"),
            ("ellipsis", "…"),
            ("hyphen", "-"),
            ("dash", "—"),
        ] {
            let upper = spoken.to_uppercase();
            let mut title = spoken.to_string();
            title[..1].make_ascii_uppercase();
            for variant in [
                spoken.to_string(),
                upper,
                title.clone(),
                format!("{title}."),
                format!("{title},"),
                format!("{title}!"),
                format!("{title}?"),
                format!("{title}\""),
                format!("{title}.\""),
            ] {
                let said = format!("We agree {variant}");
                let out = apply(&said);
                assert!(
                    out.starts_with(&format!("We agree{written}")),
                    "{said} became {out}"
                );
            }
        }
    }

    #[test]
    fn a_stray_quote_does_not_stop_a_mark_mid_sentence() {
        assert_eq!(apply("Dear Sir comma\" thank you"), "Dear Sir, thank you");
        assert_eq!(
            apply("The motion is denied period\" The parties agree"),
            "The motion is denied. The parties agree"
        );
    }

    #[test]
    fn a_real_closing_quote_stays_after_a_mark() {
        assert_eq!(apply("He said \"denied period\""), "He said \"denied.\"");
        assert_eq!(apply("He said “denied period”"), "He said “denied.”");
    }

    #[test]
    fn a_quoted_mark_stays_a_word() {
        for sentence in [
            "the word \"comma\" is spelled",
            "He said \"period\" twice",
            "the phrase \"new paragraph\" here",
        ] {
            assert_eq!(apply(sentence), sentence, "{sentence}");
        }
    }

    #[test]
    fn line_commands_alone_in_every_way_the_model_writes_them() {
        for said in [
            "new paragraph",
            "New paragraph",
            "New Paragraph",
            "NEW PARAGRAPH",
            "New paragraph.",
            "New paragraph,",
            "New paragraph!",
            "New paragraph\"",
            "New paragraph.\"",
            "New paragraph”",
            "new paragraphs",
            "New paragraphs.",
            "new para graph",
            "New para graph.",
            "new-paragraph",
            "New-paragraph.",
            "newparagraph",
            "Newparagraph.",
            "New paragraph .",
        ] {
            assert_eq!(apply(said), "\n\n", "{said}");
        }
        for said in [
            "new line",
            "New line",
            "New Line",
            "NEW LINE",
            "New line.",
            "New line,",
            "New line\"",
            "new lines",
            "New lines.",
            "new-line",
            "newline",
            "Newline.",
            "newlines",
        ] {
            assert_eq!(apply(said), "\n", "{said}");
        }
    }

    #[test]
    fn line_commands_at_the_end_of_a_phrase_or_before_a_capital() {
        for (said, written) in [
            ("Thank you new paragraph", "Thank you\n\n"),
            ("Thank you. New paragraph.", "Thank you.\n\n"),
            (
                "Thank you new paragraph The court agreed",
                "Thank you\n\nThe court agreed",
            ),
            (
                "Thank you. New paragraph. the court agreed",
                "Thank you.\n\nThe court agreed",
            ),
            (
                "Dear Sir, new paragraph, thank you",
                "Dear Sir,\n\nThank you",
            ),
            (
                "Thank you New paragraph\" We agree",
                "Thank you\n\nWe agree",
            ),
            (
                "Thank you new paragraphs The court",
                "Thank you\n\nThe court",
            ),
            (
                "Thank you new para graph. The court",
                "Thank you\n\nThe court",
            ),
            (
                "The motion is denied period new paragraph The parties agree",
                "The motion is denied.\n\nThe parties agree",
            ),
            (
                "The motion is denied period new paragraph. the parties agree",
                "The motion is denied.\n\nThe parties agree",
            ),
            ("New paragraph The court agreed", "\n\nThe court agreed"),
            ("New paragraph. open quote stop close quote", "\n\n“Stop”"),
            (
                "Main Street new line Springfield",
                "Main Street\nSpringfield",
            ),
            (
                "Main Street. New line. Springfield",
                "Main Street.\nSpringfield",
            ),
            (
                "Main Street, new line, apartment 4",
                "Main Street,\napartment 4",
            ),
            (
                "Main Street newline Springfield",
                "Main Street\nSpringfield",
            ),
            (
                "Main Street New line\" Springfield",
                "Main Street\nSpringfield",
            ),
            ("We agree period new line. the end", "We agree.\nThe end"),
        ] {
            assert_eq!(apply(said), written, "{said}");
        }
    }

    #[test]
    fn punctuation_separates_a_command_from_a_word_that_would_guard_it() {
        assert_eq!(apply("One. New paragraph. Two"), "One.\n\nTwo");
        assert_eq!(
            apply("New paragraph two says otherwise"),
            "New paragraph two says otherwise"
        );
        assert_eq!(apply("Re colon, cancer"), "Re: cancer");
        assert_eq!(apply("She has colon cancer"), "She has colon cancer");
    }

    #[test]
    fn only_a_paragraph_capitalizes_what_follows() {
        assert_eq!(apply("One. New paragraph. two"), "One.\n\nTwo");
        assert_eq!(apply("One, new line, two"), "One,\ntwo");
        assert_eq!(
            apply("Dear Sir, New paragraph, iPhone"),
            "Dear Sir,\n\nIPhone"
        );
    }

    #[test]
    fn no_stray_mark_after_a_line_command() {
        assert_eq!(apply("One new paragraph. Period."), "One\n\n");
        assert_eq!(apply("One new line. Comma."), "One\n");
    }

    #[test]
    fn line_commands_in_a_normal_sentence_stay_words() {
        for sentence in [
            "the new paragraph in the contract is fine",
            "The new paragraph in the contract is fine.",
            "the new paragraph.",
            "The new line.",
            "a new paragraph",
            "a new line",
            "We added a new paragraph.",
            "We added new paragraphs.",
            "Please add new paragraph.",
            "Insert new line.",
            "Start on a new line.",
            "Begin on new line.",
            "This new paragraph is fine.",
            "That new line.",
            "two new paragraphs.",
            "2 new lines.",
            "several new paragraphs.",
            "the new paragraphs are fine",
            "New paragraph in the contract is fine.",
            "New line of credit approved",
            "a new line of credit",
            "We signed a new line of credit.",
            "New Line Cinema released it",
            "New paragraph two says otherwise",
            "new lines of credit",
            "a brand new paragraph.",
            "the new para graph.",
            "No new line.",
        ] {
            assert_eq!(apply(sentence), sentence, "{sentence}");
        }
    }

    #[test]
    fn the_known_tradeoff_a_lowercase_run_keeps_the_command_as_words() {
        assert_eq!(
            apply("the motion is denied new paragraph the parties may appeal"),
            "the motion is denied new paragraph the parties may appeal"
        );
    }

    #[test]
    fn literal_types_a_line_command_in_every_form() {
        for (said, written) in [
            ("literal new paragraph", "new paragraph"),
            ("Literal new paragraph.", "New paragraph."),
            (
                "the words literal new paragraph here",
                "the words new paragraph here",
            ),
            ("literal new line", "new line"),
            ("Literal new line.", "New line."),
            ("literal new paragraphs", "new paragraphs"),
            ("literal new para graph", "new para graph"),
            ("literal newline", "newline"),
        ] {
            assert_eq!(apply(said), written, "{said}");
        }
    }

    #[test]
    fn line_commands_can_be_left_as_words() {
        assert_eq!(
            apply_without_line_commands("Thank you new paragraph The court comma"),
            "Thank you new paragraph The court,"
        );
    }

    #[test]
    fn plain_text_is_unchanged_apart_from_spacing() {
        assert_eq!(apply("  The court   held that. "), "The court held that.");
        assert_eq!(apply(""), "");
    }
}
