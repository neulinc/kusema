//! Pure text rules for voice editing: finding the last sentence or paragraph
//! before the caret, comparing what an app reports with what Kusema typed,
//! and which apps keep rich formatting that a replacement would lose.

/// Words a period after doesn't end a sentence ("Mr. Smith", "Smith v. Jones").
const ABBREVIATIONS: &[&str] = &[
    "mr", "mrs", "ms", "dr", "prof", "st", "v", "vs", "inc", "ltd", "co", "corp", "jr", "sr",
    "e.g", "i.e", "u.s", "cf", "al",
];
/// The same, but only before a number ("No. 12", "Art. 5"): "He said no." ends one.
const NUMBERED: &[&str] = &["no", "art", "sec", "para", "p", "pp"];

/// Closing marks that belong to the sentence before them.
fn is_closer(c: char) -> bool {
    matches!(c, '"' | '\'' | ')' | ']' | '\u{201D}' | '\u{2019}')
}

/// Whether the sentence end at byte `end` (just after the punctuation) is a
/// real one, not "Mr." or "v.".
fn ends_sentence(text: &str, punct_at: usize) -> bool {
    if &text[punct_at..punct_at + 1] != "." {
        return true;
    }
    let word: String = text[..punct_at]
        .chars()
        .rev()
        .take_while(|c| c.is_alphanumeric() || *c == '.')
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let word = word.to_lowercase();
    // A single capital is an initial ("John F. Kennedy").
    let initial = word.chars().count() == 1 && word.chars().all(char::is_alphabetic);
    let number_follows = text[punct_at + 1..]
        .trim_start()
        .starts_with(|c: char| c.is_ascii_digit());
    let numbered = number_follows && NUMBERED.contains(&word.as_str());
    !initial && !numbered && !ABBREVIATIONS.contains(&word.as_str())
}

/// The last sentence in `before` (the text up to the caret), without the
/// whitespace around it. `None` if there's nothing but whitespace.
pub fn last_sentence(before: &str) -> Option<&str> {
    let text = before.trim_end();
    if text.is_empty() {
        return None;
    }
    // Skip the sentence's own closing punctuation and quotes.
    let mut end = text.len();
    for (i, c) in text.char_indices().rev() {
        if matches!(c, '.' | '!' | '?') || is_closer(c) {
            end = i;
        } else {
            break;
        }
    }
    let body = &text[..end];
    let mut start = 0;
    let indices: Vec<(usize, char)> = body.char_indices().collect();
    for (k, &(i, c)) in indices.iter().enumerate().rev() {
        if c == '\n' || c == '\r' {
            start = i + c.len_utf8();
            break;
        }
        let next_is_space = indices.get(k + 1).is_some_and(|(_, n)| n.is_whitespace());
        if matches!(c, '.' | '!' | '?') && next_is_space && ends_sentence(body, i) {
            start = i + 1;
            break;
        }
        // "end." followed by a closing quote, then a space.
        if is_closer(c) && next_is_space {
            if let Some(&(p, pc)) = k.checked_sub(1).and_then(|j| indices.get(j)) {
                if matches!(pc, '.' | '!' | '?') && ends_sentence(body, p) {
                    start = i + c.len_utf8();
                    break;
                }
            }
        }
    }
    let sentence = text[start..].trim();
    (!sentence.is_empty()).then_some(sentence)
}

/// The last `n` words in `before`, with the punctuation attached to them, so
/// "select last word" after "ten days." is "days.", not ".".
pub fn last_words(before: &str, n: usize) -> Option<&str> {
    let text = before.trim_end();
    if n == 0 || text.is_empty() {
        return None;
    }
    let mut start = text.len();
    let mut found = 0;
    let mut in_word = false;
    for (i, c) in text.char_indices().rev() {
        if c.is_whitespace() {
            if in_word {
                found += 1;
                in_word = false;
                if found == n {
                    break;
                }
            }
        } else {
            in_word = true;
            start = i;
        }
    }
    Some(&text[start..])
}

/// The last paragraph (line-break separated) in `before`, trimmed.
pub fn last_paragraph(before: &str) -> Option<&str> {
    let text = before.trim_end();
    let start = text.rfind(['\n', '\r']).map(|i| i + 1).unwrap_or(0);
    let paragraph = text[start..].trim();
    (!paragraph.is_empty()).then_some(paragraph)
}

