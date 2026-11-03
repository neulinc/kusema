//! One rule for the AI, in normal and continuous dictation alike: speech
//! that starts with the trigger word ("Kusema, …") is routed to the
//! Assistant instead of being typed. Anything else, including a sentence that
//! mentions Kusema later on, is plain dictation.

use crate::voice_edit::commands::{self, EditCommand};

/// Where speech that started with the trigger word goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Route {
    /// A command that runs locally ("Kusema, select last word").
    Command(EditCommand),
    /// Change the selected (or last typed) text, with the preview card.
    Edit(String),
    /// Ask about the window that was in front.
    Screen(String),
    /// Ask Kusema. `about_selection`: the question is about the selected
    /// text, which goes with it.
    Ask {
        question: String,
        about_selection: bool,
    },
}

impl Route {
    /// For the pill: which of the three it is.
    pub fn kind(&self) -> &'static str {
        match self {
            Route::Command(_) | Route::Edit(_) => "edit",
            Route::Screen(_) => "ask_screen",
            Route::Ask { .. } => "ask",
        }
    }
}

/// Is there text the instruction could apply to?
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextHere {
    /// Something is selected.
    Selected,
    /// Nothing selected, but Kusema's last typed text is right before the
    /// caret, so an edit can use it ("scratch that"'s rules).
    LastTyped,
    Nothing,
}

/// What follows the trigger word, if `said` starts with it in any of its
/// `forms` (the word itself and how it's heard).
pub fn after_trigger(said: &str, forms: &[String]) -> Option<String> {
    forms
        .iter()
        .find_map(|form| commands::parse_trigger(said, form))
}

fn words(text: &str) -> Vec<String> {
    text.split(|c: char| !(c.is_alphanumeric() || c == '\''))
        .filter(|w| !w.is_empty())
        .map(|w| w.to_lowercase().replace('\u{2019}', "'"))
        .collect()
}

/// Phrases that mean "look at my screen".
const SCREEN_PHRASES: &[&[&str]] = &[
    &["my", "screen"],
    &["the", "screen"],
    &["on", "screen"],
    &["this", "window"],
    &["the", "window"],
    &["this", "screen"],
    &["what's", "on", "my"],
    &["what", "is", "on", "my"],
    &["look", "at", "this"],
    &["see", "my", "screen"],
];

fn contains_phrase(said: &[String], phrase: &[&str]) -> bool {
    said.windows(phrase.len())
        .any(|w| w.iter().zip(phrase).all(|(a, b)| a == b))
}

/// "Kusema, look at my screen", "what's on my screen", "what's in this
/// window", "look at this and tell me…".
pub fn is_screen(rest: &str) -> bool {
    let said = words(rest);
    SCREEN_PHRASES.iter().any(|p| contains_phrase(&said, p))
}

/// Verbs that start an instruction to change text.
const EDIT_VERBS: &[&str] = &[
    "make",
    "turn",
    "change",
    "convert",
    "shorten",
    "lengthen",
    "expand",
    "condense",
    "simplify",
    "rewrite",
    "rephrase",
    "reword",
    "translate",
    "fix",
    "correct",
    "proofread",
    "polish",
    "tidy",
    "clean",
    "format",
    "reformat",
    "capitalize",
    "capitalise",
    "punctuate",
    "replace",
    "remove",
    "add",
    "insert",
    "cut",
    "trim",
    "tighten",
    "soften",
    "split",
    "merge",
    "combine",
    "sort",
    "number",
    "bullet",
    "spell",
    "redo",
    "edit",
    "improve",
    "paraphrase",
    "put",
];

/// Words that start a question.
const QUESTION_WORDS: &[&str] = &[
    "what",
    "what's",
    "whats",
    "why",
    "how",
    "who",
    "who's",
    "whom",
    "whose",
    "where",
    "when",
    "which",
    "is",
    "are",
    "was",
    "were",
    "does",
    "do",
    "did",
    "can",
    "could",
    "should",
    "would",
    "will",
    "shall",
    "may",
    "might",
    "has",
    "have",
    "explain",
    "tell",
    "summarize",
    "summarise",
    "define",
    "describe",
    "list",
    "give",
    "compare",
    "suggest",
    "help",
    "please",
];

fn first_word(rest: &str) -> Option<String> {
    words(rest).into_iter().next()
}

/// An instruction to change text ("make this more formal", "translate to
/// Spanish").
pub fn is_edit_instruction(rest: &str) -> bool {
    first_word(rest).is_some_and(|w| EDIT_VERBS.contains(&w.as_str()))
}

/// A question ("what does this mean?", "explain this clause").
pub fn is_question(rest: &str) -> bool {
    rest.trim_end().ends_with('?')
        || first_word(rest).is_some_and(|w| QUESTION_WORDS.contains(&w.as_str()))
}

