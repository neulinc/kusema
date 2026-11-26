//! Kokoro's files: what to download, where it lives, and the download itself.
//!
//! Everything is pinned (revision + size + SHA-256) and fetched with Handy's
//! resumable model downloader into `<app data>/kokoro/`:
//! - ONNX Runtime, Microsoft's official build: a small zip of this platform's
//!   library from Kusema's mirror, or else Microsoft's full NuGet package from
//!   nuget.org. Only the library is kept; `ort` loads it at runtime
//!   (`load-dynamic`).
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

/// ONNX Runtime, pinned: never "latest". 1.31.1 is the newest version `ort`
/// 2.0.0-rc.15 knows (its API 31) and the one Kokoro was tested with. It comes
/// from Microsoft's official NuGet package (nuget.org packages can't change
/// once published). Until 0.5.4 Kusema downloaded the GitHub release zip; that
/// URL answered 404 to Kusema's downloads in November 2026.
const RUNTIME_MIRROR: &str =
    "https://huggingface.co/kusema/onnxruntime/resolve/caf47470a42e102ffcbfc53047b21166c9007924";
const RUNTIME_NUGET: &str = "https://api.nuget.org/v3-flatcontainer/microsoft.ml.onnxruntime/1.31.1/microsoft.ml.onnxruntime.1.31.1.nupkg";
const RUNTIME_NUGET_SIZE: u64 = 159_626_394;
const RUNTIME_NUGET_SHA256: &str =
    "6871ccbc97f3d5bdb4095a1dcec33aa78d56f6ed8de5980ef5fa1d9f2e7c679a";

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

/// A platform's ONNX Runtime library. Kokoro loads only this one file (no
/// execution providers, so no `onnxruntime_providers_shared`). Every
/// platform's is defined here so the release check can test all their
/// downloads; each build uses its own, `RUNTIME`.
#[derive(Clone, Copy)]
#[cfg_attr(not(test), allow(dead_code))]
struct Runtime {
    /// The small zip on Kusema's mirror (scripts/kusema_mirror.py builds it):
    /// just the library, at its root, plus Microsoft's LICENSE and notices.
    zip: &'static str,
    zip_size: u64,
    zip_sha256: &'static str,
    /// Where the library is inside the NuGet package, the fallback.
    member: &'static str,
    /// The name it's saved under, and its name inside the small zip.
    library: &'static str,
}

#[cfg_attr(not(test), allow(dead_code))]
const WIN_X64: Runtime = Runtime {
    zip: "onnxruntime-1.31.1-win-x64.zip",
    zip_size: 6_336_551,
    zip_sha256: "9bac539eadb05f6e11e25d90b404c60c734306924a82d8e98157d06fd574c9d7",
    member: "runtimes/win-x64/native/onnxruntime.dll",
    library: "onnxruntime.dll",
};
#[cfg_attr(not(test), allow(dead_code))]
const WIN_ARM64: Runtime = Runtime {
    zip: "onnxruntime-1.31.1-win-arm64.zip",
    zip_size: 6_309_517,
    zip_sha256: "98e080b1b05cc493a6850b45a09f7af823c7da518afe86f9940183fd2cb03110",
    member: "runtimes/win-arm64/native/onnxruntime.dll",
    library: "onnxruntime.dll",
};
#[cfg_attr(not(test), allow(dead_code))]
const OSX_ARM64: Runtime = Runtime {
    zip: "onnxruntime-1.31.1-osx-arm64.zip",
    zip_size: 12_514_848,
    zip_sha256: "0ea6c7909f0902085e8cc62285f48ee0bcd45da3fb7483652cade01b439aed7d",
    member: "runtimes/osx-arm64/native/libonnxruntime.dylib",
    library: "libonnxruntime.dylib",
};
#[cfg_attr(not(test), allow(dead_code))]
const LINUX_X64: Runtime = Runtime {
    zip: "onnxruntime-1.31.1-linux-x64.zip",
    zip_size: 11_128_618,
    zip_sha256: "1e6397b39dc5886e4c4160e86ec026d15000df19fdf1bc54db61e6365ead6418",
    member: "runtimes/linux-x64/native/libonnxruntime.so",
    library: "libonnxruntime.so",
};
#[cfg_attr(not(test), allow(dead_code))]
const LINUX_ARM64: Runtime = Runtime {
    zip: "onnxruntime-1.31.1-linux-arm64.zip",
    zip_size: 10_062_535,
    zip_sha256: "38461512dbda3d856f7da5ec17bb75f5dc3c5ececfa9ad66ab153c64d3907d65",
    member: "runtimes/linux-arm64/native/libonnxruntime.so",
    library: "libonnxruntime.so",
};

