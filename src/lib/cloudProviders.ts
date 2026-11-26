/** Kusema: what each cloud AI provider does with your text, in one line, and
 * where its own terms say so (see src-tauri/src/cloud_providers.rs). Checked
 * against the providers' pages on 2026-11-26:
 * - Anthropic: API inputs and outputs aren't used for training by default
 *   and are deleted within 30 days.
 *   https://privacy.claude.com/en/articles/7996868-is-my-data-used-for-model-training
 *   https://privacy.claude.com/en/articles/7996866-how-long-do-you-store-my-organization-s-data
 * - DeepSeek: "we directly collect, process and store your Personal Data in
 *   People's Republic of China" (privacy policy, Feb 10, 2026); its Open
 *   Platform terms are governed by PRC law.
 *   https://cdn.deepseek.com/policies/en-US/deepseek-privacy-policy.html
 * - Google Gemini: on unpaid services Google uses content "to provide,
 *   improve, and develop Google products", human reviewers may read it, and
 *   "Do not submit sensitive, confidential, or personal information to the
 *   Unpaid Services". Paid (a Cloud project with billing): not used to improve
 *   products. https://ai.google.dev/gemini-api/terms
 * - OpenAI: API data isn't used to train models unless you opt in; abuse
 *   monitoring logs are kept up to 30 days.
 *   https://developers.openai.com/api/docs/guides/your-data
 */
import { useEffect, useState } from "react";
import { commands, type AdminPolicy } from "@/bindings";

export interface PrivacyNote {
  /** Key under settings.postProcessing.api.privacy. */
  key: string;
  url: string;
}

export const PRIVACY_NOTES: Record<string, PrivacyNote> = {
  anthropic: {
    key: "anthropic",
    url: "https://privacy.claude.com/en/articles/7996866-how-long-do-you-store-my-organization-s-data",
  },
  deepseek: {
    key: "deepseek",
    url: "https://cdn.deepseek.com/policies/en-US/deepseek-privacy-policy.html",
  },
  gemini: {
    key: "gemini",
    url: "https://ai.google.dev/gemini-api/terms",
  },
  openai: {
    key: "openai",
    url: "https://developers.openai.com/api/docs/guides/your-data",
  },
};

export const privacyNote = (id: string | undefined): PrivacyNote | undefined =>
  id ? PRIVACY_NOTES[id] : undefined;

/** The AI providers with a built-in web search Kusema can use. */
export const BUILTIN_SEARCH: Record<string, string> = {
  anthropic: "Claude",
  openai: "OpenAI",
};

const NO_POLICY: AdminPolicy = {
  disable_web_access: false,
  hidden_providers: [],
  searxng_url: null,
};

let cached: AdminPolicy | null = null;
let loading: Promise<AdminPolicy> | null = null;

/** The administrator's policy (read once at startup by the app). */
export const loadAdminPolicy = (): Promise<AdminPolicy> => {
  if (cached) return Promise.resolve(cached);
  loading ??= commands
    .getAdminPolicy()
    .then((policy) => (cached = policy))
    .catch(() => NO_POLICY);
  return loading;
};

export const useAdminPolicy = (): AdminPolicy => {
  const [policy, setPolicy] = useState<AdminPolicy>(cached ?? NO_POLICY);
  useEffect(() => {
    let live = true;
    void loadAdminPolicy().then((p) => live && setPolicy(p));
    return () => {
      live = false;
    };
  }, []);
  return policy;
};

/** Whether the administrator hid this provider. */
export const isHidden = (policy: AdminPolicy, id: string): boolean =>
  policy.hidden_providers.some((hidden) => hidden.toLowerCase() === id);
