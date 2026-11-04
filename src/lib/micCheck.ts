/** Kusema microphone check: its timing, and what each finding says. */

import type { MicTip } from "@/bindings";

/** The quiet part, seconds: the room's own noise. */
export const QUIET_SECONDS = 3;
/** The sentence ends by itself after this long, seconds. */
export const READ_SECONDS = 10;

/** The translation key of each finding, most important first. */
export const tipKeys = (tips: MicTip[]): string[] =>
  tips.map((tip) => `micCheck.tips.${tip}`);
