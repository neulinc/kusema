## Before Submitting This PR

<!--
Kusema is built on Handy (https://github.com/cjpais/Handy). Fixes to the shared dictation engine, audio pipeline or speech models are usually better sent to Handy first; Kusema picks them up when it merges from upstream.

AI-assisted code is welcome, but a person must open the PR and write the Human Written Description below. PRs opened automatically, or without a human-written description, will be closed.
-->

**Please submit only one fix or feature per pull request. Pull requests containing multiple fixes or features will likely be closed.**

**Please confirm you have done the following:**

- [ ] I have searched [existing issues](https://github.com/neulinc/kusema/issues) and [pull requests](https://github.com/neulinc/kusema/pulls) (including closed ones) to ensure this isn't a duplicate
- [ ] I have read [CONTRIBUTING.md](https://github.com/neulinc/kusema/blob/main/CONTRIBUTING.md)
- [ ] New user-facing text uses translations (English in `src/i18n/locales/en/translation.json`, copied to the other locales), and `bun run lint` and `bun run format:check` pass

**If this is a feature or change that was previously closed/rejected:**

- [ ] I have explained in the description below why this should be reconsidered
- [ ] I have discussed it in an issue first (link below)

## Human Written Description

<!-- Describe your changes clearly and concisely

Please write 2-3 sentences in your own words explaining:
- What problem you noticed or idea you had
- Why you think this change matters

This section should be YOUR thinking, not AI-generated text. Even if AI helped write the code, we want to hear from you directly. Your PR may be rejected if you do not include a human-written description.
-->

## Related Issues

<!-- Link to related issues or previous PRs -->
<!-- If reopening something previously closed, explain why this should be reconsidered -->

Fixes #

## Community Feedback

<!--
PRs that were discussed first are much easier to review and merge.

For features: link to the issue where the idea was discussed.
For bug fixes: link to the issue where the bug was reported or confirmed.

If there's no issue yet, consider opening one first:
https://github.com/neulinc/kusema/issues
-->

## Which Part of Kusema?

- [ ] Dictation (Ctrl+Space) or AI cleanup / Post Processing (Ctrl+Shift+Space)
- [ ] Ask Kusema (Ctrl+Alt+Space)
- [ ] Ask about my screen (Ctrl+Alt+Shift+Space)
- [ ] Spoken replies / voices
- [ ] AI providers, API keys or privacy
- [ ] Installer, updates or release workflow
- [ ] Settings or other UI
- [ ] Docs or build only

## Testing

<!-- Describe how you tested your changes and if you need help getting additional testing -->

**Kusema version / commit tested:**

**Windows version:**

<!-- e.g. Windows 11 Home 24H2 (OS build 26100.2033) -->

**Hotkeys and features exercised:**

<!-- e.g. "Ask about my screen on a Word document with Anthropic / claude-haiku-4-5: Send, Cancel and a follow-up" -->

**Checks run:**

- [ ] `bun run lint`
- [ ] `bun run format:check`
- [ ] `cargo test` (in `src-tauri`)
- [ ] `bun run tauri dev`, and tried the change in the app

## Screenshots/Videos (if applicable)

<!-- Add screenshots or videos demonstrating the change. Check them for anything private first. -->

## AI Assistance

<!-- AI-assisted PRs are welcome! Just let us know so we can review appropriately. -->

- [ ] No AI was used in this PR
- [ ] AI was used (please describe below)

**If AI was used:**

- Tools used:
- How extensively:
