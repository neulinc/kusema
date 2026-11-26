// Standalone assert check. Run with: bun src/components/settings/PostProcessingSettingsApi/bedrockModels.test.ts
import assert from "node:assert/strict";
import { developerOf, groupByDeveloper } from "./bedrockModels";

const o = (value: string) => ({ value, label: value });

// Part of what us-east-2 listed in the live test.
const groups = groupByDeveloper(
  [
    "openai.gpt-5.6-terra",
    "qwen.qwen3-32b",
    "deepseek.v3.2",
    "anthropic.claude-haiku-4-5",
    "qwen.qwen3-235b-a22b-2507",
    "anthropic.claude-opus-4-8",
    "mistral.devstral-2-123b",
    "my-custom-model",
    "zai.glm-4.7-flash",
  ].map(o),
);

// Claude first, then developers A–Z, ids without a developer last.
assert.deepEqual(
  groups.map((g) => g.label),
  ["Anthropic", "DeepSeek", "Mistral AI", "OpenAI", "Qwen", "Z.ai", ""],
);
assert.deepEqual(
  groups[0].options.map((x) => x.value),
  ["anthropic.claude-opus-4-8", "anthropic.claude-haiku-4-5"],
);
assert.deepEqual(
  groups[4].options.map((x) => x.value),
  ["qwen.qwen3-235b-a22b-2507", "qwen.qwen3-32b"],
);
assert.equal(groups.flatMap((g) => g.options).length, 9);

// Unknown developers are named from the prefix.
assert.equal(developerOf("acme.model-1"), "Acme");
assert.equal(developerOf("no-prefix"), "");

console.log("bedrock models: ok");
