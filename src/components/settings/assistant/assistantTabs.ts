/** The Assistant page's tabs, as plain rules: arrow keys, and the remembered tab. */

export type AssistantTab = "conversation" | "settings";

export const ASSISTANT_TABS: AssistantTab[] = ["conversation", "settings"];

const STORAGE_KEY = "kusema.assistant.tab";

/** The tab a key moves to, wrapping round; null if the key doesn't move tabs. */
export const tabForKey = (
  current: AssistantTab,
  key: string,
  rtl = false,
): AssistantTab | null => {
  const index = ASSISTANT_TABS.indexOf(current);
  const count = ASSISTANT_TABS.length;
  const forward = rtl ? "ArrowLeft" : "ArrowRight";
  const back = rtl ? "ArrowRight" : "ArrowLeft";
  switch (key) {
    case forward:
      return ASSISTANT_TABS[(index + 1) % count];
    case back:
      return ASSISTANT_TABS[(index - 1 + count) % count];
    case "Home":
      return ASSISTANT_TABS[0];
    case "End":
      return ASSISTANT_TABS[count - 1];
    default:
      return null;
  }
};

/** What was saved, if it's still a tab; otherwise Conversation. */
export const parseSavedTab = (saved: string | null): AssistantTab =>
  ASSISTANT_TABS.includes(saved as AssistantTab)
    ? (saved as AssistantTab)
    : "conversation";

export const loadTab = (): AssistantTab => {
  try {
    return parseSavedTab(localStorage.getItem(STORAGE_KEY));
  } catch {
    return "conversation";
  }
};

export const saveTab = (tab: AssistantTab) => {
  try {
    localStorage.setItem(STORAGE_KEY, tab);
  } catch {
    // Unavailable: the page opens on Conversation next time, nothing worse.
  }
};
