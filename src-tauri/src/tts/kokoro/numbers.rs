//! Numbers, money, percentages, years and times as English words.

const ONES: [&str; 20] = [
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
];
const TENS: [&str; 10] = [
    "", "", "twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety",
];
const SCALES: [(u64, &str); 4] = [
    (1_000_000_000_000, "trillion"),
    (1_000_000_000, "billion"),
    (1_000_000, "million"),
    (1_000, "thousand"),
];

pub fn digit_word(c: char) -> Option<&'static str> {
    c.to_digit(10).map(|d| ONES[d as usize])
}

/// 0..=99 with a hyphen: "forty-two".
fn tens(n: u64) -> String {
    match n {
        0..=19 => ONES[n as usize].to_string(),
        _ if n.is_multiple_of(10) => TENS[(n / 10) as usize].to_string(),
        _ => format!("{}-{}", TENS[(n / 10) as usize], ONES[(n % 10) as usize]),
    }
}

/// 1..=999: "three hundred and five" (the "and" reads naturally in both
/// American and British speech).
fn hundreds(n: u64) -> String {
    match (n / 100, n % 100) {
        (0, r) => tens(r),
        (h, 0) => format!("{} hundred", ONES[h as usize]),
        (h, r) => format!("{} hundred and {}", ONES[h as usize], tens(r)),
    }
}

pub fn cardinal(n: u64) -> String {
    if n == 0 {
        return "zero".to_string();
    }
    let mut parts = Vec::new();
    let mut rest = n;
    for (scale, name) in SCALES {
        if rest >= scale {
            parts.push(format!("{} {name}", cardinal(rest / scale)));
            rest %= scale;
        }
    }
    if rest > 0 {
        parts.push(hundreds(rest));
    }
    parts.join(" ")
}

/// "twenty-one" → "twenty-first".
fn ordinal(n: u64) -> String {
    let words = cardinal(n);
    let (head, last) = match words.rfind([' ', '-']) {
        Some(i) => words.split_at(i + 1),
        None => ("", words.as_str()),
    };
    let last = match last {
        "one" => "first".to_string(),
        "two" => "second".to_string(),
        "three" => "third".to_string(),
        "five" => "fifth".to_string(),
        "eight" => "eighth".to_string(),
        "nine" => "ninth".to_string(),
        "twelve" => "twelfth".to_string(),
        w if w.ends_with('y') => format!("{}ieth", &w[..w.len() - 1]),
        w => format!("{w}th"),
    };
    format!("{head}{last}")
}

/// Years read in pairs: 1999 "nineteen ninety-nine", 2024 "twenty
/// twenty-four", 1905 "nineteen oh five"; 2000–2009 stay "two thousand …".
fn year(n: u64) -> String {
    let (hi, lo) = (n / 100, n % 100);
    match lo {
        _ if (2000..2010).contains(&n) => cardinal(n),
        0 => format!("{} hundred", tens(hi)),
        1..=9 => format!("{} oh {}", tens(hi), ONES[lo as usize]),
        _ => format!("{} {}", tens(hi), tens(lo)),
    }
}

/// "1990s" → "nineteen nineties".
fn plural(words: String) -> String {
    match words.strip_suffix('y') {
        Some(stem) => format!("{stem}ies"),
        None if words.ends_with('x') => format!("{words}es"),
        None => format!("{words}s"),
    }
}

