import React, { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Globe } from "iconoir-react";
import { commands, type ScreenCard as Card } from "@/bindings";
import { Button } from "@/components/ui/Button";

type Web = Extract<Card, { kind: "web" }>;

/** The card's page has painted; the backend shows (and focuses) it now. */
const ready = () =>
  requestAnimationFrame(() => {
    void commands.screenCardReady();
  });

/** Kusema web search: what's about to go to the web. With "Confirm
 * searches", each search can be changed first; with private context in the
 * question, it says that context stays out. Send or Cancel. */
export const WebCard: React.FC<{ card: Web }> = ({ card }) => {
  const { t } = useTranslation();
  const [queries, setQueries] = useState<string[]>(card.queries);
  const [secondsLeft, setSecondsLeft] = useState(card.timeoutSecs);
  const answered = useRef(false);
  const firstRef = useRef<HTMLInputElement>(null);

  const answer = useCallback(
    (send: boolean) => {
      if (answered.current) return;
      answered.current = true;
      void commands.answerWebCard(send ? queries : null);
    },
    [queries],
  );

  useEffect(() => {
    ready();
    const timer = setInterval(
      () => setSecondsLeft((s) => Math.max(0, s - 1)),
      1000,
    );
    const onFocus = () => {
      if (card.editable) firstRef.current?.focus();
      else document.getElementById("web-card-send")?.focus();
    };
    window.addEventListener("focus", onFocus);
    return () => {
      clearInterval(timer);
      window.removeEventListener("focus", onFocus);
    };
  }, [card.editable]);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        answer(false);
      } else if (event.key === "Enter" && !event.shiftKey) {
        event.preventDefault();
        answer(true);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [answer]);

  const kind = card.private ? t(`screenCard.web.kind.${card.private}`) : null;
  const title =
    card.action === "read_link"
      ? t("screenCard.web.title.link")
      : card.action === "builtin"
        ? t("screenCard.web.title.builtin", { name: card.provider })
        : t("screenCard.web.title.search");
  const canSend =
    card.action !== "search" || queries.some((q) => q.trim() !== "");

  return (
    <div className="flex h-full flex-col gap-3 bg-surface p-4 text-text">
      <div className="flex items-baseline justify-between gap-2">
        <h1 className="m-0 flex items-center gap-1.5 text-sm font-semibold">
          <Globe className="h-4 w-4 shrink-0 text-ink-2" aria-hidden />
          {title}
        </h1>
        <span className="text-xs text-ink-3 tabular-nums">
          {t("screenCard.autoCancel", { seconds: secondsLeft })}
        </span>
      </div>

      {kind && (
        <p className="m-0 rounded-md border border-line bg-wash p-2 text-sm">
          {card.action === "read_link"
            ? t("screenCard.web.privateLink", { kind })
            : t("screenCard.web.private", { kind })}
        </p>
      )}

      {card.action === "builtin" && card.concern && (
        <p className="m-0 text-sm text-ink-2">
          {card.concern === "private_context"
            ? t("screenCard.web.builtinPrivate", {
                name: card.provider,
                kind: kind ?? "",
              })
            : t("screenCard.web.builtinUnconfirmable", { name: card.provider })}
        </p>
      )}

      {card.action === "read_link" && card.url && (
        <p className="m-0 truncate rounded-md border border-line bg-background p-2 text-xs">
          {card.url}
        </p>
      )}

      {card.action === "search" && (
        <div className="flex min-h-0 flex-1 flex-col gap-2 overflow-y-auto">
          <p className="m-0 text-xs text-ink-2">
            {card.editable
              ? t("screenCard.web.editHint", { provider: card.provider })
              : t("screenCard.web.sendHint", { provider: card.provider })}
          </p>
          {queries.map((query, i) =>
            card.editable ? (
              <input
                key={i}
                ref={i === 0 ? firstRef : undefined}
                autoFocus={i === 0}
                value={query}
                aria-label={t("screenCard.web.searchLabel", { n: i + 1 })}
                onChange={(e) =>
                  setQueries((all) =>
                    all.map((q, j) => (j === i ? e.target.value : q)),
                  )
                }
                className="rounded-md border border-line bg-background px-2 py-1.5 text-sm focus:outline-none focus:ring-1 focus:ring-logo-primary"
              />
            ) : (
              <p
                key={i}
                className="m-0 rounded-md border border-line bg-background px-2 py-1.5 text-sm"
              >
                {query}
              </p>
            ),
          )}
        </div>
      )}

      <div className="mt-auto flex justify-end gap-2">
        <Button variant="secondary" size="sm" onClick={() => answer(false)}>
          {t("screenCard.web.cancel")}
        </Button>
        <Button
          id="web-card-send"
          variant="primary"
          size="sm"
          autoFocus={!card.editable}
          onClick={() => answer(true)}
          disabled={!canSend}
        >
          {card.action === "builtin"
            ? t("screenCard.web.searchAnyway")
            : card.action === "read_link"
              ? t("screenCard.web.continue")
              : t("screenCard.web.send")}
        </Button>
      </div>
    </div>
  );
};
