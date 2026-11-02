import React, { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { Image } from "lucide-react";
import { commands, type ChatTurn } from "@/bindings";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { Button } from "../../ui/Button";
import { Alert } from "../../ui/Alert";
import { ShortcutInput } from "../ShortcutInput";
import { AssistantVoiceSettings } from "./AssistantVoiceSettings";
import { AssistantProviderSettings } from "./AssistantProviderSettings";
import { AssistantScreenSettings } from "./AssistantScreenSettings";
import { useSettings } from "../../../hooks/useSettings";
import { PrivilegedChip, PrivilegedToggle } from "../privacy/PrivilegedToggle";
import {
  ASSISTANT_TABS,
  loadTab,
  saveTab,
  tabForKey,
  type AssistantTab,
} from "./assistantTabs";

interface AssistantErrorPayload {
  question: string;
  message: string;
}

export const AssistantSettings: React.FC<{
  onOpenPostProcessing: () => void;
}> = ({ onOpenPostProcessing }) => {
  const { t } = useTranslation();
  const { getSetting } = useSettings();
  const [messages, setMessages] = useState<ChatTurn[]>([]);
  const [error, setError] = useState<AssistantErrorPayload | null>(null);
  const bottomRef = useRef<HTMLDivElement>(null);
  const [tab, setTab] = useState<AssistantTab>(loadTab);
  const tabRefs = useRef<
    Partial<Record<AssistantTab, HTMLButtonElement | null>>
  >({});

  const chooseTab = (next: AssistantTab) => {
    setTab(next);
    saveTab(next);
  };

  const onTabKey = (ev: React.KeyboardEvent) => {
    const next = tabForKey(tab, ev.key, document.documentElement.dir === "rtl");
    if (!next) return;
    ev.preventDefault();
    chooseTab(next);
    tabRefs.current[next]?.focus();
  };

  const providerId = getSetting("post_process_provider_id") ?? "";
  const hasModel = !!getSetting("post_process_models")?.[providerId];

  // The backend owns the conversation; refetch rather than mirror it.
  const refresh = useCallback(async () => {
    try {
      setMessages(await commands.getAssistantConversation());
    } catch (e) {
      console.error("Failed to load assistant conversation:", e);
    }
  }, []);

  useEffect(() => {
    refresh();
    const unlistenReply = listen("assistant-reply", () => {
      setError(null);
      refresh();
    });
    const unlistenError = listen<AssistantErrorPayload>(
      "assistant-error",
      (event) => setError(event.payload),
    );
    return () => {
      unlistenReply.then((fn) => fn());
      unlistenError.then((fn) => fn());
    };
  }, [refresh]);

  // The newest message stays in view, including on coming back to this tab.
  useEffect(() => {
    if (tab === "conversation")
      bottomRef.current?.scrollIntoView({ block: "end" });
  }, [messages, error, tab]);

  const handleNewConversation = async () => {
    try {
      await commands.clearAssistantConversation();
      setMessages([]);
      setError(null);
    } catch (e) {
      console.error("Failed to clear assistant conversation:", e);
    }
  };

  return (
    <div className="max-w-3xl w-full mx-auto">
      <div className="mb-4 flex flex-wrap items-center justify-between gap-3">
        <div
          role="tablist"
          aria-label={t("settings.assistant.tabs.label")}
          className="flex gap-1"
          onKeyDown={onTabKey}
        >
          {ASSISTANT_TABS.map((name) => (
            <button
              key={name}
              ref={(el) => {
                tabRefs.current[name] = el;
              }}
              type="button"
              role="tab"
              id={`assistant-tab-${name}`}
              aria-selected={tab === name}
              aria-controls={`assistant-panel-${name}`}
              // One stop for Tab; the arrow keys move between tabs.
              tabIndex={tab === name ? 0 : -1}
              onClick={() => chooseTab(name)}
              className={`cursor-pointer rounded-lg border-b-2 px-3 py-1.5 text-sm font-medium transition-colors focus:outline-none focus-visible:ring-2 focus-visible:ring-logo-primary ${
                tab === name
                  ? "border-background-ui text-text"
                  : "border-transparent text-ink-2 hover:bg-wash hover:text-text"
              }`}
            >
              {t(`settings.assistant.tabs.${name}`)}
            </button>
          ))}
        </div>
        {getSetting("privileged_mode") && <PrivilegedChip />}
      </div>

      {tab === "settings" && (
        <div
          role="tabpanel"
          id="assistant-panel-settings"
          aria-labelledby="assistant-tab-settings"
          className="space-y-6 focus:outline-none"
        >
          <SettingsGroup
            title={t("settings.assistant.title")}
            description={t("settings.assistant.description")}
          >
            <ShortcutInput shortcutId="assistant" grouped={true} />
            <PrivilegedToggle />
          </SettingsGroup>

          <AssistantProviderSettings
            onOpenPostProcessing={onOpenPostProcessing}
          />

          <AssistantScreenSettings />

          <AssistantVoiceSettings />
        </div>
      )}

      {tab === "conversation" && (
        <div
          role="tabpanel"
          id="assistant-panel-conversation"
          aria-labelledby="assistant-tab-conversation"
          className="flex min-h-[420px] flex-col"
        >
          {/* Stays in reach however long the conversation gets. */}
          <div className="sticky top-0 z-10 flex justify-end bg-background py-2">
            <Button
              variant="secondary"
              size="sm"
              onClick={handleNewConversation}
              disabled={messages.length === 0 && !error}
            >
              {t("settings.assistant.conversation.new")}
            </Button>
          </div>
          <div
            role="log"
            aria-label={t("settings.assistant.tabs.conversation")}
            className="flex flex-1 flex-col gap-3 py-2"
          >
            {messages.length === 0 && !error && (
              <div className="flex flex-col items-center gap-3 py-6">
                <p className="m-0 text-sm text-ink-2 text-center">
                  {t(
                    hasModel
                      ? "settings.assistant.conversation.empty"
                      : "settings.assistant.conversation.noProvider",
                  )}
                </p>
                {!hasModel && (
                  <Button
                    variant="secondary"
                    size="sm"
                    onClick={() => chooseTab("settings")}
                  >
                    {t("settings.assistant.conversation.openSettings")}
                  </Button>
                )}
              </div>
            )}
            {messages.map((message, index) => (
              <div
                key={index}
                className={`flex ${message.role === "user" ? "justify-end" : "justify-start"}`}
              >
                <div
                  className={`max-w-[80%] rounded-lg px-3 py-2 text-sm whitespace-pre-wrap select-text ${
                    message.role === "user"
                      ? "bg-logo-primary/20"
                      : "bg-wash border border-line"
                  }`}
                >
                  {message.screenshot && (
                    <span className="mb-1 flex items-center gap-1 text-xs text-ink-2">
                      <Image className="h-3.5 w-3.5" />
                      {t("settings.assistant.conversation.screenshot")}
                    </span>
                  )}
                  {message.content}
                </div>
              </div>
            ))}
            {error && (
              <>
                <div className="flex justify-end">
                  <div className="max-w-[80%] rounded-lg px-3 py-2 text-sm whitespace-pre-wrap select-text bg-logo-primary/20">
                    {error.question}
                  </div>
                </div>
                <Alert variant="error">{error.message}</Alert>
              </>
            )}
            <div ref={bottomRef} />
          </div>
          {/* v0.5's type-to-ask box goes here, below the conversation. */}
        </div>
      )}
    </div>
  );
};
