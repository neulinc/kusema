# Kusema — Build Plan & Claude Code Prompts

_Fork of cjpais/Handy (MIT). Mapped against Handy `main` on 2026-10-27._

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
- **Code signing removed.** The upstream Windows signing command used the Handy author's Azure account. Installers are unsigned until we have our own certificate (see "Later").
- **`src/bindings.ts` gets regenerated on Windows builds** (tauri-specta writes it at dev startup). The diff is only platform-specific doc comments, such as the `isLaptop` stub. It's harmless; commit it or discard it.
- **Auto-updater disabled.** It pointed at Handy's GitHub releases and Handy's signing key. The endpoint and public key are now blank in `tauri.conf.json`, `createUpdaterArtifacts` is off, `update_checks_forced_disabled()` in `settings.rs` always returns true, and the footer "Check for updates" link and tray item are hidden. **Before distributing Kusema, set up our own update channel**: generate a signing key with `bun tauri signer generate`, put the public key and a Kusema `latest.json` endpoint in `tauri.conf.json`, turn `createUpdaterArtifacts` back on, and set `KUSEMA_UPDATE_CHANNEL_CONFIGURED` to true in `settings.rs`.
- **AI provider requests identify as Kusema.** The User-Agent, Referer and X-Title headers in `llm_client.rs` now say Kusema.

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

## Step 6 — Computer control (voice commands for Windows)

**Goal:** "Kusema, switch to dark mode" changes the setting and replies "Done." The AI only _chooses_ from a fixed whitelist of actions Kusema implements locally; it never runs arbitrary commands.

> Read docs/kusema-build-plan.md and CLAUDE.md first. We're on **Step 6: computer control via tool use**. Assistant mode (Steps 2–3) works; extend it so the LLM can call a fixed set of local actions.
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
> Add unit tests for the registry, argument validation (path and URL allowlists), and the tool-call parsing for both formats. Run `cargo check`, `cargo test`, `bun run tauri dev`. Stop any processes you started. Mark Step 6 done in the plan and commit as "Add computer control (tool use)".

**Test:** turn on "Allow computer control", then try "switch to dark mode", "set the volume to 30 percent", "open Bluetooth settings", "undo that", and "shut down" (it should ask first; say no).

---

## Step 7 — Optional wake word ("Hey Kusema")

**Goal:** hands-free activation as an alternative to hotkeys. Off by default; detection runs entirely on-device.

> Read docs/kusema-build-plan.md and CLAUDE.md first. We're on **Step 7: optional wake word**.
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
> Add tests where practical (detector wrapper, pause/resume state machine). Run `cargo check`, `cargo test`, `bun run tauri dev`. Stop any processes you started. Mark Step 7 done in the plan and commit as "Add optional wake word".

**Test:** enable the wake word, say "Hey Kusema, what's the weather like in Kingston usually in March?" with no hotkey. Then check it doesn't trigger while Kusema is speaking, and that "Pause wake word" stops it.

---

## Later (v2 ideas)

- **"Sensitive mode" toggle**: one switch that routes Assistant mode to a local Ollama model instead of a cloud provider.
- **Redaction**: optional local step stripping names, emails and phone numbers before text goes to a cloud provider.
- **Chatterbox voice cloning** with a consent step (live recording of a scripted consent line, no uploaded files).
- **Separate provider settings for Assistant vs. post-processing**, if you want, e.g., Groq for fast cleanup and Claude for questions.
- **Code signing** for the Windows installer before giving it to clients.
- **Interrupt by voice:** say "Hey Kusema" while it's talking to cut it off. Needs echo cancellation, so do this after Step 7.
- **Paste the answer:** an option to type the assistant's reply into the active app instead of speaking it ("draft a reply declining this meeting").
- **Remaining Handy links:** replace handy.computer links (Linux setup guide, macOS Secure Input help, About-page donate link) with Kusema pages or remove them.
- **More Kokoro languages:** Kokoro also has Spanish, French, Hindi, Italian, Japanese, Portuguese and Chinese voices; each needs its own phonemizer (misaki has some), without espeak-ng.
- **Line endings:** ✅ Done (2026-10-27). `.gitattributes` sets `* text=auto eol=lf`, so checkouts are LF even with `core.autocrlf=true`, and `bun run format:check` passes on Windows.

## Keeping up with Handy

```
git fetch upstream
git merge upstream/main
```

If there are conflicts, ask Claude Code: "Merge upstream/main from cjpais/Handy, resolve conflicts preserving Kusema's Assistant, TTS, keychain and branding changes, then run cargo check and the tests."
