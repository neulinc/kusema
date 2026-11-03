//! Spoken editing and navigation commands, said alone. Every phrase is in
//! `PHRASES` (and `select last <n> words` / `select <phrase>` below): the one
//! place to add or translate them. Like "scratch that", a command counts only
//! when it's all that was said, so sentences that contain these words are
//! typed as words.

use serde::Serialize;
use specta::Type;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EditCommand {
    /// Select this many words before the caret.
    SelectLastWords(u32),
    SelectLine,
    SelectAll,
    LineStart,
    LineEnd,
    DocumentStart,
    DocumentEnd,
    NewParagraph,
    NewLine,
    DeleteThat,
    UndoThat,
    CopyThat,
    CutThat,
    PasteThat,
    /// Opens the last dictated text to correct, and learns from the fix.
    CorrectThat,
    SelectLastSentence,
    SelectLastParagraph,
    /// "select <phrase>": the nearest match before the caret. Only with the
    /// editing hotkey: in continuous dictation, "Select the best option." is a
    /// sentence.
    SelectPhrase(String),
}

/// Groups for the "What can I say?" list.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum CommandGroup {
    Select,
    Move,
    Edit,
}

/// One row of the "What can I say?" list: a stable id for its translated
/// description, and the English phrases that say it.
#[derive(Clone, Debug, Serialize, Type)]
pub struct CommandInfo {
    pub id: &'static str,
    pub group: CommandGroup,
    pub phrases: Vec<&'static str>,
    /// Works in continuous dictation too, not only with the editing hotkey.
    pub in_dictation: bool,
}

/// Every fixed phrase, lowercase, without punctuation and without "the" (it's
/// dropped from what was said before matching, so "go to the end of the line"
/// and "go to end of line" are the same).
const PHRASES: &[(&str, &str, Fixed)] = &[
    ("selectLastWord", "select last word", Fixed::SelectLastWord),
    (
        "selectLastWord",
        "select previous word",
        Fixed::SelectLastWord,
    ),
    ("selectLine", "select line", Fixed::SelectLine),
    ("selectLine", "select this line", Fixed::SelectLine),
    ("selectAll", "select all", Fixed::SelectAll),
    ("selectAll", "select everything", Fixed::SelectAll),
    (
        "selectLastSentence",
        "select last sentence",
        Fixed::SelectLastSentence,
    ),
    (
        "selectLastParagraph",
        "select last paragraph",
        Fixed::SelectLastParagraph,
    ),
    ("lineStart", "go to start of line", Fixed::LineStart),
    ("lineStart", "go to beginning of line", Fixed::LineStart),
    ("lineEnd", "go to end of line", Fixed::LineEnd),
    (
        "documentStart",
        "go to start of document",
        Fixed::DocumentStart,
    ),
    (
        "documentStart",
        "go to beginning of document",
        Fixed::DocumentStart,
    ),
    (
        "documentStart",
        "go to top of document",
        Fixed::DocumentStart,
    ),
    ("documentEnd", "go to end of document", Fixed::DocumentEnd),
    (
        "documentEnd",
        "go to bottom of document",
        Fixed::DocumentEnd,
    ),
    ("newParagraph", "new paragraph", Fixed::NewParagraph),
    ("newLine", "new line", Fixed::NewLine),
    ("newLine", "newline", Fixed::NewLine),
    ("deleteThat", "delete that", Fixed::DeleteThat),
    ("undoThat", "undo that", Fixed::UndoThat),
    ("copyThat", "copy that", Fixed::CopyThat),
    ("cutThat", "cut that", Fixed::CutThat),
    ("pasteThat", "paste that", Fixed::PasteThat),
    ("correctThat", "correct that", Fixed::CorrectThat),
];

/// The open-ended forms, for the list. Parsed in `parse_hotkey`.
const SELECT_WORDS_PHRASE: &str = "select last <number> words";
const SELECT_PHRASE_PHRASE: &str = "select <words>";
/// Larger counts are almost surely misheard.
const MAX_WORDS: u32 = 50;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Fixed {
    SelectLastWord,
    SelectLine,
    SelectAll,
    SelectLastSentence,
    SelectLastParagraph,
    LineStart,
    LineEnd,
    DocumentStart,
    DocumentEnd,
    NewParagraph,
    NewLine,
    DeleteThat,
    UndoThat,
    CopyThat,
    CutThat,
    PasteThat,
    CorrectThat,
}

