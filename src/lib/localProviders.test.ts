// Standalone assert check. Run with: bun src/lib/localProviders.test.ts
import assert from "node:assert/strict";
import {
  isKeyOptional,
  isLocalProvider,
  localFirst,
  setUpUrl,
} from "./localProviders";

assert.equal(isLocalProvider("ollama"), true);
assert.equal(isLocalProvider("lmstudio"), true);
assert.equal(isLocalProvider("custom"), false);
assert.equal(isLocalProvider("openai"), false);
assert.equal(isLocalProvider(undefined), false);

// No key is asked for on this computer, or for Custom.
for (const id of ["ollama", "lmstudio", "custom"])
  assert.equal(isKeyOptional(id), true, id);
for (const id of ["openai", "anthropic", "groq", undefined])
  assert.equal(isKeyOptional(id), false, String(id));

// Each local provider has a how-to link.
assert.match(setUpUrl("ollama") ?? "", /^https:\/\/ollama\.com\//);
assert.match(setUpUrl("lmstudio") ?? "", /^https:\/\/lmstudio\.ai\//);
assert.equal(setUpUrl("openai"), undefined);

// Local ones first, whatever order a saved list has them in.
const saved = [
  { id: "openai" },
  { id: "anthropic" },
  { id: "custom" },
  { id: "ollama" },
  { id: "lmstudio" },
];
assert.deepEqual(
  localFirst(saved).map((p) => p.id),
  ["ollama", "lmstudio", "openai", "anthropic", "custom"],
);

console.log("local providers: ok");
