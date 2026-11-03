/** What the "First three wins" card shows, from the backend's ticks. */
import type { FirstWins } from "@/bindings";

export type WinId = "dictated" | "newParagraph" | "asked";

/** How it's done with the Kusema key: hold it, double-tap it, or say the trigger word. */
export type WinHow = "hold" | "doubleTap" | "say";

export interface WinRow {
  id: WinId;
  done: boolean;
  how: WinHow;
}

export const winRows = (wins: FirstWins): WinRow[] => [
  { id: "dictated", done: wins.dictated, how: "hold" },
  { id: "newParagraph", done: wins.new_paragraph, how: "doubleTap" },
  { id: "asked", done: wins.asked, how: "say" },
];

export const showWinsCard = (wins: FirstWins | undefined): boolean =>
  !!wins && wins.active && !wins.dismissed;

export const winsDone = (wins: FirstWins): number =>
  winRows(wins).filter((row) => row.done).length;
