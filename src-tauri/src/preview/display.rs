//! The live preview's text as it will be typed: spoken punctuation applied
//! ("comma" shows as ","), and the line breaks "new paragraph" and "new line"
//! make shown as ¶ and ↵, since the pill has one or two lines. Only the end
//! is kept: the pill shows the last words.

use crate::settings::AppSettings;

/// The preview keeps no more than this many characters.
const MAX_CHARS: usize = 240;
const PARAGRAPH: &str = " ¶ ";
const LINE: &str = " ↵ ";

/// The committed and tentative parts of a stream's text, ready for the pill.
/// Cleanup runs on the whole text, so a rule spanning the two parts ("new"
/// committed, "paragraph" still tentative) applies as it will at the end.
pub fn display(settings: &AppSettings, committed: &str, tentative: &str) -> (String, String) {
    let whole = clean(settings, &format!("{committed}{tentative}"));
    let settled = clean(settings, committed);
    let (committed, tentative) = match whole.strip_prefix(settled.as_str()) {
        Some(rest) => (settled.clone(), rest.to_string()),
        // Cleanup moved the boundary: all of it is still changing.
        None => (String::new(), whole),
    };
    let (committed, tentative) = (symbols(&committed), symbols(&tentative));
    let (committed, tentative) = tail(committed, tentative);
    (mark(committed), tentative)
}

fn clean(settings: &AppSettings, text: &str) -> String {
    crate::cleanup::apply(settings, text, None)
}

/// Line breaks as symbols, on one line.
fn symbols(text: &str) -> String {
    let text = text
        .replace("\r\n", "\n")
        .replace("\n\n", PARAGRAPH)
        .replace('\n', LINE);
    // Keep a trailing space: the tentative part follows.
    let ends_with_space = text.ends_with(' ');
    let mut joined = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if ends_with_space && !joined.is_empty() {
        joined.push(' ');
    }
    if text.starts_with(' ') && !joined.is_empty() {
        joined.insert(0, ' ');
    }
    joined
}

/// The last `MAX_CHARS` characters, cut at a word.
fn tail(committed: String, tentative: String) -> (String, String) {
    let total = committed.chars().count() + tentative.chars().count();
    if total <= MAX_CHARS {
        return (committed, tentative);
    }
    let drop = total - MAX_CHARS;
    let committed_chars = committed.chars().count();
    if drop >= committed_chars {
        let rest: String = tentative.chars().skip(drop - committed_chars).collect();
        return (String::new(), from_word(&rest));
    }
    let rest: String = committed.chars().skip(drop).collect();
    (from_word(&rest), tentative)
}

fn from_word(text: &str) -> String {
    match text.find(' ') {
        Some(space) => format!("…{}", &text[space..]),
        None => text.to_string(),
    }
}

/// Debug builds: `KUSEMA_DEBUG_PREVIEW_MARK` starts every preview with a
/// marker, so tests can tell preview text from typed text.
#[cfg(debug_assertions)]
fn mark(committed: String) -> String {
    if std::env::var_os("KUSEMA_DEBUG_PREVIEW_MARK").is_some() {
        format!("[preview] {committed}")
    } else {
        committed
    }
}

#[cfg(not(debug_assertions))]
fn mark(committed: String) -> String {
    committed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::get_default_settings;

    fn shown(committed: &str, tentative: &str) -> (String, String) {
        display(&get_default_settings(), committed, tentative)
    }

    #[test]
    fn plain_words_pass_through_split_as_they_came() {
        // Cleanup trims the settled part's last space; it moves to the rest.
        assert_eq!(
            shown("Please send the ", "contract"),
            ("Please send the".to_string(), " contract".to_string())
        );
        assert_eq!(shown("", ""), (String::new(), String::new()));
    }

    #[test]
    fn spoken_punctuation_and_line_breaks_show_as_they_will_be_typed() {
        let (committed, tentative) = shown("Dear Sam comma thank you new ", "paragraph The court");
        let whole = format!("{committed}{tentative}");
        assert_eq!(whole, "Dear Sam, thank you ¶ The court");
        // "new" settled, "paragraph" still changing: the rule spans the two.
        assert!(tentative.contains('¶'), "{committed} | {tentative}");
        let (c, t) = shown("First line new line ", "Second");
        assert_eq!(format!("{c}{t}"), "First line ↵ Second");
        // Where the rule says they're words, they're shown as words.
        let (c, t) = shown("we added new paragraphs", "");
        assert_eq!(format!("{c}{t}"), "we added new paragraphs");
    }

    #[test]
    fn with_cleanup_off_the_words_are_shown_as_said() {
        let mut settings = get_default_settings();
        settings.local_cleanup_enabled = false;
        let (c, t) = display(&settings, "Dear Sam comma ", "new paragraph");
        assert_eq!(format!("{c}{t}"), "Dear Sam comma new paragraph");
    }

    #[test]
    fn only_the_end_of_a_long_preview_is_kept() {
        let long = "word ".repeat(100);
        let (committed, tentative) = shown(&long, "last");
        assert!(committed.starts_with('…'), "{committed}");
        assert!(committed.chars().count() + tentative.chars().count() <= MAX_CHARS + 1);
        assert_eq!(tentative, " last");
        // All tentative, and too long: its end.
        let (committed, tentative) = shown("", &"more ".repeat(100));
        assert!(committed.is_empty());
        assert!(tentative.starts_with('…') && tentative.ends_with("more"));
    }
}
