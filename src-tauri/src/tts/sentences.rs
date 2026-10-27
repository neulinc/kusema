//! Splits streamed reply text into sentences, so each can be spoken as soon
//! as it is complete instead of waiting for the whole reply.

/// Words that end in a period without ending the sentence.
const ABBREVIATIONS: &[&str] = &[
    "mr", "mrs", "ms", "dr", "prof", "st", "sr", "jr", "vs", "etc", "e.g", "i.e", "a.m", "p.m",
    "approx", "no",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Think {
    /// Not yet known whether the reply opens with a `<think>` block.
    Undecided,
    Inside,
    Done,
}

#[derive(Debug)]
pub struct SentenceSplitter {
    buf: String,
    think: Think,
}

impl Default for SentenceSplitter {
    fn default() -> Self {
        Self {
            buf: String::new(),
            think: Think::Undecided,
        }
    }
}

impl SentenceSplitter {
    /// Add streamed text; returns the sentences it completed, ready to speak.
    pub fn push(&mut self, delta: &str) -> Vec<String> {
        self.buf.push_str(delta);
        if !self.skip_think_block() {
            return Vec::new();
        }

        let mut sentences = Vec::new();
        while let Some(end) = find_boundary(&self.buf) {
            let sentence: String = self.buf.drain(..end).collect();
            sentences.extend(clean_for_speech(&sentence));
        }
        sentences
    }

    /// The text left once the stream has ended.
    pub fn finish(&mut self) -> Option<String> {
        if self.think == Think::Inside {
            return None;
        }
        clean_for_speech(&std::mem::take(&mut self.buf))
    }

    /// Some local models stream their reasoning as a leading `<think>` block;
    /// hold it back rather than read it out. Returns whether speakable text
    /// can be taken from the buffer.
    fn skip_think_block(&mut self) -> bool {
        const OPEN: &str = "<think>";
        const CLOSE: &str = "</think>";
        if self.think == Think::Undecided {
            let start = self.buf.trim_start();
            if start.starts_with(OPEN) {
                self.think = Think::Inside;
            } else if OPEN.starts_with(start) {
                return false; // empty, or a prefix of "<think>": wait for more
            } else {
                self.think = Think::Done;
            }
        }
        if self.think == Think::Inside {
            let Some(end) = self.buf.find(CLOSE) else {
                return false;
            };
            self.buf.drain(..end + CLOSE.len());
            self.think = Think::Done;
        }
        true
    }
}

/// Byte index just past the first complete sentence in `text`, if any.
fn find_boundary(text: &str) -> Option<usize> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    for (i, &(pos, c)) in chars.iter().enumerate() {
        let end = pos + c.len_utf8();
        match c {
            '\n' => {
                if text[..pos].trim().is_empty() {
                    continue;
                }
                return Some(end);
            }
            // CJK full-width punctuation ends a sentence without a space.
            '。' | '！' | '？' => return Some(end),
            '.' | '!' | '?' | '…' => {
                // Swallow closing quotes/brackets that belong to the sentence.
                let mut j = i + 1;
                while j < chars.len() && matches!(chars[j].1, '"' | '\'' | '”' | '’' | ')' | ']')
                {
                    j += 1;
                }
                // Undecided until the next character arrives ("2.5", "e.g.").
                let next = chars.get(j)?;
                if !next.1.is_whitespace() {
                    continue;
                }
                if c == '.' && !ends_sentence(&text[..pos]) {
                    continue;
                }
                return Some(next.0);
            }
            _ => {}
        }
    }
    None
}

/// Whether a period after `before` ends the sentence, rather than following
/// an abbreviation, an initial ("J. K."), or a list number ("1.").
fn ends_sentence(before: &str) -> bool {
    let word = before
        .rsplit(|c: char| c.is_whitespace() || c == '(')
        .next()
        .unwrap_or("")
        .to_lowercase();
    if word.is_empty() {
        return false;
    }
    if before.trim().chars().all(|c| c.is_ascii_digit()) {
        return false;
    }
    let mut letters = word.chars();
    if letters.next().is_some_and(char::is_alphabetic) && letters.next().is_none() {
        return false;
    }
    !ABBREVIATIONS.contains(&word.as_str())
}

