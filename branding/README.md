# Kusema branding kit

Original Kusema artwork replacing Handy's. See `preview.png`.

## Palette

| Token                          | Light theme           | Dark theme                   |
| ------------------------------ | --------------------- | ---------------------------- |
| Brand primary (`logo-primary`) | `#a9a6ff`             | `#9b97fb`                    |
| Ink / wordmark (`logo-stroke`) | `#24204f`             | (unchanged, uses text color) |
| App icon gradient              | `#3b35b5` → `#6d5ce8` |                              |
| Warm accent (middle voice bar) | `#ffb547`             |                              |
| Coloured tray icons            | `#7c78f2`             |                              |

## Contents

- `kusema-icon-1024.png` / `kusema-icon.svg`: master app icon. Generate all platform icons with `bun tauri icon branding/kusema-icon-1024.png`.
- `tray/`: all 12 tray PNGs (64×64), using Handy's exact filenames so no code changes are needed. Copy over `src-tauri/resources/`.
- `components/HandyTextLogo.tsx`: Kusema wordmark (same component name and props).
- `components/HandyHand.tsx`: Kusema mark for the sidebar and file-drop screen.
- `components/HandyMascot-replacement.tsx`: replacement for `HandyMascot.tsx`.
- `components/tray-icon-paths.txt`: SVG paths for `TrayIcon.tsx`.
- `kusema-wordmark.svg` and PNGs: for your website, videos and docs.

Wordmark set in Inter Display Bold (SIL Open Font License), converted to outlines.
