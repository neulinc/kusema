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

/** A classic shortcut's control is hidden while they're off. The Ask box's
 * Ctrl+Alt+K is classic too (T while holding the key opens it), unless it was
 * changed (`askBoxCustomized`): then it stays the user's own. */
export const hiddenShortcut = (
  id: string,
  classicOn: boolean,
  askBoxCustomized = false,
): boolean =>
  !classicOn &&
  (CLASSIC_SHORTCUTS.includes(id) || (id === "ask_box" && !askBoxCustomized));

/** The Ask box's key was changed from its default. */
export const askBoxCustomized = (
  binding: { current_binding: string; default_binding: string } | undefined,
): boolean =>
  !!binding &&
  binding.current_binding.trim().toLowerCase() !==
    binding.default_binding.toLowerCase();

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
