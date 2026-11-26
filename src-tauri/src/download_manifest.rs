//! Kusema: every file the app can download, with the size and SHA-256 it must
//! have, for the release check. Built from the same code the downloads use, so
//! it can't drift from them:
//!
//!   KUSEMA_DOWNLOAD_MANIFEST=manifest.json cargo test --lib \
//!     download_manifest::writes_the_manifest -- --ignored
//!   python scripts/check_downloads.py manifest.json
//!
//! The release workflow runs both and stops the release if any URL doesn't
//! answer 200 with the pinned size and hash.

use serde::Serialize;

#[derive(Debug, Serialize)]
struct Download {
    /// What it is, for the report.
    what: String,
    url: String,
    /// Bytes; `None` where the app pins only the hash (the legacy models).
    size: Option<u64>,
    sha256: String,
}

/// The legacy Whisper and Breeze files, read from where `ModelManager`
/// registers them (only its constructor lists them): each `url:` line, then
/// the pinned hash that follows it.
fn legacy_models() -> Vec<Download> {
    let source = include_str!("managers/model.rs");
    let lines: Vec<&str> = source.lines().collect();
    let mut out = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let Some(rest) = line.trim().strip_prefix("url: \"") else {
            continue;
        };
        let url = rest.split('"').next().unwrap_or_default();
        if !url.starts_with("https://") || url.contains("example.com") {
            continue;
        }
        let sha256 = lines[i + 1..(i + 8).min(lines.len())]
            .iter()
            .flat_map(|l| l.split('"'))
            .find(|part| part.len() == 64 && part.bytes().all(|b| b.is_ascii_hexdigit()))
            .unwrap_or_else(|| panic!("no pinned hash after {url}"))
            .to_string();
        out.push(Download {
            what: "legacy model".to_string(),
            url: url.to_string(),
            size: None,
            sha256,
        });
    }
    out
}

fn manifest() -> Vec<Download> {
    let mut all: Vec<Download> = crate::catalog::download_urls()
        .into_iter()
        .map(|(what, url, size, sha256)| Download {
            what,
            url,
            size: Some(size),
            sha256,
        })
        .collect();
    all.extend(legacy_models());
    use crate::tts::kokoro::assets;
    // Every platform's ONNX Runtime (the small zip, then the NuGet package),
    // not just the one this check happens to run on.
    for (zip, source) in assets::every_runtime_source() {
        if all.iter().any(|d| d.url == source.url) {
            continue; // the NuGet package serves every platform
        }
        all.push(Download {
            what: format!("Kokoro: ONNX Runtime ({zip} or its fallback)"),
            url: source.url,
            size: Some(source.size),
            sha256: source.sha256.to_string(),
        });
    }
    for asset in assets::assets() {
        for source in asset.sources {
            if all.iter().any(|d| d.url == source.url) {
                continue; // this platform's runtime, already listed
            }
            all.push(Download {
                what: format!("Kokoro: {}", asset.file),
                url: source.url,
                size: Some(source.size),
                sha256: source.sha256.to_string(),
            });
        }
    }
    all
}

#[test]
fn every_download_is_pinned_and_listed_once() {
    let all = manifest();
    let mut urls: Vec<&str> = all.iter().map(|d| d.url.as_str()).collect();
    urls.sort_unstable();
    let count = urls.len();
    urls.dedup();
    assert_eq!(urls.len(), count, "a URL is listed twice");
    for d in &all {
        assert!(d.url.starts_with("https://"), "{}", d.url);
        assert!(
            !d.url.contains("/latest"),
            "never resolve latest: {}",
            d.url
        );
        assert_eq!(d.sha256.len(), 64, "{}", d.url);
    }
    // Every kind of download is in it: models, mirrors, the language
    // detector, the legacy models, and Kokoro's runtime, model, voices and
    // dictionaries.
    assert!(all.iter().any(|d| d.what == "model"));
    assert!(all.iter().any(|d| d.what == "model mirror"));
    assert!(all.iter().any(|d| d.what == "language detector"));
    assert_eq!(all.iter().filter(|d| d.what == "legacy model").count(), 5);
    // ONNX Runtime: every platform's small zip, wherever this runs, and the
    // full NuGet package once.
    for platform in [
        "win-x64",
        "win-arm64",
        "osx-arm64",
        "linux-x64",
        "linux-arm64",
    ] {
        let zip = format!("kusema/onnxruntime/resolve/caf47470a42e102ffcbfc53047b21166c9007924/onnxruntime-1.31.1-{platform}.zip");
        assert!(all.iter().any(|d| d.url.ends_with(&zip)), "{platform}");
    }
    assert_eq!(
        all.iter()
            .filter(|d| d.url.contains("api.nuget.org"))
            .count(),
        1
    );
    assert!(all.iter().any(|d| d.url.ends_with("us_gold.json")));
    assert!(all.iter().any(|d| d.url.ends_with("af_heart.bin")));
}

/// Writes the manifest to `KUSEMA_DOWNLOAD_MANIFEST` for the release check.
#[test]
#[ignore]
fn writes_the_manifest() {
    let path = std::env::var("KUSEMA_DOWNLOAD_MANIFEST").expect("KUSEMA_DOWNLOAD_MANIFEST");
    let all = manifest();
    std::fs::write(&path, serde_json::to_string_pretty(&all).unwrap()).unwrap();
    println!("{} downloads written to {path}", all.len());
}
