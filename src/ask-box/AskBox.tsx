import React, { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import {
  Import as FileInput,
  Computer as Monitor,
  Square,
  Xmark as X,
} from "iconoir-react";
import { commands, type AskBoxInfo, type SetupPage } from "@/bindings";
import { Button } from "@/components/ui/Button";
import i18n, { syncLanguageFromSettings } from "@/i18n";
import { getLanguageDirection } from "@/lib/utils/rtl";
import {
  answerFor,
  boxKey,
  replay,
  type AnswerState,
  type AssistantEvent,
} from "./askBoxRules";

const EMPTY: AnswerState = { id: null, question: "", text: "", status: "idle" };

/** Kusema type-to-ask: the floating "Ask Kusema" box. */
const AskBox: React.FC = () => {
  const { t } = useTranslation();
  const [info, setInfo] = useState<AskBoxInfo | null>(null);
  const [draft, setDraft] = useState("");
  const [answer, setAnswer] = useState<AnswerState>(EMPTY);
  const [problem, setProblem] = useState<string | null>(null);
  const [errorSetup, setErrorSetup] = useState<SetupPage | null>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const rootRef = useRef<HTMLDivElement>(null);
  const answerRef = useRef(answer);
  answerRef.current = answer;
  const draftRef = useRef(draft);
  draftRef.current = draft;
  // Events that arrive while a question is being sent, before its id.
  const early = useRef<AssistantEvent[] | null>(null);
  const direction = getLanguageDirection(i18n.language);

  const busy = answer.status === "thinking";

  // Every opening starts fresh, with the keyboard in the question.
  useEffect(() => {
    const opened = listen<AskBoxInfo | null>("ask-box-opened", (event) => {
      void syncLanguageFromSettings();
      setInfo(event.payload);
      setDraft("");
      setAnswer(EMPTY);
      setProblem(null);
      requestAnimationFrame(() => inputRef.current?.focus());
    });
    // The question's streamed text, answer, error or quiet end.
    const apply = (event: AssistantEvent) => {
      if (early.current) early.current.push(event);
      else setAnswer((current) => answerFor(current, event));
    };
    const stream = listen<{ id: number; text: string }>(
      "assistant-stream",
      (e) => apply({ kind: "stream", ...e.payload }),
    );
    const reply = listen<{ id: number; answer: string }>(
      "assistant-reply",
      (e) => apply({ kind: "reply", id: e.payload.id, text: e.payload.answer }),
    );
    const error = listen<{
      id: number;
      message: string;
      key?: string;
      name?: string | null;
      setup?: SetupPage | null;
    }>("assistant-error", (e) => {
      setErrorSetup(e.payload.setup ?? null);
      apply({
        kind: "error",
        id: e.payload.id,
        text: e.payload.key
          ? i18n.t(e.payload.key, { name: e.payload.name ?? "" })
          : e.payload.message,
      });
    });
    const ended = listen<number>("assistant-ended", (e) =>
      apply({ kind: "ended", id: e.payload, text: "" }),
    );
    commands
      .askBoxReady()
      .then((first) => {
        setInfo(first);
        inputRef.current?.focus();
      })
      .catch(console.error);
    return () => {
      for (const unlisten of [opened, stream, reply, error, ended]) {
        void unlisten.then((fn) => fn());
      }
    };
  }, []);

  // The window follows the content's height.
  useEffect(() => {
    const root = rootRef.current;
    if (!root) return;
    const observer = new ResizeObserver(() => {
      void commands.setAskBoxHeight(Math.ceil(root.scrollHeight));
    });
    observer.observe(root);
    return () => observer.disconnect();
  }, []);

  // Clicking away closes an empty box; the backend decides.
  useEffect(() => {
    const onBlur = () => {
      const current = answerRef.current;
      void commands.askBoxBlurred(
        draftRef.current.trim() === "" && current.status === "idle",
        current.status === "done" || current.status === "error",
      );
    };
    window.addEventListener("blur", onBlur);
    return () => window.removeEventListener("blur", onBlur);
  }, []);

  const submit = useCallback(
    async (screen: boolean) => {
      const question = draft.trim();
      if (!question || busy) return;
      setProblem(null);
      early.current = [];
      const result = await commands.askBoxSubmit(question, screen);
      const before = early.current;
      early.current = null;
      if (result.status === "ok") {
        setAnswer(
          replay(
            { id: result.data, question, text: "", status: "thinking" },
            before ?? [],
          ),
        );
        setDraft("");
      } else {
        setProblem(
          result.error === "noWindow" ? t("askBox.noWindow") : result.error,
        );
      }
    },
    [draft, busy, t],
  );

  const stop = () => {
    if (answer.id !== null) void commands.askBoxStop(answer.id);
    setAnswer((current) => ({ ...current, status: "idle", text: "" }));
  };

  const onKeyDown = (event: React.KeyboardEvent) => {
    const key = boxKey(event.key, event.shiftKey);
    if (key === "close") {
      event.preventDefault();
      void commands.askBoxClose();
    } else if (key === "submit") {
      event.preventDefault();
      void submit(false);
    }
  };

  const canInsert = answer.status === "done" && !!info?.app;

  return (
    <div
      ref={rootRef}
      dir={direction}
      onKeyDown={onKeyDown}
      className="flex flex-col gap-2 bg-surface p-3 text-text"
    >
      <div className="flex items-center gap-2">
        <span className="text-sm font-semibold">{t("askBox.title")}</span>
        {info?.app && (
          <span className="truncate text-xs text-ink-2" title={info.app}>
            {t("askBox.typesInto", { app: info.app })}
          </span>
        )}
        {info?.style && (
          <span className="shrink-0 rounded-full bg-selected px-2 py-0.5 text-xs">
            {t("askBox.style", {
              name:
                info.style.name ??
                t(`settings.appStyles.builtIn.${info.style.id}.name`, {
                  defaultValue: info.style.id,
                }),
            })}
          </span>
        )}
        <button
          type="button"
          onClick={() => void commands.askBoxClose()}
          aria-label={t("askBox.close")}
          title={t("askBox.close")}
          className="ms-auto rounded p-1 text-ink-2 hover:bg-wash hover:text-text focus:outline-none focus-visible:ring-2 focus-visible:ring-logo-primary"
        >
          <X className="h-4 w-4" />
        </button>
      </div>

      <div className="flex items-end gap-2">
        <textarea
          ref={inputRef}
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          rows={Math.min(4, Math.max(1, draft.split("\n").length))}
          placeholder={t("askBox.placeholder")}
          aria-label={t("askBox.placeholder")}
          className="min-h-9 flex-1 resize-none rounded-lg border border-line bg-wash px-3 py-2 text-sm focus:outline-none focus-visible:ring-2 focus-visible:ring-logo-primary"
        />
        <Button
          variant="primary"
          size="sm"
          onClick={() => void submit(false)}
          disabled={!draft.trim() || busy}
        >
          {t("askBox.ask")}
        </Button>
        <button
          type="button"
          onClick={() => void submit(true)}
          disabled={!draft.trim() || busy || !info?.can_see_screen}
          aria-label={t("askBox.screen")}
          title={
            info?.can_see_screen
              ? t("askBox.screen")
              : t("askBox.screenUnavailable")
          }
          className="flex h-8 w-8 items-center justify-center rounded-lg border border-line text-ink-2 hover:bg-wash hover:text-text disabled:cursor-not-allowed disabled:opacity-40 focus:outline-none focus-visible:ring-2 focus-visible:ring-logo-primary"
        >
          <Monitor className="h-4 w-4" />
        </button>
      </div>

      {problem && (
        <p role="alert" className="m-0 text-xs text-error">
          {problem}
        </p>
      )}

      {answer.status !== "idle" && (
        <div className="flex flex-col gap-2 border-t border-line pt-2">
          <p
            className="m-0 truncate text-xs text-ink-2"
            title={answer.question}
          >
            {answer.question}
          </p>
          <div
            role="log"
            aria-live="polite"
            className={`max-h-72 overflow-y-auto whitespace-pre-wrap text-sm select-text ${
              answer.status === "error" ? "text-error" : ""
            }`}
          >
            {answer.text ||
              (answer.status === "thinking" ? t("askBox.thinking") : "")}
          </div>
          <div className="flex items-center gap-2">
            {busy && (
              <Button variant="secondary" size="sm" onClick={stop}>
                <Square className="me-1 h-3 w-3" />
                {t("askBox.stop")}
              </Button>
            )}
            {answer.status === "error" && errorSetup && (
              <Button
                variant="secondary"
                size="sm"
                onClick={() => {
                  void commands.askBoxClose();
                  void commands.openSetupPage(errorSetup);
                }}
              >
                {t("overlay.setUp")}
              </Button>
            )}
            {canInsert && (
              <Button
                variant="secondary"
                size="sm"
                onClick={() => void commands.askBoxInsert(answer.text)}
              >
                <FileInput className="me-1 h-3.5 w-3.5" />
                {t("askBox.insert")}
              </Button>
            )}
            <span className="ms-auto text-[11px] text-ink-3">
              {t("askBox.hint")}
            </span>
          </div>
        </div>
      )}
    </div>
  );
};

export default AskBox;