#[cfg(all(windows, target_arch = "x86_64"))]
const RUNTIME: Option<Runtime> = Some(WIN_X64);
#[cfg(all(windows, target_arch = "aarch64"))]
const RUNTIME: Option<Runtime> = Some(WIN_ARM64);
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
const RUNTIME: Option<Runtime> = Some(OSX_ARM64);
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
const RUNTIME: Option<Runtime> = Some(LINUX_X64);
#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
const RUNTIME: Option<Runtime> = Some(LINUX_ARM64);
#[cfg(not(any(
    all(windows, any(target_arch = "x86_64", target_arch = "aarch64")),
    all(target_os = "macos", target_arch = "aarch64"),
    all(
        target_os = "linux",
        any(target_arch = "x86_64", target_arch = "aarch64")
    ),
)))]
const RUNTIME: Option<Runtime> = None;

/// One place a file can be fetched from. Each source has its own size and
/// SHA-256, checked after download.
pub(crate) struct Source {
    pub(crate) url: String,
    pub(crate) size: u64,
    pub(crate) sha256: &'static str,
    /// For an archive: the file to take out of it (the runtime library).
    member: Option<&'static str>,
}

/// One downloadable file.
pub(crate) struct Asset {
    /// Where it's saved, relative to the Kokoro folder.
    pub(crate) file: String,
    /// Where it's fetched from, tried in order until one downloads and
    /// matches its own size and SHA-256.
    pub(crate) sources: Vec<Source>,
    /// The runtime library, unpacked from an archive.
    runtime: bool,
}

impl Asset {
    /// A file with one source, saved as downloaded.
    fn plain(file: String, url: String, size: u64, sha256: &'static str) -> Self {
        Asset {
            file,
            sources: vec![Source {
                url,
                size,
                sha256,
                member: None,
            }],
            runtime: false,
        }
    }

    /// The size counted for progress and the total shown: the first source's.
    pub(crate) fn size(&self) -> u64 {
        self.sources[0].size
    }
}

/// The runtime's sources: the small zip on Kusema's mirror, then Microsoft's
/// full NuGet package. Debug builds can replace the first URL with
/// `KUSEMA_DEBUG_RUNTIME_PRIMARY_URL`, to test the fallback.
fn runtime_sources(rt: &Runtime) -> Vec<Source> {
    #[allow(unused_mut)]
    let mut primary = format!("{RUNTIME_MIRROR}/{}", rt.zip);
    #[cfg(debug_assertions)]
    if let Ok(url) = std::env::var("KUSEMA_DEBUG_RUNTIME_PRIMARY_URL") {
        primary = url;
    }
    vec![
        Source {
            url: primary,
            size: rt.zip_size,
            sha256: rt.zip_sha256,
            member: Some(rt.library),
        },
        Source {
            url: RUNTIME_NUGET.to_string(),
            size: RUNTIME_NUGET_SIZE,
            sha256: RUNTIME_NUGET_SHA256,
            member: Some(rt.member),
        },
    ]
}

/// Every platform's runtime sources, for the release check.
#[cfg(test)]
pub(crate) fn every_runtime_source() -> Vec<(&'static str, Source)> {
    [WIN_X64, WIN_ARM64, OSX_ARM64, LINUX_X64, LINUX_ARM64]
        .iter()
        .flat_map(|rt| runtime_sources(rt).into_iter().map(|s| (rt.zip, s)))
        .collect()
}