impl Fixed {
    fn command(self) -> EditCommand {
        match self {
            Fixed::SelectLastWord => EditCommand::SelectLastWords(1),
            Fixed::SelectLine => EditCommand::SelectLine,
            Fixed::SelectAll => EditCommand::SelectAll,
            Fixed::SelectLastSentence => EditCommand::SelectLastSentence,
            Fixed::SelectLastParagraph => EditCommand::SelectLastParagraph,
            Fixed::LineStart => EditCommand::LineStart,
            Fixed::LineEnd => EditCommand::LineEnd,
            Fixed::DocumentStart => EditCommand::DocumentStart,
            Fixed::DocumentEnd => EditCommand::DocumentEnd,
            Fixed::NewParagraph => EditCommand::NewParagraph,
            Fixed::NewLine => EditCommand::NewLine,
            Fixed::DeleteThat => EditCommand::DeleteThat,
            Fixed::UndoThat => EditCommand::UndoThat,
            Fixed::CopyThat => EditCommand::CopyThat,
            Fixed::CutThat => EditCommand::CutThat,
            Fixed::PasteThat => EditCommand::PasteThat,
            Fixed::CorrectThat => EditCommand::CorrectThat,
        }
    }

    fn group(self) -> CommandGroup {
        match self {
            Fixed::SelectLastWord
            | Fixed::SelectLine
            | Fixed::SelectAll
            | Fixed::SelectLastSentence
            | Fixed::SelectLastParagraph => CommandGroup::Select,
            Fixed::LineStart | Fixed::LineEnd | Fixed::DocumentStart | Fixed::DocumentEnd => {
                CommandGroup::Move
            }
            _ => CommandGroup::Edit,
        }
    }
}

/// Lowercase words without punctuation; an apostrophe stays inside a word.
fn words(text: &str) -> Vec<String> {
    text.split(|c: char| !(c.is_alphanumeric() || c == '\''))
        .map(|w| w.trim_matches('\'').to_lowercase())
        .filter(|w| !w.is_empty())
        .collect()
}

fn without_the(words: &[String]) -> String {
    words
        .iter()
        .filter(|w| *w != "the")
        .cloned()
        .collect::<Vec<_>>()
        .join(" ")
}

fn number(word: &str) -> Option<u32> {
    const WORDS: [&str; 21] = [
        "zero",
        "one",
        "two",
        "three",
        "four",
        "five",
        "six",
        "seven",
        "eight",
        "nine",
        "ten",
        "eleven",
        "twelve",
        "thirteen",
        "fourteen",
        "fifteen",
        "sixteen",
        "seventeen",
        "eighteen",
        "nineteen",
        "twenty",
    ];
    word.parse::<u32>()
        .ok()
        .or_else(|| WORDS.iter().position(|w| *w == word).map(|n| n as u32))
        .filter(|n| (1..=MAX_WORDS).contains(n))
}

/// A fixed phrase or "select last <n> words".
fn parse_fixed(said: &[String]) -> Option<EditCommand> {
    let spoken = without_the(said);
    if let Some((_, _, fixed)) = PHRASES.iter().find(|(_, phrase, _)| *phrase == spoken) {
        return Some(fixed.command());
    }
    // "select last three words", "select the last 3 words"
    let rest: Vec<&str> = spoken.split(' ').collect();
    if let ["select", "last" | "previous", n, "words" | "word"] = rest.as_slice() {
        return number(n).map(EditCommand::SelectLastWords);
    }
    None
}

/// What was said with the editing hotkey: any command, including
/// "select <phrase>". `None` means it's an instruction for the AI.
pub fn parse_hotkey(text: &str) -> Option<EditCommand> {
    let said = words(text);
    if said.is_empty() {
        return None;
    }
    if let Some(command) = parse_fixed(&said) {
        return Some(command);
    }
    if said[0] == "select" && said.len() > 1 {
        let phrase = phrase_after_select(text);
        if !phrase.is_empty() {
            return Some(EditCommand::SelectPhrase(phrase));
        }
    }
    None
}

/// A whole continuous dictation segment that's a command. "new paragraph" and
/// "new line" stay with dictation's own commands, and "select <phrase>" isn't
/// one here.
pub fn parse_in_dictation(segment: &str) -> Option<EditCommand> {
    match parse_fixed(&words(segment))? {
        EditCommand::NewParagraph | EditCommand::NewLine => None,
        command => Some(command),
    }
}

/// The words after "select", as said: case kept, the model's closing
/// punctuation and surrounding quotes dropped.
fn phrase_after_select(text: &str) -> String {
    let text = text.trim();
    let after = text
        .char_indices()
        .find(|(_, c)| c.is_whitespace() || *c == ',' || *c == ':')
        .map(|(i, _)| &text[i..])
        .unwrap_or("");
    after
        .trim_start_matches(|c: char| c.is_whitespace() || c == ',' || c == ':')
        .trim_end_matches(|c: char| {
            matches!(c, '.' | '!' | '?' | ',' | ';' | ':') || c.is_whitespace()
        })
        .trim_matches(|c: char| matches!(c, '"' | '\u{201C}' | '\u{201D}' | '\''))
        .trim()
        .to_string()
}

