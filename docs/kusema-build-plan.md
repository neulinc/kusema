# Kusema — Build Plan & Claude Code Prompts

*Fork of cjpais/Handy (MIT). Mapped against Handy `main` on 2026-10-27.*

## What Handy already gives us

| Need | Already in Handy | Where |
|---|---|---|
| Hotkey → record → local transcription | Yes | `actions.rs` → `TranscribeAction` |
| Send text to Claude / OpenAI / Groq / OpenRouter | Yes (used for "post-processing") | `llm_client.rs` |
| Ollama / any local model | Yes, via the "Custom" provider (base URL `http://localhost:11434/v1`) | `settings.rs` `default_post_process_providers()` |
| Provider, model and API key settings UI | Yes | `src/components/settings/post-processing/` |
| Audio playback library | Yes (`rodio`) | `Cargo.toml` |
| Repo instructions for AI agents | Yes | `CLAUDE.md`, `AGENTS.md` (Claude Code reads these automatically) |

## What's missing

1. **An "Assistant" mode.** Today every transcription ends in a paste (`utils::paste(...)` inside `TranscribeAction::stop`, around line 826 of `actions.rs`). We add a third hotkey that sends the text to the LLM as a *question* and keeps the answer.
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

## Step 1 — Rebrand to Kusema

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

---

## Step 2 — Assistant mode (text replies first)

> Read CLAUDE.md first. We're adding an **Assistant mode** to Kusema: the user holds a hotkey, speaks a question, it's transcribed locally (as now), the text is sent to the LLM provider the user configured, and the reply is kept, without pasting anything.
>
> Context from the codebase:
> - `src-tauri/src/actions.rs`: `TranscribeAction { post_process: bool }` handles the hotkey. In `stop()`, after transcription and optional post-processing, it spawns a "paste" thread that calls `utils::paste(final_text, ...)`. Actions are registered in `ACTION_MAP`.
> - `src-tauri/src/settings.rs`: default hotkeys are created in the default settings (`transcribe`, `transcribe_with_post_process`, `cancel`). Providers come from `default_post_process_providers()`; the active one from `settings.active_post_process_provider()`; keys from `post_process_api_keys`; model from `post_process_models`.
> - `src-tauri/src/llm_client.rs`: already speaks both the Anthropic Messages format and the OpenAI chat format, but only single-turn.
>
> Do this:
> 1. Create a new module `src-tauri/src/assistant/` with `mod.rs` and `conversation.rs`. Keep a `Conversation` holding an ordered list of user/assistant messages plus a system prompt (default: "You are Kusema, a helpful voice assistant. Answers will be spoken aloud, so reply in short, natural spoken sentences. No markdown, lists, code blocks or emoji unless asked."). Store it in Tauri managed state.
> 2. In `llm_client.rs`, add a multi-turn function (e.g. `send_conversation`) that accepts a system prompt plus a message list and works for both the Anthropic format (system goes in the top-level `system` field) and OpenAI format (system as the first message). Reuse the existing headers/client helpers. Don't change the behavior of the existing post-processing functions.
> 3. Add a new hotkey binding `assistant` (Windows default `ctrl+alt+space`; pick sensible macOS/Linux defaults) named "Ask Kusema", and register it in `ACTION_MAP`. Implement it with the smallest possible change to `TranscribeAction`: e.g. replace the `post_process: bool` with a small enum mode (`Dictate`, `DictateWithPostProcess`, `Assistant`), or add an `assistant: bool`. In Assistant mode, skip the paste thread entirely and instead hand the final text to the assistant module.
> 4. The assistant module appends the user message, calls the LLM with the full conversation using the same provider/model/key the user configured for post-processing, appends the reply, and emits a Tauri event `assistant-reply` with `{ question, answer }`. On error emit `assistant-error` with a human-readable message (e.g. "No API key set for Anthropic").
> 5. Add a minimal "Assistant" page to the settings sidebar in the React frontend showing the conversation (question/answer bubbles, newest at bottom), with a "New conversation" button that clears the history via a Tauri command.
> 6. Make sure the tray icon and overlay return to idle after the reply, and that the existing Cancel hotkey cancels an in-flight assistant request.
>
> Don't add text-to-speech yet; that's the next step. Run `cargo check`, `cargo test` and `bun run tauri dev`. Commit as "Add Assistant mode (text replies)".

