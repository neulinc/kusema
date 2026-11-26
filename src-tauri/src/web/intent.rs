//! Kusema web search: what a question asks of the web, from its words alone.
//! "Search the web for…" and the like ask for a search; an address in the
//! question, or "this link" (meaning the one copied), asks to read a page.
//! Like the trigger word, these phrases are English.

/// What a question asks of the web.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Intent {
    /// Nothing said about the web.
    None,
    /// "Search the web for X": search for X.
    Search(String),
    /// Read a page: the address in the question, or (`None`) the one the user
    /// copied ("summarize this link").
    ReadLink(Option<String>),
}

/// Phrases that ask for a search, followed by what to search for.
const SEARCH_LEADS: &[&str] = &[
    "search the web for",
    "search the internet for",
    "search online for",
    "search the web about",
    "do a web search for",
    "do a web search on",
    "web search for",
    "web search",
    "look up online",
    "look online for",
    "check online for",
    "find online",
    "google",
    "search for",
    "look up",
];

/// Phrases that put "online" after what to search for: "look up X online".
const SEARCH_WRAPS: &[(&str, &[&str])] = &[
    ("look up ", &[" online", " on the web", " on the internet"]),
    (
        "search for ",
        &[" online", " on the web", " on the internet"],
    ),
    ("search ", &[" online", " on the web", " on the internet"]),
    ("find ", &[" online", " on the web", " on the internet"]),
    ("check ", &[" online", " on the web"]),
];

/// Questions that are about now, searched as asked: "what's the latest on
/// X", "what's the current filing fee", "news about X".
const CURRENT_LEADS: &[&str] = &[
    "what's the latest",
    "what is the latest",
    "whats the latest",
    "what's the current",
    "what is the current",
    "whats the current",
    "what are the latest",
    "what are the current",
    "news about",
    "news on",
    "latest news",
    "any news",
    "the latest news",
    "recent news",
];

/// "What's the current time" and the like: nothing to search for.
const NOT_NEWS: &[&str] = &["time", "date", "day", "year", "month", "week"];

/// Searches about the user's own things ("look up X in my contacts"), which
/// the web can't answer: left to the AI.
fn about_own_things(query: &str) -> bool {
    let places = [
        "document",
        "file",
        "files",
        "email",
        "e-mail",
        "text",
        "inbox",
        "folder",
        "selection",
        "notes",
        "message",
        "thread",
        "calendar",
        "contacts",
        "computer",
    ];
    ["in this ", "in my ", "in the ", "on my ", "on this "]
        .iter()
        .any(|lead| {
            places
                .iter()
                .any(|place| query.contains(&format!("{lead}{place}")))
        })
}

/// The query as the user wrote it (same position, original casing).
fn original(question: &str, query: &str) -> String {
    let start = question.to_lowercase().find(query).unwrap_or(0);
    question
        .get(start..start + query.len())
        .unwrap_or(query)
        .to_string()
}

/// Phrases meaning "the link I copied".
const COPIED_LINK: &[&str] = &[
    "this link",
    "this url",
    "this page",
    "this article",
    "this web page",
    "this webpage",
    "the link i copied",
    "the copied link",
    "that link",
];

/// Words that, with one of the phrases above, ask to read the page.
const READ_VERBS: &[&str] = &[
    "summarize",
    "summarise",
    "summary",
    "read",
    "what does",
    "what do",
    "what's in",
    "what is in",
    "say about",
    "says about",
    "tell me about",
    "explain",
    "open",
    "look at",
];

fn trim_query(text: &str) -> String {
    text.trim()
        .trim_start_matches([':', ',', '-'])
        .trim()
        .trim_end_matches(['.', '?', '!', ','])
        .trim()
        .trim_matches('"')
        .trim()
        .to_string()
}

/// The first http(s) address in `text`, or a bare "www." one made https.
pub fn find_url(text: &str) -> Option<String> {
    for word in text.split_whitespace() {
        let word = word
            .trim_start_matches(['(', '<', '"', '\'', '['])
            .trim_end_matches([')', '>', '"', '\'', ']', ',', '.', ';', '!', '?']);
        let lower = word.to_ascii_lowercase();
        if (lower.starts_with("https://") || lower.starts_with("http://")) && word.len() > 8 {
            return Some(word.to_string());
        }
        if lower.starts_with("www.") && word.len() > 6 && word[4..].contains('.') {
            return Some(format!("https://{word}"));
        }
    }
    None
}

/// Whether the text is nothing but one address (the clipboard, for "this link").
pub fn single_url(text: &str) -> Option<String> {
    let text = text.trim();
    if text.split_whitespace().count() != 1 {
        return None;
    }
    find_url(text)
}

