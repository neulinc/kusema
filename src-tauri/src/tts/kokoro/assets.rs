//! Kokoro's files: what to download, where it lives, and the download itself.
//!
//! Everything is pinned (revision + size + SHA-256) and fetched with Handy's
//! resumable model downloader into `<app data>/kokoro/`:
//! - ONNX Runtime, the official Microsoft build. Only its shared library is
//!   kept; `ort` loads it at runtime (`load-dynamic`).
//! - The Kokoro-82M v1.0 int8 ONNX model and the English voice styles, from
//!   onnx-community/Kokoro-82M-v1.0-ONNX (Apache-2.0).
//! - misaki's US and UK pronunciation dictionaries (Apache-2.0), Kokoro's own
//!   phonemizer data.

use super::Kokoro;
use crate::managers::model::download::{HttpDownloadEvent, HttpDownloadOutcome};
use crate::managers::model::ModelManager;
use crate::settings;
use hf_hub::api::tokio::CancellationToken;
use log::{info, warn};
use serde::Serialize;
use specta::Type;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, State};

const MODEL_BASE: &str = "https://huggingface.co/onnx-community/Kokoro-82M-v1.0-ONNX/resolve/1939ad2a8e416c0acfeecc08a694d14ef25f2231";
const MISAKI_BASE: &str =
    "https://raw.githubusercontent.com/hexgrad/misaki/fba1236595f2d2bf21d414ba6e57d25256afada3/misaki/data";
const ORT_BASE: &str = "https://github.com/microsoft/onnxruntime/releases/download/v1.31.1";

pub const MODEL_FILE: &str = "kokoro-v1.0-int8.onnx";
pub const VOCAB_FILE: &str = "tokenizer.json";
pub const VOICES_DIR: &str = "voices";

/// The voice the picker switches to after the first download.
pub const DEFAULT_VOICE: &str = "af_heart";

