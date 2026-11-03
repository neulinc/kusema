//! Citation forms for the legal pack: how a dictated citation is written.
//! Reporters only between a volume and a page ("123 F 3d 456" → "123 F.3d
//! 456"), codes after a title number ("42 USC" → "42 U.S.C."), "v." between
//! two capitalized names ("Smith v Jones" → "Smith v. Jones") and rule
//! subdivisions ("Rule 12 b 6" → "Rule 12(b)(6)"). The rules ask for that
//! context so ordinary words ("us", "versus" in prose) stay as they are.

use once_cell::sync::Lazy;
use regex::{Captures, Regex};

/// (written form, spoken variants as a regex body). Ordinals can be spoken
/// as "2d", "2nd" or "second".
const REPORTERS: &[(&str, &str)] = &[
    ("U.S.", r"u\.?\s?s\.?"),
    ("S. Ct.", r"s\.?\s?ct\.?|s\.?\s?court"),
    ("L. Ed. 2d", r"l\.?\s?ed\.?\s?(?:2d|2nd|second)"),
    ("L. Ed.", r"l\.?\s?ed\.?"),
    (
        "F. Supp. 3d",
        r"f\.?\s?supp?\.?\s?(?:3d|3rd|third)|f\.?\s?supplement\s(?:3d|3rd|third)",
    ),
    (
        "F. Supp. 2d",
        r"f\.?\s?supp?\.?\s?(?:2d|2nd|second)|f\.?\s?supplement\s(?:2d|2nd|second)",
    ),
    ("F. Supp.", r"f\.?\s?supp?\.?|f\.?\s?supplement"),
    ("F. App'x", r"f\.?\s?app'?x|f\.?\s?appendix"),
    ("F.4th", r"f\.?\s?(?:4th|fourth)"),
    ("F.3d", r"f\.?\s?(?:3d|3rd|third)"),
    ("F.2d", r"f\.?\s?(?:2d|2nd|second)"),
    ("P.3d", r"p\.?\s?(?:3d|3rd|third)"),
    ("P.2d", r"p\.?\s?(?:2d|2nd|second)"),
    ("A.3d", r"a\.?\s?(?:3d|3rd|third)"),
    ("A.2d", r"a\.?\s?(?:2d|2nd|second)"),
    ("N.E.3d", r"n\.?\s?e\.?\s?(?:3d|3rd|third)"),
    ("N.E.2d", r"n\.?\s?e\.?\s?(?:2d|2nd|second)"),
    ("N.W.2d", r"n\.?\s?w\.?\s?(?:2d|2nd|second)"),
    ("S.E.2d", r"s\.?\s?e\.?\s?(?:2d|2nd|second)"),
    ("S.W.3d", r"s\.?\s?w\.?\s?(?:3d|3rd|third)"),
    ("S.W.2d", r"s\.?\s?w\.?\s?(?:2d|2nd|second)"),
    ("So. 3d", r"so\.?\s?(?:3d|3rd|third)"),
    ("So. 2d", r"so\.?\s?(?:2d|2nd|second)"),
    ("B.R.", r"b\.?\s?r\.?"),
];

/// Codes cited after a title number.
const CODES: &[(&str, &str)] = &[
    ("U.S.C.", r"u\.?\s?s\.?\s?c\.?"),
    ("C.F.R.", r"c\.?\s?f\.?\s?r\.?"),
];

struct Rule {
    re: Regex,
    written: &'static str,
}

static REPORTER_RULES: Lazy<Vec<Rule>> = Lazy::new(|| {
    REPORTERS
        .iter()
        .map(|(written, body)| Rule {
            // volume, reporter, page
            // The model sometimes runs reporter and page together ("F3D456").
            re: Regex::new(&format!(r"(?i)\b(\d+)\s+(?:{body})\s*(\d+)\b")).unwrap(),
            written,
        })
        .collect()
});

static CODE_RULES: Lazy<Vec<Rule>> = Lazy::new(|| {
    CODES
        .iter()
        .map(|(written, body)| Rule {
            // title, code, then a section sign, a number, punctuation or the end
            re: Regex::new(&format!(r"(?i)\b(\d+)\s+(?:{body})(\s|$|[,;:)])")).unwrap(),
            written,
        })
        .collect()
});

