// Standalone assert check. Run with: bun src/components/onboarding/setup/flow.test.ts
import assert from "node:assert/strict";
import {
  ENGLISH_MODEL,
  OTHER_LANGUAGES_MODEL,
  assistantPlan,
  defaultModelFor,
  languageChoices,
  resolveStep,
  setupModelChoice,
  stepAfter,
  stepBefore,
  visibleSteps,
} from "./flow";

// The screens, in order. Windows has no permissions screen.
const windows = visibleSteps(false);
assert.deepEqual(windows, [
  "welcome",
  "microphone",
  "practice",
  "assistant",
  "done",
]);
assert.equal(visibleSteps(true)[1], "permissions");

// Walking the flow.
assert.equal(stepAfter(windows, "welcome"), "microphone");
assert.equal(stepAfter(windows, "assistant"), "done");
assert.equal(stepAfter(windows, "done"), null);
assert.equal(stepBefore(windows, "microphone"), "welcome");
assert.equal(stepBefore(windows, "welcome"), null);

// A step saved by an older version, or one no longer shown, starts over.
assert.equal(resolveStep(windows, "practice"), "practice");
assert.equal(resolveStep(windows, "permissions"), "welcome");

// The default model follows the language.
assert.equal(defaultModelFor("en"), ENGLISH_MODEL);
assert.equal(defaultModelFor("fr"), OTHER_LANGUAGES_MODEL);

const hears = (id: string, language: string) =>
  id === OTHER_LANGUAGES_MODEL || language === "en";
const choose = (
  language: string,
  current: string | null,
  onDisk: string[] = [],
) => setupModelChoice({ language, current, onDisk, hears });

// A fresh install.
assert.equal(choose("en", null), ENGLISH_MODEL);
assert.equal(choose("fr", null), OTHER_LANGUAGES_MODEL);
// Switching language mid-download switches the model.
assert.equal(choose("fr", ENGLISH_MODEL), OTHER_LANGUAGES_MODEL);
assert.equal(choose("en", OTHER_LANGUAGES_MODEL), ENGLISH_MODEL);
// A finished download that hears the language is kept: no second download.
assert.equal(
  choose("en", OTHER_LANGUAGES_MODEL, [OTHER_LANGUAGES_MODEL]),
  OTHER_LANGUAGES_MODEL,
);
assert.equal(choose("en", null, ["canary"]), "canary");
// One on disk that can't hear the language doesn't count.
assert.equal(
  choose("fr", ENGLISH_MODEL, [ENGLISH_MODEL]),
  OTHER_LANGUAGES_MODEL,
);
// The default wins when it's already there.
assert.equal(choose("en", null, ["canary", ENGLISH_MODEL]), ENGLISH_MODEL);

// Languages: English first, the rest by name, English only once.
assert.deepEqual(
  languageChoices(["fr", "en", "de"], (c) =>
    c === "fr" ? "French" : c === "de" ? "German" : "English",
  ),
  ["en", "fr", "de"],
);

// The assistant screen saves only what's complete.
assert.deepEqual(assistantPlan(null, "", "", ""), { kind: "none" });
assert.deepEqual(assistantPlan("skip", "llama", "openai", "gpt"), {
  kind: "none",
});
assert.deepEqual(assistantPlan("local", "", "", ""), { kind: "none" });
assert.deepEqual(assistantPlan("local", "llama", "", ""), {
  kind: "local",
  model: "llama",
});
assert.deepEqual(assistantPlan("cloud", "", "openai", ""), { kind: "none" });
assert.deepEqual(assistantPlan("cloud", "llama", "openai", "gpt"), {
  kind: "cloud",
  providerId: "openai",
  model: "gpt",
});

console.log("setup flow: ok");