/// The English voices (`a` = American, `b` = British; `f`/`m` = female/male),
/// with each file's SHA-256. Every file is 522,240 bytes (510 × 256 floats).
pub const VOICES: &[(&str, &str)] = &[
    (
        "af_alloy",
        "c4a6b876047fd7fb472edf4ebd63cfac7c3b958a7cae7c106e8f038ca6308c45",
    ),
    (
        "af_aoede",
        "4a004c33430762e2461eedb2013fad808ef4ab3121f5300f554476caf58d8361",
    ),
    (
        "af_bella",
        "f69d836209b78eb8c66e75e3cda491e26ea838a3674257e9d4e5703cbaf55c8b",
    ),
    (
        "af_heart",
        "d583ccff3cdca2f7fae535cb998ac07e9fcb90f09737b9a41fa2734ec44a8f0b",
    ),
    (
        "af_jessica",
        "a240a5e3c15b43563d6e923bdca8ef5613a23471d9b77653694012435df23bd8",
    ),
    (
        "af_kore",
        "9be5221b6a941c04b561959b8ff0b06e809444dcc4ab7e75a7b23606f691819e",
    ),
    (
        "af_nicole",
        "cd2191ab31b914ed7b318416b0e4440fdf392ddad9106a060819aa600a64f59a",
    ),
    (
        "af_nova",
        "18778272caa0d0eebaea251c35fd635f038434f9eee5e691d02a174bd328414f",
    ),
    (
        "af_river",
        "00a2bcf82b1d86e8f19902ede58c65ccf6c0e43b44b7d74fad54e5d8933c9c30",
    ),
    (
        "af_sarah",
        "4409fbc125afabacc615d94db5398d847006a737b0247d6892b7a9a0007a2f0a",
    ),
    (
        "af_sky",
        "4435255c9744f3f31659e0d714ab7689bf65d9e77ec1cce060f083912614f0b9",
    ),
    (
        "am_adam",
        "162b035ed91cfc48b6046982184c645f72edcdd1b82843347f605d7bf7b15716",
    ),
    (
        "am_echo",
        "3968b92c3c4cd1c4416dbded36c13eaa388a90d5788d02a13e4d781f5f8cf3c3",
    ),
    (
        "am_eric",
        "e8b5be17edd1e3636901ce7598baafe2dc8dd8ff707a0c23bf9e461add7e2832",
    ),
    (
        "am_fenrir",
        "c27989f741f7ee34d273a39d8a595cc0837d35f5ced9a29b7cc162614616df43",
    ),
    (
        "am_liam",
        "52403be32fd047c6a44517cb0bcd6b134f2a18baa73e70ef41651e0eab921ade",
    ),
    (
        "am_michael",
        "1d1f21dd8da39c30705cd4c75d039d265e9bc4a2a93ed09bc9e1b1225eb95ba1",
    ),
    (
        "am_onyx",
        "da5d135b424164916d75a68ffb4c2abce3d7d5ccc82dd1ee6cf447ce286145e6",
    ),
    (
        "am_puck",
        "fcf73c989033e9233e0b98713eca600c8c74dcc1614b37009d5450ff4a2274a0",
    ),
    (
        "am_santa",
        "61150cf726ab6c5ed7a99f90a304f91f5a72c00c592e89ec94e5df11c319227a",
    ),
    (
        "bf_alice",
        "08afa6ba24da61ea5e8efa139e5aadc938d83f0a6da5a900adaf763ac1da5573",
    ),
    (
        "bf_emma",
        "669fe0647f9dd04fcab92f1439a40eeb4c8b4ab1f82e4996fe3d918ce4a63b73",
    ),
    (
        "bf_isabella",
        "3754352c4aaa46d17f27654ab7518d65b62ad6163a0f55a5f4330c2da2c4e94f",
    ),
    (
        "bf_lily",
        "5e0ee32ebe64a467124976b14e69590746f1c4ce41a12b587a50c862edfea335",
    ),
    (
        "bm_daniel",
        "6b3194bbceffb746733cbc22c8f593dd44e401a71d53895a2dca891bc595a1e8",
    ),
    (
        "bm_fable",
        "f889083196807b4adb15e9204252165f503b8d33d3982e681c52443c49d798f1",
    ),
    (
        "bm_george",
        "c4b235a4c1f2cd3b939fed08b899ce9385638b763f7b73a59616c4fc9bd6c9bc",
    ),
    (
        "bm_lewis",
        "b8f671cef828c30e66fdf0b0756a76bba58f6bb3398cbbf27058642acbcedb97",
    ),
];
const VOICE_SIZE: u64 = 522_240;

/// The official ONNX Runtime build for this platform: archive, the shared
/// library inside it, and the name it's saved under.
struct Runtime {
    archive: &'static str,
    size: u64,
    sha256: &'static str,
    member: &'static str,
    library: &'static str,
}

