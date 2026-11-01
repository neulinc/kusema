# Kusema — Build Plan & Claude Code Prompts

_Fork of cjpais/Handy (MIT). Mapped against Handy `main` on 2026-10-27; last merged 2026-10-31._

## Who Kusema is for

The primary market is **small and solo US law firms on Windows**, plus their paralegals and assistants. They need confidentiality they can explain to a client, and setup that doesn't need an IT department. **Accessibility users** are the community channel. Steps 1–6 and 5b are done; the [Roadmap](#roadmap) below is built around this.

## What Handy already gives us

| Need                                             | Already in Handy                                                      | Where                                                            |
| ------------------------------------------------ | --------------------------------------------------------------------- | ---------------------------------------------------------------- |
| Hotkey → record → local transcription            | Yes                                                                   | `actions.rs` → `TranscribeAction`                                |
| Send text to Claude / OpenAI / Groq / OpenRouter | Yes (used for "post-processing")                                      | `llm_client.rs`                                                  |
| Ollama / any local model                         | Yes, via the "Custom" provider (base URL `http://localhost:11434/v1`) | `settings.rs` `default_post_process_providers()`                 |
| Provider, model and API key settings UI          | Yes                                                                   | `src/components/settings/post-processing/`                       |
| Audio playback library                           | Yes (`rodio`)                                                         | `Cargo.toml`                                                     |
| Repo instructions for AI agents                  | Yes                                                                   | `CLAUDE.md`, `AGENTS.md` (Claude Code reads these automatically) |

## What's missing

1. **An "Assistant" mode.** Today every transcription ends in a paste (`utils::paste(...)` inside `TranscribeAction::stop`, around line 826 of `actions.rs`). We add a third hotkey that sends the text to the LLM as a _question_ and keeps the answer.
2. **Conversation memory.** `send_anthropic_message` sends one user message only. Assistant mode needs a message history.
3. **Text-to-speech.** Nothing speaks yet.
4. **Somewhere to see the conversation.** A small panel or window.
5. **Safer API key storage.** Keys live in the settings file (`SecretMap` in `settings.rs`). They're redacted from logs, but stored as plain text on disk. Move them to Windows Credential Manager.

## Ground rule for every prompt

Keep new code in **new files/modules** and make the **smallest possible edits** to Handy's existing files. That keeps `git merge upstream/main` painless, so you keep receiving Handy's bug fixes.

---

## Step 0 — Get the unmodified fork running (you, manually)

On Windows, install:

1. Rust via https://rustup.rs (default MSVC toolchain)
2. Visual Studio Build Tools → "Desktop development with C++"
3. Bun: `powershell -c "irm bun.sh/install.ps1 | iex"`
4. Git

Then in your fork folder:

```
git remote add upstream https://github.com/cjpais/Handy.git
bun install
bun run tauri dev
```

Read `BUILD.md` if anything fails. In the running app, pick **Parakeet V3** as the speech model (Whisper crashes on some Windows setups). Confirm dictation works before changing anything.

If the build fails, open Claude Code in the folder and paste:

> Read BUILD.md and CLAUDE.md. I'm on Windows. `bun run tauri dev` fails with the error below. Diagnose and fix my environment (not the code) so the unmodified app builds and runs. [paste error]

---

## Step 1 — Rebrand to Kusema ✅ Done (2026-10-27)

