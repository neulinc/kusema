//! The local record of outbound requests: where, why, what kind of data and how
//! many bytes. There is no column for content, so it can't be stored by accident.

use anyhow::Result;
use rusqlite::{params, params_from_iter, types::Value, Connection};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::path::{Path, PathBuf};

/// Why a request was made.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum Purpose {
    AssistantQuestion,
    ScreenQuestion,
    PostProcessing,
    /// Listing a provider's models in the settings: the API key, no content.
    ProviderModels,
    ModelDownload,
    VoiceDownload,
    UpdateCheck,
}

impl Purpose {
    pub fn as_str(self) -> &'static str {
        match self {
            Purpose::AssistantQuestion => "assistant_question",
            Purpose::ScreenQuestion => "screen_question",
            Purpose::PostProcessing => "post_processing",
            Purpose::ProviderModels => "provider_models",
            Purpose::ModelDownload => "model_download",
            Purpose::VoiceDownload => "voice_download",
            Purpose::UpdateCheck => "update_check",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "assistant_question" => Purpose::AssistantQuestion,
            "screen_question" => Purpose::ScreenQuestion,
            "post_processing" => Purpose::PostProcessing,
            "provider_models" => Purpose::ProviderModels,
            "model_download" => Purpose::ModelDownload,
            "voice_download" => Purpose::VoiceDownload,
            "update_check" => Purpose::UpdateCheck,
            _ => return None,
        })
    }
}

/// What kind of data a request carries. Audio isn't one: no code path can send
/// it, and the Privacy page's "0 bytes" rests on that.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum DataKind {
    Text,
    Image,
    None,
}