#[cfg(all(windows, target_arch = "x86_64"))]
const RUNTIME: Option<Runtime> = Some(Runtime {
    archive: "onnxruntime-win-x64-1.31.1.zip",
    size: 82_929_944,
    sha256: "5869e26bb25314375f2d5ef1877e3ea06b1543d6c710e6e4a1ec2f7177ab4b2c",
    member: "onnxruntime-win-x64-1.31.1/lib/onnxruntime.dll",
    library: "onnxruntime.dll",
});
#[cfg(all(windows, target_arch = "aarch64"))]
const RUNTIME: Option<Runtime> = Some(Runtime {
    archive: "onnxruntime-win-arm64-1.31.1.zip",
    size: 85_851_069,
    sha256: "817f4129073f84f81cde3ff92b1a4e06d007691e7208deed08a8b6d6df143f7f",
    member: "onnxruntime-win-arm64-1.31.1/lib/onnxruntime.dll",
    library: "onnxruntime.dll",
});
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
const RUNTIME: Option<Runtime> = Some(Runtime {
    archive: "onnxruntime-osx-arm64-1.31.1.tgz",
    size: 36_936_435,
    sha256: "2e1344a1926ee50eb25b38153cb6992c3567515aef0e677a1a520a1798896d6f",
    member: "onnxruntime-osx-arm64-1.31.1/lib/libonnxruntime.1.31.1.dylib",
    library: "libonnxruntime.dylib",
});
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
const RUNTIME: Option<Runtime> = Some(Runtime {
    archive: "onnxruntime-linux-x64-1.31.1.tgz",
    size: 11_413_655,
    sha256: "c849e36a1ef3e22d4ff217b9d1ce4a3cf5b9467b975610913ae959ad5b16bdc9",
    member: "onnxruntime-linux-x64-1.31.1/lib/libonnxruntime.so.1.31.1",
    library: "libonnxruntime.so",
});
#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
const RUNTIME: Option<Runtime> = Some(Runtime {
    archive: "onnxruntime-linux-aarch64-1.31.1.tgz",
    size: 10_332_381,
    sha256: "5317505e2da99be65f8e2775ba3c2afd6d2961fd78aba1bd8f56159f48820c78",
    member: "onnxruntime-linux-aarch64-1.31.1/lib/libonnxruntime.so.1.31.1",
    library: "libonnxruntime.so",
});
#[cfg(not(any(
    all(windows, any(target_arch = "x86_64", target_arch = "aarch64")),
    all(target_os = "macos", target_arch = "aarch64"),
    all(
        target_os = "linux",
        any(target_arch = "x86_64", target_arch = "aarch64")
    ),
)))]
const RUNTIME: Option<Runtime> = None;

/// One downloadable file.
struct Asset {
    /// Where it's saved, relative to the Kokoro folder.
    file: String,
    url: String,
    size: u64,
    sha256: &'static str,
    /// The runtime archive: unpack its library instead of keeping it.
    runtime: bool,
}

fn assets() -> Vec<Asset> {
    let mut assets = Vec::new();
    if let Some(rt) = &RUNTIME {
        assets.push(Asset {
            file: rt.archive.to_string(),
            url: format!("{ORT_BASE}/{}", rt.archive),
            size: rt.size,
            sha256: rt.sha256,
            runtime: true,
        });
    }
    assets.push(Asset {
        file: MODEL_FILE.to_string(),
        url: format!("{MODEL_BASE}/onnx/model_quantized.onnx"),
        size: 92_361_116,
        sha256: "fbae9257e1e05ffc727e951ef9b9c98418e6d79f1c9b6b13bd59f5c9028a1478",
        runtime: false,
    });
    assets.push(Asset {
        file: VOCAB_FILE.to_string(),
        url: format!("{MODEL_BASE}/tokenizer.json"),
        size: 3_497,
        sha256: "77a02c8e164413299b4b4c403b14f8e0e1c1b727db4d46a09d6327b861060a34",
        runtime: false,
    });
    for (file, size, sha256) in [
        (
            "us_gold.json",
            3_000_469,
            "dc414872a49a28ae6c141463d502fd945f3b2fde040484fdc47d00cc4612686f",
        ),
        (
            "us_silver.json",
            3_099_517,
            "de8f67be911bb6c659187b4a65fd966b6a30e56350e0f790d763210b053ac475",
        ),
        (
            "gb_gold.json",
            2_838_552,
            "29e62f4b60261c88f7f3c2c7811ca3825978948090b72d2b27d565b729282f71",
        ),
        (
            "gb_silver.json",
            3_663_898,
            "48131e2d92ccc41655f4543e87e0f938e71463eb5a54be7f0693bb712ebb6bce",
        ),
    ] {
        assets.push(Asset {
            file: file.to_string(),
            url: format!("{MISAKI_BASE}/{file}"),
            size,
            sha256,
            runtime: false,
        });
    }
    for (name, sha256) in VOICES {
        assets.push(Asset {
            file: format!("{VOICES_DIR}/{name}.bin"),
            url: format!("{MODEL_BASE}/voices/{name}.bin"),
            size: VOICE_SIZE,
            sha256,
            runtime: false,
        });
    }
    assets
}