> Read CLAUDE.md and AGENTS.md first. This repo is a fork of Handy that we're rebranding as **Kusema** (Swahili for "to speak"), published by Neulinc. Handy's license requires forks to use their own name and branding.
>
> 1. Change the product name to "Kusema" and the bundle identifier from `com.pais.handy` to `com.neulinc.kusema` in `src-tauri/tauri.conf.json`, `tauri.windows.conf.json`, `Cargo.toml` package metadata, `package.json`, and the NSIS installer config.
> 2. Replace user-visible "Handy" strings in the UI, tray menu, window titles and the English locale (`src/i18n/locales/en/translation.json`). Leave other locales for now.
> 3. Do NOT rename internal Rust module names, function names or file paths. Keep the diff small so future merges from upstream Handy stay easy.
> 4. Keep the MIT LICENSE file intact, and add a line to README crediting Handy (https://github.com/cjpais/Handy) as the original project.
> 5. Note: changing the identifier changes the app data folder, so existing Handy settings and models won't carry over. That's fine.
> 6. List any icon/logo files I need to replace with my own artwork; don't generate artwork.
>
> Then run `bun run tauri dev` to confirm it still builds. Commit as "Rebrand to Kusema".

### Notes from Step 1

- **Artwork: replaced with the Kusema branding kit ✅ (2026-10-27).** `branding/` is the source of truth (see `branding/README.md`).
  - App icons: `bun tauri icon branding/kusema-icon-1024.png` regenerates `src-tauri/icons/`. `logo.png` isn't produced by that command, so copy `kusema-icon-1024.png` over it.
  - Tray icons: `branding/tray/*.png` are copied over `src-tauri/resources/`.
  - React components in `src/components/icons/` keep Handy's file and component names, so upstream merges stay easy. If upstream changes these files, keep our versions.
  - Theme colors are in `src/styles/theme.css`. Primary buttons (`--color-background-ui`) use `#6d5ce8` from the icon gradient.
- **Code signing removed.** The upstream Windows signing command used the Handy author's Azure account. Installers are unsigned until we have our own certificate (see "Launch gate").
- **`src/bindings.ts` gets regenerated on Windows builds** (tauri-specta writes it at dev startup). The diff is only platform-specific doc comments, such as the `isLaptop` stub. It's harmless; commit it or discard it.
- **Auto-updater disabled.** It pointed at Handy's GitHub releases and Handy's signing key. The endpoint and public key are now blank in `tauri.conf.json`, `createUpdaterArtifacts` is off, `update_checks_forced_disabled()` in `settings.rs` always returns true, and the footer "Check for updates" link and tray item are hidden. **Before distributing Kusema, set up our own update channel**: generate a signing key with `bun tauri signer generate`, put the public key and a Kusema `latest.json` endpoint in `tauri.conf.json`, turn `createUpdaterArtifacts` back on, and set `KUSEMA_UPDATE_CHANNEL_CONFIGURED` to true in `settings.rs`. ✅ Done in Step 5b.
- **AI provider requests identify as Kusema.** The User-Agent, Referer and X-Title headers in `llm_client.rs` now say Kusema.
- **Version numbers track Kusema, not Handy** (2026-10-27). Kusema is v0.1.0 in `tauri.conf.json`, `src-tauri/Cargo.toml` and `package.json`, and is versioned on its own from here on. When merging upstream, keep ours if Handy bumps its version in those files. Handy's "What's new" release notes are for Handy versions above 0.1.0, so they no longer pop up.

---

## Step 2 — Assistant mode (text replies first) ✅ Done (2026-10-27)

> Read CLAUDE.md first. We're adding an **Assistant mode** to Kusema: the user holds a hotkey, speaks a question, it's transcribed locally (as now), the text is sent to the LLM provider the user configured, and the reply is kept, without pasting anything.
>
> Context from the codebase:
>
> - `src-tauri/src/actions.rs`: `TranscribeAction { post_process: bool }` handles the hotkey. In `stop()`, after transcription and optional post-processing, it spawns a "paste" thread that calls `utils::paste(final_text, ...)`. Actions are registered in `ACTION_MAP`.
> - `src-tauri/src/settings.rs`: default hotkeys are created in the default settings (`transcribe`, `transcribe_with_post_process`, `cancel`). Providers come from `default_post_process_providers()`; the active one from `settings.active_post_process_provider()`; keys from `post_process_api_keys`; model from `post_process_models`.
> - `src-tauri/src/llm_client.rs`: already speaks both the Anthropic Messages format and the OpenAI chat format, but only single-turn.
>
> Do this:
>
> 1. Create a new module `src-tauri/src/assistant/` with `mod.rs` and `conversation.rs`. Keep a `Conversation` holding an ordered list of user/assistant messages plus a system prompt (default: "You are Kusema, a helpful voice assistant. Answers will be spoken aloud, so reply in short, natural spoken sentences. No markdown, lists, code blocks or emoji unless asked."). Store it in Tauri managed state.
> 2. In `llm_client.rs`, add a multi-turn function (e.g. `send_conversation`) that accepts a system prompt plus a message list and works for both the Anthropic format (system goes in the top-level `system` field) and OpenAI format (system as the first message). Reuse the existing headers/client helpers. Don't change the behavior of the existing post-processing functions.
> 3. Add a new hotkey binding `assistant` (Windows default `ctrl+alt+space`; pick sensible macOS/Linux defaults) named "Ask Kusema", and register it in `ACTION_MAP`. Implement it with the smallest possible change to `TranscribeAction`: e.g. replace the `post_process: bool` with a small enum mode (`Dictate`, `DictateWithPostProcess`, `Assistant`), or add an `assistant: bool`. In Assistant mode, skip the paste thread entirely and instead hand the final text to the assistant module.
> 4. The assistant module appends the user message, calls the LLM with the full conversation using the same provider/model/key the user configured for post-processing, appends the reply, and emits a Tauri event `assistant-reply` with `{ question, answer }`. On error emit `assistant-error` with a human-readable message (e.g. "No API key set for Anthropic").
> 5. Add a minimal "Assistant" page to the settings sidebar in the React frontend showing the conversation (question/answer bubbles, newest at bottom), with a "New conversation" button that clears the history via a Tauri command.
> 6. Make sure the tray icon and overlay return to idle after the reply, and that the existing Cancel hotkey cancels an in-flight assistant request.
>
> Don't add text-to-speech yet; that's the next step. Run `cargo check`, `cargo test` and `bun run tauri dev`. Commit as "Add Assistant mode (text replies)".

**Test:** In settings, choose Anthropic as the post-processing provider, paste your Claude API key, pick a model. Press Ctrl+Alt+Space, ask "What's the capital of Ghana?", and check the reply appears on the Assistant page. Then ask a follow-up ("What's its population?") to confirm memory works.

### Notes from Step 2

- **Where things live:** `src-tauri/src/assistant/` (`Conversation`, `ask()`, the `get_assistant_conversation` / `clear_assistant_conversation` commands), `llm_client::send_conversation`, and `src/components/settings/assistant/AssistantSettings.tsx`. The Handy files were touched only lightly: `actions.rs` (an `assistant` flag on `TranscribeAction` plus one branch in `stop()`), `shortcut/handler.rs` (Cancel also fires while a request is in flight), `transcription_coordinator.rs` (`assistant` counts as a transcribe binding), `settings.rs` (the default binding) and `lib.rs`.
- **For Step 3:** speak the reply in `assistant::ask()`, where it emits `assistant-reply`. The request is awaited inside the transcription pipeline, so the overlay and tray stay busy and Cancel (Escape) works until `ask()` returns.
- **Post Process page:** it only appears when Post Processing is turned on in Advanced settings, so turn that on to configure the provider. The hotkey itself works either way.
- **History:** assistant questions are also saved to History like normal dictations. The conversation itself is in memory only and is lost on restart.
- **Other locales:** the new strings were copied in English into every locale so `bun run check:translations` passes. Translate them later.

---

## Step 3 — Speak the replies (Windows built-in voices) ✅ Done (2026-10-27)

> Read CLAUDE.md first. Kusema's Assistant mode now emits `assistant-reply`. Add text-to-speech so the reply is spoken aloud.
>
> 1. Create `src-tauri/src/tts/` with a `TtsEngine` trait (`list_voices()`, `speak(text, voice_id, rate)`, `stop()`), and a first implementation `system.rs` using the OS's built-in voices. Use the `tts` crate (Windows SAPI/WinRT, macOS AVSpeech, Linux speech-dispatcher) unless you find a better-maintained option. Justify your choice briefly.
> 2. Speak replies **sentence by sentence as soon as each sentence is available**, so long answers start speaking quickly. (If the LLM call isn't streaming yet, add streaming for both Anthropic and OpenAI formats using reqwest's stream feature, which is already enabled in Cargo.toml.)
> 3. Pressing the Cancel hotkey (Escape) or starting a new recording must stop speech immediately. The user must always be able to interrupt.
> 4. Add settings: "Speak replies" on/off, voice dropdown populated from `list_voices()`, speaking rate slider, and a "Preview voice" button. Persist them in AppSettings with serde defaults so existing settings files still load.
> 5. Show a "speaking" state in the overlay/tray while talking.
>
> Run `cargo check`, `cargo test`, `bun run tauri dev`. Commit as "Speak assistant replies".

### Notes from Step 3

- **Engine choice:** the `windows` crate (already a dependency) calling WinRT `Windows.Media.SpeechSynthesis` directly, not the `tts` crate. It gives the OneCore voices (David, Zira, Mark and any installed language packs) rather than legacy SAPI ones. Each sentence comes back as WAV and plays through rodio, so speech uses Kusema's selected output device and Cancel stops it instantly. The `tts` crate does its own playback (no output-device choice), brings in another `windows` version, and needs speech-dispatcher libraries to build on Linux. Windows 11's "Natural" Narrator voices aren't available to apps through this API.
- **macOS / Linux:** `system.rs` uses `say` on macOS and `espeak-ng` on Linux, through the same WAV and rodio path. This code compiles on those platforms but hasn't been run there yet.
- **Where things live:** `src-tauri/src/tts/` (`TtsEngine` trait, `SpeechState`, `ReplySpeech`, `sentences.rs` splitter, `system.rs` engine). `llm_client::send_conversation` now streams (SSE) for both formats, and falls back to a plain JSON reply if a server ignores `stream: true`.
- **Interrupts:** `utils::cancel_current_operation` (Escape, the overlay ✕, the tray's Cancel and `--cancel`), any recording-hotkey press, `TranscribeAction::start`, "New conversation", and turning "Speak replies" off all call `tts::stop`.
- **For Step 5 (Kokoro):** ✅ done; see Notes from Step 5.

**Milestone:** at this point the full privacy loop works: your voice stays local, only text goes to Claude, and the reply is spoken locally.

---

## Step 4 — Store API keys in Windows Credential Manager ✅ Done (2026-10-27)

> Read CLAUDE.md first. API keys are stored in the settings file via `SecretMap` (`post_process_api_keys` in `settings.rs`). They're redacted from logs but sit in plain text on disk. Move them to the OS keychain using the `keyring` crate (Windows Credential Manager, macOS Keychain, Linux Secret Service).
>
> - Service name `com.neulinc.kusema`, one entry per provider id.
> - Migrate existing keys on startup: if a key exists in the settings file, move it to the keychain and blank it in the file.
> - If the keychain is unavailable (some Linux setups), fall back to the current behavior and log a warning, never crash.
> - Keep the settings UI working unchanged from the user's point of view.
> - Keep the diff small and localized; this file is heavily touched upstream.
>
> Add tests where practical. Commit as "Store API keys in OS keychain".

### Notes from Step 4

- **How it works:** `src-tauri/src/secrets.rs`. In memory, `AppSettings.post_process_api_keys` still holds the real keys, so nothing that reads them changed (actions, assistant, the settings UI). Only the copy written to `settings_store.json` changes: `secrets::for_disk` moves each key into the keychain and blanks it in the file, and `secrets::load_keys` fills the blanks back in on every `get_settings` (cached, so the keychain is read once per provider per run). The `settings.rs` diff is the four `store.set` calls plus one `load_keys` line each.
- **Entries:** service `com.neulinc.kusema`, user = provider id. On Windows they show in Credential Manager as `anthropic.com.neulinc.kusema` and so on, under Windows Credentials > Generic Credentials.
- **Migration:** a plaintext key found in the file moves to the keychain on first read, and the file is rewritten without it. Clearing a key in the UI deletes the keychain entry.
- **Fallback:** if the keychain can't be used (e.g. Linux without a Secret Service), keys stay in the file as before, with a single warning in the log.
- **Crate:** `keyring` 4 (Windows Credential Manager, macOS Keychain, and a pure-Rust Secret Service client on Linux, so no new system libraries). Keychain calls run on a short-lived OS thread so a blocking backend can't stall the async runtime.
- **Downgrade caveat:** a build from before this step will see blank keys, because it doesn't read the keychain. Re-enter the key there if you ever need an older build.

---

## Step 5 — Kokoro voices + voice picker ✅ Done (2026-10-27)

> Read CLAUDE.md first. Add **Kokoro** (Kokoro-82M, Apache 2.0) as a second `TtsEngine` in `src-tauri/src/tts/kokoro.rs`, running locally via ONNX.
>
> 1. Investigate existing Rust options first (e.g. `ort` crate with the Kokoro ONNX model, or an existing Rust Kokoro crate) and pick the most maintainable approach. Explain the trade-off before writing lots of code. Note that Handy already ships ONNX/ML runtimes for Parakeet; reuse whatever is already a dependency if possible rather than adding a second runtime.
> 2. Download the model and voice files on demand, using the same download/progress mechanism Handy uses for speech models (see `managers/model.rs` and `catalog/`), and store them in the app data folder.
> 3. Voice dropdown groups voices by engine ("Kokoro — af_heart", "Windows — Microsoft Zira"), with Preview, and falls back to system voices if Kokoro isn't downloaded.
> 4. Must run on CPU in real time on a mid-range laptop. Measure time-to-first-audio and report it.
>
> Commit as "Add Kokoro voices".

### Notes from Step 5

- **No ONNX runtime to reuse.** Upstream Handy now runs every model (Parakeet included) through `transcribe-cpp` (GGML), which has no TTS. Kokoro on GGML (kokoro.cpp) would bring a second copy of ggml next to transcribe-cpp's, and needs espeak-ng.
- **Engine: our own small module on `ort` (ONNX Runtime), not a Kokoro crate.** Every Rust Kokoro crate phonemizes with espeak-ng (GPL-3.0); `kokoro-tts` even compiles extracted espeak-ng C code while calling itself Apache-2.0. Kusema instead uses **misaki's dictionaries** (Kokoro's own phonemizer data, Apache-2.0, ~180k US and UK entries in Kokoro's phoneme set) plus Rust for numbers/money/times, inflections, compounds and acronyms, and public-domain NRL letter-to-sound rules for unknown words (mostly names). English only; Kokoro's other languages need other phonemizers.
- **Runtime: `ort` with `load-dynamic`.** The official Microsoft ONNX Runtime 1.31.1 library is downloaded with the voices and loaded at runtime, so the installer doesn't grow and builds don't fetch anything. Supported: Windows x64/ARM64, macOS Apple Silicon, Linux x64/ARM64 (Intel Macs show no Kokoro option). Only Windows x64 has been run.
- **Download:** ~202 MB once (ONNX Runtime archive 83 MB, of which only the 17 MB DLL is kept; int8 model 92 MB; 28 English voices 15 MB; dictionaries 13 MB), 131 MB on disk, in `<app data>/kokoro/`. Every file is pinned by URL revision, size and SHA-256 and fetched with Handy's resumable downloader (`managers/model/download.rs`, made `pub(crate)`; no other change to it). Remove deletes the files; on Windows the loaded DLL stays until restart and is reused by the next download.
- **Where things live:** `src-tauri/src/tts/kokoro/` (`mod.rs` engine, `assets.rs` manifest/download/commands, `g2p.rs`, `numbers.rs`, `rules.rs`) and `src-tauri/src/tts/player.rs`, the shared playback queue that used to be in `system.rs`. Both engines implement a small `Synthesizer` trait; voice ids `kokoro:<name>` go to Kokoro, everything else to the system voices (old voice settings keep working). Frontend: `KokoroVoices.tsx` (download / progress / cancel / remove) in the Voice section, and engine-labelled voices ("Kokoro — Heart (en-US)", "Windows — Microsoft Zira (en-US)").
- **Fallbacks:** if Kokoro isn't downloaded or fails on a sentence, that sentence is spoken with the default system voice. The first download switches "System default" to Kokoro Heart; removing Kokoro switches back.
- **Speed (Ryzen 5 5600H, a mid-range laptop CPU, int8 model):** real-time factor 0.26–0.32, i.e. ~3× faster than playback. Model load 1.0 s plus 0.5 s for the dictionary, done in the background when an Ask Kusema recording starts, so it overlaps the user speaking. **Time to first audio** (after the reply's first sentence arrives): 0.6 s for a short sentence, 0.75 s for a 6-second one. Long sentences are split at clause punctuation, so the first clause plays while the rest is synthesized, with no gaps measured. The fp32 model was ~15% slower and 3.5× larger, so it isn't offered.
- **Memory:** the model is freed after 10 idle minutes and reloaded on the next question.
- **Tests:** unit tests for phonemization, numbers, rules and the manifest. Two opt-in tests: `kokoro_downloads` (downloads and verifies everything into `KUSEMA_KOKORO_DOWNLOAD_DIR`) and `kokoro_speaks` (synthesizes with the real model from `KUSEMA_KOKORO_DIR`, prints timings, writes `kokoro-test.wav`).

---

## Step 5b — Kusema release channel and auto-updates ✅ Done (2026-10-27)

**Goal:** testers (and later customers) get new versions automatically. Testers on 0.1.0 have the updater disabled, so they must install the _next_ version manually **once**; every version after that updates itself.

> Read docs/kusema-build-plan.md and CLAUDE.md first. We're on **Step 5b: Kusema's own release channel and auto-updater**. In Step 1 we disabled Handy's updater (`KUSEMA_UPDATE_CHANNEL_CONFIGURED = false`, blank pubkey/endpoint in `tauri.conf.json`). Set up our own.
>
> 1. **Updater signing key:** walk me through generating a Tauri updater key pair (`bun tauri signer generate`), protected with a password. The **private key and password must never be committed**: tell me where to store them (password manager, plus GitHub Actions secrets `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`). Put only the public key in `tauri.conf.json`. Explain that this key is separate from Windows code signing (Authenticode), which we still don't have.
> 2. **Endpoint:** point the updater at `https://github.com/neulinc/kusema/releases/latest/download/latest.json`. Check whether the repo is public; if it's private, release assets need auth and the updater can't fetch them. In that case tell me and propose an alternative host (e.g. a public releases-only repo or Cloudflare R2) instead of guessing.
> 3. Set `createUpdaterArtifacts` back on, re-enable the updater (`KUSEMA_UPDATE_CHANNEL_CONFIGURED = true`) and restore the "Check for updates" UI, keeping any behavior that disables updates via `HANDY_DISABLE_UPDATER` or portable mode.
> 4. **Release workflow:** add or adapt a GitHub Actions workflow that, when I push a tag like `v0.2.0`, builds the Windows NSIS and MSI installers, signs the update artifacts with the secrets, and publishes a GitHub Release with the installers plus `latest.json`. Windows only for now. Remove or disable Handy's upstream release workflows that would publish to the wrong place or need Handy's secrets.
> 5. **Release checklist** in `docs/releasing.md`: bump the version in `tauri.conf.json`, `Cargo.toml` and `package.json`, write short release notes, tag, push, verify the release, and test the update on a machine with the previous version.
> 6. Bump the version to 0.2.0 for the first release with the updater enabled.
>
> Run lint, `cargo check`, tests and a local `bun run tauri build` to confirm updater artifacts are produced (signing with my key via environment variables, not files in the repo). Stop any processes you started. Mark Step 5b done and commit as "Add Kusema release channel and auto-updates".

**Test:** install 0.2.0 from the release on a test machine, publish 0.2.1 with a trivial change, and confirm the app offers and installs the update with settings, models, voices and the API key intact.

### Notes from Step 5b

- **Host: GitHub Releases on `neulinc/kusema`.** The repo is public, so the app can fetch `releases/latest/download/latest.json` and the installers without auth. If the repo ever goes private, move releases to a public host and change the endpoint.
- **Updater re-enabled by reverting Step 1's changes** rather than flipping a constant: `update_checks_forced_disabled()`, the footer and the "locked" text are back to Handy's code, so `HANDY_DISABLE_UPDATER` and portable mode behave as upstream (portable installs get a "download the installer" dialog, now pointing at Kusema's releases).
- **Release workflow:** `.github/workflows/release.yml` (replaces Handy's) runs on a `v*.*.*` tag. It checks that the signing secrets exist, that the tag matches the version in all three files, and that `src/content/release-notes/<version>.md` exists; creates a draft release from those notes; builds Windows x64 through Handy's `build.yml` (which also installs and smoke-tests both installers); and publishes the release only once `latest.json`, both installers and both `.sig` files are uploaded, so the updater never sees a half-finished release. `main-build.yml` no longer runs on every push (its macOS legs need Apple secrets); keep our `release.yml` and `main-build.yml` on upstream merges.
- **GitHub Actions has never run on this fork**: GitHub disables workflows on forks until they're enabled in the Actions tab. The workflow also needs "Read and write" workflow permissions. Both are in `docs/releasing.md`.
- **Local signing:** `TAURI_SIGNING_PRIVATE_KEY` must hold the key's contents or its path; `TAURI_SIGNING_PRIVATE_KEY_PATH` is ignored by tauri-cli 2.11.4 (the build fails at the signing step). `bun run tauri build --no-sign` builds without the key. Verified locally with a throwaway key: `Kusema_0.2.0_x64-setup.exe`, `Kusema_0.2.0_x64_en-US.msi` and their `.sig` files.
- **Release notes** in `src/content/release-notes/<version>.md` show once in the app after an update and are the GitHub Release text. Handy's `0.9.0.md` and `0.9.7.md` are still there and would show when Kusema reaches 0.9; delete them before then.
- **0.2.0 is the first version with the updater.** Testers on 0.1.0 install it by hand once.
- **Updater public key** (minisign key ID `A8BC6AC0C1F11AB4`) is in `tauri.conf.json`. The private key and password are kept outside the repo (password manager and GitHub Actions secrets).

---

## Step 6 — "Ask about my screen" (window screenshot + vision) ✅ Done (2026-10-27)

**Goal:** the user holds a dedicated hotkey and asks "what do you think of this document?" Kusema captures the **active window**, shows the user exactly what will be sent, and only after confirmation sends the screenshot plus the question to a vision-capable model. The voice stays local as always; the screenshot leaves only on explicit approval.

> Read docs/kusema-build-plan.md and CLAUDE.md first. We're on **Step 6: Ask about my screen**. Assistant mode (Steps 2–5) works. Add a screen-aware variant.
>
> 1. **Hotkey:** new binding `assistant_screen`, "Ask about my screen", Windows default `ctrl+alt+shift+space` (sensible macOS/Linux defaults), registered like `assistant`. Plain Ask Kusema must never capture anything.
> 2. **Capture, at hotkey press:** record the foreground window _before_ any Kusema overlay appears, and capture only that window, never the full screen or other monitors. If the foreground window is Kusema itself, the desktop or the taskbar, don't capture; explain instead. Evaluate the `xcap` crate versus the Windows Graphics Capture API directly; handle DPI scaling and multiple monitors correctly; explain your choice. Briefly show a subtle border flash around the captured window so the user knows what was captured.
> 3. **In memory only:** never write screenshots to disk, logs or History. History records the question and answer with a "[screenshot]" marker only. Discard the image when the conversation is cleared or the app exits.
> 4. **Confirmation (default ON):** after transcription, show a small confirmation window with a thumbnail of the captured window (click to enlarge), the transcribed question, the provider/model it will go to, and **Send** / **Cancel** buttons (Enter / Escape). Kusema can say "Send this screenshot to Claude?" if speech is on. Auto-cancel after 30 seconds. Nothing is uploaded before Send. A setting "Confirm before sending screenshots" defaults on; turning it off shows a clear warning.
> 5. **Preparing the image:** downscale so the long edge is at most ~1568 px (good for Claude vision; keeps cost and latency down) and encode as JPEG (~85%) or PNG for text-heavy windows, whichever is smaller.
> 6. **Sending:** extend `send_conversation` to support image content in a user turn: Anthropic format (`image` content block, base64 source) and OpenAI format (`image_url` with a data URL). Ollama's OpenAI-compatible endpoint should work for local vision models through the Custom provider. If the selected model or provider doesn't support images, show and speak a clear error ("This model can't see images. Choose a vision model like Claude, or a local vision model in Ollama") instead of failing silently.
> 7. **Follow-ups:** keep the most recent screenshot in the conversation so follow-ups like "summarise the second paragraph" work, but drop older screenshots from history to control cost.
> 8. **Provider for screen questions (setting):** "Use for screen questions: same as Assistant / a different provider", so users can force screenshots to a **local** model (Ollama) while normal questions go to Claude. This is the privacy option for confidential documents.
> 9. Update the assistant system prompt for screen questions: describe what it sees, answer the question, and keep spoken answers short. Let the user ask for detail.
> 10. English strings copied to other locales, as before.
>
> Add tests for image encoding/downscaling, both request formats with images, and the "drop older screenshots" logic. Run `cargo check`, `cargo test`, lint, `bun run tauri dev`. Stop any processes you started. Mark Step 6 done and commit as "Add Ask about my screen".

**Test:** open a Word document or PDF, press Ctrl+Alt+Shift+Space, ask "what do you think of this document?", check the confirmation shows the right window, click Send, and listen to the answer. Then follow up with "make the first paragraph shorter." Also test Cancel (nothing sent), a non-vision model (clear error), and the local-model setting if Ollama is installed.

### Notes from Step 6

- **Capture: GDI copy of the window's area of the screen, not `xcap` or Windows Graphics Capture.** At the hotkey press, before any overlay, Kusema takes the foreground window, refuses Kusema's own windows, the desktop, taskbar/Start, minimized and cloaked windows (a notice card explains, and is spoken), takes its visible bounds from DWM (`DWMWA_EXTENDED_FRAME_BOUNDS`, physical pixels; Kusema is per-monitor DPI aware) clipped to the virtual screen, and copies that rectangle with `BitBlt`. It's what the user sees, GPU-rendered content included, on any monitor or across two, in ~10–30 ms. `xcap` uses `PrintWindow` on Windows, which waits on (and can hang with) the target app, and on Linux pulls in PipeWire/XCB system libraries the CI and Nix builds don't have. WGC can capture a covered window, but needs a D3D capture session, delivers frames asynchronously and draws a yellow border on Windows 10. The trade-off: an always-on-top window over the target ends up in the shot, which the confirmation card shows. Windows only; elsewhere the hotkey explains that.
- **Border flash:** a click-through layered window shaped as a 4 px frame (`SetWindowRgn`) in the logo colour, held 250 ms then faded; drawn after the capture, so never in it.
- **In memory only:** the capture sits in `ScreenState` from press to transcription and is taken (and dropped) by the stop task. `ChatImage`'s `Debug` prints only its size, `ChatTurn.image` is `#[serde(skip)]` (the UI gets a `screenshot` flag), History gets "[screenshot] question". Cleared with the conversation; nothing on disk.
- **Image:** long edge ≤ 1568 px (Catmull-Rom), then PNG and JPEG (q85) are both encoded and the smaller is sent. `image`, `png` and `fdeflate` are built optimized in dev, or resizing takes seconds.
- **Confirmation card:** a fresh webview window per card (`screen_card_<n>`, page `src/screen-card/`), placed beside the window on its monitor if there's room, else above/below, else inside its bottom-right corner, never over the middle. It shows the exact image to be sent (click to enlarge), the question and provider/model, says "Send this screenshot to …?" when replies are spoken, takes focus once rendered (Enter/Escape), gives focus back to the window after, and cancels after 30 s. The Cancel hotkey also cancels it. Errors after capture (no vision, no key, provider errors) appear on a notice card beside the window and are spoken, as well as on the Assistant page.
- **Requests:** `send_conversation` sends a turn's image as an Anthropic `image` block (base64) or an OpenAI `image_url` data URL, image before the text. A rejection mentioning images/vision/content types becomes "This model can't see images…".
- **Follow-ups and cost:** the latest screenshot stays attached, so plain Ask Kusema follow-ups can see it; a new screenshot drops older ones, and their questions are marked "[An earlier screenshot, no longer attached]". The screen system prompt is added once a conversation has a screenshot.
- **Privacy option:** "Use for screen questions" (provider plus optional model, keys from Post Process). Once a conversation includes a screenshot, every follow-up goes to that provider too, as the answers describe the screen; an image only goes to the provider it was approved for.
- **Tested by the user** with a vision model: capture, card, Send and follow-ups worked. Not yet tried: a non-vision model, a local Ollama vision model, and confirmation turned off.

---

## Testing rule (hard rule)

**No test or dev run may use the real user's app data, settings, models folder, ledger, History, API keys, startup entry or clipboard.** This became a hard rule after real data was touched twice.

- **Launch only through `scripts/dev_isolated.py start`,** never `bun run tauri dev` directly, and that includes "run the app so I can try it". It sets up an isolated folder first, launches, then verifies the folder is in use and stops the app if it can't. `stop` ends the run and reports whether anything real changed.
- **Never run setup and launch in parallel.** Setup finishes and is confirmed before anything starts, so a failed setup can't turn into a run on real data. This is how it went wrong the second time.
- **The build enforces it too.** With `KUSEMA_DEBUG_ISOLATED` set, a debug build refuses to start without its isolated folder, keeps API keys in memory rather than Credential Manager, leaves the Windows startup entry alone and refuses to touch the clipboard, so tests type with the Direct paste method. `HF_HOME` keeps the shared model cache out of reach. Release builds ignore all of this.
- **Headless runs count.** Regenerating bindings with `--list-devices` also goes through the isolated folder; `tauri dev` regenerates them, so just use the launcher.
- **If real data is touched anyway:** stop at once, restore it, and tell the user exactly what changed.
- **What the isolated run can't separate:** global hotkeys and the foreground window are shared with the real desktop. Tests only type into a private test window.

---

## Roadmap

| Version | Steps                                                                                                                                       |
| ------- | ------------------------------------------------------------------------------------------------------------------------------------------- |
| v0.4    | Step 7 continuous dictation; Step 8 privacy ledger and Local-only mode; Step 9 smart cleanup by default; Step 10 two-minute first-run setup |
| v0.5    | Step 11 voice editing, dictionary, legal vocabulary, Dragon import, styles per app, type-to-ask, paste-the-answer                           |
| v0.6    | Step 12 legal templates and snippets, foot pedal and dictation mics, transcribing recordings, personal stats                                |
| Launch  | Launch gate: model hosting ✅, Windows code signing, product page, launch                                                                   |

---

## Step 7 (v0.4) — Continuous dictation with voice commands ✅ Done (2026-11-01)

**Goal:** a lawyer starts dictating once and keeps talking, for a long memo if need be. Text appears in the active app at each pause, and spoken commands like "new paragraph" or "scratch that" act instead of being typed. Everything happens on the computer.

**Test (still to do with a real voice):** start it in Word and dictate several paragraphs for at least 10 minutes, using each command. Say "new paragraph" inside a sentence and check it's typed, not obeyed. Switch windows mid-session, then stay silent until it stops by itself.

### Notes from Step 7

- **Code:** `src-tauri/src/dictation/`. `segmenter.rs`, `join.rs`, `voice_commands.rs` and `guard.rs` are pure and tested; `mod.rs` runs the session; `platform.rs` holds the Windows calls. Windows only for now: elsewhere the binding exists but a session refuses to start, because the window guard can't keep its promise there.
- **What's reused:** the microphone and Silero VAD (`CaptureSink` gained an optional `FrameTap`, so audio goes to the segmenter instead of building up), the coordinator (so other hotkeys are ignored during a session), the cancel shortcut and local batch transcription. `audio_toolkit/chunking.rs` isn't used for cutting (it plans 30 s cuts with 5 s of lookahead), and neither are the streaming paths (only some models have them, and they yield one provisional text, not segments). "Keep Mic Open" isn't needed: a session holds the microphone.
- **Default key:** `Ctrl+Alt+Shift+D`. `Ctrl+Alt+D` would be the natural choice, but it inserts an endnote in Word. handy-keys supports Numpad + (`keypadplus`), and the Tauri backend has `NumpadAdd`, so the "Use Dragon's key" button sets whichever applies. While it's set, Numpad + doesn't type a plus sign.
- **Escape is captured globally while dictating.** Like any recording, a session registers the dynamic cancel shortcut, so Escape anywhere stops dictation and doesn't reach the app in front (it won't close a Word dialog, for example). Every way a session ends goes through one stop path that unregisters it. Checked in the running app: the log shows it unregistered after Escape, "stop dictation" and the silence auto-stop, and pressing Escape afterwards no longer reaches Kusema.
- **Text said while paused:** speech that began before the window changed is held and typed on return. Speech that began after it is saved to History but not typed, and the overlay says so, because it may have been meant for someone in the room. A session's History entry can therefore contain more than the document does.
- **Typing:** each segment is saved to History first, then typed. The window is checked on the input thread immediately before the keystroke; if it changed, or Alt, Shift or Windows is held (mid Alt+Tab, Ctrl+V would become another shortcut and the text would silently not arrive), the segment is held instead. Auto-submit and the trailing-space setting never apply. Clipboard writes carry the markers that keep text out of clipboard history and cloud sync. Clipboard handling "copy to clipboard" copies the whole session at its end.
- **"Scratch that":** deletes with Backspace and also removes the text from History. Refused, with a message in the overlay, if the focused window or control changed, or if the keyboard or mouse was used at all since the last typing (a click can move the caret; Windows' last-input time doesn't say which input it was). It only goes back one segment.
- **Auto-stop** counts from the last words, not the last sound: in testing, room noise set off the VAD about every 30 seconds and transcribed to nothing.
- **Capitalization** only ever adds a capital: after a sentence end (not after abbreviations like "v." or "Inc."), and after a line break. It doesn't lowercase what the model capitalized, so names are safe, but a model that leaves off a period gives "the motion The parties". Step 9's cleanup is the place to fix that.
- **Debug hooks:** in debug builds, `KUSEMA_DEBUG_ISOLATED` (set by `scripts/dev_isolated.py`; see the testing rule) runs beside an installed Kusema, and `KUSEMA_DEBUG_DICTATION_WAV` / `KUSEMA_DEBUG_DICTATION_SPEED` feed a session from a file through the real VAD and model.
- **Memory:** a simulated 30-minute session (30 min of synthesized speech fed through the real VAD, Parakeet and History at about 7x, so 4.5 minutes of wall time; about 350 segments, 3,402 words; debug build, paste method None so no keystrokes) stayed flat: app 169–176 MB, transcribe worker 177–178 MB, WebViews 567–588 MB and falling. The app dropped to 63 MB when the session ended. CPU came to about 270 s, or roughly 15% of one core if spread over the real 30 minutes. The segmenter holds at most one 30 s segment (under 2 MB). A slow model can't grow the queue past 5 minutes of audio: the session stops and what's queued finishes.
- **Known limits:** room noise can occasionally transcribe to a stray word ("On." once in 4 minutes of an idle room here); filtering harder would drop real short words like "no", so it's left to "scratch that". Changing the overlay position setting during a session resizes the overlay to the normal width until the next session.

---

## Step 8 (v0.4) — Privacy ledger and Local-only ("Privileged") mode ✅ Done (2026-11-01)

**Goal:** a firm can see and control what leaves the computer. A local ledger lists everything sent: when, to which provider and model, and what kind (text or screenshot). One "Privileged" switch guarantees that no dictation, question or screenshot leaves the computer: cloud requests are refused and explained, never sent quietly.

### Notes from Step 8

- **Code:** `src-tauri/src/network/`. `network::send` is the one way HTTP leaves Kusema: it records the request, then sends or refuses it. `policy.rs` (the rules) and `ledger.rs` (the record) are pure and tested. A test scans the source and fails if a new HTTP client appears outside `network`, which is also how a merge from Handy that adds one gets caught.
- **Every way out of Kusema:**

| What                                                          | Goes to                                                     | How it's controlled                                                                                                                                                                   |
| ------------------------------------------------------------- | ----------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Ask Kusema, screen questions, AI cleanup, provider model list | The configured provider                                     | `network::send`, enforced in Rust                                                                                                                                                     |
| Mirror and URL model downloads, Kokoro voices                 | `huggingface.co`, `raw.githubusercontent.com`, `github.com` | `network::send`, enforced in Rust                                                                                                                                                     |
| Hugging Face model downloads                                  | `huggingface.co` and its CDN                                | **Not routed:** `hf_hub` makes its own requests. Logged and refused with `network::announce` just before it starts; its redirects to the CDN aren't listed                            |
| Update check and update install                               | `github.com`                                                | **Not routed:** the updater plugin runs from the webview. The frontend asks `authorize_update_check` first, which logs and refuses; that check lives in the frontend, unlike the rest |
| Links the user clicks (About, help)                           | The default browser                                         | Not Kusema's request. Not listed                                                                                                                                                      |
| Webview                                                       | Nothing                                                     | The frontend makes no requests, and a Content Security Policy now stops it (it was `null`, inherited from Handy). `connect-src` allows only Kusema's own IPC and asset protocols      |

- **Protections the audit added:** each redirect is checked like a request, since a local server could otherwise 307 a question to the cloud with its body. System proxies are skipped for local addresses, since one could carry "localhost" traffic off the computer. A hostname that only looks local (`ollama`, `localhost.example.com`) counts as public: Privileged mode needs an address it can check, so LAN servers need a private IP or a `.local` name.
- **Windows clipboard:** normal dictation's paste now carries the markers that keep text out of clipboard history and cloud clipboard sync, as continuous dictation's did. Without them, Windows could have uploaded dictated text outside anything this ledger can see. (The "copy to clipboard" setting and Copy buttons stay ordinary copies, because the user asked for them.)
- **Audio:** the "0 bytes" on the Privacy page is true by construction, not counted. `llm_client` sends only text and images, `DataKind` has no audio, and recordings are only read by the local engine and the local player.
- **Telemetry:** none, including anything inherited from Handy. No analytics library in `Cargo.lock` or `package.json`, and no such calls in the source. Note that model and update requests necessarily show the server the computer's IP address, though downloads no longer touch Handy's own server (see "Launch gate"). AI requests identify themselves as Kusema in the `User-Agent`, `Referer` and `X-Title` headers.
- **Outside what this can see:** Windows and the WebView2 runtime have their own network traffic (Edge updates, for example). This wasn't measured. An "external script" paste method runs whatever the user configured, with the dictated text.
- **Webview file access:** narrowed from `**` to the recordings folder, the only thing the frontend reads from disk (History's audio player). Models and voices load in Rust, and images are bundled.
- **Refusals:** AI cleanup still types the raw dictation and shows the message. A blocked screenshot skips the confirmation card. Blocked requests list the size they would have been. The message is English only, like the rest of the backend's messages.
- **Debug hooks:** Step 7's still apply. For a test against a real page, `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9333` opens the WebViews to DevTools.

---

## Step 9 (v0.4) — Smart cleanup on by default, with a local option ✅ Done (2026-11-01)

**Goal:** dictated text reads well from the first sentence (punctuation, filler words, false starts) without the user finding Post Processing or getting an API key. A local option means this also works in Privileged mode. Also spoken punctuation for Dragon users: comma, period/full stop, question mark, exclamation mark, colon, semicolon, open/close quote, open/close paren(thesis), hyphen/dash, new paragraph/new line already done; applied locally, in continuous and normal dictation.

### Notes from Step 9

- **Code:** `src-tauri/src/cleanup/` (spoken punctuation, pure and tested) and `dictation/join.rs` (missing periods). It runs after Handy's filler removal, before any AI cleanup, for dictated text only: questions to the Assistant and transcribed files stay as they were. Other languages pass through, because the rules are English words.
- **The false-positive rule:** speech recognition gives no reliable pauses, and Parakeet adds its own period after a spoken one, so the rule reads the surrounding words. "period" counts only at the end of a phrase or before a capitalized word, never after a determiner or a word that makes a time period ("the limitations period expired", "within the limitations period."). Other marks count anywhere unless a determiner comes before them ("the colon") or a neighboring word marks them as ordinary ("colon cancer", "Oxford comma").
- **The trade-offs:** "denied period the parties may appeal", one lowercase run, stays as words. "a busy period." at the end of a phrase becomes punctuation, because "busy" isn't on the list. "literal period" and "scratch that" are the ways out, and the setting turns it all off. The word lists are in `punctuation.rs`.
- **"literal"** only counts before a spoken mark or a voice command, so "a literal reading" stays as said.
- **Missing periods:** added between continuous dictation segments only when this segment could finish a sentence (at least three words, not ending in a word like "the" or an abbreviation like "Mr", "U.S" or "Inc") and the next opens with a word from a list of sentence openers. A list, not "any capital", so names never qualify. Nothing is ever lowercased. No period at a "new line" or "new paragraph", because those are often address lines. Not in normal dictation, which has no next segment.
- **The one switch:** Local Cleanup off also stops filler removal, so text is left as transcribed (Handy's stutter and spacing cleanup still runs).
- **AI cleanup:** moved from Experimental to the General page's Cleanup section, with its provider status. The default prompt is rewritten for professional writing; settings holding Handy's untouched prompt move to it, and an edited prompt is left alone. It only runs with its own shortcut, never in continuous dictation, and goes through the privacy ledger and Privileged mode like everything else.
- **Tested on real Parakeet output** with synthesized speech. That found two bugs unit tests missed: the model's period after "close quote" was being dropped, and the Direct paste method doubled line breaks.

---

## Step 10 (v0.4) — Two-minute first-run setup

**Goal:** someone non-technical goes from opening the installer to their first dictated sentence and first spoken answer in under two minutes, with sensible defaults and no decisions they can't make yet.

**Proposed model list (not built yet):** a short list for English-speaking law firms, replacing the full catalog in setup and on the Models page. Error is the catalog's English benchmark, not legal dictation, so check it against real dictation first. Speed is times real time on CPU.

| Model                        | Shown as                                 | Size   | English error | Speed | License           |
| ---------------------------- | ---------------------------------------- | ------ | ------------- | ----- | ----------------- |
| **Parakeet Unified English** | Default, chosen for the user             | 731 MB | 3.99%         | 14x   | NVIDIA Open Model |
| Parakeet TDT 0.6B v3         | "Clients who speak other languages" (25) | 740 MB | 4.83%         | 14x   | CC-BY-4.0         |
| Canary 180M Flash            | "Older computers or little disk space"   | 218 MB | 5.98%         | 22x   | CC-BY-4.0         |
| Nemotron 3.5 Streaming       | "Live text as you speak" (less accurate) | 751 MB | 7.90%         | 11x   | OpenMDW           |

Everything else moves behind a "Show all models" link under Advanced, including the legacy Whisper and Breeze files, which stay usable for anyone who already has them. Models whose licenses don't fit a paid product are already gone: see "Keeping up with Handy".

---

## Step 11 (v0.5) — Voice editing, dictionary, styles and more ways to ask

**Goal:** fix and shape text by voice, and have Kusema learn how the user's practice talks.

- **Voice editing:** change text by voice ("change Tuesday to Thursday").
- **Learning personal dictionary:** picks up names and terms from the user's corrections.
- **Legal vocabulary pack:** legal terms, Latin and citation forms recognized correctly out of the box.
- **Dragon custom-word import:** bring over the word list from Dragon.
- **Styles per app:** different cleanup for email, a legal document or a chat.
- **Type-to-ask:** ask Kusema by typing, when speaking isn't possible.
- **Paste-the-answer:** type the assistant's reply into the active app instead of speaking it ("draft a reply declining this meeting").
- **"Clean up this dictation session":** AI cleanup of a whole continuous session, with a preview, at its end. Deferred from Step 9 because replacing text already in the document isn't safe: clicking the preview loses the caret, the app can't see the document, and deleting thousands of characters by count could eat other text, the same risk "scratch that" refuses. A version that replaces only the History entry, for the user to copy, is safe if that's enough.
- **Live preview while dictating:** show words in the overlay as they're spoken (like Dragon's results box) while still typing into the document only at pauses. Evaluate periodic re-transcription vs. a streaming engine (Moonshine).

---

## Step 12 (v0.6) — Legal templates, dictation hardware, recordings and stats

**Goal:** fit the way legal staff already dictate.

- **Legal templates and snippets:** insert standard letters and clauses by voice.
- **Foot pedal and dictation mic support:** start and stop from the hardware legal staff already own.
- **Transcribing recordings:** turn recorded dictation into text.
- **Personal stats:** words dictated and time saved, kept on the computer.
- **Watch folder:** automatically transcribe audio files saved by a dictation recorder into a chosen folder (local only, results saved as text/Word alongside).

---

## Launch gate

Nothing is sold until all of these are done.

- **Host model downloads ourselves:** ✅ Done (2026-11-01). See the notes below.
- **Windows code signing (Azure Artifact Signing):** removes the SmartScreen warning from the installer and updates.
- **Product page:** what Kusema does, its privacy claims and how to get it, written for small law firms.
- **IT documentation:** what Kusema sends (ledger), what it never sends (audio), and Windows/WebView2 network traffic outside Kusema's control.
- **License review:** have a lawyer review model and dependency licenses (especially NVIDIA Open Model License terms) before selling.
- **Detach from the Handy fork network:** detach neulinc/kusema via GitHub Support (so it appears in GitHub search); keep the upstream remote, MIT license and Handy credit.
- **Launch:** to small and solo US law firms, with accessibility users as the community channel.

### Notes on model hosting

- **What's hosted, at [huggingface.co/kusema](https://huggingface.co/kusema):** the five legacy files (`kusema/whisper-ggml`, `kusema/breeze-asr-ggml`) and the default quant of four catalog models (`kusema/parakeet-unified-en-0.6b-gguf`, `parakeet-tdt-0.6b-v3-gguf`, `nemotron-3.5-asr-streaming-0.6b-gguf`, `canary-180m-flash-gguf`): 7.2 GB in all. Each is byte-identical to what Kusema pins, with a model card, attribution and license files; the NVIDIA Open Model License repo also has its required `NOTICE`. `scripts/kusema_mirror.py` publishes and re-verifies them. Its only remaining use of Handy's server is as the source of the legacy files, in case one ever has to be published again.
- **How downloads work now:** legacy models come only from Kusema's repos. Catalog models come from their `handy-computer/*` repo on Hugging Face at the pinned revision; the four above fall back to Kusema's copy, and everything else, including the language detector, has no fallback. Size and SHA-256 checks are unchanged.
- **What still isn't ours:** the primary source of every catalog model is still Handy's organization on Hugging Face. If those repos go away, the four mirrored models keep working through the fallback and the rest can't be downloaded. Kokoro voices still come from `huggingface.co/onnx-community`, `raw.githubusercontent.com` and `github.com`.
- **Privacy ledger:** direct downloads go through `network::send` and `hf_hub` through `network::announce`, so both list `huggingface.co`, not the CDN hosts it redirects to.

---

## Later

Not scheduled. The first two keep their detailed prompts, ready to use.

### Voice commands for Windows settings (computer control)

**Goal:** "Kusema, switch to dark mode" changes the setting and replies "Done." The AI only _chooses_ from a fixed whitelist of actions Kusema implements locally; it never runs arbitrary commands.

> Read docs/kusema-build-plan.md and CLAUDE.md first. We're on **voice commands for Windows settings: computer control via tool use**. Assistant mode (Steps 2–3) works; extend it so the LLM can call a fixed set of local actions.
>
> 1. **Tool registry:** create `src-tauri/src/tools/` with a `Tool` trait (name, description, JSON-schema parameters, risk level `Safe | Confirm`, `execute(args) -> Result<String>`) and a registry. The registry is a fixed whitelist. There must be no tool that runs arbitrary shell commands, scripts, or file deletion.
> 2. **LLM tool calling:** extend `send_conversation` in `llm_client.rs` to send tool definitions and handle tool-use responses in both the Anthropic format (`tools`, `tool_use` / `tool_result` blocks) and the OpenAI format (`tools`, `tool_calls`). Loop: model requests a tool, Kusema executes it locally, returns the result, model gives the final spoken reply. Cap at 5 tool rounds per question. Keep streaming speech for the final text reply. Models or providers without tool support must fail gracefully ("This model can't control the computer; try Claude or GPT").
> 3. **First Windows actions** (gate each behind `#[cfg(target_os = "windows")]`, and return "not supported on this OS" elsewhere):
>    - `set_theme(mode: light|dark)`: `HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize` (`AppsUseLightTheme`, `SystemUsesLightTheme`), then broadcast `WM_SETTINGCHANGE` ("ImmersiveColorSet") so it applies immediately.
>    - `set_wallpaper(path)`: `SystemParametersInfoW(SPI_SETDESKWALLPAPER, …)`. Only allow image files in the user's Pictures folder or a Kusema wallpapers folder; reject other paths.
>    - `set_volume(level 0–100)`, `change_volume(delta)`, `mute(bool)`: Windows Core Audio API.
>    - `open_settings_page(page)`: launch from an allowlisted set of `ms-settings:` URIs (display, bluetooth, sound, wifi, personalization, notifications…).
>    - `open_app(name)`: launch from an allowlist of common apps (Notepad, Calculator, Explorer, the default browser, Outlook, Word, Excel), never a raw path from the model.
>    - `open_url(url)`: https only, opened in the default browser.
>    - `lock_screen()`, risk `Safe`.
>    - `get_system_info()`: battery, time, current theme, volume.
>    - `sleep()` and `shutdown()`: risk `Confirm`.
> 4. **Confirmation:** for `Confirm` tools, speak/show "Shut down now? Say yes or click Confirm," and only proceed on explicit approval (a button in the overlay/Assistant page, or a spoken "yes" via a quick follow-up recording). Time out to "cancelled" after 15 seconds.
> 5. **Activity log and undo:** record every executed action (time, tool, arguments, result) in a visible list on the Assistant page. Where possible, store the previous value so the user can say or click "undo" (theme, wallpaper, volume).
> 6. **Settings:** "Allow computer control" (default **off**), with per-tool toggles grouped by risk. When off, no tools are sent to the model at all.
> 7. Update the assistant system prompt to tell the model which tools exist, to confirm completed actions in one short spoken sentence, and to say plainly when something isn't possible.
>
> Add unit tests for the registry, argument validation (path and URL allowlists), and the tool-call parsing for both formats. Run `cargo check`, `cargo test`, `bun run tauri dev`. Stop any processes you started. Mark this done in the plan and commit as "Add computer control (tool use)".

**Test:** turn on "Allow computer control", then try "switch to dark mode", "set the volume to 30 percent", "open Bluetooth settings", "undo that", and "shut down" (it should ask first; say no).

### Optional wake word ("Hey Kusema")

**Goal:** hands-free activation as an alternative to hotkeys. Off by default; detection runs entirely on-device.

> Read docs/kusema-build-plan.md and CLAUDE.md first. We're on **the optional wake word**.
>
> 1. **Engine:** evaluate openWakeWord (Apache 2.0, ONNX models; custom phrases trainable from synthetic speech) and Rustpotter (pure Rust, Apache 2.0). Prefer openWakeWord via the ONNX runtime Kusema already ships, if practical. Explain the choice before writing much code. Don't use Porcupine (commercial license).
> 2. **Wake phrase:** "Hey Kusema". Document (in `docs/wake-word.md`) how to train/regenerate the custom model with openWakeWord's training pipeline, and ship a pre-trained model file downloaded on demand like the speech models. Until a trained model exists, support a built-in openWakeWord phrase (e.g. "hey jarvis") as a stand-in, clearly labelled in settings.
> 3. **Listening pipeline:** a background listener on the selected microphone feeding the detector from a short in-memory ring buffer. Nothing is written to disk or sent anywhere before detection.
> 4. **On detection:** play a short chime, start an Assistant-mode recording, and stop automatically when the speaker goes quiet, using the existing Silero VAD (~1 second of silence, with a max length cap). Then follow the normal Assistant flow (transcribe → LLM → speak).
> 5. **Never trigger on itself:** pause detection while Kusema is speaking and while any hotkey recording is active. Also pause when the screen is locked, and optionally when on battery.
> 6. **Settings:** "Wake word" toggle (default **off**) with a one-time explanation dialog: "Kusema will listen for 'Hey Kusema' on this device. Audio is processed locally and discarded." Add a sensitivity slider and a "Test wake word" mode that shows detections without starting the assistant.
> 7. **Visibility and control:** a distinct tray state while listening, a tray menu item "Pause wake word", and a hotkey to toggle it (e.g. during client calls).
> 8. Measure and report idle CPU usage while listening; target under ~2% on a mid-range laptop.
>
> Add tests where practical (detector wrapper, pause/resume state machine). Run `cargo check`, `cargo test`, `bun run tauri dev`. Stop any processes you started. Mark this done in the plan and commit as "Add optional wake word".

**Test:** enable the wake word, say "Hey Kusema, what's the weather like in Kingston usually in March?" with no hotkey. Then check it doesn't trigger while Kusema is speaking, and that "Pause wake word" stops it.

### Linux

Make Linux a supported platform, with its own builds and release channel. It may compile today, but isn't supported or released.

### Moonshine streaming engine

Evaluate Moonshine as a low-latency streaming engine for dictation.

### Meeting notes with consent

Turn meetings into notes, only with a clear consent step: recording a conversation has legal consent requirements, and some US states require every party's consent.

### Other ideas

- **Local API / MCP server** so other tools can use Kusema's private transcription.
- **Redaction**: optional local step stripping names, emails and phone numbers before text goes to a cloud provider.
- **Chatterbox voice cloning** with a consent step (live recording of a scripted consent line, no uploaded files).
- **Separate provider settings for Assistant vs. post-processing**, if you want, e.g., Groq for fast cleanup and Claude for questions.
- **Offline/air-gapped installer (Pro):** bundle the speech model, ONNX Runtime and Kokoro voices in the installer for offices that block downloads.
- **Interrupt by voice:** say "Hey Kusema" while it's talking to cut it off. Needs echo cancellation, so do this after the wake word.
- **Remaining Handy links:** replace handy.computer links (Linux setup guide, macOS Secure Input help, About-page donate link) with Kusema pages or remove them.
- **More Kokoro languages:** Kokoro also has Spanish, French, Hindi, Italian, Japanese, Portuguese and Chinese voices; each needs its own phonemizer (misaki has some), without espeak-ng.
- **Line endings:** ✅ Done (2026-10-27). `.gitattributes` sets `* text=auto eol=lf`, so checkouts are LF even with `core.autocrlf=true`, and `bun run format:check` passes on Windows.

## Keeping up with Handy

**Don't reuse code from AGPL/GPL projects** (e.g. debpalash/VoiceStudio); ideas only.

**Files to watch so downloads don't switch back to Handy's server.** `src-tauri/src/catalog/mod.rs` (`KUSEMA_MIRRORS` replaced Handy's `MIRRORS`), the `ModelSource::Url` entries in `src-tauri/src/managers/model.rs`, and `src-tauri/src/catalog/catalog.json` (a new revision can change a pinned hash, so a mirrored model's copy must be republished with `scripts/kusema_mirror.py`). `cargo test` fails if `blob.handy.computer` appears anywhere in `src-tauri/src`, if a legacy URL leaves `huggingface.co/kusema/`, or if a mirror isn't Kusema's. `scripts/mirror_models.py` is Handy's own tool for its server; Kusema doesn't use it.

**Licenses to re-check after every merge.** Kusema's model list is Handy's catalog minus `REMOVED_MODELS` in `src-tauri/src/catalog/mod.rs`, repeated in `src/lib/models/removed.ts`. The catalog's license field can be wrong (it calls the non-English Moonshine models MIT), so after a merge that changes `catalog.json` or bumps `@handy-computer/models`, read the Hugging Face card of anything new. `cargo test` fails on any new model whose license isn't MIT, Apache-2.0, CC-BY-4.0 or CC-BY-SA-4.0 until someone reviews it, but it can't catch a wrong label, so re-check these by hand:

- **Removed:** `canary-1b` (CC-BY-NC); `granite-speech-5.0-470m-turboctc-nc` (CC-BY-NC-SA); `moonshine-{base,tiny}-{ar,ja,ko,uk,vi,zh}` (Moonshine Community License, non-commercial, labeled MIT); `medasr` (Google HAI-DEF terms); `SenseVoiceSmall`, `Fun-ASR-MLT-Nano-2512`, `Fun-ASR-Nano-2512` (FunASR Model License, "for reference and learning purposes only"; the Fun-ASR upstream cards say Apache-2.0, so these two could come back if that's confirmed).
- **Kept, conditions apply:** `parakeet-unified-en-0.6b` (labeled CC-BY-4.0, now the NVIDIA Open Model License), `multitalker-parakeet-streaming-0.6b-v1` and `nemotron-speech-streaming-en-0.6b` (NVIDIA Open Model License: commercial use allowed, its Trustworthy AI terms apply); `nemotron-3.5-asr-streaming-0.6b` (OpenMDW); `orukeet` (CC-BY-SA, share-alike); the Cohere and Voxtral models (Apache-2.0, with an upstream access form on some).

```
git fetch upstream
git merge upstream/main
```

If there are conflicts, ask Claude Code: "Merge upstream/main from cjpais/Handy, resolve conflicts preserving Kusema's Assistant, TTS, keychain and branding changes, then run cargo check and the tests."
