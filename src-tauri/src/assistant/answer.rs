//! What a question asks of its answer, as plain rules: whether it should be
//! typed into the document ("…and type it"), whether to explain step by step
//! (tutor style), and the instructions those add to the system prompt.

use serde::{Deserialize, Serialize};
use specta::Type;

/// Where an answer goes (the "Answer goes to" setting).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum AnswerTo {
    /// Spoken (when replies are spoken) and shown in the Conversation tab.
    #[default]
    Speak,
    /// Typed into the window that was active when the question was asked.
    Type,
    Both,
}

/// What to do with one answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Delivery {
    pub speak: bool,
    pub type_it: bool,
}

/// `typed`: the question was typed, not spoken, so it's spoken only with
/// `speak_typed`. A question that ends "…and type it" is typed whatever the
/// setting, and then spoken only if the setting says both.
pub fn delivery(
    setting: AnswerTo,
    asked_to_type: bool,
    typed: bool,
    speak_typed: bool,
) -> Delivery {
    let (speak, type_it) = match (setting, asked_to_type) {
        (AnswerTo::Both, _) => (true, true),
        (_, true) | (AnswerTo::Type, false) => (false, true),
        (AnswerTo::Speak, false) => (true, false),
    };
    Delivery {
        speak: speak && (!typed || speak_typed),
        type_it,
    }
}

/// A question with what it asks of its answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    /// The question without an "…and type it" ending.
    pub question: String,
    /// Ended with "…and type it", "…put it in the document" and the like.
    pub type_it: bool,
    /// Said "step by step" (tutor style for this question).
    pub step_by_step: bool,
}

/// Endings that ask for the answer to be typed. Each has to follow a joining
/// word or a pause ("and", "then", a comma or full stop), so "How do I type
/// it?" or "Where do I put it in the document?" stay questions.
const TYPE_ENDINGS: &[&str] = &[
    "type it",
    "type that",
    "type it in",
    "type that in",
    "type it out",
    "type that out",
    "type it for me",
    "type it here",
    "type the answer",
    "paste it",
    "paste that",
    "paste it in",
    "insert it",
    "insert that",
    "put it in the document",
    "put that in the document",
    "put it in my document",
    "put it in the doc",
    "put it into the document",
    "add it to the document",
    "insert it into the document",
    "insert it in the document",
    "write it in the document",
    "write it into the document",
];

const JOINS: &[&str] = &["and then", "and", "then", "please"];

fn words(text: &str) -> Vec<String> {
    text.split(|c: char| !(c.is_alphanumeric() || c == '\''))
        .filter(|w| !w.is_empty())
        .map(|w| w.to_lowercase())
        .collect()
}

/// Byte offset where the last `count` words of `text` start.
fn start_of_last_words(text: &str, count: usize) -> usize {
    let mut seen = 0;
    let mut in_word = false;
    for (i, c) in text.char_indices().rev() {
        let word_char = c.is_alphanumeric() || c == '\'';
        if word_char && !in_word {
            in_word = true;
        } else if !word_char && in_word {
            in_word = false;
            seen += 1;
            if seen == count {
                return i + c.len_utf8();
            }
        }
    }
    0
}

/// Splits off an "…and type it" ending.
fn strip_type_ending(text: &str) -> Option<String> {
    let trimmed = text.trim_end_matches(|c: char| c.is_whitespace() || ".!?".contains(c));
    let all = words(trimmed);
    let mut best: Option<usize> = None;
    for ending in TYPE_ENDINGS {
        let ending = words(ending);
        if all.len() <= ending.len() || !all.ends_with(&ending) {
            continue;
        }
        let before = &all[..all.len() - ending.len()];
        let rest = &trimmed[..start_of_last_words(trimmed, ending.len())];
        let joined = JOINS.iter().map(|j| words(j)).find(|j| before.ends_with(j));
        let paused = rest
            .trim_end()
            .ends_with([',', '.', ';', ':', '-', '\u{2014}']);
        let cut = match joined {
            Some(join) if before.len() > join.len() => join.len() + ending.len(),
            _ if paused => ending.len(),
            _ => continue,
        };
        best = Some(best.map_or(cut, |b: usize| b.max(cut)));
    }
    let cut = best?;
    let question = trimmed[..start_of_last_words(trimmed, cut)]
        .trim_end_matches(|c: char| c.is_whitespace() || ",;:-\u{2014}".contains(c))
        .to_string();
    (!question.is_empty()).then_some(question)
}

/// "step by step" (or "step-by-step") anywhere in the question.
fn asks_step_by_step(text: &str) -> bool {
    let all = words(&text.replace('-', " "));
    all.windows(3).any(|w| w == ["step", "by", "step"])
}

pub fn parse(question: &str) -> Request {
    let question = question.trim();
    let (question, type_it) = match strip_type_ending(question) {
        Some(rest) => (rest, true),
        None => (question.to_string(), false),
    };
    Request {
        step_by_step: asks_step_by_step(&question),
        question,
        type_it,
    }
}

/// Added when the answer is typed into a document.
pub const DOCUMENT_PROMPT: &str = "This answer will be typed straight into the user's document. \
Reply with only the text to insert, ready to use as it is: no introduction (\"Sure, here's...\"), \
no closing remarks or offers, no quotation marks around it, and no markdown unless they ask. \
Ignore the earlier advice about spoken sentences when it would make the text worse.";

/// Added in tutor style.
pub const TUTOR_PROMPT: &str = "Be a patient tutor. Explain step by step, one step at a time, \
in plain words. Check understanding by ending with one short question that lets the user try the \
next step themselves. Don't just hand over the final answer to homework, test or exam questions; \
guide them to it, unless they clearly ask for the answer itself.";