/// Path of the ONNX Runtime library, once unpacked.
pub fn runtime_library(dir: &Path) -> Option<PathBuf> {
    RUNTIME.as_ref().map(|rt| dir.join(rt.library))
}

fn is_present(dir: &Path, asset: &Asset) -> bool {
    if asset.runtime {
        return runtime_library(dir).is_some_and(|lib| lib.is_file());
    }
    dir.join(&asset.file)
        .metadata()
        .is_ok_and(|m| m.len() == asset.size)
}

/// Whether every file is in place (sizes only; hashes are checked on download).
pub fn is_ready(dir: &Path) -> bool {
    RUNTIME.is_some() && assets().iter().all(|a| is_present(dir, a))
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct KokoroStatus {
    /// Kokoro can run on this platform.
    pub supported: bool,
    pub downloaded: bool,
    pub downloading: bool,
    /// Total download size.
    pub size_mb: u64,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct KokoroDownloadProgress {
    pub downloaded: u64,
    pub total: u64,
    pub percentage: f64,
    pub bytes_per_sec: Option<f64>,
}

/// Download bookkeeping for [`Kokoro`].
#[derive(Default)]
pub struct Downloads {
    active: AtomicBool,
    cancel: Mutex<Option<CancellationToken>>,
}

/// Clears the download flag however `download` exits.
struct ActiveGuard<'a>(&'a AtomicBool);

impl Drop for ActiveGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

impl Kokoro {
    pub fn status(&self) -> KokoroStatus {
        KokoroStatus {
            supported: RUNTIME.is_some(),
            downloaded: is_ready(&self.dir),
            downloading: self.downloads.active.load(Ordering::SeqCst),
            size_mb: assets()
                .iter()
                .map(|a| a.size)
                .sum::<u64>()
                .div_ceil(1_000_000),
        }
    }

    fn status_changed(&self) {
        let _ = self.app.emit("kokoro-status-changed", self.status());
    }

    /// Fetch whatever is missing. Resumes partial files; returns `Ok` when
    /// cancelled (the partials are kept for next time).
    pub async fn download(&self) -> Result<(), String> {
        if RUNTIME.is_none() {
            return Err("Kokoro voices aren't available on this platform".to_string());
        }
        if self.downloads.active.swap(true, Ordering::SeqCst) {
            return Err("Kokoro voices are already downloading".to_string());
        }
        let _active = ActiveGuard(&self.downloads.active);
        let token = CancellationToken::new();
        *self
            .downloads
            .cancel
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = Some(token.clone());
        self.status_changed();
        let app = self.app.clone();
        let result = fetch_missing(&self.dir, &token, move |progress| {
            let _ = app.emit("kokoro-download-progress", progress);
        })
        .await;
        *self
            .downloads
            .cancel
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = None;
        drop(_active);
        self.status_changed();

        if result.is_ok() && is_ready(&self.dir) {
            info!("Kokoro voices downloaded");
            // First download: switch from the system default to Kokoro.
            let mut settings = settings::get_settings(&self.app);
            if settings.assistant_voice.is_none() {
                settings.assistant_voice = Some(format!("{}{DEFAULT_VOICE}", super::VOICE_PREFIX));
                settings::write_settings(&self.app, settings);
                voice_setting_changed(&self.app);
            }
        }
        result
    }

    pub fn cancel_download(&self) {
        if let Some(token) = self
            .downloads
            .cancel
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
        {
            token.cancel();
        }
    }

    /// Remove the downloaded files. A loaded ONNX Runtime library can't be
    /// deleted on Windows until Kusema restarts; it's left behind (and reused
    /// by the next download) if so.
    pub fn delete(&self) -> Result<(), String> {
        if self.downloads.active.load(Ordering::SeqCst) {
            return Err("Cancel the download first".to_string());
        }
        self.unload();
        let mut result = Ok(());
        let entries = fs::read_dir(&self.dir).into_iter().flatten().flatten();
        for entry in entries {
            let path = entry.path();
            let removed = if path.is_dir() {
                fs::remove_dir_all(&path)
            } else {
                fs::remove_file(&path)
            };
            if let Err(e) = removed {
                if runtime_library(&self.dir).as_deref() == Some(path.as_path()) {
                    info!("ONNX Runtime is in use; it will be removed with the next delete after a restart");
                } else {
                    warn!("Could not delete {}: {e}", path.display());
                    result = Err(e.to_string());
                }
            }
        }
        // Speak with the system voice from now on.
        let mut settings = settings::get_settings(&self.app);
        if settings
            .assistant_voice
            .as_deref()
            .is_some_and(|v| v.starts_with(super::VOICE_PREFIX))
        {
            settings.assistant_voice = None;
            settings::write_settings(&self.app, settings);
            voice_setting_changed(&self.app);
        }
        self.status_changed();
        result
    }
}

/// Download every missing file into `dir`, reporting overall progress.
/// Returns `Ok` early when cancelled (partials are kept for next time).
async fn fetch_missing(
    dir: &Path,
    token: &CancellationToken,
    progress: impl Fn(KokoroDownloadProgress) + Send + Sync,
) -> Result<(), String> {
    fs::create_dir_all(dir.join(VOICES_DIR)).map_err(|e| e.to_string())?;
    let assets = assets();
    let total: u64 = assets.iter().map(|a| a.size).sum();
    let mut done: u64 = assets
        .iter()
        .filter(|a| is_present(dir, a))
        .map(|a| a.size)
        .sum();

    for asset in assets.iter().filter(|a| !is_present(dir, a)) {
        let partial = dir.join(format!("{}.partial", asset.file));
        let base = done;
        let emit = |event: HttpDownloadEvent<'_>| {
            if let HttpDownloadEvent::Progress(p) = event {
                let downloaded = base + p.downloaded;
                progress(KokoroDownloadProgress {
                    downloaded,
                    total,
                    percentage: downloaded as f64 * 100.0 / total as f64,
                    bytes_per_sec: p.bytes_per_sec,
                });
            }
        };
        let outcome = ModelManager::download_http_resumable_with_events(
            "kokoro",
            &asset.url,
            &partial,
            Some(asset.size),
            Some(asset.sha256),
            token,
            &emit,
        )
        .await
        .map_err(|e| format!("Downloading {} failed: {e}", asset.file))?;
        if let HttpDownloadOutcome::Cancelled = outcome {
            info!("Kokoro download cancelled");
            return Ok(());
        }
        if asset.runtime {
            let archive = partial.clone();
            let target = runtime_library(dir).expect("runtime is supported");
            tokio::task::spawn_blocking(move || unpack_runtime(&archive, &target))
                .await
                .map_err(|e| e.to_string())??;
            let _ = fs::remove_file(&partial);
        } else {
            fs::rename(&partial, dir.join(&asset.file)).map_err(|e| e.to_string())?;
        }
        done += asset.size;
    }
    Ok(())
}