/// Where `rest` (what followed the trigger word) goes. A local command first;
/// then the screen; then, with text to work on, an edit unless it's plainly a
/// question about that text; without, a question unless it's plainly an
/// instruction to change text (which then uses the last typed text, or asks
/// for a selection).
pub fn route(rest: &str, text: TextHere) -> Route {
    if let Some(command) = commands::parse_hotkey(rest) {
        return Route::Command(command);
    }
    if is_screen(rest) {
        return Route::Screen(rest.to_string());
    }
    let selected = text == TextHere::Selected;
    if is_edit_instruction(rest) {
        return Route::Edit(rest.to_string());
    }
    if is_question(rest) {
        return Route::Ask {
            question: rest.to_string(),
            about_selection: selected,
        };
    }
    match text {
        TextHere::Selected | TextHere::LastTyped => Route::Edit(rest.to_string()),
        TextHere::Nothing => Route::Ask {
            question: rest.to_string(),
            about_selection: false,
        },
    }
}

/// A question about the selection, with the selection attached as content.
pub fn question_about(question: &str, selection: &str) -> String {
    format!("{question}\n\nThe text I've selected:\n\"\"\"\n{selection}\n\"\"\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn forms() -> Vec<String> {
        vec!["Kusema".into(), "Kusama".into(), "Cusema".into()]
    }

    fn routed(said: &str, text: TextHere) -> Option<Route> {
        after_trigger(said, &forms()).map(|rest| route(&rest, text))
    }

    fn ask(q: &str, about_selection: bool) -> Option<Route> {
        Some(Route::Ask {
            question: q.into(),
            about_selection,
        })
    }

    #[test]
    fn a_question_goes_to_ask_kusema() {
        assert_eq!(
            routed("Kusema, what's the capital of Ghana?", TextHere::Nothing),
            ask("what's the capital of Ghana?", false)
        );
        assert_eq!(
            routed("Kusema how long is a fortnight", TextHere::Nothing),
            ask("how long is a fortnight", false)
        );
        // Without text to change, anything that isn't an instruction is asked.
        assert_eq!(
            routed("Kusema: the boiling point of water", TextHere::Nothing),
            ask("the boiling point of water", false)
        );
    }

    #[test]
    fn the_screen_goes_to_ask_about_my_screen() {
        for said in [
            "Kusema, look at my screen and tell me what's wrong",
            "Kusema, what's on my screen?",
            "Kusema, summarize this window",
            "Kusema, what do you think of this window",
            "Kusema, read the screen",
        ] {
            assert!(
                matches!(routed(said, TextHere::Selected), Some(Route::Screen(_))),
                "{said}"
            );
        }
    }

    #[test]
    fn instructions_with_text_are_edits() {
        for said in [
            "Kusema, make this more formal",
            "Kusema, shorten that",
            "Kusema, rewrite it as a list",
            "Kusema, translate to Spanish",
            "Kusema, fix the grammar",
        ] {
            assert_eq!(
                routed(said, TextHere::Selected).map(|r| r.kind()),
                Some("edit"),
                "{said}"
            );
            // Nothing selected: still an edit, on the last typed text (or the
            // edit asks for a selection).
            assert_eq!(
                routed(said, TextHere::Nothing).map(|r| r.kind()),
                Some("edit")
            );
        }
        // With a selection, a plain phrase is an instruction too.
        assert_eq!(
            routed("Kusema, more formal please", TextHere::Selected),
            Some(Route::Edit("more formal please".into()))
        );
        assert_eq!(
            routed("Kusema, in plain English", TextHere::LastTyped),
            Some(Route::Edit("in plain English".into()))
        );
    }

    #[test]
    fn a_question_about_selected_text_is_a_question() {
        assert_eq!(
            routed("Kusema, what does this clause mean?", TextHere::Selected),
            ask("what does this clause mean?", true)
        );
        assert_eq!(
            routed("Kusema, explain this", TextHere::Selected),
            ask("explain this", true)
        );
        assert_eq!(
            routed("Kusema, is this paragraph too long", TextHere::Selected),
            ask("is this paragraph too long", true)
        );
        // Last typed text isn't a selection: it isn't sent with a question.
        assert_eq!(
            routed("Kusema, what does this mean?", TextHere::LastTyped),
            ask("what does this mean?", false)
        );
        let full = question_about("what does this mean?", "res ipsa loquitur");
        assert!(full.starts_with("what does this mean?"));
        assert!(full.contains("\"\"\"\nres ipsa loquitur\n\"\"\""));
    }

    #[test]
    fn commands_run_locally() {
        assert_eq!(
            routed("Kusema, select last word", TextHere::Nothing),
            Some(Route::Command(EditCommand::SelectLastWords(1)))
        );
        assert!(matches!(
            routed("Kusema, undo that", TextHere::Selected),
            Some(Route::Command(_))
        ));
    }

    #[test]
    fn without_the_trigger_word_it_is_dictation() {
        for said in [
            "What's the capital of Ghana?",
            "Make this more formal",
            "I told Kusema to look at my screen",
            "Thanks to Kusema, the letter went out on time.",
            "We met at the Kusema office",
            "Kusemas are great",
            "",
            "Kusema",
            "Kusema.",
        ] {
            assert_eq!(routed(said, TextHere::Selected), None, "{said}");
        }
    }

    #[test]
    fn the_trigger_is_heard_in_its_spoken_forms() {
        assert_eq!(
            routed("Kusama, what time is it in Tokyo?", TextHere::Nothing),
            ask("what time is it in Tokyo?", false)
        );
        assert_eq!(
            routed("cusema make this shorter", TextHere::Selected).map(|r| r.kind()),
            Some("edit")
        );
    }
}
