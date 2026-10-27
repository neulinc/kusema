# Contributing to Kusema

Thank you for your interest in contributing to Kusema! This guide will help you get started with contributing to this open source dictation app and spoken AI assistant.

## 🧭 Kusema and Handy

Kusema is built on [Handy](https://github.com/cjpais/Handy), which provides its dictation engine, audio pipeline and much of its interface. Kusema adds the assistant, spoken replies and its own settings on top.

- **Problems with Kusema**, including anything about Ask Kusema, voices, AI providers or installing and updating Kusema: report them in [Kusema's issues](https://github.com/neulinc/kusema/issues).
- **Problems that also happen in Handy** (dictation itself, speech models, microphones): those are best fixed upstream, in [Handy's issues](https://github.com/cjpais/Handy/issues). Kusema picks up Handy's fixes when it merges from upstream.

## 📖 Philosophy

Kusema keeps Handy's priorities:

- **Simplicity**: Clear, maintainable code over clever solutions
- **Extensibility**: Make it easy for others to fork and customize
- **Privacy**: Your voice stays on your computer, and nothing else leaves it unless you choose to send it
- **Accessibility**: Free tooling that belongs in everyone's hands

## 🚀 Getting Started

### Prerequisites

Before you begin, ensure you have the following installed:

- [Rust](https://rustup.rs/) (latest stable)
- [Bun](https://bun.sh/) package manager
- Platform-specific build tools (see [BUILD.md](BUILD.md))

### Setting Up Your Development Environment

1. **Fork the repository** on GitHub

2. **Clone your fork**:

   ```bash
   git clone git@github.com:YOUR_USERNAME/kusema.git
   cd kusema
   ```

3. **Add upstream remote**:

   ```bash
   git remote add upstream git@github.com:neulinc/kusema.git
   ```

4. **Install dependencies**:

   ```bash
   bun install
   ```

   The voice-activity model (`src-tauri/resources/models/silero-vad-v6.2-F32.gguf`) is already in the repository, so there's nothing else to download.

5. **Run in development mode**:
   ```bash
   bun run tauri dev
   # On macOS if you encounter cmake errors:
   CMAKE_POLICY_VERSION_MINIMUM=3.5 bun run tauri dev
   ```

For detailed platform-specific setup instructions, see [BUILD.md](BUILD.md).

### Understanding the Codebase

Kusema follows Handy's architecture:

**Backend (Rust - `src-tauri/src/`):**

- `lib.rs` - Main application entry point with Tauri setup
- `managers/` - Core business logic (audio, model, transcription)
- `audio_toolkit/` - Low-level audio processing (recording, VAD)
- `commands/` - Tauri command handlers for frontend communication
- `shortcut/` - Global keyboard shortcut handling
- `settings.rs` - Application settings management
- `assistant/` - Ask Kusema: the conversation and AI provider requests (Kusema)
- `tts/` - Spoken replies: Kokoro and system voices (Kusema)

**Frontend (React/TypeScript - `src/`):**

- `App.tsx` - Main application component
- `components/` - React UI components
- `hooks/` - Reusable React hooks
- `lib/types.ts` - Shared TypeScript types

For more details, see the Architecture Overview in [AGENTS.md](AGENTS.md).

## 🐛 Reporting Bugs

### Before Submitting a Bug Report

1. **Search existing issues** at [github.com/neulinc/kusema/issues](https://github.com/neulinc/kusema/issues)
2. **Try the latest release** to see if the issue has been fixed
3. **Enable debug mode** (`Cmd/Ctrl+Shift+D`) to gather diagnostic information

### Submitting a Bug Report

When creating a bug report, please include:

**System Information:**

- App version (found in settings or about section)
- Operating System (e.g., macOS 14.1, Windows 11, Ubuntu 22.04)
- CPU (e.g., Apple M2, Intel i7-12700K, AMD Ryzen 7 5800X)
- GPU (e.g., Apple M2 GPU, NVIDIA RTX 4080, Intel UHD Graphics)

**Bug Details:**

- Clear description of the bug
- Steps to reproduce
- Expected behavior
- Actual behavior
- Screenshots or logs if applicable
- Information from debug mode if relevant

Use the [Bug Report template](.github/ISSUE_TEMPLATE/bug_report.md) when creating an issue.

## 💡 Suggesting Features

### Before Suggesting a Feature

1. **Search existing issues** at [github.com/neulinc/kusema/issues](https://github.com/neulinc/kusema/issues), open and closed
2. **Check the build plan** in [docs/kusema-build-plan.md](docs/kusema-build-plan.md): it may already be planned

### Submitting a Feature Request

1. Open a [new issue](https://github.com/neulinc/kusema/issues/new)
2. Describe your feature idea including:
   - The problem you're trying to solve
   - Your proposed solution
   - Any alternatives you've considered
   - How it fits with Kusema's philosophy

## 🔧 Making Code Contributions

### Before You Start

**This is critical:** Before writing any code, please do the following:

1. **Search existing issues and PRs** - Check both open AND closed issues and pull requests. Someone may have already addressed this, or there may be a reason it was closed.
   - [Open issues](https://github.com/neulinc/kusema/issues)
   - [Closed issues](https://github.com/neulinc/kusema/issues?q=is%3Aissue+is%3Aclosed)
   - [Open PRs](https://github.com/neulinc/kusema/pulls)
   - [Closed PRs](https://github.com/neulinc/kusema/pulls?q=is%3Apr+is%3Aclosed)

2. **If something was previously closed** - If you want to revisit a closed issue or PR, you need to:
   - Provide a strong argument for why it should be reconsidered
   - Discuss it in an issue first
   - Link to that issue in your PR

3. **Discuss features first** - Open an issue describing the feature before writing it, and link to it in your PR. This helps keep Kusema focused and useful without becoming bloated.

### Development Workflow

1. **Create a feature branch**:

   ```bash
   git checkout -b feature/your-feature-name
   # or
   git checkout -b fix/your-bug-fix
   ```

2. **Make your changes**:
   - Write clean, maintainable code
   - Follow existing code style and patterns
   - Add comments for complex logic
   - Keep commits focused and atomic

3. **Test thoroughly**:
   - Test on your target platform(s)
   - Verify existing functionality still works
   - Test edge cases and error conditions
   - Use debug mode to verify audio/transcription behavior

4. **Commit your changes**:

   ```bash
   git add .
   git commit -m "feat: add your feature description"
   # or
   git commit -m "fix: describe the bug fix"
   ```

   Use conventional commit messages:
   - `feat:` for new features
   - `fix:` for bug fixes
   - `docs:` for documentation changes
   - `refactor:` for code refactoring
   - `test:` for test additions/changes
   - `chore:` for maintenance tasks

5. **Keep your fork updated**:

   ```bash
   git fetch upstream
   git rebase upstream/main
   ```

6. **Push to your fork**:

   ```bash
   git push origin feature/your-feature-name
   ```

7. **Create a Pull Request**:
   - Go to the [Kusema repository](https://github.com/neulinc/kusema)
   - Click "New Pull Request"
   - Select your fork and branch
   - Fill out the PR template completely, including:
     - Clear description of changes
     - Links to related issues
     - **Community feedback** (especially important for features)
     - How you tested the changes
     - Screenshots/videos if applicable
     - Breaking changes (if any)

   **Remember:** PRs linked to an [issue](https://github.com/neulinc/kusema/issues) where the change was discussed are much easier to review and merge.

### AI Assistance Disclosure

**AI-assisted PRs are welcome!** Use whatever tools help you contribute, just be upfront about it.

In your PR description, please include:

- Whether AI was used (yes/no)
- Which tools were used (e.g., "Claude Code", "GitHub Copilot", "ChatGPT")
- How extensively it was used (e.g., "generated boilerplate", "helped debug", "wrote most of the code")

### Code Style Guidelines

**Rust:**

- Follow standard Rust formatting (`cargo fmt`)
- Run `cargo clippy` and address warnings
- Use descriptive variable and function names
- Add doc comments for public APIs
- Handle errors explicitly (avoid unwrap in production code)

**TypeScript/React:**

- Use TypeScript strictly, avoid `any` types
- Follow React hooks best practices
- Use functional components
- Keep components small and focused
- Use Tailwind CSS for styling

**General:**

- Write self-documenting code
- Add comments for non-obvious logic
- Keep functions small and single-purpose
- Prioritize readability over cleverness

### Testing Your Changes

**Manual Testing:**

- Run the app in development mode: `bun run tauri dev`
- Test your changes with debug mode enabled
- Verify on multiple platforms if possible
- Test with different audio devices
- Try various transcription scenarios

**Building for Production:**

```bash
bun run tauri build
```

Test the production build to ensure it works as expected.

## 📝 Documentation Contributions

Documentation improvements are highly valued! You can contribute by:

- Improving README.md, BUILD.md, or this CONTRIBUTING.md
- Adding code comments and doc comments
- Creating tutorials or guides
- Improving error messages
- Updating the project website content

## 🤝 Community Guidelines

- **Be respectful and inclusive** - We welcome contributors of all skill levels
- **Be patient** - This is maintained by a small team, responses may take time
- **Be constructive** - Focus on solutions and improvements
- **Be collaborative** - Help others and share knowledge
- **Search first** - Check existing issues before creating new ones

## 🎯 Good First Issues

Look for issues labeled `good first issue` or `help wanted` if you're new to the project. These are typically:

- Well-defined and scoped
- Good for learning the codebase
- Mentor support available

## 📞 Getting Help

- **Issues**: Ask questions or report problems in [Kusema's issues](https://github.com/neulinc/kusema/issues)

## 📜 License

By contributing to Kusema, you agree that your contributions will be licensed under the MIT License. See [LICENSE](LICENSE) for details.

---

**Thank you for contributing to Kusema!** Your efforts help make voice and AI tools more accessible, private, and extensible for everyone.
