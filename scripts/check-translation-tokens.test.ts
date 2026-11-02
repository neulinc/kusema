// Standalone assert check. Run with: bun scripts/check-translation-tokens.test.ts
import assert from "node:assert/strict";
import { checkLanguage } from "./check-translation-tokens";

const english = new Map([
  ["a.hello", "Hello {{name}}"],
  ["a.keys", "Hold <keys/> to talk"],
  ["a.days_one", "{{count}} day"],
  ["a.days_other", "{{count}} days"],
]);
const good = new Map([
  ["a.hello", "Hallo {{name}}"],
  ["a.keys", "Halte <keys/> gedrückt"],
  ["a.days_one", "{{count}} Tag"],
  ["a.days_other", "{{count}} Tage"],
]);
assert.deepEqual(checkLanguage("de", english, good), []);

// A placeholder renamed, dropped or added is caught.
const broken = new Map(good);
broken.set("a.hello", "Hallo {{nom}}");
broken.set("a.keys", "Halte gedrückt");
broken.set("a.days_other", "{{count}} Tage {{extra}}");
assert.equal(checkLanguage("de", english, broken).length, 3);

// "one" may say the number in words; "other" may not.
const words = new Map(good);
words.set("a.days_one", "Ein Tag");
assert.deepEqual(checkLanguage("de", english, words), []);
words.set("a.days_other", "Tage");
assert.equal(checkLanguage("de", english, words).length, 1);

// Russian needs "few" and "many", or i18next shows the key for 3 or 5 days.
const russian = new Map([
  ["a.hello", "Привет, {{name}}"],
  ["a.keys", "Удерживайте <keys/>"],
  ["a.days_one", "{{count}} день"],
  ["a.days_other", "{{count}} дня"],
]);
assert.deepEqual(checkLanguage("ru", english, russian), [
  "a.days_few: missing plural form",
  "a.days_many: missing plural form",
]);
russian.set("a.days_few", "{{count}} дня");
russian.set("a.days_many", "{{count}} дней");
assert.deepEqual(checkLanguage("ru", english, russian), []);

// Japanese has one form only; extra ones are not required.
assert.deepEqual(
  checkLanguage(
    "ja",
    english,
    new Map([
      ["a.hello", "{{name}} さん"],
      ["a.keys", "<keys/> を押したまま"],
      ["a.days_other", "{{count}} 日"],
    ]),
  ),
  [],
);

console.log("translation tokens: ok");