/// What `question` asks of the web.
pub fn detect(question: &str) -> Intent {
    if let Some(url) = find_url(question) {
        return Intent::ReadLink(Some(url));
    }
    let lowered = question.trim().to_lowercase();
    let mut lower = lowered.as_str();
    for polite in ["please ", "can you ", "could you ", "would you ", "please "] {
        lower = lower.strip_prefix(polite).unwrap_or(lower);
    }
    if COPIED_LINK.iter().any(|p| lower.contains(p)) && READ_VERBS.iter().any(|v| lower.contains(v))
    {
        return Intent::ReadLink(None);
    }
    let search = |query: String| {
        if query.is_empty() || about_own_things(&query) {
            Intent::None
        } else {
            Intent::Search(original(question, &query))
        }
    };
    // "look up X online": the place goes, X is the search.
    for (lead, tails) in SEARCH_WRAPS {
        if let Some(rest) = lower.strip_prefix(lead) {
            let rest = rest.trim_end_matches(['.', '?', '!']).trim_end();
            for tail in *tails {
                if let Some(query) = rest.strip_suffix(tail) {
                    let query = trim_query(query);
                    if !query.is_empty() {
                        return search(query);
                    }
                }
            }
        }
    }
    for lead in SEARCH_LEADS {
        if let Some(rest) = lower.strip_prefix(lead) {
            // "google" alone, or "googled", isn't a search request.
            if !rest.is_empty() && !rest.starts_with(' ') && !rest.starts_with(':') {
                continue;
            }
            // "look up to someone", "look up at the sky": not a search.
            if *lead == "look up" && (rest.starts_with(" to ") || rest.starts_with(" at ")) {
                return Intent::None;
            }
            let query = trim_query(rest);
            if !query.is_empty() {
                return search(query);
            }
        }
    }
    for lead in CURRENT_LEADS {
        if let Some(rest) = lower.strip_prefix(lead) {
            if !rest.is_empty() && !rest.starts_with(' ') {
                continue;
            }
            let rest = trim_query(rest);
            let words = rest.split_whitespace().count();
            // "What's the latest?" or "the current time": nothing to search.
            if rest.is_empty() || (words <= 2 && NOT_NEWS.iter().any(|w| rest.starts_with(w))) {
                return Intent::None;
            }
            // Searched as asked: search engines read whole questions.
            return search(trim_query(&lowered));
        }
    }
    Intent::None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn search(q: &str) -> Intent {
        Intent::Search(q.to_string())
    }

    #[test]
    fn asking_to_search_gives_the_search_text() {
        assert_eq!(
            detect("Search the web for Ohio statute of limitations for fraud."),
            search("Ohio statute of limitations for fraud")
        );
        assert_eq!(
            detect("please search online for: new filing fees in Cook County?"),
            search("new filing fees in Cook County")
        );
        assert_eq!(
            detect("Look up the court's holiday hours online"),
            search("the court's holiday hours")
        );
        assert_eq!(
            detect("Google Smith v. Jones 2024"),
            search("Smith v. Jones 2024")
        );
        assert_eq!(
            detect("Can you search the web for \"rule 26 amendments\""),
            search("rule 26 amendments")
        );
    }

    #[test]
    fn more_ways_of_asking_to_search() {
        assert_eq!(
            detect("Look up the Ohio filing fee"),
            search("the Ohio filing fee")
        );
        assert_eq!(
            detect("look up rule 26 amendments online."),
            search("rule 26 amendments")
        );
        assert_eq!(
            detect("Search for court holidays in 2026"),
            search("court holidays in 2026")
        );
        assert_eq!(
            detect("search for Smith v. Jones online"),
            search("Smith v. Jones")
        );
        assert_eq!(
            detect("Google the new e-filing rules"),
            search("the new e-filing rules")
        );
        assert_eq!(
            detect("Find the court's phone number online"),
            search("the court's phone number")
        );
        assert_eq!(
            detect("Could you look up today's interest rate?"),
            search("today's interest rate")
        );
    }

    #[test]
    fn questions_about_now_are_searched_as_asked() {
        assert_eq!(
            detect("What's the latest on the Johnson merger?"),
            search("What's the latest on the Johnson merger")
        );
        assert_eq!(
            detect("What is the current filing fee in Cook County?"),
            search("What is the current filing fee in Cook County")
        );
        assert_eq!(
            detect("News about the Supreme Court ruling"),
            search("News about the Supreme Court ruling")
        );
        assert_eq!(
            detect("any news on the bar exam results?"),
            search("any news on the bar exam results")
        );
        assert_eq!(
            detect("Latest news about remote notarization"),
            search("Latest news about remote notarization")
        );
    }

    #[test]
    fn near_misses_are_not_searches() {
        for q in [
            "What's the current time?",
            "what's the current date",
            "What's the latest?",
            "Look up to your mentors, they said",
            "look up at the ceiling",
            "Look up the client's number in my contacts",
            "Search for the word indemnify in this document",
            "find the clause in my notes online",
            "Newsletters are due Friday",
            "What is the currently pending motion about?",
            "Lookup tables are fast",
            "The news about our client is confidential",
        ] {
            assert_eq!(detect(q), Intent::None, "{q}");
        }
    }

    #[test]
    fn ordinary_questions_ask_nothing_of_the_web() {
        for q in [
            "What's the capital of Ghana?",
            "Explain the hearsay rule",
            "Draft a letter about the search warrant",
            "Googled things are often wrong, right?",
            "How do I search my inbox?",
            "Search for",
        ] {
            assert_eq!(detect(q), Intent::None, "{q}");
        }
    }

    #[test]
    fn an_address_in_the_question_is_read() {
        assert_eq!(
            detect("What does https://example.com/rules?page=2 say about deadlines?"),
            Intent::ReadLink(Some("https://example.com/rules?page=2".into()))
        );
        assert_eq!(
            detect("Summarize (www.example.org/article)."),
            Intent::ReadLink(Some("https://www.example.org/article".into()))
        );
    }

    #[test]
    fn this_link_means_the_copied_one() {
        for q in [
            "Summarize this link",
            "What does this page say about fees?",
            "please read this article",
            "Can you summarize the link I copied",
        ] {
            assert_eq!(detect(q), Intent::ReadLink(None), "{q}");
        }
        // Mentioning a link isn't asking to read one.
        assert_eq!(detect("This link is broken"), Intent::None);
    }

    #[test]
    fn only_a_lone_address_counts_as_a_copied_link() {
        assert_eq!(
            single_url("  https://example.com/a  "),
            Some("https://example.com/a".into())
        );
        assert_eq!(single_url("see https://example.com/a"), None);
        assert_eq!(single_url("Dear Sam, thanks"), None);
        assert_eq!(single_url("file:///C:/secret.txt"), None);
    }
}
