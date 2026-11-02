// Checks what a translation can break without a missing key:
// - every {{placeholder}} and <tag> in English is in each translation, and no
//   new ones appear (a plural form for one, zero or two may leave {{count}} out,
//   since the number is in its words);
// - each language has every plural form its grammar needs (Intl.PluralRules,
//   which i18next uses), or i18next shows the raw key for those numbers.
//
//   bun scripts/check-translation-tokens.ts
import fs from "fs";
import path from "path";
import { fileURLToPath } from "url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const LOCALES = path.join(__dirname, "..", "src", "i18n", "locales");
const TOKENS = /\{\{[^}]+\}\}|<[^>]+>/g;
const PLURAL_SUFFIX = /_(zero|one|two|few|many|other)$/;

type Tree = { [key: string]: string | Tree };

const flatten = (tree: Tree, prefix = ""): Map<string, string> => {
  const out = new Map<string, string>();
  for (const [key, value] of Object.entries(tree)) {
    const full = prefix ? `${prefix}.${key}` : key;
    if (typeof value === "string") out.set(full, value);
    else for (const [k, v] of flatten(value, full)) out.set(k, v);
  }
  return out;
};

const load = (lang: string) =>
  flatten(
    JSON.parse(
      fs.readFileSync(path.join(LOCALES, lang, "translation.json"), "utf8"),
    ),
  );

const tokens = (text: string) => (text.match(TOKENS) ?? []).sort();

/** Problems in one language's strings, compared with English. */
export const checkLanguage = (
  lang: string,
  english: Map<string, string>,
  strings: Map<string, string>,
): string[] => {
  const problems: string[] = [];
  const pluralBases = new Set(
    [...english.keys()]
      .filter((k) => k.endsWith("_other"))
      .map((k) => k.replace(PLURAL_SUFFIX, "")),
  );
  for (const [key, text] of strings) {
    const base = key.replace(PLURAL_SUFFIX, "");
    const source = english.get(key) ?? english.get(`${base}_other`);
    if (source === undefined) continue; // the key check reports these
    const want = tokens(source);
    const got = tokens(text);
    const mayDropCount = /_(zero|one|two)$/.test(key) && pluralBases.has(base);
    const ok = mayDropCount
      ? got.every((t) => want.includes(t))
      : want.join() === got.join();
    if (!ok)
      problems.push(
        `${key}: expected ${want.join(" ")}, found ${got.join(" ")}`,
      );
  }
  const categories = new Intl.PluralRules(lang).resolvedOptions()
    .pluralCategories;
  for (const base of pluralBases)
    for (const category of categories)
      if (!strings.has(`${base}_${category}`))
        problems.push(`${base}_${category}: missing plural form`);
  return problems;
};

if (import.meta.main) {
  const english = load("en");
  const languages = fs
    .readdirSync(LOCALES)
    .filter(
      (d) => d !== "en" && fs.statSync(path.join(LOCALES, d)).isDirectory(),
    );
  let failed = false;
  for (const lang of languages) {
    const problems = checkLanguage(lang, english, load(lang));
    if (problems.length) {
      failed = true;
      console.log(`✗ ${lang}: ${problems.length} problems`);
      for (const p of problems.slice(0, 15)) console.log(`    ${p}`);
    }
  }
  if (failed) process.exit(1);
  console.log(
    `✓ Placeholders, tags and plural forms match in all ${languages.length} languages.`,
  );
}
