//! Kusema link reading: fetches the one page the user named and keeps its
//! readable text. Only http(s) on the public internet (checked before the
//! request, at every redirect and when names resolve: see
//! `network::public_only`), at most 3 redirects, 2 MB, 15 seconds; no cookies,
//! no login, no JavaScript, and no link on the page is ever followed.

use crate::network::{self, public_only, DataKind, Purpose, RequestResult, WebKind};
use std::time::Duration;

/// Bytes of page read at most; the rest is never downloaded.
pub const MAX_BYTES: usize = 2 * 1024 * 1024;
pub const TIMEOUT: Duration = Duration::from_secs(15);

/// A page, as read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Page {
    /// Where it ended up, after redirects.
    pub url: String,
    pub title: String,
    pub text: String,
    /// The page was longer than `MAX_BYTES`.
    pub truncated: bool,
    pub bytes: usize,
}

/// What kind of content Kusema can read.
fn readable(content_type: &str) -> Option<bool> {
    let kind = content_type
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    match kind.as_str() {
        "text/html" | "application/xhtml+xml" | "" => Some(true),
        "text/plain" | "text/markdown" => Some(false),
        _ => None,
    }
}

fn host_of(url: &reqwest::Url) -> String {
    url.host_str().unwrap_or_default().to_string()
}

/// Says why a request failed, in plain words.
fn describe(error: &network::SendError) -> String {
    match error {
        network::SendError::Blocked(blocked) => blocked.to_string(),
        network::SendError::Http(e) => {
            if let Some(not_public) = public_only::not_public_in(e) {
                return not_public.to_string();
            }
            if e.is_timeout() {
                "the page took longer than 15 seconds to answer".to_string()
            } else if e.is_redirect() {
                "the page redirected too many times".to_string()
            } else if e.is_connect() {
                "couldn't connect to the site".to_string()
            } else {
                e.to_string()
            }
        }
    }
}

/// Reads the page at `address`, listing it in the ledger with the bytes received.
pub async fn read(address: &str) -> Result<Page, String> {
    read_with(address, TIMEOUT).await
}

async fn read_with(address: &str, timeout: Duration) -> Result<Page, String> {
    let url = reqwest::Url::parse(address.trim())
        .map_err(|_| public_only::NotPublic::Invalid.to_string())?;
    if let Err(not_public) = public_only::check_url(&url) {
        network::record_web(
            WebKind::Fetch,
            &host_of(&url),
            url.as_str(),
            RequestResult::Blocked,
        );
        return Err(not_public.to_string());
    }
    let id = network::record_web(
        WebKind::Fetch,
        &host_of(&url),
        url.as_str(),
        RequestResult::Sent,
    );
    let mut received = 0usize;
    let outcome = fetch(&url, timeout, &mut received).await;
    let result = match &outcome {
        Ok(_) => RequestResult::Sent,
        Err(e) if network::is_blocked(e) || network::is_blocked_by_admin(e) => {
            RequestResult::Blocked
        }
        Err(_) => RequestResult::Failed,
    };
    network::finish_web(id, result, None, Some(received));
    outcome
}

async fn fetch(
    url: &reqwest::Url,
    timeout: Duration,
    received: &mut usize,
) -> Result<Page, String> {
    let client = network::link_client_builder(timeout)
        .build()
        .map_err(|e| e.to_string())?;
    let request = client.get(url.clone()).header(
        "Accept",
        "text/html,application/xhtml+xml,text/plain;q=0.9,*/*;q=0.1",
    );
    let mut response = network::send(Purpose::LinkFetch, DataKind::None, request)
        .await
        .map_err(|e| describe(&e))?;
    let status = response.status();
    if status.is_redirection() {
        // Only a redirect that wasn't followed gets here: one to an address
        // that isn't a public web page (file:, ftp:...).
        return Err(
            "the page redirected somewhere Kusema doesn't open (it only reads public web pages)"
                .to_string(),
        );
    }
    if !status.is_success() {
        return Err(format!("the site answered {status}"));
    }
    let final_url = response.url().to_string();
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();
    let Some(is_html) = readable(&content_type) else {
        return Err(format!(
            "it isn't a web page Kusema can read ({}); Kusema reads web pages and plain text",
            content_type.split(';').next().unwrap_or_default().trim()
        ));
    };
    let mut body: Vec<u8> = Vec::new();
    let mut truncated = false;
    let read_all = async {
        while let Some(chunk) = response.chunk().await.map_err(|e| {
            if e.is_timeout() {
                "the page took longer than 15 seconds to answer".to_string()
            } else {
                e.to_string()
            }
        })? {
            let room = MAX_BYTES - body.len();
            if chunk.len() >= room {
                body.extend_from_slice(&chunk[..room]);
                truncated = true;
                break;
            }
            body.extend_from_slice(&chunk);
        }
        Ok::<(), String>(())
    };
    let read_result = read_all.await;
    *received = body.len();
    read_result?;
    let raw = String::from_utf8_lossy(&body);
    let (title, text) = if is_html {
        (title_of(&raw), html_to_text(&raw))
    } else {
        (String::new(), raw.trim().to_string())
    };
    if text.trim().is_empty() {
        return Err("the page has no text Kusema can read (it may need JavaScript)".to_string());
    }
    Ok(Page {
        url: final_url,
        title,
        text,
        truncated,
        bytes: body.len(),
    })
}

