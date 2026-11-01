/** What the "First three wins" card shows, from the backend's ticks. */
import type { FirstWins } from "@/bindings";

export type WinId = "dictated" | "newParagraph" | "asked";

export interface WinRow {
  id: WinId;
  done: boolean;
  /** The shortcut that does it. */
  binding: string;
}

export const winRows = (wins: FirstWins): WinRow[] => [
  { id: "dictated", done: wins.dictated, binding: "transcribe" },
  {
    id: "newParagraph",
    done: wins.new_paragraph,
    binding: "dictate_continuous",
  },
  { id: "asked", done: wins.asked, binding: "assistant" },
];

export const showWinsCard = (wins: FirstWins | undefined): boolean =>
  !!wins && wins.active && !wins.dismissed;

export const winsDone = (wins: FirstWins): number =>
  winRows(wins).filter((row) => row.done).length;
