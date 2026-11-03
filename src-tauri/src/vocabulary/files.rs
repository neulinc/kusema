//! Vocabulary files: Kusema's own JSON, a CSV a firm can edit in a
//! spreadsheet, and Dragon's word lists.
//!
//! - **JSON:** `{"kusema_vocabulary": 1, "entries": [{"written": "Kanella",
//!   "sounds_like": ["Canela"]}]}`.
//! - **CSV:** `written,sounds_like` with a header, the spoken forms separated by
//!   `|`; fields may be quoted.
//! - **Dragon:** a text file, one entry per line: the written form alone, or
//!   `written\\spoken` (two backslashes, as Nuance documents it; one is
//!   accepted too). An `@version=Plato-UTF8` first line is skipped. UTF-8 or,
//!   as Windows tools often write, UTF-16 with a byte-order mark.

use serde::{Deserialize, Serialize};

use super::VocabEntry;

pub const JSON_VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
struct JsonFile {
    kusema_vocabulary: u32,
    entries: Vec<JsonEntry>,
}

#[derive(Serialize, Deserialize)]
struct JsonEntry {
    written: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    sounds_like: Vec<String>,
}

/// The file formats Kusema writes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum ExportFormat {
    Json,
    Csv,
}

/// Bytes to text: UTF-16 with a byte-order mark, else UTF-8 (BOM dropped).
pub fn decode(bytes: &[u8]) -> Result<String, String> {
    let utf16 = |le: bool| {
        let units: Vec<u16> = bytes[2..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&c| {
                if le {
                    u16::from_le_bytes(c)
                } else {
                    u16::from_be_bytes(c)
                }
            })
            .collect();
        String::from_utf16(&units).map_err(|_| "The file isn't valid UTF-16 text.".to_string())
    };
    match bytes {
        [0xFF, 0xFE, ..] => utf16(true),
        [0xFE, 0xFF, ..] => utf16(false),
        [0xEF, 0xBB, 0xBF, rest @ ..] => String::from_utf8(rest.to_vec())
            .map_err(|_| "The file isn't valid UTF-8 text.".to_string()),
        _ => String::from_utf8(bytes.to_vec())
            .map_err(|_| "The file isn't text Kusema can read (UTF-8 or UTF-16).".to_string()),
    }
}

/// Reads any of the three formats, by its content.
pub fn parse(text: &str) -> Result<Vec<VocabEntry>, String> {
    let trimmed = text.trim_start();
    let entries = if trimmed.starts_with('{') {
        let file: JsonFile = serde_json::from_str(trimmed)
            .map_err(|e| format!("This isn't a Kusema vocabulary file ({e})."))?;
        file.entries
            .into_iter()
            .map(|e| VocabEntry::new(&e.written, e.sounds_like, false))
            .collect()
    } else if is_csv(trimmed) {
        parse_csv(trimmed)
    } else {
        parse_dragon(trimmed)
    };
    Ok(entries
        .into_iter()
        .filter(|e| !e.written.is_empty())
        .collect())
}

fn is_csv(text: &str) -> bool {
    text.lines().next().is_some_and(|header| {
        header
            .trim()
            .to_lowercase()
            .replace(['"', ' '], "")
            .starts_with("written,")
    })
}

/// Dragon's list: `written`, or `written\\spoken`.
fn parse_dragon(text: &str) -> Vec<VocabEntry> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('@') && !line.starts_with('#'))
        .map(|line| {
            let (written, spoken) = match line.split_once("\\\\").or_else(|| line.split_once('\\'))
            {
                Some((written, spoken)) => (written.trim(), Some(spoken.trim())),
                None => (line, None),
            };
            let spoken: Vec<String> = spoken
                .filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case(written))
                .map(|s| vec![s.to_string()])
                .unwrap_or_default();
            VocabEntry::new(written, spoken, false)
        })
        .collect()
}

/// One CSV line's fields: commas separate, quotes protect, "" is a quote.
fn csv_fields(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match (c, quoted) {
            ('"', true) if chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            ('"', _) => quoted = !quoted,
            (',', false) => fields.push(std::mem::take(&mut field)),
            (c, _) => field.push(c),
        }
    }
    fields.push(field);
    fields
}