fn re(pattern: &str) -> regex::Regex {
    regex::Regex::new(pattern).expect("valid regex")
}

/// The page's `<title>`.
pub fn title_of(html: &str) -> String {
    re(r"(?is)<title\b[^>]*>(.*?)</title\s*>")
        .captures(html)
        .and_then(|c| c.get(1))
        .map(|m| {
            decode_entities(&re(r"\s+").replace_all(m.as_str(), " "))
                .trim()
                .to_string()
        })
        .unwrap_or_default()
}

/// The readable text of an HTML page: no scripts, styles, navigation or
/// markup; paragraphs and list items on their own lines.
pub fn html_to_text(html: &str) -> String {
    let mut text = re(r"(?s)<!--.*?-->").replace_all(html, " ").to_string();
    for element in [
        "script", "style", "noscript", "template", "svg", "head", "title", "nav", "footer",
        "header", "aside", "form", "iframe", "button", "select",
    ] {
        text = re(&format!(r"(?is)<{element}\b.*?</{element}\s*>"))
            .replace_all(&text, " ")
            .to_string();
    }
    text = re(r"(?i)<li\b[^>]*>")
        .replace_all(&text, "\n• ")
        .to_string();
    text = re(r"(?i)<(br|hr)\b[^>]*>")
        .replace_all(&text, "\n")
        .to_string();
    text = re(r"(?i)</?(p|div|section|article|main|h[1-6]|tr|table|ul|ol|blockquote|pre|dd|dt|figcaption)\b[^>]*>")
        .replace_all(&text, "\n")
        .to_string();
    text = re(r"(?i)</t[dh]\s*>")
        .replace_all(&text, " \t ")
        .to_string();
    text = re(r"(?s)<[^>]*>").replace_all(&text, "").to_string();
    let text = decode_entities(&text);
    let lines: Vec<String> = text
        .lines()
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .collect();
    let mut out = String::new();
    let mut blank = 0;
    for line in lines {
        if line.is_empty() || line == "•" {
            blank += 1;
            continue;
        }
        if !out.is_empty() {
            out.push_str(if blank > 0 { "\n\n" } else { "\n" });
        }
        out.push_str(&line);
        blank = 0;
    }
    out
}

