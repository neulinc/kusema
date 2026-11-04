// Standalone assert check. Run with: bun src/overlay/pillLayout.test.ts
import assert from "node:assert/strict";
import {
  LABEL_GAP,
  MAX_LABEL_HEIGHT,
  labelRoom,
  noticeRoom,
  pillLabels,
  previewShown,
} from "./pillLayout";

const t = (key: string, options?: Record<string, string>) =>
  options?.name ? `${key}(${options.name})` : key;
const none = {
  assistantTake: false,
  editing: false,
  styleName: null,
  privileged: false,
};

// A plain take has no label row at all.
assert.deepEqual(pillLabels(none, t), []);
assert.equal(labelRoom(0, false), 0);

// Each label in its own chip, the most important first.
assert.deepEqual(
  pillLabels(
    { ...none, assistantTake: true, styleName: "Email", privileged: true },
    t,
  ).map((l) => l.kind),
  ["assistant", "style", "privileged"],
);
assert.equal(
  pillLabels({ ...none, styleName: "Email" }, t)[0].text,
  "overlay.withStyle(Email)",
);
// Editing says so, rather than Ask Kusema.
assert.deepEqual(
  pillLabels({ ...none, editing: true, assistantTake: true }, t).map(
    (l) => l.kind,
  ),
  ["editing"],
);

// The window grows by the row and its gap, within bounds; closed, not at all.
assert.equal(labelRoom(18.2, true), 19 + LABEL_GAP);
assert.equal(labelRoom(18.2, false), 0);
assert.equal(labelRoom(900, true), MAX_LABEL_HEIGHT);

// A message grows the window only past its own room, and never too far.
assert.equal(noticeRoom(60), 0);
assert.equal(noticeRoom(130.5), 31);
assert.equal(noticeRoom(1000), MAX_LABEL_HEIGHT);

// The live preview shows words while listening, and while the take turns
// into text; never with nothing said, on a message or the AI's rewrite.
const said = { committed: "Please send ", tentative: "the contract" };
const nothing = { committed: "", tentative: " " };
for (const state of ["streaming", "recording", "dictating", "transcribing"]) {
  assert.equal(previewShown(state, said), true, state);
  assert.equal(previewShown(state, nothing), false, state);
}
for (const state of ["notice", "processing", "speaking", "editing"]) {
  assert.equal(previewShown(state, said), false, state);
}
assert.equal(
  previewShown("streaming", { committed: "", tentative: "So" }),
  true,
);

console.log("pillLayout: ok");
