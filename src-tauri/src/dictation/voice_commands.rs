//! Spoken commands that act on a whole segment. A command counts only when the
//! segment is nothing but its phrase, so normal sentences containing these words
//! are always typed. "new paragraph" and "new line" also work mid-flow, and in
//! normal dictation, through `cleanup`.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VoiceCommand {
    NewParagraph,
    NewLine,
    ScratchThat,
    StopDictation,
}

/// Every phrase, lowercase and without punctuation. The one place to translate
/// or extend them. Models write some of them as one word.
const PHRASES: &[(&str, VoiceCommand)] = &[
    ("new paragraph", VoiceCommand::NewParagraph),
    ("new line", VoiceCommand::NewLine),
    ("newline", VoiceCommand::NewLine),
    ("scratch that", VoiceCommand::ScratchThat),
    ("stop dictation", VoiceCommand::StopDictation),
    ("stop dictating", VoiceCommand::StopDictation),
];

/// Every command phrase, for the list in `cleanup` that "literal" protects.
#[cfg(test)]
pub fn phrases() -> impl Iterator<Item = &'static str> {
    PHRASES.iter().map(|(phrase, _)| *phrase)
}

/// The command `segment` consists of, if any. Case and punctuation are ignored,
/// because models add them ("New paragraph.").
pub fn parse(segment: &str) -> Option<VoiceCommand> {
    let words: Vec<String> = segment
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
        .collect();
    let spoken = words.join(" ");
    PHRASES
        .iter()
        .find(|(phrase, _)| *phrase == spoken)
        .map(|(_, command)| *command)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_each_phrase_on_its_own() {
        assert_eq!(parse("new paragraph"), Some(VoiceCommand::NewParagraph));
        assert_eq!(parse("new line"), Some(VoiceCommand::NewLine));
        assert_eq!(parse("scratch that"), Some(VoiceCommand::ScratchThat));
        assert_eq!(parse("stop dictation"), Some(VoiceCommand::StopDictation));
    }

    #[test]
    fn ignores_the_case_and_punctuation_models_add() {
        assert_eq!(parse("New paragraph."), Some(VoiceCommand::NewParagraph));
        assert_eq!(parse("  NEW LINE!  "), Some(VoiceCommand::NewLine));
        assert_eq!(parse("Scratch that."), Some(VoiceCommand::ScratchThat));
        assert_eq!(parse("Stop, dictation."), Some(VoiceCommand::StopDictation));
        assert_eq!(parse("New-line"), Some(VoiceCommand::NewLine));
        assert_eq!(parse("Newline."), Some(VoiceCommand::NewLine));
    }

    #[test]
    fn sentences_containing_the_words_are_not_commands() {
        for sentence in [
            "The new paragraph in the contract is fine.",
            "Add a new paragraph",
            "new paragraph please",
            "We drew a new line in the sand.",
            "the new line",
            "Please scratch that clause from the lease.",
            "scratch that off the list",
            "Did you stop dictation yesterday?",
            "I will stop dictation now",
            "Scratch",
            "new",
        ] {
            assert_eq!(parse(sentence), None, "{sentence}");
        }
    }

    #[test]
    fn empty_text_is_not_a_command() {
        assert_eq!(parse(""), None);
        assert_eq!(parse(" . "), None);
    }
}
