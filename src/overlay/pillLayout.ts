/**
 * The pill's label row: what goes in it, and how much the window grows for
 * it. Labels ("Ask Kusema", "Editing", a style's name, Privileged) sit in
 * their own row above the animation, never on it; the row opens upward (or
 * downward for a pill at the top of the screen) and the animation keeps its
 * size and place.
 */

export type LabelKind = "assistant" | "editing" | "style" | "privileged";

export interface PillLabel {
  kind: LabelKind;
  text: string;
}

export interface LabelState {
  /** Space was tapped while holding: the take is for the Assistant. */
  assistantTake: boolean;
  editing: boolean;
  /** The app style shaping the text, if any. */
  styleName: string | null;
  privileged: boolean;
}

/** Space between the label row and the row below it, px. */
export const LABEL_GAP = 6;
/** The tallest the label row may make the pill grow, px (backend agrees). */
export const MAX_LABEL_HEIGHT = 120;
/** A message's card fits in this height before the window must grow, px:
 * the notice window's own room (2 × OVERLAY_HEIGHT in overlay.rs). */
export const NOTICE_CARD_ROOM = 100;
/** How long the row takes to open or close, ms (RecordingOverlay.css). */
export const LABEL_MS = 260;

/** The labels to show, most important first. */
export const pillLabels = (
  state: LabelState,
  t: (key: string, options?: Record<string, string>) => string,
): PillLabel[] => {
  const labels: PillLabel[] = [];
  if (state.editing)
    labels.push({ kind: "editing", text: t("overlay.editing") });
  else if (state.assistantTake)
    labels.push({ kind: "assistant", text: t("overlay.assistantTake") });
  if (state.styleName)
    labels.push({
      kind: "style",
      text: t("overlay.withStyle", { name: state.styleName }),
    });
  if (state.privileged)
    labels.push({ kind: "privileged", text: t("overlay.privileged") });
  return labels;
};

/** How much taller the window must be for a label row `height` px tall. */
export const labelRoom = (height: number, open: boolean): number =>
  open && height > 0
    ? Math.min(Math.ceil(height) + LABEL_GAP, MAX_LABEL_HEIGHT)
    : 0;

/** How much taller the window must be for a message `height` px tall. */
export const noticeRoom = (height: number): number =>
  Math.min(Math.max(0, Math.ceil(height) - NOTICE_CARD_ROOM), MAX_LABEL_HEIGHT);
