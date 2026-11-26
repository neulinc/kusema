// Kusema: Amazon Bedrock's ~50 models, grouped by developer with Claude
// first, so the list is easy to scan. Ids look like `developer.model`.

export type ModelGroup<T extends { value: string }> = {
  label: string;
  options: T[];
};

/** Developers' own names for the id prefixes Bedrock uses. Unknown
 * prefixes are shown capitalised. */
const DEVELOPERS: Record<string, string> = {
  anthropic: "Anthropic",
  amazon: "Amazon",
  ai21: "AI21 Labs",
  cohere: "Cohere",
  deepseek: "DeepSeek",
  google: "Google",
  meta: "Meta",
  minimax: "MiniMax",
  mistral: "Mistral AI",
  moonshotai: "Moonshot AI",
  nvidia: "NVIDIA",
  openai: "OpenAI",
  qwen: "Qwen",
  writer: "Writer",
  zai: "Z.ai",
};

export const developerOf = (id: string): string => {
  const prefix = id.includes(".") ? id.split(".")[0].toLowerCase() : "";
  if (!prefix) return "";
  return DEVELOPERS[prefix] ?? prefix.charAt(0).toUpperCase() + prefix.slice(1);
};

const byNameNewestFirst = (a: string, b: string) =>
  b.localeCompare(a, "en", { numeric: true });

/** Anthropic first, then the other developers A–Z; within a developer,
 * newest-looking ids first. Ids without a developer go last. */
export const groupByDeveloper = <T extends { value: string }>(
  options: T[],
): ModelGroup<T>[] => {
  const groups = new Map<string, T[]>();
  for (const option of options) {
    const name = developerOf(option.value);
    groups.set(name, [...(groups.get(name) ?? []), option]);
  }
  const rank = (name: string) =>
    name === "Anthropic" ? 0 : name === "" ? 2 : 1;
  return [...groups.entries()]
    .sort(([a], [b]) => rank(a) - rank(b) || a.localeCompare(b, "en"))
    .map(([label, items]) => ({
      label,
      options: [...items].sort((x, y) => byNameNewestFirst(x.value, y.value)),
    }));
};