/// The extra instructions for one question.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Extras<'a> {
    pub document: bool,
    pub tutor: bool,
    /// The style for the app the answer is typed into.
    pub style: Option<&'a str>,
}

/// `base` (the conversation's prompt) with the question's extras.
pub fn compose(base: &str, extras: &Extras) -> String {
    let mut prompt = base.to_string();
    if extras.tutor {
        prompt = format!("{prompt} {TUTOR_PROMPT}");
    }
    if extras.document {
        prompt = format!("{prompt} {DOCUMENT_PROMPT}");
        if let Some(style) = extras.style {
            prompt = format!(
                "{prompt} {}",
                crate::app_styles::instruction_for_prompt(style)
            );
        }
    }
    prompt
}

/// Tutor style applies to this question.
pub fn tutor(setting: bool, request: &Request) -> bool {
    setting || request.step_by_step
}

#[cfg(test)]
mod tests {
    use super::*;

    fn typed(question: &str) -> Option<String> {
        let request = parse(question);
        request.type_it.then_some(request.question)
    }

    #[test]
    fn type_endings_are_found_and_removed() {
        for (said, question) in [
            (
                "Draft a thank-you note to Sam and type it",
                "Draft a thank-you note to Sam",
            ),
            (
                "Summarise the meeting, and type it.",
                "Summarise the meeting",
            ),
            ("Write a two line bio then type it", "Write a two line bio"),
            (
                "Write a two line bio and then type it in",
                "Write a two line bio",
            ),
            (
                "Give me a subject line. Type it.",
                "Give me a subject line.",
            ),
            (
                "Translate this into French and put it in the document",
                "Translate this into French",
            ),
            (
                "List three risks, put it in the document",
                "List three risks",
            ),
            (
                "Reword my opening line and paste it",
                "Reword my opening line",
            ),
            (
                "Write the closing paragraph and insert it into the document",
                "Write the closing paragraph",
            ),
            (
                "Draft a reply and type it out please",
                "Draft a reply and type it out please",
            ),
        ] {
            if said.ends_with("please") {
                // "…type it out please" isn't one of the endings: left alone.
                assert_eq!(typed(said), None, "{said}");
            } else {
                assert_eq!(typed(said).as_deref(), Some(question), "{said}");
            }
        }
    }

    #[test]
    fn ordinary_questions_about_typing_are_not_misread() {
        for question in [
            "What type of contract is this?",
            "How do I type an em dash?",
            "How do I type it?",
            "Can you type faster than me?",
            "What's the best way to type it?",
            "Where do I put it in the document?",
            "Should I paste it?",
            "Why won't Word let me insert it?",
            "Which blood type is the universal donor?",
            "Type it",
            "and type it",
            "Is typing it better than writing it by hand?",
        ] {
            let request = parse(question);
            assert!(!request.type_it, "{question}");
            assert_eq!(request.question, question.trim());
        }
    }

    #[test]
    fn step_by_step_is_heard_anywhere() {
        assert!(parse("Explain step by step how to factor x squared minus 4").step_by_step);
        assert!(parse("Walk me through it step-by-step").step_by_step);
        assert!(parse("Solve this step by step and type it").step_by_step);
        assert!(!parse("What's the next step?").step_by_step);
        assert!(!parse("Step on it").step_by_step);
    }

    #[test]
    fn where_the_answer_goes() {
        let speak = AnswerTo::Speak;
        // Spoken question, default setting: spoken only.
        assert_eq!(
            delivery(speak, false, false, false),
            Delivery {
                speak: true,
                type_it: false
            }
        );
        // "…and type it" types it, and doesn't also read it out.
        assert_eq!(
            delivery(speak, true, false, false),
            Delivery {
                speak: false,
                type_it: true
            }
        );
        assert_eq!(
            delivery(AnswerTo::Type, false, false, false),
            Delivery {
                speak: false,
                type_it: true
            }
        );
        assert_eq!(
            delivery(AnswerTo::Both, true, false, false),
            Delivery {
                speak: true,
                type_it: true
            }
        );
        // Typed questions are spoken only with their own setting on.
        assert_eq!(
            delivery(speak, false, true, false),
            Delivery {
                speak: false,
                type_it: false
            }
        );
        assert_eq!(
            delivery(speak, false, true, true),
            Delivery {
                speak: true,
                type_it: false
            }
        );
        assert_eq!(
            delivery(AnswerTo::Both, false, true, false),
            Delivery {
                speak: false,
                type_it: true
            }
        );
    }

    #[test]
    fn tutor_style_is_chosen_by_the_setting_or_the_question() {
        let plain = parse("What is photosynthesis?");
        let stepwise = parse("Explain photosynthesis step by step");
        assert!(!tutor(false, &plain));
        assert!(tutor(true, &plain));
        assert!(tutor(false, &stepwise));

        let base = "Base.";
        let prompt = compose(
            base,
            &Extras {
                tutor: true,
                ..Default::default()
            },
        );
        assert!(prompt.starts_with(base));
        assert!(prompt.contains("step by step"));
        assert!(prompt.contains("Don't just hand over the final answer"));
        assert_eq!(compose(base, &Extras::default()), base);
    }

    #[test]
    fn document_answers_have_no_preamble_and_take_the_app_style() {
        let prompt = compose(
            "Base.",
            &Extras {
                document: true,
                tutor: false,
                style: Some("Write it as an email."),
            },
        );
        assert!(prompt.contains("only the text to insert"));
        assert!(prompt.contains("no introduction"));
        assert!(prompt.ends_with("Write it as an email."));
        // The style only matters for text that's typed.
        let spoken = compose(
            "Base.",
            &Extras {
                document: false,
                tutor: false,
                style: Some("Email."),
            },
        );
        assert_eq!(spoken, "Base.");
    }
}
