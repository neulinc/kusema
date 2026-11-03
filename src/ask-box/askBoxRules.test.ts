// Standalone assert check. Run with: bun src/ask-box/askBoxRules.test.ts
import assert from "node:assert/strict";
import {
  answerFor,
  boxKey,
  replay,
  type AnswerState,
  type AssistantEvent,
} from "./askBoxRules";

const asked: AnswerState = {
  id: 7,
  question: "Capital of Ghana?",
  text: "",
  status: "thinking",
};
const ev = (
  kind: AssistantEvent["kind"],
  id: number,
  text = "",
): AssistantEvent => ({ kind, id, text });

// The answer streams in, then the full reply replaces it.
let state = answerFor(asked, ev("stream", 7, "Ac"));
state = answerFor(state, ev("stream", 7, "cra."));
assert.equal(state.text, "Accra.");
assert.equal(state.status, "thinking");
state = answerFor(state, ev("reply", 7, "Accra."));
assert.equal(state.status, "done");

// Another question's events (a spoken one at the same time) are ignored.
assert.deepEqual(answerFor(asked, ev("stream", 8, "noise")), asked);
assert.deepEqual(answerFor(asked, ev("reply", 8, "other")), asked);
const idle: AnswerState = { ...asked, id: null, status: "idle" };
assert.deepEqual(answerFor(idle, ev("reply", 7, "late")), idle);

// Errors show; a cancelled or unsent question goes back to idle.
assert.equal(answerFor(asked, ev("error", 7, "No API key")).status, "error");
assert.equal(answerFor(asked, ev("ended", 7)).status, "idle");

// Streamed text after the reply doesn't add to it.
const done = answerFor(asked, ev("reply", 7, "Accra."));
assert.equal(answerFor(done, ev("stream", 7, "x")).text, "Accra.");

// An error that beat the id back to the page still lands.
assert.equal(replay(asked, [ev("error", 7, "Privileged")]).status, "error");
assert.equal(replay(asked, [ev("error", 6, "older")]).status, "thinking");

// Keys: Escape closes, Enter asks, Shift+Enter is a new line.
assert.equal(boxKey("Escape", false), "close");
assert.equal(boxKey("Enter", false), "submit");
assert.equal(boxKey("Enter", true), null);
assert.equal(boxKey("a", false), null);

console.log("askBoxRules: ok");
