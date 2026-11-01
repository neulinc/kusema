/**
 * Catalog models Kusema doesn't offer, because their licenses don't fit a paid
 * product. The catalog stays an exact copy of Handy's, so they're filtered here
 * and in `REMOVED_MODELS` in src-tauri/src/catalog/mod.rs. A test there keeps
 * the two lists identical.
 */
export const REMOVED_MODEL_IDS: readonly string[] = [
  "canary-1b",
  "granite-speech-5.0-470m-turboctc-nc",
  "medasr",
  "moonshine-base-ar",
  "moonshine-base-ja",
  "moonshine-base-ko",
  "moonshine-base-uk",
  "moonshine-base-vi",
  "moonshine-base-zh",
  "moonshine-tiny-ar",
  "moonshine-tiny-ja",
  "moonshine-tiny-ko",
  "moonshine-tiny-uk",
  "moonshine-tiny-vi",
  "moonshine-tiny-zh",
  "SenseVoiceSmall",
  "Fun-ASR-MLT-Nano-2512",
  "Fun-ASR-Nano-2512",
];

export const isOfferedByKusema = (model: { id: string }): boolean =>
  !REMOVED_MODEL_IDS.includes(model.id);
