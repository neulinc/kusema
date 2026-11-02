/** AI providers that run on this computer (see src-tauri/src/local_providers.rs). */

export const LOCAL_PROVIDER_IDS = ["ollama", "lmstudio"] as const;

const HOW_TO: Record<string, string> = {
  ollama: "https://ollama.com/download",
  lmstudio: "https://lmstudio.ai/docs",
};

export const isLocalProvider = (id: string | undefined): boolean =>
  !!id && (LOCAL_PROVIDER_IDS as readonly string[]).includes(id);

/** No API key needed: the local providers, and Custom (usually a local server). */
export const isKeyOptional = (id: string | undefined): boolean =>
  isLocalProvider(id) || id === "custom";

export const setUpUrl = (id: string): string | undefined => HOW_TO[id];

/** Local providers first, in their own order; the rest as they were. */
export const localFirst = <T extends { id: string }>(providers: T[]): T[] => [
  ...providers.filter((p) => isLocalProvider(p.id)),
  ...providers.filter((p) => !isLocalProvider(p.id)),
];