fn voice_setting_changed(app: &AppHandle) {
    let _ = app.emit(
        "settings-changed",
        serde_json::json!({ "setting": "assistant_voice" }),
    );
}

/// Copy the runtime library out of the downloaded archive.
fn unpack_runtime(archive: &Path, target: &Path) -> Result<(), String> {
    let rt = RUNTIME.as_ref().ok_or("Unsupported platform")?;
    let tmp = target.with_extension("unpacking");
    let unpacked = extract_member(archive, rt.member, &tmp);
    if let Err(e) = unpacked {
        let _ = fs::remove_file(&tmp);
        return Err(format!("Could not unpack ONNX Runtime: {e}"));
    }
    fs::rename(&tmp, target).map_err(|e| e.to_string())
}

#[cfg(windows)]
fn extract_member(archive: &Path, member: &str, out: &Path) -> Result<(), String> {
    let mut zip = zip::ZipArchive::new(fs::File::open(archive).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let mut entry = zip.by_name(member).map_err(|e| e.to_string())?;
    let mut file = fs::File::create(out).map_err(|e| e.to_string())?;
    std::io::copy(&mut entry, &mut file).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(unix)]
fn extract_member(archive: &Path, member: &str, out: &Path) -> Result<(), String> {
    let file = fs::File::open(archive).map_err(|e| e.to_string())?;
    let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(file));
    for entry in tar.entries().map_err(|e| e.to_string())? {
        let mut entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path().map_err(|e| e.to_string())?;
        if path.to_str() == Some(member) && entry.header().entry_type().is_file() {
            let mut file = fs::File::create(out).map_err(|e| e.to_string())?;
            std::io::copy(&mut entry, &mut file).map_err(|e| e.to_string())?;
            return Ok(());
        }
    }
    Err(format!("{member} is missing from the archive"))
}

