// Standalone assert check. Run with: bun src/lib/models/curated.test.ts
import assert from "node:assert/strict";
import { catalog } from "@handy-computer/models";
import { CURATED_MODELS, splitCurated } from "./curated";

interface Fake {
  name: string;
  repo: string | null;
  is_downloaded: boolean;
  is_custom: boolean;
}

const fake = (
  name: string,
  repo: string | null,
  over: Partial<Fake> = {},
): Fake => ({ name, repo, is_downloaded: false, is_custom: false, ...over });

const split = (models: Fake[]) => splitCurated(models, (m) => m.repo);

// Every model on the short list exists in the catalog.
const repositories = catalog.models.map((m) => m.repository);
for (const { repository } of CURATED_MODELS)
  assert.ok(repositories.includes(repository), repository);
assert.equal(CURATED_MODELS[0].key, "parakeetUnified", "the default is first");

const unified = fake("unified", CURATED_MODELS[0].repository);
const v3 = fake("v3", CURATED_MODELS[1].repository);
const canary = fake("canary", CURATED_MODELS[2].repository);
const nemotron = fake("nemotron", CURATED_MODELS[3].repository);
const other = fake("other", "someone/other-gguf");

// Only the four show, in the list's order, whatever order the app gives.
const all = split([other, nemotron, canary, v3, unified]);
assert.deepEqual(
  all.curated.map((c) => c.model.name),
  ["unified", "v3", "canary", "nemotron"],
);
assert.deepEqual(all.others, [], "not on disk: only under Show all models");

// Anything else on disk stays visible, whichever kind it is.
const onDisk = fake("other", "someone/other-gguf", { is_downloaded: true });
const legacy = fake("old", null, { is_downloaded: true });
assert.deepEqual(split([unified, onDisk, legacy]).others, [onDisk, legacy]);

// A second copy of a short-list model isn't lost.
const quant = fake("quant", unified.repo, { is_downloaded: true });
const copies = split([unified, quant]);
assert.equal(copies.curated[0].model, unified);
assert.deepEqual(copies.others, [quant]);

// A custom model never stands in for a short-list one.
const custom = fake("mine", unified.repo, { is_custom: true });
assert.deepEqual(split([custom]).curated, []);

// A model the app doesn't offer is left out.
assert.deepEqual(
  split([unified]).curated.map((c) => c.key),
  ["parakeetUnified"],
);

console.log("curated models: ok");