/// Text as apps report it compared with text as typed: line breaks, curly
/// quotes (Word's AutoFormat) and non-breaking spaces don't count.
pub fn normalize(text: &str) -> String {
    text.replace("\r\n", "\n")
        .replace('\r', "\n")
        .chars()
        .map(|c| match c {
            '\u{201C}' | '\u{201D}' => '"',
            '\u{2018}' | '\u{2019}' => '\'',
            '\u{00A0}' => ' ',
            c => c,
        })
        .collect()
}

/// Whether `reported` (what the app says is selected) is `typed`, give or take
/// what `normalize` ignores and a trailing space some apps drop.
pub fn same_text(typed: &str, reported: &str) -> bool {
    let (typed, reported) = (normalize(typed), normalize(reported));
    typed == reported || typed.trim_end() == reported.trim_end() && !reported.trim().is_empty()
}

/// How many Shift+Left presses cover `text`: a line break is one step, even
/// as CR LF.
pub fn caret_steps(text: &str) -> usize {
    normalize(text).chars().count()
}

/// Apps that keep bold, styles and the like, which replacing the selection
/// with plain text loses. By process (lowercase file name) or, for rich edit
/// controls in other apps, the focused control's class.
pub fn keeps_rich_text(process: &str, class: &str) -> bool {
    const RICH_APPS: &[&str] = &[
        "winword.exe",
        "outlook.exe",
        "olk.exe",
        "onenote.exe",
        "onenoteim.exe",
        "powerpnt.exe",
        "wordpad.exe",
        "wordpadfilter.exe",
        "soffice.bin",
        "swriter.exe",
    ];
    let process = process.to_lowercase();
    if RICH_APPS.contains(&process.as_str()) {
        return true;
    }
    // Notepad is a rich edit control that only holds plain text.
    class.to_lowercase().contains("richedit") && process != "notepad.exe"
}

/// Apps that draw their own text and don't show it to Windows UI Automation,
/// so asking it would only waste time: the keyboard and the clipboard are the
/// way in. By process (lowercase file name) and window title.
///
/// Checked on this machine: ONLYOFFICE Desktop Editors (`editors.exe`): its
/// focused control is a plain pane with no text pattern. Not checked here
/// (not installed, or needs an account): LibreOffice, which reports through
/// IAccessible2 rather than UI Automation's text pattern, and Google Docs,
/// which draws on a canvas unless its screen reader support is turned on.
pub fn hides_text(process: &str, title: &str) -> bool {
    const APPS: &[&str] = &[
        "editors.exe",
        "desktopeditors.exe",
        "soffice.bin",
        "soffice.exe",
        "swriter.exe",
    ];
    const BROWSERS: &[&str] = &["chrome.exe", "msedge.exe", "firefox.exe", "brave.exe"];
    let process = process.to_lowercase();
    APPS.contains(&process.as_str())
        || (BROWSERS.contains(&process.as_str()) && title.contains("Google Docs"))
}

/// What to do after Ctrl+Shift+Left selected `selected`, when `wanted` words
/// were asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WordSelection {
    Done,
    /// Only punctuation or too few words so far ("." after "Hello."): once more.
    Extend,
    /// It reached into the line before: select from the line start instead.
    LineStart,
}