/// The default word that starts an AI edit in continuous dictation.
pub const DEFAULT_TRIGGER: &str = "Kusema";

/// A continuous dictation segment that starts with the trigger word ("Kusema,
/// make this more formal"): what follows it, as said. `None` for anything
/// else, which is typed as dictated. Case and the punctuation models put after
/// a name ("Kusema," "Kusema:") don't matter; the trigger may be several words.
pub fn parse_trigger(segment: &str, trigger: &str) -> Option<String> {
    let wanted = words(trigger);
    if wanted.is_empty() {
        return None;
    }
    // Each word of the segment, with where it ends in the original.
    let mut found = 0;
    let mut end = 0;
    let mut chars = segment.char_indices().peekable();
    while found < wanted.len() {
        // Skip to the next word.
        while let Some(&(_, c)) = chars.peek() {
            if c.is_alphanumeric() {
                break;
            }
            chars.next();
        }
        let start = chars.peek()?.0;
        let mut stop = segment.len();
        while let Some(&(i, c)) = chars.peek() {
            if !(c.is_alphanumeric() || c == '\'') {
                stop = i;
                break;
            }
            chars.next();
        }
        if segment[start..stop].trim_matches('\'').to_lowercase() != wanted[found] {
            return None;
        }
        found += 1;
        end = stop;
    }
    let rest = segment[end..]
        .trim_start_matches(|c: char| {
            c.is_whitespace() || matches!(c, ',' | ':' | ';' | '.' | '!' | '-')
        })
        .trim();
    (rest.chars().any(char::is_alphanumeric)).then(|| rest.to_string())
}

