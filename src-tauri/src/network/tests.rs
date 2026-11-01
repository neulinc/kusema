use super::*;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

const SECRET: &str = "Client Jane Doe confessed privately";

struct TestGate {
    gate: Arc<Gate>,
    ledger: Ledger,
    _dir: TempDir,
}

fn test_gate(rules: Rules) -> TestGate {
    test_gate_with_retention(rules, 30)
}

fn test_gate_with_retention(rules: Rules, retention_days: u32) -> TestGate {
    let dir = TempDir::new().unwrap();
    let ledger = Ledger::open(&dir.path().join("ledger.db")).unwrap();
    let gate = Arc::new(Gate::new(
        move || GateConfig {
            rules,
            retention_days,
        },
        Some(ledger.clone()),
        || {},
    ));
    set_test_gate(gate.clone());
    TestGate {
        gate,
        ledger,
        _dir: dir,
    }
}

const PRIVILEGED: Rules = Rules {
    privileged: true,
    allow_lan: false,
    block_downloads: false,
};

fn entries(ledger: &Ledger) -> Vec<LedgerEntry> {
    ledger.list(&LedgerFilter::default(), 100).unwrap()
}

fn blocking_send(
    url: &str,
    body: &str,
    rules: Rules,
) -> (Result<u16, SendError>, Vec<LedgerEntry>) {
    let test = test_gate(rules);
    let client = client_builder(Purpose::AssistantQuestion, url)
        .build()
        .unwrap();
    let request = client.post(url).body(body.to_string());
    let result =
        tauri::async_runtime::block_on(send(Purpose::AssistantQuestion, DataKind::Text, request))
            .map(|response| response.status().as_u16());
    let listed = entries(&test.ledger);
    (result, listed)
}

/// A one-shot HTTP server on loopback that answers with `response`.
fn serve_once(response: &'static str) -> String {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut buffer = [0u8; 4096];
            let _ = stream.read(&mut buffer);
            let _ = stream.write_all(response.as_bytes());
        }
    });
    format!("http://{address}/v1/chat?api_key=hidden")
}

const OK: &str = "HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";

#[test]
fn privileged_mode_blocks_a_cloud_request_without_sending_it() {
    let (result, listed) =
        blocking_send("https://api.anthropic.com/v1/messages", SECRET, PRIVILEGED);
    assert!(matches!(
        result,
        Err(SendError::Blocked(Blocked {
            verdict: BlockReason::CloudAi,
            ..
        }))
    ));
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].host, "api.anthropic.com");
    assert_eq!(listed[0].result, RequestResult::Blocked);
    assert_eq!(listed[0].purpose, Purpose::AssistantQuestion);
    assert_eq!(listed[0].data_kind, DataKind::Text);
    assert_eq!(listed[0].bytes, SECRET.len() as i64);
}

#[test]
fn privileged_mode_lets_loopback_through() {
    let url = serve_once(OK);
    let (result, listed) = blocking_send(&url, SECRET, PRIVILEGED);
    assert_eq!(result.unwrap(), 200);
    assert_eq!(listed[0].result, RequestResult::Sent);
    assert_eq!(listed[0].host, "127.0.0.1");
}

#[test]
fn a_lan_address_needs_the_setting() {
    let url = "http://192.168.1.20:11434/v1/chat";
    let (result, listed) = blocking_send(url, SECRET, PRIVILEGED);
    assert!(matches!(
        result,
        Err(SendError::Blocked(Blocked {
            verdict: BlockReason::LanAi,
            ..
        }))
    ));
    assert_eq!(listed[0].result, RequestResult::Blocked);

    let test = test_gate(Rules {
        allow_lan: true,
        ..PRIVILEGED
    });
    assert_eq!(
        test.gate
            .verdict(Purpose::AssistantQuestion, "192.168.1.20"),
        None
    );
    assert_eq!(
        test.gate
            .verdict(Purpose::AssistantQuestion, "api.openai.com"),
        Some(BlockReason::CloudAi)
    );
}

#[test]
fn with_privileged_mode_off_a_request_goes_and_is_listed() {
    let url = serve_once(OK);
    let (result, listed) = blocking_send(&url, SECRET, Rules::default());
    assert_eq!(result.unwrap(), 200);
    assert_eq!(listed[0].result, RequestResult::Sent);
    assert_eq!(listed[0].bytes, SECRET.len() as i64);
}

#[test]
fn a_request_that_cannot_connect_is_listed_as_failed() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/", listener.local_addr().unwrap());
    drop(listener);
    let (result, listed) = blocking_send(&url, SECRET, Rules::default());
    assert!(matches!(result, Err(SendError::Http(_))));
    assert_eq!(listed[0].result, RequestResult::Failed);
}

/// The redirect a local server could use to get text to the cloud.
#[test]
fn a_local_server_cannot_redirect_a_request_to_the_cloud() {
    let url = serve_once(
        "HTTP/1.1 307 Temporary Redirect\r\nLocation: https://api.openai.com/v1/chat\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
    );
    let (result, listed) = blocking_send(&url, SECRET, PRIVILEGED);
    assert!(
        matches!(
            result,
            Err(SendError::Blocked(Blocked {
                verdict: BlockReason::CloudAi,
                ..
            }))
        ),
        "{result:?}"
    );
    assert!(listed
        .iter()
        .any(|e| e.host == "api.openai.com" && e.result == RequestResult::Blocked));
}