pub fn check_word_selection(selected: &str, wanted: usize) -> WordSelection {
    let is_word = |w: &&str| w.chars().any(char::is_alphanumeric);
    if let Some(at) = selected.rfind(['\n', '\r']) {
        let on_this_line = &selected[at + 1..];
        if on_this_line.split_whitespace().filter(is_word).count() > 0 {
            return WordSelection::LineStart;
        }
        return WordSelection::Done;
    }
    if selected.split_whitespace().filter(is_word).count() < wanted {
        WordSelection::Extend
    } else {
        WordSelection::Done
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn last_sentence_is_the_one_before_the_caret() {
        assert_eq!(
            last_sentence("The motion is denied. The parties may appeal."),
            Some("The parties may appeal.")
        );
        assert_eq!(
            last_sentence("One sentence only"),
            Some("One sentence only")
        );
        assert_eq!(
            last_sentence("First? Second!  "),
            Some("Second!"),
            "trailing spaces after the caret's sentence are dropped"
        );
        assert_eq!(last_sentence("   \n "), None);
    }

    #[test]
    fn abbreviations_and_initials_do_not_end_a_sentence() {
        assert_eq!(
            last_sentence("We wrote to him. Mr. Smith replied in Smith v. Jones."),
            Some("Mr. Smith replied in Smith v. Jones.")
        );
        assert_eq!(
            last_sentence("Done. John F. Kennedy spoke."),
            Some("John F. Kennedy spoke.")
        );
        assert_eq!(
            last_sentence("Filed. See Case No. 12 and Art. 5 of the treaty."),
            Some("See Case No. 12 and Art. 5 of the treaty.")
        );
        assert_eq!(last_sentence("He said no. We left."), Some("We left."));
    }

    #[test]
    fn a_line_break_starts_a_sentence_and_quotes_stay_with_theirs() {
        assert_eq!(last_sentence("Dear Sir,\nThank you"), Some("Thank you"));
        assert_eq!(
            last_sentence("He said \"no.\" Then he left."),
            Some("Then he left.")
        );
        assert_eq!(
            last_sentence("Fine. He said \"no.\""),
            Some("He said \"no.\"")
        );
    }

    #[test]
    fn last_words_keep_their_punctuation() {
        assert_eq!(last_words("within ten days.", 1), Some("days."));
        assert_eq!(last_words("within ten days.  ", 2), Some("ten days."));
        assert_eq!(
            last_words("Dear Sir,\r\nThe end", 3),
            Some("Sir,\r\nThe end")
        );
        assert_eq!(last_words("two words", 5), Some("two words"));
        assert_eq!(last_words("   ", 1), None);
        assert_eq!(last_words("x", 0), None);
    }

    #[test]
    fn last_paragraph_is_after_the_last_line_break() {
        assert_eq!(
            last_paragraph("Dear Sir,\r\n\r\nThank you. We agree.\r\n"),
            Some("Thank you. We agree.")
        );
        assert_eq!(last_paragraph("Only one"), Some("Only one"));
        assert_eq!(last_paragraph("\n\n"), None);
    }

    #[test]
    fn typed_text_matches_what_apps_report() {
        assert!(same_text("Dear Sir,\nThanks", "Dear Sir,\r\nThanks"));
        assert!(same_text(
            "\"Yes,\" he said",
            "\u{201C}Yes,\u{201D} he said"
        ));
        assert!(same_text("Done. ", "Done."), "a dropped trailing space");
        assert!(!same_text("Done.", "Done. And more"));
        assert!(!same_text(" ", ""));
        assert!(!same_text("the motion", "the notion"));
    }

    #[test]
    fn caret_steps_count_a_line_break_once() {
        assert_eq!(caret_steps("ab\r\ncd"), 5);
        assert_eq!(caret_steps("ab\ncd"), 5);
        assert_eq!(caret_steps("héllo"), 5);
    }

    #[test]
    fn rich_text_apps_are_recognized() {
        assert!(keeps_rich_text("WINWORD.EXE", "_WwG"));
        assert!(keeps_rich_text("olk.exe", "Chrome_RenderWidgetHostHWND"));
        assert!(keeps_rich_text("someapp.exe", "RICHEDIT50W"));
        assert!(!keeps_rich_text("notepad.exe", "RichEditD2DPT"));
        assert!(!keeps_rich_text(
            "chrome.exe",
            "Chrome_RenderWidgetHostHWND"
        ));
    }
    #[test]
    fn apps_that_hide_their_text_are_recognized() {
        assert!(hides_text("editors.exe", "Document1.docx - ONLYOFFICE"));
        assert!(hides_text("soffice.bin", "Untitled 1 - LibreOffice Writer"));
        assert!(hides_text(
            "chrome.exe",
            "Memo - Google Docs - Google Chrome"
        ));
        assert!(!hides_text("chrome.exe", "Inbox - Gmail"));
        assert!(!hides_text("winword.exe", "Memo - Word"));
        assert!(!hides_text("notepad.exe", "notes.txt - Notepad"));
    }

    #[test]
    fn a_selected_period_alone_is_extended_to_its_word() {
        use WordSelection::*;
        assert_eq!(check_word_selection(".", 1), Extend);
        assert_eq!(check_word_selection("Hello.", 1), Done);
        assert_eq!(check_word_selection(" ", 1), Extend);
        assert_eq!(check_word_selection("ten days.", 3), Extend);
        assert_eq!(check_word_selection("within ten days.", 3), Done);
    }

    #[test]
    fn a_selection_into_the_line_before_selects_from_the_line_start() {
        use WordSelection::*;
        // One word on the line: Ctrl+Shift+Left went past its start.
        assert_eq!(check_word_selection("Sir,\r\nHello.", 2), LineStart);
        assert_eq!(check_word_selection("Sir,\nHello", 2), LineStart);
        // Nothing but punctuation on this line: leave it.
        assert_eq!(check_word_selection("Sir,\n.", 1), Done);
    }
}