/// HTML character references: the common named ones and all numeric ones.
pub fn decode_entities(text: &str) -> String {
    re(r"&(#[0-9]{1,7}|#[xX][0-9a-fA-F]{1,6}|[a-zA-Z]{2,8});")
        .replace_all(text, |caps: &regex::Captures| {
            let entity = &caps[1];
            let decoded = if let Some(hex) = entity
                .strip_prefix("#x")
                .or_else(|| entity.strip_prefix("#X"))
            {
                u32::from_str_radix(hex, 16).ok().and_then(char::from_u32)
            } else if let Some(dec) = entity.strip_prefix('#') {
                dec.parse::<u32>().ok().and_then(char::from_u32)
            } else {
                match entity {
                    "amp" => Some('&'),
                    "lt" => Some('<'),
                    "gt" => Some('>'),
                    "quot" => Some('"'),
                    "apos" => Some('\''),
                    "nbsp" => Some(' '),
                    "ndash" => Some('–'),
                    "mdash" => Some('—'),
                    "hellip" => Some('…'),
                    "lsquo" => Some('‘'),
                    "rsquo" => Some('’'),
                    "ldquo" => Some('“'),
                    "rdquo" => Some('”'),
                    "copy" => Some('©'),
                    "reg" => Some('®'),
                    "trade" => Some('™'),
                    "sect" => Some('§'),
                    "para" => Some('¶'),
                    "middot" => Some('·'),
                    "bull" => Some('•'),
                    "euro" => Some('€'),
                    "pound" => Some('£'),
                    _ => None,
                }
            };
            decoded.map_or_else(|| caps[0].to_string(), |c| c.to_string())
        })
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    #[test]
    fn readable_text_without_scripts_styles_or_navigation() {
        let html = r#"<!doctype html><html><head><title>Filing &amp; Fees</title>
            <style>body{color:red}</style><script>steal()</script></head>
            <body><nav><a href="/x">Menu</a></nav>
            <h1>Fees</h1><p>The fee is&nbsp;$402.</p><!-- hidden -->
            <ul><li>One</li><li>Two &#8211; three</li></ul>
            <script type="text/javascript">alert('x')</script>
            <footer>© Court</footer></body></html>"#;
        assert_eq!(title_of(html), "Filing & Fees");
        let text = html_to_text(html);
        assert!(text.contains("Fees"));
        assert!(text.contains("The fee is $402."));
        assert!(text.contains("• One"));
        assert!(text.contains("• Two – three"));
        for gone in [
            "steal",
            "alert",
            "color:red",
            "Menu",
            "hidden",
            "© Court",
            "<",
            ">",
        ] {
            assert!(!text.contains(gone), "{gone} in {text}");
        }
    }

    #[test]
    fn only_pages_and_plain_text_are_read() {
        assert_eq!(readable("text/html; charset=utf-8"), Some(true));
        assert_eq!(readable("text/plain"), Some(false));
        assert_eq!(readable("application/pdf"), None);
        assert_eq!(readable("application/octet-stream"), None);
        assert_eq!(readable("image/png"), None);
    }

    #[test]
    fn non_web_and_local_links_are_refused_before_anything_is_sent() {
        for address in [
            "file:///C:/Windows/win.ini",
            "ftp://example.com/x",
            "http://localhost:8080/",
            "http://127.0.0.1/",
            "http://192.168.1.10/",
            "http://169.254.169.254/latest/meta-data/",
            "http://[::1]/",
            "not a url",
        ] {
            let error = tauri::async_runtime::block_on(read(address)).unwrap_err();
            assert!(!error.is_empty(), "{address}");
        }
    }

    /// A server on loopback: only reachable for these tests, which call
    /// `fetch` with a client that's allowed to reach it.
    fn serve(responses: Vec<String>) -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            for response in responses {
                if let Ok((mut stream, _)) = listener.accept() {
                    let mut buffer = [0u8; 4096];
                    let _ = stream.read(&mut buffer);
                    let _ = stream.write_all(response.as_bytes());
                }
            }
        });
        format!("http://{address}")
    }

    fn page(port: &str, path: &str) -> String {
        format!("http://site.{}:{port}{path}", public_only::TEST_DOMAIN)
    }

    fn port_of(server: &str) -> String {
        server.rsplit(':').next().unwrap().to_string()
    }

    fn ok_html(body: &str) -> String {
        format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    }

    fn redirect(to: &str) -> String {
        format!("HTTP/1.1 302 Found\r\nLocation: {to}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
    }

    #[test]
    fn a_page_is_read_as_text() {
        let server = serve(vec![ok_html(
            "<html><title>Rules</title><body><p>Deadline is 30 days.</p></body></html>",
        )]);
        let read =
            tauri::async_runtime::block_on(read(&page(&port_of(&server), "/rules"))).unwrap();
        assert_eq!(read.title, "Rules");
        assert_eq!(read.text, "Deadline is 30 days.");
        assert!(!read.truncated);
    }

    #[test]
    fn a_redirect_to_this_computer_or_the_office_network_is_refused() {
        for target in [
            "http://127.0.0.1:9/secret",
            "http://localhost:9/secret",
            "http://192.168.1.1/",
            "http://169.254.169.254/latest/meta-data/",
            "file:///C:/Windows/win.ini",
        ] {
            let server = serve(vec![redirect(target)]);
            let error =
                tauri::async_runtime::block_on(read(&page(&port_of(&server), "/go"))).unwrap_err();
            assert!(
                error.contains("private network")
                    || error.contains("only reads web pages")
                    || error.contains("doesn't open"),
                "{target}: {error}"
            );
        }
    }

    #[test]
    fn at_most_three_redirects_are_followed() {
        // Four hops to reach the page: one too many.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port().to_string();
        let hops: Vec<String> = (1..=4)
            .map(|i| redirect(&page(&port, &format!("/hop{i}"))))
            .chain(std::iter::once(ok_html("<p>end</p>")))
            .collect();
        std::thread::spawn(move || {
            for response in hops {
                if let Ok((mut stream, _)) = listener.accept() {
                    let mut buffer = [0u8; 4096];
                    let _ = stream.read(&mut buffer);
                    let _ = stream.write_all(response.as_bytes());
                }
            }
        });
        let error = tauri::async_runtime::block_on(read(&page(&port, "/start"))).unwrap_err();
        assert!(error.contains("redirected too many times"), "{error}");

        // Three hops is fine.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port().to_string();
        let hops: Vec<String> = (1..=3)
            .map(|i| redirect(&page(&port, &format!("/hop{i}"))))
            .chain(std::iter::once(ok_html("<p>end</p>")))
            .collect();
        std::thread::spawn(move || {
            for response in hops {
                if let Ok((mut stream, _)) = listener.accept() {
                    let mut buffer = [0u8; 4096];
                    let _ = stream.read(&mut buffer);
                    let _ = stream.write_all(response.as_bytes());
                }
            }
        });
        let read = tauri::async_runtime::block_on(read(&page(&port, "/start"))).unwrap();
        assert_eq!(read.text, "end");
    }

    #[test]
    fn a_huge_page_stops_at_two_megabytes() {
        let body = format!("<p>{}</p>", "a".repeat(3 * 1024 * 1024));
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let server = serve(vec![response]);
        let read = tauri::async_runtime::block_on(read(&page(&port_of(&server), "/big"))).unwrap();
        assert!(read.truncated);
        assert_eq!(read.bytes, MAX_BYTES);
    }

    #[test]
    fn a_slow_page_times_out() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port().to_string();
        std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buffer = [0u8; 4096];
                let _ = stream.read(&mut buffer);
                std::thread::sleep(Duration::from_secs(3));
                let _ = stream.write_all(ok_html("<p>late</p>").as_bytes());
            }
        });
        let started = std::time::Instant::now();
        let error = tauri::async_runtime::block_on(read_with(
            &page(&port, "/slow"),
            Duration::from_millis(500),
        ))
        .unwrap_err();
        assert!(started.elapsed() < Duration::from_secs(3));
        assert!(error.contains("longer than"), "{error}");
    }

    #[test]
    fn files_that_arent_pages_are_not_read() {
        let server = serve(vec![
            "HTTP/1.1 200 OK\r\nContent-Type: application/pdf\r\nContent-Length: 4\r\nConnection: close\r\n\r\n%PDF".into(),
        ]);
        let error =
            tauri::async_runtime::block_on(read(&page(&port_of(&server), "/x.pdf"))).unwrap_err();
        assert!(error.contains("application/pdf"), "{error}");
    }

    #[test]
    fn privileged_mode_blocks_link_reading_and_the_ledger_says_so() {
        use crate::network::{Gate, GateConfig, Ledger, Rules};
        let dir = tempfile::TempDir::new().unwrap();
        let ledger = Ledger::open(&dir.path().join("l.db")).unwrap();
        network::set_test_gate(std::sync::Arc::new(Gate::new(
            || GateConfig {
                rules: Rules {
                    privileged: true,
                    ..Default::default()
                },
                retention_days: 30,
            },
            Some(ledger.clone()),
            || {},
        )));
        let error = tauri::async_runtime::block_on(read("https://example.com/")).unwrap_err();
        assert!(error.contains("Privileged mode"), "{error}");
        let web = ledger.list_web(None, 10).unwrap();
        assert_eq!(web.len(), 1);
        assert_eq!(web[0].detail, "https://example.com/");
        assert_eq!(web[0].result, RequestResult::Blocked);
    }

    #[test]
    fn the_admin_switch_blocks_link_reading() {
        use crate::network::{Gate, GateConfig, Rules};
        network::set_test_gate(std::sync::Arc::new(Gate::new(
            || GateConfig {
                rules: Rules {
                    web_disabled: true,
                    ..Default::default()
                },
                retention_days: 30,
            },
            None,
            || {},
        )));
        let error = tauri::async_runtime::block_on(read("https://example.com/")).unwrap_err();
        assert!(error.contains("administrator"), "{error}");
    }

    #[test]
    fn a_link_to_this_computer_is_refused_even_by_name() {
        // A server really is listening; the name must still not reach it.
        let server = serve(vec![
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: 2\r\n\r\nhi".into(),
        ]);
        let port = server.rsplit(':').next().unwrap();
        let error =
            tauri::async_runtime::block_on(read(&format!("http://localhost:{port}/"))).unwrap_err();
        assert!(error.contains("private network"), "{error}");
    }
}
