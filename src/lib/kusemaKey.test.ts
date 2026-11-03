// Standalone assert check. Run with: bun src/lib/kusemaKey.test.ts
import assert from "node:assert/strict";
import { hiddenShortcut, keyLabel, triggerWord } from "./kusemaKey";

const t = (key: string) => `[${key}]`;

// Presets are translated; a key the user pressed keeps Windows' name.
assert.equal(
  keyLabel({ kind: "right_ctrl" }, t),
  "[kusemaKey.presets.right_ctrl]",
);
assert.equal(keyLabel({ kind: "copilot" }, t), "[kusemaKey.presets.copilot]");
assert.equal(keyLabel({ kind: "custom", vk: 19, name: "Pause" }, t), "Pause");
assert.equal(keyLabel(undefined, t), "[kusemaKey.presets.right_ctrl]");

// Classic shortcuts hide while off; the Ask box and Cancel never do.
assert.equal(hiddenShortcut("transcribe", false), true);
assert.equal(hiddenShortcut("dictate_continuous", false), true);
assert.equal(hiddenShortcut("transcribe", true), false);
assert.equal(hiddenShortcut("ask_box", false), false);
assert.equal(hiddenShortcut("cancel", false), false);

assert.equal(triggerWord(" Computer "), "Computer");
assert.equal(triggerWord(""), "Kusema");

console.log("kusemaKey: ok");