fn parse_csv(text: &str) -> Vec<VocabEntry> {
    text.lines()
        .skip(1)
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let fields = csv_fields(line);
            let written = fields.first().map(|s| s.trim()).unwrap_or("");
            let spoken = fields
                .get(1)
                .map(|s| {
                    s.split('|')
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default();
            VocabEntry::new(written, spoken, false)
        })
        .collect()
}

fn csv_field(value: &str) -> String {
    if value.contains([',', '"', '\n', '|']) || value.starts_with(' ') || value.ends_with(' ') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

pub fn write(entries: &[VocabEntry], format: ExportFormat) -> String {
    match format {
        ExportFormat::Json => {
            let file = JsonFile {
                kusema_vocabulary: JSON_VERSION,
                entries: entries
                    .iter()
                    .map(|e| JsonEntry {
                        written: e.written.clone(),
                        sounds_like: e.sounds_like.clone(),
                    })
                    .collect(),
            };
            serde_json::to_string_pretty(&file).unwrap_or_default() + "\n"
        }
        ExportFormat::Csv => {
            let mut out = String::from("written,sounds_like\n");
            for e in entries {
                // `|` separates spoken forms, so the whole field is quoted
                // when it has more than one.
                let spoken = e.sounds_like.join("|");
                out.push_str(&csv_field(&e.written));
                out.push(',');
                out.push_str(&if spoken.contains('|')
                    || spoken.contains(',')
                    || spoken.contains('"')
                {
                    format!("\"{}\"", spoken.replace('"', "\"\""))
                } else {
                    spoken
                });
                out.push('\n');
            }
            out
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<VocabEntry> {
        vec![
            VocabEntry::new("Kanella", vec!["canela".into()], false),
            VocabEntry::new("Smith, Jones & Partners", vec![], false),
            VocabEntry::new("O\"Brien", vec!["o brian".into(), "obrien".into()], false),
            VocabEntry::new("habeas corpus", vec!["habeus corpus".into()], false),
        ]
    }

    fn plain(entries: &[VocabEntry]) -> Vec<(String, Vec<String>)> {
        entries
            .iter()
            .map(|e| (e.written.clone(), e.sounds_like.clone()))
            .collect()
    }

    #[test]
    fn json_round_trip() {
        let text = write(&sample(), ExportFormat::Json);
        assert!(text.contains("\"kusema_vocabulary\": 1"));
        assert_eq!(plain(&parse(&text).unwrap()), plain(&sample()));
    }

    #[test]
    fn csv_round_trip_with_commas_quotes_and_several_spoken_forms() {
        let text = write(&sample(), ExportFormat::Csv);
        assert!(text.starts_with("written,sounds_like\n"));
        assert_eq!(plain(&parse(&text).unwrap()), plain(&sample()));
    }

    #[test]
    fn a_spreadsheet_csv_with_spaces_and_quotes() {
        let text = "Written, Sounds_like\r\n\"Kanella\",\"Canela | Canella\"\r\nvoir dire,\r\n";
        assert_eq!(
            plain(&parse(text).unwrap()),
            vec![
                ("Kanella".into(), vec!["canela".into(), "canella".into()]),
                ("voir dire".into(), vec![]),
            ]
        );
    }

    #[test]
    fn dragon_lists_with_double_or_single_backslashes() {
        let text = "@version=Plato-UTF8\r\nMB\\\\megabyte\r\nKanella\r\nRobert F. Kennedy\\\\RFK\r\nAcme\\a c m e\r\n\r\n";
        assert_eq!(
            plain(&parse(text).unwrap()),
            vec![
                ("MB".into(), vec!["megabyte".into()]),
                ("Kanella".into(), vec![]),
                ("Robert F. Kennedy".into(), vec!["rfk".into()]),
                ("Acme".into(), vec!["a c m e".into()]),
            ]
        );
    }

    #[test]
    fn dragon_files_in_utf16_are_read() {
        let mut bytes = vec![0xFF, 0xFE];
        for unit in "@version=Plato-UTF8\r\nKanella\\\\Canela\r\n".encode_utf16() {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        let text = decode(&bytes).unwrap();
        assert_eq!(
            plain(&parse(&text).unwrap()),
            vec![("Kanella".into(), vec!["canela".into()])]
        );
        assert!(decode(&[0xEF, 0xBB, 0xBF, b'a']).is_ok());
        assert!(decode(&[0xC3, 0x28]).is_err());
    }

    #[test]
    fn not_a_vocabulary_file() {
        assert!(parse("{\"something\": 1}").is_err());
    }
}