**Test:** In settings, choose Anthropic as the post-processing provider, paste your Claude API key, pick a model. Press Ctrl+Alt+Space, ask "What's the capital of Ghana?", and check the reply appears on the Assistant page. Then ask a follow-up ("What's its population?") to confirm memory works.

---

## Step 3 — Speak the replies (Windows built-in voices)

> Read CLAUDE.md first. Kusema's Assistant mode now emits `assistant-reply`. Add text-to-speech so the reply is spoken aloud.
>
> 1. Create `src-tauri/src/tts/` with a `TtsEngine` trait (`list_voices()`, `speak(text, voice_id, rate)`, `stop()`), and a first implementation `system.rs` using the OS's built-in voices. Use the `tts` crate (Windows SAPI/WinRT, macOS AVSpeech, Linux speech-dispatcher) unless you find a better-maintained option. Justify your choice briefly.
> 2. Speak replies **sentence by sentence as soon as each sentence is available**, so long answers start speaking quickly. (If the LLM call isn't streaming yet, add streaming for both Anthropic and OpenAI formats using reqwest's stream feature, which is already enabled in Cargo.toml.)
> 3. Pressing the Cancel hotkey (Escape) or starting a new recording must stop speech immediately. The user must always be able to interrupt.
> 4. Add settings: "Speak replies" on/off, voice dropdown populated from `list_voices()`, speaking rate slider, and a "Preview voice" button. Persist them in AppSettings with serde defaults so existing settings files still load.
> 5. Show a "speaking" state in the overlay/tray while talking.
>
> Run `cargo check`, `cargo test`, `bun run tauri dev`. Commit as "Speak assistant replies".

**Milestone:** at this point the full privacy loop works: your voice stays local, only text goes to Claude, and the reply is spoken locally.

---

## Step 4 — Store API keys in Windows Credential Manager

> Read CLAUDE.md first. API keys are stored in the settings file via `SecretMap` (`post_process_api_keys` in `settings.rs`). They're redacted from logs but sit in plain text on disk. Move them to the OS keychain using the `keyring` crate (Windows Credential Manager, macOS Keychain, Linux Secret Service).
>
> - Service name `com.neulinc.kusema`, one entry per provider id.
> - Migrate existing keys on startup: if a key exists in the settings file, move it to the keychain and blank it in the file.
> - If the keychain is unavailable (some Linux setups), fall back to the current behavior and log a warning, never crash.
> - Keep the settings UI working unchanged from the user's point of view.
> - Keep the diff small and localized; this file is heavily touched upstream.
>
> Add tests where practical. Commit as "Store API keys in OS keychain".

---

## Step 5 — Kokoro voices + voice picker

> Read CLAUDE.md first. Add **Kokoro** (Kokoro-82M, Apache 2.0) as a second `TtsEngine` in `src-tauri/src/tts/kokoro.rs`, running locally via ONNX.
>
> 1. Investigate existing Rust options first (e.g. `ort` crate with the Kokoro ONNX model, or an existing Rust Kokoro crate) and pick the most maintainable approach. Explain the trade-off before writing lots of code. Note that Handy already ships ONNX/ML runtimes for Parakeet; reuse whatever is already a dependency if possible rather than adding a second runtime.
> 2. Download the model and voice files on demand, using the same download/progress mechanism Handy uses for speech models (see `managers/model.rs` and `catalog/`), and store them in the app data folder.
> 3. Voice dropdown groups voices by engine ("Kokoro — af_heart", "Windows — Microsoft Zira"), with Preview, and falls back to system voices if Kokoro isn't downloaded.
> 4. Must run on CPU in real time on a mid-range laptop. Measure time-to-first-audio and report it.
>
> Commit as "Add Kokoro voices".

---

## Later (v2 ideas)

- **"Sensitive mode" toggle**: one switch that routes Assistant mode to a local Ollama model instead of a cloud provider.
- **Redaction**: optional local step stripping names, emails and phone numbers before text goes to a cloud provider.
- **Chatterbox voice cloning** with a consent step (live recording of a scripted consent line, no uploaded files).
- **Separate provider settings for Assistant vs. post-processing**, if you want, e.g., Groq for fast cleanup and Claude for questions.
- **Code signing** for the Windows installer before giving it to clients.

## Keeping up with Handy

```
git fetch upstream
git merge upstream/main
```
If there are conflicts, ask Claude Code: "Merge upstream/main from cjpais/Handy, resolve conflicts preserving Kusema's Assistant, TTS, keychain and branding changes, then run cargo check and the tests."