impl DataKind {
    fn as_str(self) -> &'static str {
        match self {
            DataKind::Text => "text",
            DataKind::Image => "image",
            DataKind::None => "none",
        }
    }

    fn parse(value: &str) -> Self {
        match value {
            "text" => DataKind::Text,
            "image" => DataKind::Image,
            _ => DataKind::None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum RequestResult {
    Sent,
    Blocked,
    Failed,
}

impl RequestResult {
    fn as_str(self) -> &'static str {
        match self {
            RequestResult::Sent => "sent",
            RequestResult::Blocked => "blocked",
            RequestResult::Failed => "failed",
        }
    }

    fn parse(value: &str) -> Self {
        match value {
            "blocked" => RequestResult::Blocked,
            "failed" => RequestResult::Failed,
            _ => RequestResult::Sent,
        }
    }
}

/// One request, as listed.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
pub struct LedgerEntry {
    pub id: i64,
    /// Seconds since the epoch.
    pub timestamp: i64,
    /// The host only: never a path or query, which can hold keys.
    pub host: String,
    pub purpose: Purpose,
    pub data_kind: DataKind,
    /// Bytes of request body sent (or that would have been sent).
    pub bytes: i64,
    pub result: RequestResult,
}

/// What to list. Everything is optional.
#[derive(Clone, Debug, Default, Serialize, Deserialize, Type)]
pub struct LedgerFilter {
    pub purpose: Option<Purpose>,
    pub result: Option<RequestResult>,
    pub host: Option<String>,
    /// Seconds since the epoch.
    pub since: Option<i64>,
}

/// Requests to one destination.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
pub struct DestinationCount {
    pub host: String,
    pub sent: i64,
    pub blocked: i64,
    pub failed: i64,
    pub bytes: i64,
}

#[derive(Clone, Debug)]
pub struct Ledger {
    path: PathBuf,
}

const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS requests (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        timestamp INTEGER NOT NULL,
        host TEXT NOT NULL,
        purpose TEXT NOT NULL,
        data_kind TEXT NOT NULL,
        bytes INTEGER NOT NULL,
        result TEXT NOT NULL
    );
    CREATE INDEX IF NOT EXISTS requests_timestamp ON requests (timestamp);";

impl Ledger {
    pub fn open(path: &Path) -> Result<Self> {
        let ledger = Self {
            path: path.to_path_buf(),
        };
        ledger.connection()?.execute_batch(SCHEMA)?;
        Ok(ledger)
    }

    fn connection(&self) -> Result<Connection> {
        Ok(Connection::open(&self.path)?)
    }

    pub fn record(
        &self,
        timestamp: i64,
        host: &str,
        purpose: Purpose,
        data_kind: DataKind,
        bytes: usize,
        result: RequestResult,
    ) -> Result<i64> {
        insert(
            &self.connection()?,
            timestamp,
            host,
            purpose,
            data_kind,
            bytes,
            result,
        )
    }

    pub fn mark_failed(&self, id: i64) -> Result<()> {
        self.connection()?.execute(
            "UPDATE requests SET result = ?1 WHERE id = ?2",
            params![RequestResult::Failed.as_str(), id],
        )?;
        Ok(())
    }

    pub fn prune(&self, cutoff: i64) -> Result<usize> {
        prune(&self.connection()?, cutoff)
    }

    pub fn list(&self, filter: &LedgerFilter, limit: i64) -> Result<Vec<LedgerEntry>> {
        list(&self.connection()?, filter, limit)
    }

    pub fn by_destination(&self, since: i64) -> Result<Vec<DestinationCount>> {
        by_destination(&self.connection()?, since)
    }

    pub fn clear(&self) -> Result<()> {
        self.connection()?.execute("DELETE FROM requests", [])?;
        Ok(())
    }
}

fn insert(
    conn: &Connection,
    timestamp: i64,
    host: &str,
    purpose: Purpose,
    data_kind: DataKind,
    bytes: usize,
    result: RequestResult,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO requests (timestamp, host, purpose, data_kind, bytes, result)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            timestamp,
            host,
            purpose.as_str(),
            data_kind.as_str(),
            bytes as i64,
            result.as_str()
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Deletes everything older than `cutoff`.
fn prune(conn: &Connection, cutoff: i64) -> Result<usize> {
    Ok(conn.execute("DELETE FROM requests WHERE timestamp < ?1", params![cutoff])?)
}

fn list(conn: &Connection, filter: &LedgerFilter, limit: i64) -> Result<Vec<LedgerEntry>> {
    let mut sql = "SELECT id, timestamp, host, purpose, data_kind, bytes, result
                   FROM requests WHERE 1 = 1"
        .to_string();
    let mut values: Vec<Value> = Vec::new();
    if let Some(purpose) = filter.purpose {
        sql.push_str(" AND purpose = ?");
        values.push(Value::Text(purpose.as_str().to_string()));
    }
    if let Some(result) = filter.result {
        sql.push_str(" AND result = ?");
        values.push(Value::Text(result.as_str().to_string()));
    }
    if let Some(host) = filter.host.as_deref().filter(|h| !h.is_empty()) {
        sql.push_str(" AND host = ?");
        values.push(Value::Text(host.to_string()));
    }
    if let Some(since) = filter.since {
        sql.push_str(" AND timestamp >= ?");
        values.push(Value::Integer(since));
    }
    sql.push_str(" ORDER BY id DESC LIMIT ?");
    values.push(Value::Integer(limit));

    let mut statement = conn.prepare(&sql)?;
    let rows = statement.query_map(params_from_iter(values), |row| {
        let purpose: String = row.get(3)?;
        let kind: String = row.get(4)?;
        let result: String = row.get(6)?;
        Ok((
            LedgerEntry {
                id: row.get(0)?,
                timestamp: row.get(1)?,
                host: row.get(2)?,
                purpose: Purpose::UpdateCheck, // replaced below
                data_kind: DataKind::parse(&kind),
                bytes: row.get(5)?,
                result: RequestResult::parse(&result),
            },
            purpose,
        ))
    })?;
    let mut entries = Vec::new();
    for row in rows {
        let (mut entry, purpose) = row?;
        // A purpose from a newer version is skipped, not misreported.
        if let Some(purpose) = Purpose::parse(&purpose) {
            entry.purpose = purpose;
            entries.push(entry);
        }
    }
    Ok(entries)
}

fn by_destination(conn: &Connection, since: i64) -> Result<Vec<DestinationCount>> {
    let mut statement = conn.prepare(
        "SELECT host,
                SUM(result = 'sent'), SUM(result = 'blocked'), SUM(result = 'failed'),
                SUM(CASE WHEN result = 'sent' THEN bytes ELSE 0 END)
         FROM requests WHERE timestamp >= ?1
         GROUP BY host ORDER BY COUNT(*) DESC, host",
    )?;
    let rows = statement.query_map(params![since], |row| {
        Ok(DestinationCount {
            host: row.get(0)?,
            sent: row.get(1)?,
            blocked: row.get(2)?,
            failed: row.get(3)?,
            bytes: row.get(4)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// Stops a spreadsheet from running a field as a formula.
fn csv_field(value: &str) -> String {
    let safe = if value.starts_with(['=', '+', '-', '@']) {
        format!("'{value}")
    } else {
        value.to_string()
    };
    if safe.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", safe.replace('"', "\"\""))
    } else {
        safe
    }
}

/// The entries (newest first) as CSV. `format_time` renders a timestamp.
pub fn to_csv(entries: &[LedgerEntry], format_time: impl Fn(i64) -> String) -> String {
    let mut csv = String::from("time,destination,purpose,data,bytes,result\r\n");
    for entry in entries {
        csv.push_str(&format!(
            "{},{},{},{},{},{}\r\n",
            csv_field(&format_time(entry.timestamp)),
            csv_field(&entry.host),
            entry.purpose.as_str(),
            entry.data_kind.as_str(),
            entry.bytes,
            entry.result.as_str(),
        ));
    }
    csv
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(SCHEMA).unwrap();
        conn
    }

    fn add(conn: &Connection, timestamp: i64, host: &str, result: RequestResult) -> i64 {
        insert(
            conn,
            timestamp,
            host,
            Purpose::AssistantQuestion,
            DataKind::Text,
            100,
            result,
        )
        .unwrap()
    }

    #[test]
    fn records_and_lists_newest_first() {
        let conn = conn();
        add(&conn, 10, "a.example", RequestResult::Sent);
        add(&conn, 20, "b.example", RequestResult::Blocked);
        let entries = list(&conn, &LedgerFilter::default(), 100).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].host, "b.example");
        assert_eq!(entries[0].result, RequestResult::Blocked);
        assert_eq!(entries[1].purpose, Purpose::AssistantQuestion);
        assert_eq!(entries[1].data_kind, DataKind::Text);
        assert_eq!(entries[1].bytes, 100);
    }

    #[test]
    fn the_table_has_nowhere_to_put_content() {
        let conn = conn();
        let mut statement = conn
            .prepare("SELECT name FROM pragma_table_info('requests')")
            .unwrap();
        let columns: Vec<String> = statement
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(
            columns,
            [
                "id",
                "timestamp",
                "host",
                "purpose",
                "data_kind",
                "bytes",
                "result"
            ]
        );
    }

    #[test]
    fn filters_combine() {
        let conn = conn();
        add(&conn, 10, "a.example", RequestResult::Sent);
        add(&conn, 20, "a.example", RequestResult::Blocked);
        add(&conn, 30, "b.example", RequestResult::Blocked);
        insert(
            &conn,
            40,
            "huggingface.co",
            Purpose::ModelDownload,
            DataKind::None,
            0,
            RequestResult::Sent,
        )
        .unwrap();

        let hosts = |filter: LedgerFilter| -> Vec<i64> {
            list(&conn, &filter, 100)
                .unwrap()
                .into_iter()
                .map(|e| e.timestamp)
                .collect()
        };
        assert_eq!(
            hosts(LedgerFilter {
                host: Some("a.example".into()),
                ..Default::default()
            }),
            [20, 10]
        );
        assert_eq!(
            hosts(LedgerFilter {
                result: Some(RequestResult::Blocked),
                since: Some(25),
                ..Default::default()
            }),
            [30]
        );
        assert_eq!(
            hosts(LedgerFilter {
                purpose: Some(Purpose::ModelDownload),
                ..Default::default()
            }),
            [40]
        );
    }

    #[test]
    fn the_list_is_limited() {
        let conn = conn();
        for i in 0..5 {
            add(&conn, i, "a.example", RequestResult::Sent);
        }
        assert_eq!(list(&conn, &LedgerFilter::default(), 2).unwrap().len(), 2);
    }

    #[test]
    fn retention_deletes_only_what_is_older_than_the_cutoff() {
        let conn = conn();
        add(&conn, 100, "old.example", RequestResult::Sent);
        add(&conn, 200, "edge.example", RequestResult::Sent);
        add(&conn, 300, "new.example", RequestResult::Sent);
        assert_eq!(prune(&conn, 200).unwrap(), 1);
        let hosts: Vec<String> = list(&conn, &LedgerFilter::default(), 100)
            .unwrap()
            .into_iter()
            .map(|e| e.host)
            .collect();
        assert_eq!(hosts, ["new.example", "edge.example"]);
    }

    #[test]
    fn counts_by_destination_only_add_up_bytes_actually_sent() {
        let conn = conn();
        add(&conn, 10, "too-old.example", RequestResult::Sent);
        add(&conn, 100, "a.example", RequestResult::Sent);
        add(&conn, 110, "a.example", RequestResult::Sent);
        add(&conn, 120, "a.example", RequestResult::Blocked);
        add(&conn, 130, "a.example", RequestResult::Failed);
        add(&conn, 140, "b.example", RequestResult::Blocked);
        assert_eq!(
            by_destination(&conn, 50).unwrap(),
            [
                DestinationCount {
                    host: "a.example".into(),
                    sent: 2,
                    blocked: 1,
                    failed: 1,
                    bytes: 200,
                },
                DestinationCount {
                    host: "b.example".into(),
                    sent: 0,
                    blocked: 1,
                    failed: 0,
                    bytes: 0,
                },
            ]
        );
    }

    #[test]
    fn csv_has_a_header_and_quotes_what_needs_it() {
        let entry = LedgerEntry {
            id: 1,
            timestamp: 5,
            host: "a,b.example".into(),
            purpose: Purpose::ScreenQuestion,
            data_kind: DataKind::Image,
            bytes: 42,
            result: RequestResult::Blocked,
        };
        assert_eq!(
            to_csv(&[entry], |t| format!("t{t}")),
            "time,destination,purpose,data,bytes,result\r\nt5,\"a,b.example\",screen_question,image,42,blocked\r\n"
        );
    }

    #[test]
    fn csv_defuses_formulas() {
        assert_eq!(csv_field("=cmd()"), "'=cmd()");
        assert_eq!(csv_field("@x"), "'@x");
        assert_eq!(csv_field("api.example.com"), "api.example.com");
    }
}