/// Every file Kokoro downloads, on this platform.
pub(crate) fn assets() -> Vec<Asset> {
    let mut assets = Vec::new();
    if let Some(rt) = RUNTIME.as_ref() {
        assets.push(Asset {
            file: rt.library.to_string(),
            sources: runtime_sources(rt),
            runtime: true,
        });
    }
    assets.push(Asset::plain(
        MODEL_FILE.to_string(),
        format!("{MODEL_BASE}/onnx/model_quantized.onnx"),
        92_361_116,
        "fbae9257e1e05ffc727e951ef9b9c98418e6d79f1c9b6b13bd59f5c9028a1478",
    ));
    assets.push(Asset::plain(
        VOCAB_FILE.to_string(),
        format!("{MODEL_BASE}/tokenizer.json"),
        3_497,
        "77a02c8e164413299b4b4c403b14f8e0e1c1b727db4d46a09d6327b861060a34",
    ));
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
        assets.push(Asset::plain(
            file.to_string(),
            format!("{MISAKI_BASE}/{file}"),
            size,
            sha256,
        ));
    }
    for (name, sha256) in VOICES {
        assets.push(Asset::plain(
            format!("{VOICES_DIR}/{name}.bin"),
            format!("{MODEL_BASE}/voices/{name}.bin"),
            VOICE_SIZE,
            sha256,
        ));
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
        .is_ok_and(|m| m.len() == asset.size())
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
                .map(|a| a.size())
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

        if let Err(e) = &result {
            warn!("Kokoro voices couldn't be downloaded: {e}");
        }
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
    let total: u64 = assets.iter().map(|a| a.size()).sum();
    let mut done: u64 = assets
        .iter()
        .filter(|a| is_present(dir, a))
        .map(|a| a.size())
        .sum();

    for asset in assets.iter().filter(|a| !is_present(dir, a)) {
        // Each source in turn. A file that doesn't match that source's size
        // and SHA-256 is deleted by the downloader, and counts as a failure.
        let mut failures = Vec::new();
        let mut fetched = None;
        for source in &asset.sources {
            let partial = partial_path(dir, asset, source);
            let base = done;
            // A fallback can be bigger than the first source; the total grows
            // with it so progress never passes 100%.
            let total = total - asset.size() + source.size;
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
            match ModelManager::download_http_resumable_with_events(
                crate::network::Purpose::VoiceDownload,
                "kokoro",
                &source.url,
                &partial,
                Some(source.size),
                Some(source.sha256),
                token,
                &emit,
            )
            .await
            {
                Ok(outcome) => {
                    fetched = Some((outcome, source, partial));
                    break;
                }
                Err(e) => {
                    warn!(
                        "Kokoro: downloading {} from {} failed: {e}",
                        asset.file, source.url
                    );
                    failures.push(format!("{}: {e}", source.url));
                }
            }
        }
        let (outcome, source, partial) = fetched.ok_or_else(|| {
            format!(
                "Downloading {} failed. {}",
                asset.file,
                failures.join(" | ")
            )
        })?;
        if let HttpDownloadOutcome::Cancelled = outcome {
            info!("Kokoro download cancelled");
            return Ok(());
        }
        let target = dir.join(&asset.file);
        if let Some(member) = source.member {
            let archive = partial.clone();
            tokio::task::spawn_blocking(move || unpack_runtime(&archive, member, &target))
                .await
                .map_err(|e| e.to_string())??;
            info!("Kokoro: ONNX Runtime unpacked from {}", source.url);
            let _ = fs::remove_file(&partial);
        } else {
            fs::rename(&partial, &target).map_err(|e| e.to_string())?;
        }
        if asset.runtime {
            remove_runtime_partials(dir);
        }
        done += asset.size();
    }
    Ok(())
}

/// Where a source is downloaded to before it's checked. The runtime's
/// sources are different files, so each gets its own partial (named by its
/// SHA-256): an interrupted download only ever resumes the same file.
fn partial_path(dir: &Path, asset: &Asset, source: &Source) -> PathBuf {
    if asset.runtime {
        dir.join(format!("runtime-{}.partial", &source.sha256[..16]))
    } else {
        dir.join(format!("{}.partial", asset.file))
    }
}

/// The runtime's leftover partials, once it's in place: a source that failed
/// part way, and the GitHub release archives used before 0.5.4.
fn remove_runtime_partials(dir: &Path) {
    let entries = fs::read_dir(dir).into_iter().flatten().flatten();
    for entry in entries {
        let name = entry.file_name().to_string_lossy().to_string();
        let runtime = name.starts_with("runtime-") || name.starts_with("onnxruntime-");
        if runtime && name.ends_with(".partial") {
            let _ = fs::remove_file(entry.path());
        }
    }
}

fn voice_setting_changed(app: &AppHandle) {
    let _ = app.emit(
        "settings-changed",
        serde_json::json!({ "setting": "assistant_voice" }),
    );
}

/// Copy the runtime library (`member`) out of the downloaded archive.
fn unpack_runtime(archive: &Path, member: &str, target: &Path) -> Result<(), String> {
    let tmp = target.with_extension("unpacking");
    let unpacked = extract_member(archive, member, &tmp);
    if let Err(e) = unpacked {
        let _ = fs::remove_file(&tmp);
        return Err(format!("Could not unpack ONNX Runtime: {e}"));
    }
    fs::rename(&tmp, target).map_err(|e| e.to_string())
}