/// Drop markdown symbols that would be read out literally, collapse
/// whitespace, and skip fragments with nothing to say.
pub fn clean_for_speech(text: &str) -> Option<String> {
    let mut out = String::with_capacity(text.len());
    for line in text.lines() {
        let line = line.trim_start();
        let line = line.trim_start_matches('#');
        let line = line
            .strip_prefix("- ")
            .or_else(|| line.strip_prefix("• "))
            .unwrap_or(line);
        if !out.is_empty() {
            out.push(' ');
        }
        out.extend(line.chars().filter(|c| !matches!(c, '*' | '`' | '_' | '~')));
    }
    let cleaned = out.split_whitespace().collect::<Vec<_>>().join(" ");
    cleaned
        .chars()
        .any(char::is_alphanumeric)
        .then_some(cleaned)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn split_all(chunks: &[&str]) -> Vec<String> {
        let mut splitter = SentenceSplitter::default();
        let mut out: Vec<String> = chunks.iter().flat_map(|c| splitter.push(c)).collect();
        out.extend(splitter.finish());
        out
    }

    #[test]
    fn sentences_are_released_as_soon_as_complete() {
        let mut splitter = SentenceSplitter::default();
        assert!(splitter.push("The capital of Ghana").is_empty());
        assert!(splitter.push(" is Accra.").is_empty()); // might be "Accra.com"
        assert_eq!(
            splitter.push(" It has"),
            vec!["The capital of Ghana is Accra."]
        );
        assert!(splitter.push(" about 2.5 million people").is_empty());
        assert_eq!(
            splitter.finish().as_deref(),
            Some("It has about 2.5 million people")
        );
    }

    #[test]
    fn question_and_exclamation_marks_split() {
        assert_eq!(
            split_all(&["Really? Yes! Great."]),
            vec!["Really?", "Yes!", "Great."]
        );
    }

    #[test]
    fn abbreviations_initials_and_numbers_do_not_split() {
        assert_eq!(
            split_all(&["Dr. Smith met J. K. Rowling at 3.30 p.m. today. Then left."]),
            vec![
                "Dr. Smith met J. K. Rowling at 3.30 p.m. today.",
                "Then left."
            ]
        );
        assert_eq!(
            split_all(&["1. First\n2. Second"]),
            vec!["1. First", "2. Second"]
        );
    }

    #[test]
    fn closing_quotes_stay_with_their_sentence() {
        assert_eq!(
            split_all(&["He said \"hello.\" Then he left."]),
            vec!["He said \"hello.\"", "Then he left."]
        );
    }

    #[test]
    fn cjk_punctuation_splits_without_spaces() {
        assert_eq!(
            split_all(&["你好。今天很好！"]),
            vec!["你好。", "今天很好！"]
        );
    }

    #[test]
    fn leading_think_block_is_not_spoken() {
        assert_eq!(
            split_all(&["<thi", "nk>Let me reason. Okay.</thi", "nk>\nAccra. Yes."]),
            vec!["Accra.", "Yes."]
        );
        let mut unfinished = SentenceSplitter::default();
        assert!(unfinished.push("<think>still thinking. More.").is_empty());
        assert_eq!(unfinished.finish(), None);
    }

    #[test]
    fn markdown_symbols_are_removed() {
        assert_eq!(
            split_all(&["## Answer\n- **Accra** is the `capital`.\n"]),
            vec!["Answer", "Accra is the capital."]
        );
    }

    #[test]
    fn fragments_without_words_are_skipped() {
        assert_eq!(clean_for_speech(" ** \n"), None);
        assert_eq!(split_all(&["---\n"]), Vec::<String>::new());
    }
}
