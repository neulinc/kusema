// Standalone assert check. Run with: bun src/components/settings/general/firstWins.test.ts
import assert from "node:assert/strict";
import type { FirstWins } from "@/bindings";
import { showWinsCard, winRows, winsDone } from "./firstWins";

const wins = (over: Partial<FirstWins> = {}): FirstWins => ({
  active: true,
  dictated: false,
  new_paragraph: false,
  asked: false,
  dismissed: false,
  ...over,
});

// Shown from the end of setup until dismissed, and not for existing users.
assert.equal(showWinsCard(undefined), false);
assert.equal(showWinsCard(wins({ active: false })), false);
assert.equal(showWinsCard(wins()), true);
assert.equal(showWinsCard(wins({ dismissed: true })), false);

// Each tick from the backend shows on its own row.
assert.deepEqual(
  winRows(wins()).map((r) => r.done),
  [false, false, false],
);
assert.deepEqual(
  winRows(wins({ new_paragraph: true })).map((r) => [r.id, r.done]),
  [
    ["dictated", false],
    ["newParagraph", true],
    ["asked", false],
  ],
);
assert.equal(winsDone(wins()), 0);
assert.equal(winsDone(wins({ dictated: true, asked: true })), 2);
assert.equal(
  winsDone(wins({ dictated: true, new_paragraph: true, asked: true })),
  3,
);
// Finishing all three leaves the card up until it's dismissed.
assert.equal(
  showWinsCard(wins({ dictated: true, new_paragraph: true, asked: true })),
  true,
);

console.log("first wins: ok");

// Kusema key: each win says how, with the key or the trigger word.
assert.deepEqual(
  winRows(wins()).map((r) => r.how),
  ["hold", "doubleTap", "say"],
);