/// "v", "vs", "vee" (with or without a period) between capitalized names.
static CASE_V: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\b([A-Z][\w'&.-]*)\s+(?:v|V|vs|Vs|VS|vee|Vee)\.?\s+([A-Z])").unwrap()
});

/// "Rule 12 b 6": two or three subdivisions, said as separate words.
static RULE_PARTS: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\b([Rr]ule)\s+(\d+)((?:\s+(?:[a-z]|\d{1,2})){2,3})\b").unwrap());
static RULE_PART: Lazy<Regex> = Lazy::new(|| Regex::new(r"[a-z]|\d{1,2}").unwrap());
/// "Rule 12B6" as the model sometimes writes it.
static RULE_GLUED: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\b([Rr]ule)\s+(\d+)([A-Za-z])(\d{1,2})\b").unwrap());

pub fn apply(text: &str) -> String {
    let mut text = text.to_string();
    for rule in REPORTER_RULES.iter() {
        text = rule
            .re
            .replace_all(&text, |c: &Captures| {
                format!("{} {} {}", &c[1], rule.written, &c[2])
            })
            .into_owned();
    }
    for rule in CODE_RULES.iter() {
        text = rule
            .re
            .replace_all(&text, |c: &Captures| {
                format!("{} {}{}", &c[1], rule.written, &c[2])
            })
            .into_owned();
    }
    text = CASE_V.replace_all(&text, "$1 v. $2").into_owned();
    text = RULE_GLUED
        .replace_all(&text, |c: &Captures| {
            format!("{} {}({})({})", &c[1], &c[2], c[3].to_lowercase(), &c[4])
        })
        .into_owned();
    text = RULE_PARTS
        .replace_all(&text, |c: &Captures| {
            let parts: String = RULE_PART
                .find_iter(&c[3])
                .map(|m| format!("({})", m.as_str()))
                .collect();
            format!("{} {}{}", &c[1], &c[2], parts)
        })
        .into_owned();
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reporters_between_volume_and_page() {
        assert_eq!(apply("See 123 F 3d 456."), "See 123 F.3d 456.");
        assert_eq!(apply("See 123 F third 456."), "See 123 F.3d 456.");
        assert_eq!(apply("12 F. Supp. 2d 34"), "12 F. Supp. 2d 34");
        assert_eq!(apply("12 F supp second 34"), "12 F. Supp. 2d 34");
        assert_eq!(apply("410 US 113"), "410 U.S. 113");
        assert_eq!(apply("130 S Ct 876"), "130 S. Ct. 876");
        assert_eq!(apply("55 F 4th 90"), "55 F.4th 90");
        assert_eq!(apply("Jones, 123 F3D456."), "Jones, 123 F.3d 456.");
    }

    #[test]
    fn codes_after_a_title_number() {
        assert_eq!(apply("42 USC § 1983"), "42 U.S.C. § 1983");
        assert_eq!(apply("under 42 u.s.c. 1983"), "under 42 U.S.C. 1983");
        assert_eq!(apply("29 CFR, part 1910"), "29 C.F.R., part 1910");
    }

    #[test]
    fn v_between_case_names() {
        assert_eq!(apply("Smith v Jones"), "Smith v. Jones");
        assert_eq!(
            apply("Brown vs Board of Education"),
            "Brown v. Board of Education"
        );
        assert_eq!(apply("Marbury vee Madison"), "Marbury v. Madison");
        assert_eq!(apply("Roe v. Wade"), "Roe v. Wade");
    }

    #[test]
    fn rule_subdivisions() {
        assert_eq!(apply("a Rule 12 b 6 motion"), "a Rule 12(b)(6) motion");
        assert_eq!(apply("under rule 23 b 3"), "under rule 23(b)(3)");
        assert_eq!(apply("Rule 12(b)(6) motion"), "Rule 12(b)(6) motion");
        assert_eq!(apply("under Rule 12B6."), "under Rule 12(b)(6).");
    }

    #[test]
    fn ordinary_words_are_left_alone() {
        for text in [
            "Tell us what happened.",
            "Give us 3 days and us 4 more.",
            "Democrats versus Republicans",
            "the plan vs the budget",
            "Rule 11 a lawyer must follow",
            "He was 5 F tall",
            "Section 2 so 3 parts",
        ] {
            assert_eq!(apply(text), text, "{text}");
        }
    }
}
