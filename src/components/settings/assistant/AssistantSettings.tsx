import React, { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { commands, type ChatTurn } from "@/bindings";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { Button } from "../../ui/Button";
import { Alert } from "../../ui/Alert";
import { ShortcutInput } from "../ShortcutInput";
import { AssistantVoiceSettings } from "./AssistantVoiceSettings";
import { AssistantProviderSettings } from "./AssistantProviderSettings";

interface AssistantErrorPayload {
  question: string;
  message: string;
}

export const AssistantSettings: React.FC<{
  onOpenPostProcessing: () => void;
}> = ({ onOpenPostProcessing }) => {
  const { t } = useTranslation();
  const [messages, setMessages] = useState<ChatTurn[]>([]);
  const [error, setError] = useState<AssistantErrorPayload | null>(null);
  const bottomRef = useRef<HTMLDivElement>(null);

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

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ block: "end" });
  }, [messages, error]);

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
    <div className="max-w-3xl w-full mx-auto space-y-6">
      <SettingsGroup
        title={t("settings.assistant.title")}
        description={t("settings.assistant.description")}
      >
        <ShortcutInput shortcutId="assistant" grouped={true} />
      </SettingsGroup>

      <AssistantProviderSettings onOpenPostProcessing={onOpenPostProcessing} />

      <AssistantVoiceSettings />

      <SettingsGroup title={t("settings.assistant.conversation.title")}>
        <div className="flex flex-col gap-3 p-4">
          {messages.length === 0 && !error && (
            <p className="text-sm text-ink-2 text-center py-6">
              {t("settings.assistant.conversation.empty")}
            </p>
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
                {message.content}
              </div>
            </div>
          ))}
          {error && (
            <>
              <div className="flex justify-end">
                <div className="max-w-[80%] rounded-lg px-3 py-2 text-sm whitespace-pre-wrap select-text bg-logo-primary/20 opacity-70">
                  {error.question}
                </div>
              </div>
              <Alert variant="error">{error.message}</Alert>
            </>
          )}
          <div ref={bottomRef} />
        </div>
        <div className="flex justify-end p-3">
          <Button
            variant="secondary"
            size="sm"
            onClick={handleNewConversation}
            disabled={messages.length === 0 && !error}
          >
            {t("settings.assistant.conversation.new")}
          </Button>
        </div>
      </SettingsGroup>
    </div>
  );
};