/// Spell out a number token from the tokenizer: `$3.50`, `1,200`, `21st`,
/// `24%`, `1990s`, `3.14`.
pub fn expand(token: &str) -> String {
    let (currency, rest) = match token.chars().next() {
        Some(c @ ('$' | '£' | '€')) => (Some(c), &token[c.len_utf8()..]),
        _ => (None, token),
    };
    let digits_end = rest
        .find(|c: char| !(c.is_ascii_digit() || c == ',' || c == '.'))
        .unwrap_or(rest.len());
    let (number, suffix) = rest.split_at(digits_end);
    let number = number.trim_end_matches(['.', ',']);
    let (int_part, frac_part) = match number.split_once('.') {
        Some((i, f)) => (i, Some(f)),
        None => (number, None),
    };
    let int_digits: String = int_part.chars().filter(char::is_ascii_digit).collect();
    // Too long to be a quantity (a phone or card number): read the digits.
    let Ok(int) = int_digits.parse::<u64>() else {
        return digits(&int_digits);
    };
    if int_digits.len() > 15 {
        return digits(&int_digits);
    }
    let is_year = currency.is_none()
        && frac_part.is_none()
        && !int_part.contains(',')
        && int_part.len() == 4
        && (1100..=2099).contains(&int);

    if let Some(c) = currency {
        let (unit, units, cent, cents) = match c {
            '£' => ("pound", "pounds", "penny", "pence"),
            '€' => ("euro", "euros", "cent", "cents"),
            _ => ("dollar", "dollars", "cent", "cents"),
        };
        let mut words = format!("{} {}", cardinal(int), if int == 1 { unit } else { units });
        if let Some(f) = frac_part.filter(|f| !f.is_empty()) {
            let cents_value: u64 = format!("{f:0<2}")[..2].parse().unwrap_or(0);
            if cents_value > 0 {
                words = format!(
                    "{words} and {} {}",
                    cardinal(cents_value),
                    if cents_value == 1 { cent } else { cents }
                );
            }
        }
        return words;
    }

    let mut words = match suffix {
        "st" | "nd" | "rd" | "th" => return ordinal(int),
        "s" if is_year || int.is_multiple_of(10) => {
            return plural(if is_year { year(int) } else { cardinal(int) });
        }
        _ if is_year => year(int),
        _ => cardinal(int),
    };
    if let Some(f) = frac_part.filter(|f| !f.is_empty()) {
        words = format!("{words} point {}", digits(f));
    }
    if suffix == "%" {
        words.push_str(" percent");
    }
    words
}

/// "3:30" → "three thirty", "9:05" → "nine oh five", "10:00" → "ten o'clock".
pub fn time(token: &str) -> String {
    let Some((h, m)) = token.split_once(':') else {
        return expand(token);
    };
    let (Ok(h), Ok(m)) = (h.parse::<u64>(), m.parse::<u64>()) else {
        return token.to_string();
    };
    match m {
        0 => format!("{} o'clock", cardinal(h)),
        1..=9 => format!("{} oh {}", cardinal(h), ONES[m as usize]),
        _ => format!("{} {}", cardinal(h), tens(m)),
    }
}

fn digits(s: &str) -> String {
    s.chars()
        .filter_map(digit_word)
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cardinals() {
        assert_eq!(cardinal(0), "zero");
        assert_eq!(cardinal(42), "forty-two");
        assert_eq!(cardinal(305), "three hundred and five");
        assert_eq!(cardinal(1_200), "one thousand two hundred");
        assert_eq!(cardinal(2_500_000), "two million five hundred thousand");
    }

    #[test]
    fn tokens() {
        assert_eq!(expand("1,200"), "one thousand two hundred");
        assert_eq!(expand("3.14"), "three point one four");
        assert_eq!(expand("21st"), "twenty-first");
        assert_eq!(expand("12th"), "twelfth");
        assert_eq!(expand("40th"), "fortieth");
        assert_eq!(expand("24%"), "twenty-four percent");
        assert_eq!(expand("$3.50"), "three dollars and fifty cents");
        assert_eq!(expand("$1"), "one dollar");
        assert_eq!(expand("£20"), "twenty pounds");
    }

    #[test]
    fn years_and_decades() {
        assert_eq!(expand("1999"), "nineteen ninety-nine");
        assert_eq!(expand("2024"), "twenty twenty-four");
        assert_eq!(expand("2005"), "two thousand five");
        assert_eq!(expand("1905"), "nineteen oh five");
        assert_eq!(expand("1900"), "nineteen hundred");
        assert_eq!(expand("1990s"), "nineteen nineties");
        assert_eq!(expand("5000"), "five thousand");
    }

    #[test]
    fn times() {
        assert_eq!(time("3:30"), "three thirty");
        assert_eq!(time("9:05"), "nine oh five");
        assert_eq!(time("10:00"), "ten o'clock");
    }
}
