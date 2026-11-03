/** The Kusema key on the frontend: its name, and which shortcuts it replaces. */
import type { KusemaKey } from "@/bindings";

/** The per-action shortcuts the Kusema key replaces (kusema_key::CLASSIC). */
export const CLASSIC_SHORTCUTS = [
  "transcribe",
  "transcribe_with_post_process",
  "assistant",
  "assistant_screen",
  "dictate_continuous",
  "voice_edit",
];

/** The presets offered, in order. */
export const PRESETS: KusemaKey["kind"][] = [
  "right_ctrl",
  "copilot",
  "numpad_plus",
];

/** A classic shortcut's control is hidden while they're off. */
export const hiddenShortcut = (id: string, classicOn: boolean): boolean =>
  !classicOn && CLASSIC_SHORTCUTS.includes(id);

/** The key's label: a translation for the presets, Windows' name otherwise. */
export const keyLabel = (
  key: KusemaKey | undefined,
  t: (key: string) => string,
): string => {
  if (!key) return t("kusemaKey.presets.right_ctrl");
  return key.kind === "custom" ? key.name : t(`kusemaKey.presets.${key.kind}`);
};

/** The trigger word as it's shown ("Kusema" when it's turned off). */
export const triggerWord = (setting: string | undefined): string =>
  (setting ?? "").trim() || "Kusema";