/// Copy `member` out of a zip `archive` (a NuGet package is one) to `out`.
fn extract_member(archive: &Path, member: &str, out: &Path) -> Result<(), String> {
    let mut zip = zip::ZipArchive::new(fs::File::open(archive).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let mut entry = zip
        .by_name(member)
        .map_err(|_| format!("{member} is missing from the package"))?;
    let mut file = fs::File::create(out).map_err(|e| e.to_string())?;
    std::io::copy(&mut entry, &mut file).map_err(|e| e.to_string())?;
    Ok(())
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
            assert!(!asset.sources.is_empty(), "{}", asset.file);
            for source in &asset.sources {
                assert_eq!(source.sha256.len(), 64, "{}", asset.file);
                assert!(source.sha256.bytes().all(|b| b.is_ascii_hexdigit()));
                assert!(source.url.starts_with("https://"));
                assert!(source.size > 0);
            }
        }
        assert!(VOICES.iter().any(|(name, _)| *name == DEFAULT_VOICE));
    }

    /// Downloads everything (~126 MB) through the real downloader, checking
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
    fn the_runtime_is_pinned_slim_first_with_nuget_as_the_fallback() {
        let assets = assets();
        // Nothing from the GitHub release zips any more, and never "latest".
        for source in assets.iter().flat_map(|a| &a.sources) {
            assert!(!source.url.contains("onnxruntime/releases"));
            assert!(!source.url.contains("latest"));
        }
        let Some(rt) = RUNTIME.as_ref() else {
            assert!(assets.iter().all(|a| !a.runtime));
            return;
        };
        let runtime = assets.iter().find(|a| a.runtime).unwrap();
        assert_eq!(runtime.file, rt.library);
        let [slim, nuget] = runtime.sources.as_slice() else {
            panic!("the runtime has two sources");
        };
        // First: the small zip on the mirror, at a fixed revision, with its
        // own size and hash; the library sits at its root.
        assert!(slim.url.starts_with(
            "https://huggingface.co/kusema/onnxruntime/resolve/caf47470a42e102ffcbfc53047b21166c9007924/onnxruntime-1.31.1-"
        ));
        assert!(slim.url.ends_with(".zip"));
        assert_eq!(slim.member, Some(rt.library));
        assert!(slim.size < 20_000_000);
        // Then Microsoft's full package, with its own size and hash.
        assert_eq!(nuget.url, RUNTIME_NUGET);
        assert!(nuget.url.starts_with(
            "https://api.nuget.org/v3-flatcontainer/microsoft.ml.onnxruntime/1.31.1/"
        ));
        assert_eq!(
            (nuget.size, nuget.sha256),
            (RUNTIME_NUGET_SIZE, RUNTIME_NUGET_SHA256)
        );
        assert_eq!(nuget.member, Some(rt.member));
        assert_ne!(slim.sha256, nuget.sha256);
        // Separate partials, so a resume never mixes the two files.
        assert_ne!(
            partial_path(Path::new("k"), runtime, slim),
            partial_path(Path::new("k"), runtime, nuget)
        );
        // The total shown counts the small zip.
        assert_eq!(runtime.size(), rt.zip_size);
    }

    #[test]
    fn the_runtime_library_is_unpacked_from_the_package() {
        use std::io::Write;
        let dir = std::env::temp_dir().join(format!("kusema-ort-unpack-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let package = dir.join("test.nupkg");
        let mut zip = zip::ZipWriter::new(fs::File::create(&package).unwrap());
        let stored = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        zip.start_file("LICENSE", stored).unwrap();
        zip.write_all(b"MIT").unwrap();
        zip.start_file("runtimes/win-x64/native/onnxruntime.dll", stored)
            .unwrap();
        zip.write_all(b"the library").unwrap();
        zip.finish().unwrap();

        let out = dir.join("onnxruntime.dll");
        unpack_runtime(&package, "runtimes/win-x64/native/onnxruntime.dll", &out).unwrap();
        assert_eq!(fs::read(&out).unwrap(), b"the library");
        assert!(!dir.join("onnxruntime.unpacking").exists());

        // The small zip: the library at its root.
        let slim = dir.join("slim.zip");
        let mut zip = zip::ZipWriter::new(fs::File::create(&slim).unwrap());
        let deflated = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        zip.start_file("onnxruntime.dll", deflated).unwrap();
        zip.write_all(b"the small library").unwrap();
        zip.start_file("LICENSE", deflated).unwrap();
        zip.write_all(b"MIT").unwrap();
        zip.finish().unwrap();
        unpack_runtime(&slim, "onnxruntime.dll", &out).unwrap();
        assert_eq!(fs::read(&out).unwrap(), b"the small library");
        let missing = extract_member(&package, "runtimes/nowhere/lib.so", &dir.join("x"));
        assert!(missing.unwrap_err().contains("missing from the package"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn runtime_partials_are_cleared() {
        let dir = std::env::temp_dir().join(format!("kusema-ort-partials-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("onnxruntime-win-x64-1.31.1.zip.partial"), b"x").unwrap();
        fs::write(dir.join("runtime-6871ccbc97f3d5bd.partial"), b"x").unwrap();
        fs::write(dir.join("tokenizer.json.partial"), b"x").unwrap();
        remove_runtime_partials(&dir);
        assert!(!dir.join("onnxruntime-win-x64-1.31.1.zip.partial").exists());
        assert!(!dir.join("runtime-6871ccbc97f3d5bd.partial").exists());
        assert!(dir.join("tokenizer.json.partial").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn empty_folder_is_not_ready() {
        let dir = std::env::temp_dir().join("kusema-kokoro-empty-test");
        assert!(!is_ready(&dir));
    }
}
