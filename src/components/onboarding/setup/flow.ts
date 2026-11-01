/** Kusema's setup flow, as plain rules: which screens, and which speech model. */

export type OnboardingStep =
  | "welcome"
  | "permissions"
  | "microphone"
  | "practice"
  | "assistant"
  | "done";

export const SETUP_STEPS: OnboardingStep[] = [
  "welcome",
  "permissions",
  "microphone",
  "practice",
  "assistant",
  "done",
];

export const ENGLISH_MODEL = "parakeet-unified-en-0.6b";
export const OTHER_LANGUAGES_MODEL = "parakeet-tdt-0.6b-v3";

/** The screens to show. Permissions only when something needs granting. */
export const visibleSteps = (permissionsNeeded: boolean): OnboardingStep[] =>
  SETUP_STEPS.filter((s) => s !== "permissions" || permissionsNeeded);

/** A saved step that no longer shows falls back to the first screen. */
export const resolveStep = (
  steps: OnboardingStep[],
  saved: OnboardingStep,
): OnboardingStep => (steps.includes(saved) ? saved : steps[0]);

/** The screen after this one; null on the last, which finishes setup. */
export const stepAfter = (
  steps: OnboardingStep[],
  step: OnboardingStep,
): OnboardingStep | null => steps[steps.indexOf(step) + 1] ?? null;

/** The screen before this one; null on the first. */
export const stepBefore = (
  steps: OnboardingStep[],
  step: OnboardingStep,
): OnboardingStep | null => steps[steps.indexOf(step) - 1] ?? null;

/** The default model for a spoken language. */
export const defaultModelFor = (language: string): string =>
  language === "en" ? ENGLISH_MODEL : OTHER_LANGUAGES_MODEL;

/**
 * The model setup should use. One already on disk that hears the language
 * saves a download; otherwise the default for the language.
 */
export const setupModelChoice = ({
  language,
  current,
  onDisk,
  hears,
}: {
  language: string;
  /** The model being downloaded or used so far. */
  current: string | null;
  onDisk: string[];
  hears: (id: string, language: string) => boolean;
}): string => {
  const wanted = defaultModelFor(language);
  const usable = onDisk.filter((id) => hears(id, language));
  if (current && usable.includes(current)) return current;
  if (usable.includes(wanted)) return wanted;
  return usable[0] ?? wanted;
};

/** English first, then the rest by name. */
export const languageChoices = (
  codes: readonly string[],
  nameOf: (code: string) => string,
): string[] => [
  "en",
  ...codes
    .filter((c) => c !== "en")
    .sort((a, b) => nameOf(a).localeCompare(nameOf(b))),
];

/** What Continue does on the AI assistant screen. */
export type AssistantPlan =
  | { kind: "none" }
  | { kind: "local"; model: string }
  | { kind: "cloud"; providerId: string; model: string };

/** A choice left unfinished saves nothing, the same as skipping. */
export const assistantPlan = (
  choice: "local" | "cloud" | "skip" | null,
  localModel: string,
  providerId: string,
  cloudModel: string,
): AssistantPlan => {
  if (choice === "local" && localModel)
    return { kind: "local", model: localModel };
  if (choice === "cloud" && providerId && cloudModel)
    return { kind: "cloud", providerId, model: cloudModel };
  return { kind: "none" };
};
