// Standalone assert check. Run with: bun src/components/settings/assistant/assistantTabs.test.ts
import assert from "node:assert/strict";
import { parseSavedTab, tabForKey } from "./assistantTabs";

// Arrow keys move and wrap round.
assert.equal(tabForKey("conversation", "ArrowRight"), "settings");
assert.equal(tabForKey("settings", "ArrowRight"), "conversation");
assert.equal(tabForKey("conversation", "ArrowLeft"), "settings");
assert.equal(tabForKey("settings", "ArrowLeft"), "conversation");

// Home and End go to the ends.
assert.equal(tabForKey("settings", "Home"), "conversation");
assert.equal(tabForKey("conversation", "End"), "settings");
assert.equal(tabForKey("conversation", "Home"), "conversation");

// Other keys leave it alone, so Tab and typing behave as usual.
for (const key of ["Tab", "Enter", " ", "a", "ArrowUp", "ArrowDown"])
  assert.equal(tabForKey("conversation", key), null, key);

// Right-to-left languages mirror the arrows.
assert.equal(tabForKey("conversation", "ArrowLeft", true), "settings");
assert.equal(tabForKey("settings", "ArrowRight", true), "conversation");

// The saved tab, and anything unexpected falling back to the default.
assert.equal(parseSavedTab("settings"), "settings");
assert.equal(parseSavedTab("conversation"), "conversation");
assert.equal(parseSavedTab(null), "conversation");
assert.equal(parseSavedTab("old-tab"), "conversation");

console.log("assistant tabs: ok");