#[cfg(not(any(windows, unix)))]
fn extract_member(_: &Path, _: &str, _: &Path) -> Result<(), String> {
    Err("Unsupported platform".to_string())
}

#[tauri::command]
#[specta::specta]
pub fn get_kokoro_status(kokoro: State<'_, Arc<Kokoro>>) -> KokoroStatus {
    kokoro.status()
}

#[tauri::command]
#[specta::specta]
pub async fn download_kokoro(kokoro: State<'_, Arc<Kokoro>>) -> Result<(), String> {
    let kokoro = Arc::clone(&kokoro);
    kokoro.download().await
}

#[tauri::command]
#[specta::specta]
pub fn cancel_kokoro_download(kokoro: State<'_, Arc<Kokoro>>) {
    kokoro.cancel_download();
}

#[tauri::command]
#[specta::specta]
pub fn delete_kokoro(app: AppHandle, kokoro: State<'_, Arc<Kokoro>>) -> Result<(), String> {
    crate::tts::stop_and_idle(&app);
    kokoro.delete()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_is_consistent() {
        let assets = assets();
        let mut files: Vec<_> = assets.iter().map(|a| a.file.as_str()).collect();
        files.sort_unstable();
        files.dedup();
        assert_eq!(files.len(), assets.len(), "duplicate file names");
        for asset in &assets {
            assert_eq!(asset.sha256.len(), 64, "{}", asset.file);
            assert!(asset.sha256.bytes().all(|b| b.is_ascii_hexdigit()));
            assert!(asset.url.starts_with("https://"));
        }
        assert!(VOICES.iter().any(|(name, _)| *name == DEFAULT_VOICE));
    }

    /// Downloads everything (~200 MB) through the real downloader, checking
    /// every URL, size and hash, and unpacks ONNX Runtime:
    /// `KUSEMA_KOKORO_DOWNLOAD_DIR=... cargo test kokoro_downloads -- --ignored --nocapture`
    #[tokio::test]
    #[ignore]
    async fn kokoro_downloads() {
        let dir = PathBuf::from(std::env::var("KUSEMA_KOKORO_DOWNLOAD_DIR").unwrap());
        let started = std::time::Instant::now();
        let last = Mutex::new(0u64);
        fetch_missing(&dir, &CancellationToken::new(), |p| {
            let mut last = last.lock().unwrap();
            if p.downloaded >= *last + 20_000_000 || p.downloaded == p.total {
                *last = p.downloaded;
                println!("{:.0}% of {} MB", p.percentage, p.total / 1_000_000);
            }
        })
        .await
        .unwrap();
        println!("done in {:?}", started.elapsed());
        assert!(is_ready(&dir));
        assert!(fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .all(|e| !e.file_name().to_string_lossy().ends_with(".partial")));
    }

    #[test]
    fn empty_folder_is_not_ready() {
        let dir = std::env::temp_dir().join("kusema-kokoro-empty-test");
        assert!(!is_ready(&dir));
    }
}
