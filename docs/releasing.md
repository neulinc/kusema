# Releasing Kusema

Kusema updates itself from GitHub Releases. Installed copies check
`https://github.com/neulinc/kusema/releases/latest/download/latest.json`; pushing
a tag like `v0.2.0` runs [`.github/workflows/release.yml`](../.github/workflows/release.yml),
which builds the Windows x64 installers, signs the update files and publishes
the release. Windows only for now.

## Two different kinds of signing

- **Updater signing (we have this).** A minisign key pair made by
  `bun tauri signer generate`. The public key is in `src-tauri/tauri.conf.json`;
  the app refuses any update not signed by the matching private key. This
  protects users from fake updates. It does nothing for Windows' own warnings.
- **Windows code signing / Authenticode (we don't have this yet).** A
  certificate from a certificate authority (or Azure Trusted Signing) that
  signs the `.exe`/`.msi` themselves. Without it, Windows SmartScreen shows
  "Windows protected your PC" on first install (testers click **More info →
  Run anyway**). Needed before selling; see "Later" in the build plan.

## One-time setup

### 1. Generate the updater key

In PowerShell, outside the repo:

```powershell
bun tauri signer generate -w "$env:USERPROFILE\.tauri\kusema-updater.key"
```

Choose a strong password when asked. This writes
`kusema-updater.key` (private, password-protected) and `kusema-updater.key.pub`
(public).

- **Never commit the private key or the password.** `.tauri` is outside the
  repo, so `git` can't pick it up.
- Save the private key file's contents **and** the password in your password
  manager. If either is lost, existing installs can never be updated again;
  everyone would have to reinstall a build with a new key by hand.
- Put the contents of `kusema-updater.key.pub` in `plugins.updater.pubkey` in
  `src-tauri/tauri.conf.json`. The public key is safe to commit.

### 2. Add the GitHub Actions secrets

On GitHub: **neulinc/kusema → Settings → Secrets and variables → Actions →
New repository secret**:

| Name                                 | Value                                     |
| ------------------------------------ | ----------------------------------------- |
| `TAURI_SIGNING_PRIVATE_KEY`          | the full contents of `kusema-updater.key` |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | the password                              |

### 3. Turn on GitHub Actions for the repo

GitHub disables workflows on forks until you enable them: **Actions tab → "I
understand my workflows, go ahead and enable them"**. Then in **Settings →
Actions → General → Workflow permissions**, choose **Read and write
permissions** so the workflow can create releases.

The repo must stay **public**: the app downloads `latest.json` and the
installers without logging in. If it ever goes private, move releases to a
public host (a releases-only public repo, or Cloudflare R2) and change the
endpoint in `tauri.conf.json`.

## Every release

1. **Pick the version** (e.g. `0.2.1`) and set it in all three files:
   - `src-tauri/tauri.conf.json` → `"version"`
   - `src-tauri/Cargo.toml` → `version` under `[package]` (then run
     `cargo check` in `src-tauri` so `Cargo.lock` updates)
   - `package.json` → `"version"`
2. **Write release notes** in `src/content/release-notes/<version>.md`: a few
   short, user-facing lines. Users see them once in the app after updating
   ("What's new"), and the workflow uses the same file as the GitHub Release
   text. The workflow fails if the file is missing.
3. **Check:** `bun run lint`, `bun run format:check`, `bun run
check:translations`, and `cargo test` in `src-tauri`.
4. **Commit and tag**, then push both:

   ```powershell
   git commit -am "Release 0.2.1"
   git tag v0.2.1
   git push origin main v0.2.1
   ```

   The tag must be `v` + the version in `tauri.conf.json`, or the workflow
   stops before building.

5. **Watch the run** in the Actions tab (~30–40 min). It creates a draft
   release, builds and uploads, and publishes the release only once
   `latest.json`, both installers and their `.sig` files are all there. If it
   fails, the draft stays unpublished and users see nothing; delete the draft
   and the tag (`git push --delete origin v0.2.1`), fix, and tag again.
6. **Verify the release** on GitHub: it's marked **Latest**, and has
   `Kusema_<version>_x64-setup.exe`, `Kusema_<version>_x64_en-US.msi`, their
   `.sig` files and `latest.json`. Open
   <https://github.com/neulinc/kusema/releases/latest/download/latest.json> and
   check it shows the new version.
7. **Test the update** on a machine (or VM) with the previous version: the
   footer shows "Update available" (or use **Check for Updates…** in the tray);
   install it; Kusema restarts on the new version with settings, models,
   Kokoro voices and the API key still there, and shows the release notes.

## Testers on 0.1.0

0.1.0 shipped with the updater turned off, so testers on it must install 0.2.0
(or later) **once** by hand from the Releases page. Every version after that
updates itself.

## Building locally

`bun run tauri build` makes updater files too, so it needs the private key.
`TAURI_SIGNING_PRIVATE_KEY` takes either the key's contents or its path
(`TAURI_SIGNING_PRIVATE_KEY_PATH` is ignored by this Tauri CLI):

```powershell
$env:TAURI_SIGNING_PRIVATE_KEY = "$env:USERPROFILE\.tauri\kusema-updater.key"
$p = Read-Host "Updater key password" -AsSecureString
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = [Runtime.InteropServices.Marshal]::PtrToStringAuto([Runtime.InteropServices.Marshal]::SecureStringToBSTR($p))
bun run tauri build
```

The installers and `.sig` files land in `src-tauri/target/release/bundle/nsis`
and `…/msi`. Close the PowerShell window afterwards so the password doesn't stay
in the session.

For a quick local build without the key, skip signing:

```powershell
bun run tauri build --no-sign
```

This makes the installers without `.sig` files; the app still checks for
updates normally.

## Notes

- Portable installs (`/PORTABLE`) still detect updates but can't install them
  in place; they open the matching installer from the release instead.
- Setting `HANDY_DISABLE_UPDATER=1` turns update checks off entirely (inherited
  from Handy).
- Handy's other build workflows (`main-build.yml`, `build-test.yml`,
  `pr-test-build.yml`) are manual only and build every platform; their macOS
  legs need Apple secrets we don't have. `release.yml` is ours: keep it when
  merging from upstream.