/// Every command and its phrases, for the "What can I say?" list.
pub fn list() -> Vec<CommandInfo> {
    let mut rows: Vec<CommandInfo> = Vec::new();
    for (id, phrase, fixed) in PHRASES {
        let in_dictation = parse_in_dictation(phrase).is_some()
            || matches!(fixed, Fixed::NewParagraph | Fixed::NewLine);
        match rows.iter_mut().find(|row| row.id == *id) {
            Some(row) => row.phrases.push(phrase),
            None => rows.push(CommandInfo {
                id,
                group: fixed.group(),
                phrases: vec![phrase],
                in_dictation,
            }),
        }
        if *id == "selectLastWord" && !rows.iter().any(|r| r.id == "selectLastWords") {
            rows.push(CommandInfo {
                id: "selectLastWords",
                group: CommandGroup::Select,
                phrases: vec![SELECT_WORDS_PHRASE],
                in_dictation: true,
            });
        }
    }
    rows.push(CommandInfo {
        id: "selectPhrase",
        group: CommandGroup::Select,
        phrases: vec![SELECT_PHRASE_PHRASE],
        in_dictation: false,
    });
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use EditCommand::*;

    #[test]
    fn each_fixed_phrase_parses_on_its_own() {
        for (_, phrase, fixed) in PHRASES {
            assert_eq!(parse_hotkey(phrase), Some(fixed.command()), "{phrase}");
        }
    }

    #[test]
    fn the_case_punctuation_and_the_that_models_add_are_ignored() {
        assert_eq!(
            parse_hotkey("Select the last word."),
            Some(SelectLastWords(1))
        );
        assert_eq!(parse_hotkey("Go to the end of the line."), Some(LineEnd));
        assert_eq!(
            parse_hotkey("Go to the top of the document"),
            Some(DocumentStart)
        );
        assert_eq!(parse_hotkey("Undo that!"), Some(UndoThat));
        assert_eq!(parse_hotkey("  SELECT ALL  "), Some(SelectAll));
        assert_eq!(parse_hotkey("Delete, that."), Some(DeleteThat));
        assert_eq!(
            parse_hotkey("Select the last sentence."),
            Some(SelectLastSentence)
        );
    }

    #[test]
    fn counts_of_words_in_digits_or_words() {
        assert_eq!(
            parse_hotkey("select last 3 words"),
            Some(SelectLastWords(3))
        );
        assert_eq!(
            parse_hotkey("Select the last three words."),
            Some(SelectLastWords(3))
        );
        assert_eq!(
            parse_hotkey("select previous twelve words"),
            Some(SelectLastWords(12))
        );
        assert_eq!(
            parse_hotkey("select last one word"),
            Some(SelectLastWords(1))
        );
        // Out of range: not a count, so it's a phrase to find.
        assert_eq!(
            parse_hotkey("select last 0 words"),
            Some(SelectPhrase("last 0 words".into()))
        );
        assert!(matches!(
            parse_hotkey("select last 500 words"),
            Some(SelectPhrase(_))
        ));
    }

    #[test]
    fn select_followed_by_anything_else_finds_that_phrase() {
        assert_eq!(
            parse_hotkey("Select the motion is denied."),
            Some(SelectPhrase("the motion is denied".into()))
        );
        assert_eq!(
            parse_hotkey("select \u{201C}Jane Doe\u{201D}"),
            Some(SelectPhrase("Jane Doe".into()))
        );
        assert_eq!(
            parse_hotkey("Select: section 4(b)."),
            Some(SelectPhrase("section 4(b)".into()))
        );
        assert_eq!(parse_hotkey("select"), None);
        assert_eq!(parse_hotkey("Select."), None);
    }

    #[test]
    fn instructions_for_the_ai_are_not_commands() {
        for instruction in [
            "make this more formal",
            "shorten to two sentences",
            "turn this into bullet points",
            "fix the grammar",
            "translate to Spanish",
            "delete the last sentence and add a thank you",
            "copy that into a list",
            "",
        ] {
            assert_eq!(parse_hotkey(instruction), None, "{instruction}");
        }
    }

    #[test]
    fn in_dictation_only_whole_segments_count() {
        assert_eq!(
            parse_in_dictation("Select the last word."),
            Some(SelectLastWords(1))
        );
        assert_eq!(parse_in_dictation("Undo that."), Some(UndoThat));
        assert_eq!(
            parse_in_dictation("select last two words"),
            Some(SelectLastWords(2))
        );
        for sentence in [
            "Please delete that clause.",
            "We should undo that decision.",
            "Select the best option.",
            "select Jane Doe",
            "Copy that to the client",
            "go to the end of the document and sign",
            // Dictation's own commands.
            "new paragraph",
            "New line.",
        ] {
            assert_eq!(parse_in_dictation(sentence), None, "{sentence}");
        }
    }

    #[test]
    fn the_list_has_every_command_once_with_its_phrases() {
        let rows = list();
        let mut ids: Vec<&str> = rows.iter().map(|r| r.id).collect();
        ids.sort();
        let before = ids.len();
        ids.dedup();
        assert_eq!(before, ids.len(), "no duplicate rows");
        let line_start = rows.iter().find(|r| r.id == "lineStart").unwrap();
        assert_eq!(
            line_start.phrases,
            vec!["go to start of line", "go to beginning of line"]
        );
        assert!(line_start.in_dictation);
        assert!(
            !rows
                .iter()
                .find(|r| r.id == "selectPhrase")
                .unwrap()
                .in_dictation
        );
        assert!(
            rows.iter()
                .find(|r| r.id == "newParagraph")
                .unwrap()
                .in_dictation
        );
        // Every listed phrase that isn't a pattern parses.
        for row in &rows {
            for phrase in &row.phrases {
                if !phrase.contains('<') {
                    assert!(parse_hotkey(phrase).is_some(), "{phrase}");
                }
            }
        }
    }
    #[test]
    fn the_trigger_word_starts_an_ai_edit_in_dictation() {
        assert_eq!(
            parse_trigger("Kusema, make this more formal.", "Kusema").as_deref(),
            Some("make this more formal.")
        );
        assert_eq!(
            parse_trigger("kusema shorten that", "Kusema").as_deref(),
            Some("shorten that")
        );
        assert_eq!(
            parse_trigger("Kusema: select last word", "Kusema").as_deref(),
            Some("select last word")
        );
        // Any language after it, and a trigger of more than one word.
        assert_eq!(
            parse_trigger("Kusema, rends ce texte plus formel.", "Kusema").as_deref(),
            Some("rends ce texte plus formel.")
        );
        assert_eq!(
            parse_trigger("Hey Kusema, fix the grammar", "hey kusema").as_deref(),
            Some("fix the grammar")
        );
    }

    #[test]
    fn without_the_trigger_word_sentences_are_dictated() {
        for sentence in [
            "Make this more formal.",
            "We met Kusema, the new associate.",
            "Kusemas are great",
            "Kusema.",
            "Kusema,",
            "",
        ] {
            assert_eq!(parse_trigger(sentence, "Kusema"), None, "{sentence}");
        }
        assert_eq!(parse_trigger("Kusema, shorten that", ""), None);
        assert_eq!(parse_trigger("Hey, fix it", "hey kusema"), None);
    }
}