#[test]
fn downloads_stay_allowed_and_listed_unless_also_blocked() {
    let test = test_gate(PRIVILEGED);
    announce(Purpose::ModelDownload, "https://huggingface.co").unwrap();
    let listed = entries(&test.ledger);
    assert_eq!(listed[0].host, "huggingface.co");
    assert_eq!(listed[0].purpose, Purpose::ModelDownload);
    assert_eq!(listed[0].data_kind, DataKind::None);
    assert_eq!(listed[0].bytes, 0);
    assert_eq!(listed[0].result, RequestResult::Sent);

    let test = test_gate(Rules {
        block_downloads: true,
        ..PRIVILEGED
    });
    let blocked = announce(Purpose::UpdateCheck, "https://github.com/x/latest.json").unwrap_err();
    assert_eq!(blocked.verdict, BlockReason::Download);
    assert!(is_blocked(&blocked.to_string()));
    assert_eq!(entries(&test.ledger)[0].result, RequestResult::Blocked);
    assert!(would_block(
        Purpose::VoiceDownload,
        "https://huggingface.co/x"
    ));
}

#[test]
fn the_ledger_records_sizes_but_never_content_or_paths() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("ledger.db");
    let ledger = Ledger::open(&path).unwrap();
    set_test_gate(Arc::new(Gate::new(
        || GateConfig {
            rules: PRIVILEGED,
            retention_days: 30,
        },
        Some(ledger.clone()),
        || {},
    )));
    let url = "https://api.openai.com/v1/secretpath?api_key=SECRETKEY";
    let client = client_builder(Purpose::ScreenQuestion, url)
        .build()
        .unwrap();
    let request = client
        .post(url)
        .header("Authorization", "Bearer SECRETKEY")
        .body(SECRET.to_string());
    let _ = tauri::async_runtime::block_on(send(Purpose::ScreenQuestion, DataKind::Image, request));

    let listed = entries(&ledger);
    assert_eq!(listed[0].bytes, SECRET.len() as i64);
    assert_eq!(listed[0].data_kind, DataKind::Image);
    drop(ledger);

    let raw = std::fs::read(&path).unwrap();
    let raw = String::from_utf8_lossy(&raw);
    assert!(
        raw.contains("api.openai.com"),
        "the host is what's recorded"
    );
    for private in ["Jane Doe", "SECRETKEY", "secretpath", "api_key"] {
        assert!(!raw.contains(private), "{private} reached the ledger file");
    }
}

#[test]
fn retention_removes_old_entries_when_they_come_due() {
    let test = test_gate_with_retention(Rules::default(), 30);
    let days_ago = |days: i64| now() - days * SECS_PER_DAY;
    let add = |timestamp, host: &str| {
        test.ledger
            .record(
                timestamp,
                host,
                Purpose::UpdateCheck,
                DataKind::None,
                0,
                RequestResult::Sent,
            )
            .unwrap();
    };
    add(days_ago(31), "old.example");
    add(days_ago(29), "recent.example");

    test.gate.prune();
    let hosts: Vec<String> = entries(&test.ledger).into_iter().map(|e| e.host).collect();
    assert_eq!(hosts, ["recent.example"]);
}

#[test]
fn recording_a_request_also_cleans_up_when_due() {
    let test = test_gate_with_retention(Rules::default(), 30);
    test.ledger
        .record(
            now() - 40 * SECS_PER_DAY,
            "old.example",
            Purpose::UpdateCheck,
            DataKind::None,
            0,
            RequestResult::Sent,
        )
        .unwrap();
    announce(Purpose::UpdateCheck, "https://github.com").unwrap();
    let hosts: Vec<String> = entries(&test.ledger).into_iter().map(|e| e.host).collect();
    assert_eq!(hosts, ["github.com"]);
}

#[test]
fn the_refusal_names_the_provider() {
    assert_eq!(
        refusal_message("Anthropic"),
        "Privileged mode is on, so this wasn't sent to Anthropic. Use a local model or turn off Privileged mode."
    );
}

fn rust_files(dir: &Path, found: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_files(&path, found);
        } else if path.extension().is_some_and(|e| e == "rs") {
            found.push(path);
        }
    }
}

/// The promise "every request goes through here" is only true while nobody adds
/// another way out, for example in a merge from Handy. If this fails, route the
/// new request through `network::send`, or `announce` if a library makes it.
#[test]
fn nothing_else_in_the_source_makes_http_requests_directly() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    rust_files(&src, &mut files);
    assert!(files.len() > 50);

    // Written in pieces so this file doesn't match itself.
    let banned = [
        concat!("reqwest", "::Client::"),
        concat!("reqwest", "::get("),
        concat!("reqwest", "::blocking"),
        concat!("ClientBuilder", "::new"),
        concat!("ApiBuilder", "::"),
        concat!("ureq", "::"),
        concat!("hyper", "::Client"),
        concat!("TcpStream", "::connect"),
    ];
    let allowed = [
        // The gate itself.
        "network/mod.rs",
        // hf_hub: gated with `announce` immediately before its use.
        "managers/model.rs",
    ];
    let mut offenders = Vec::new();
    for file in &files {
        let name = file
            .strip_prefix(&src)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        if allowed.contains(&name.as_str()) || name.ends_with("tests.rs") {
            continue;
        }
        let text = std::fs::read_to_string(file).unwrap();
        // Tests may talk to their own local servers.
        let code = text.split("#[cfg(test)]").next().unwrap_or_default();
        for pattern in banned {
            if code.contains(pattern) {
                offenders.push(format!("{name}: {pattern}"));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "direct HTTP outside network: {offenders:#?}"
    );
}
