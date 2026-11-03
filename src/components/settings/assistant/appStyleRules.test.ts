// Standalone assert check. Run with: bun src/components/settings/assistant/appStyleRules.test.ts
import assert from "node:assert/strict";
import { addApp, newStyleId, normalizeApp, removeApp } from "./appStyleRules";

// Program names are tidied the way the backend compares them.
assert.equal(normalizeApp("  Outlook "), "outlook.exe");
assert.equal(normalizeApp("WINWORD.EXE"), "winword.exe");
assert.equal(normalizeApp("C:\\Program Files\\Slack\\slack.exe"), "slack.exe");
assert.equal(normalizeApp("   "), "");

// Adding: new apps go on the end, repeats and blanks change nothing.
const apps = ["outlook.exe"];
assert.deepEqual(addApp(apps, "Notepad"), ["outlook.exe", "notepad.exe"]);
assert.equal(addApp(apps, "OUTLOOK"), apps);
assert.equal(addApp(apps, " "), apps);
assert.deepEqual(removeApp(["a.exe", "b.exe"], "a.exe"), ["b.exe"]);

// New styles get an unused id.
assert.equal(newStyleId(["email", "formal"]), "custom-1");
assert.equal(newStyleId(["custom-1", "custom-2"]), "custom-3");

console.log("appStyleRules: ok");
