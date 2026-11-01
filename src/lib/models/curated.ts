/** The short model list on the Models page; the rest is behind "Show all models". */

export interface CuratedModel {
  /** Under settings.models.curated.rows in the translations. */
  key: "parakeetUnified" | "parakeetV3" | "canary" | "nemotron";
  repository: string;
}

export const CURATED_MODELS: CuratedModel[] = [
  {
    key: "parakeetUnified",
    repository: "handy-computer/parakeet-unified-en-0.6b-gguf",
  },
  {
    key: "parakeetV3",
    repository: "handy-computer/parakeet-tdt-0.6b-v3-gguf",
  },
  {
    key: "canary",
    repository: "handy-computer/canary-180m-flash-gguf",
  },
  {
    key: "nemotron",
    repository: "handy-computer/nemotron-3.5-asr-streaming-0.6b-gguf",
  },
];

/** Enough of a model to sort it. */
export interface ListedModel {
  is_downloaded: boolean;
  is_custom: boolean;
}

/**
 * The short list in its order, each with the app's first copy of the model;
 * and anything else on disk, so a model in use never vanishes from the page.
 */
export const splitCurated = <T extends ListedModel>(
  models: T[],
  repoOf: (model: T) => string | null,
) => {
  const curated = CURATED_MODELS.flatMap((entry) => {
    const model = models.find(
      (m) => !m.is_custom && repoOf(m) === entry.repository,
    );
    return model ? [{ ...entry, model }] : [];
  });
  const listed = new Set<T>(curated.map((c) => c.model));
  return {
    curated,
    others: models.filter((m) => m.is_downloaded && !listed.has(m)),
  };
};
