import React, { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { AlertTriangle, X } from "lucide-react";
import { commands, type ScreenCard as Card } from "@/bindings";
import { Button } from "@/components/ui/Button";

type Confirm = Extract<Card, { kind: "confirm" }>;
type Notice = Extract<Card, { kind: "notice" }>;

/** The card's page has painted; the backend shows (and focuses) it now. */
const ready = () =>
  requestAnimationFrame(() => {
    void commands.screenCardReady();
  });

const ConfirmCard: React.FC<{ card: Confirm }> = ({ card }) => {
  const { t } = useTranslation();
  const [enlarged, setEnlarged] = useState(false);
  const [secondsLeft, setSecondsLeft] = useState(card.timeoutSecs);
  const answered = useRef(false);

  const answer = useCallback((send: boolean) => {
    if (answered.current) return;
    answered.current = true;
    void commands.answerScreenCard(send);
  }, []);

  const toggleEnlarged = () => {
    const next = !enlarged;
    setEnlarged(next);
    void commands.setScreenCardEnlarged(next);
  };

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Enter") {
        event.preventDefault();
        answer(true);
      } else if (event.key === "Escape") {
        event.preventDefault();
        answer(false);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [answer]);

  // The backend cancels at the deadline; this only shows how long is left.
  useEffect(() => {
    const timer = setInterval(
      () => setSecondsLeft((s) => Math.max(0, s - 1)),
      1000,
    );
    return () => clearInterval(timer);
  }, []);

  // Keyboard focus inside the page once the window has it.
  useEffect(() => {
    const onFocus = () => document.getElementById("screen-card-send")?.focus();
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, []);

  return (
    <div className="flex h-full flex-col gap-3 bg-background p-4 text-text select-none">
      <div className="flex items-baseline justify-between gap-2">
        <h1 className="text-sm font-semibold">{t("screenCard.title")}</h1>
        <span className="text-xs text-ink-3 tabular-nums">
          {t("screenCard.autoCancel", { seconds: secondsLeft })}
        </span>
      </div>

      <button
        type="button"
        onClick={toggleEnlarged}
        title={enlarged ? t("screenCard.shrink") : t("screenCard.enlarge")}
        className="min-h-0 flex-1 cursor-zoom-in overflow-hidden rounded-md border border-line bg-wash"
        style={{ cursor: enlarged ? "zoom-out" : "zoom-in" }}
        tabIndex={-1}
      >
        <img
          src={card.image}
          alt={t("screenCard.imageAlt")}
          onLoad={ready}
          onError={ready}
          className="h-full w-full object-contain"
          draggable={false}
        />
      </button>

      {!enlarged && (
        <div className="space-y-1">
          <p className="line-clamp-3 text-sm select-text">
            {t("screenCard.question", { question: card.question })}
          </p>
          <p className="text-xs text-ink-2">
            {t("screenCard.sendsTo", {
              provider: card.provider,
              model: card.model,
            })}
          </p>
          <p className="text-xs text-ink-3">{t("screenCard.onlyThis")}</p>
        </div>
      )}

      <div className="flex justify-end gap-2">
        <Button variant="secondary" size="sm" onClick={() => answer(false)}>
          {t("screenCard.cancel")}
        </Button>
        <Button
          id="screen-card-send"
          variant="primary"
          size="sm"
          autoFocus
          onClick={() => answer(true)}
        >
          {t("screenCard.send")}
        </Button>
      </div>
    </div>
  );
};

const NoticeCard: React.FC<{ card: Notice }> = ({ card }) => {
  const { t } = useTranslation();

  useEffect(() => {
    ready();
  }, []);

  return (
    <div className="flex h-full items-start gap-3 bg-background p-4 text-text">
      <AlertTriangle className="mt-0.5 h-5 w-5 shrink-0 text-warning" />
      <div className="min-w-0 flex-1 space-y-1">
        <h1 className="text-sm font-semibold">{t("screenCard.noticeTitle")}</h1>
        <p className="text-sm text-ink-2 select-text">{card.message}</p>
      </div>
      <button
        type="button"
        onClick={() => void commands.dismissScreenCard()}
        title={t("screenCard.close")}
        className="rounded p-1 text-ink-3 hover:bg-wash hover:text-text"
      >
        <X className="h-4 w-4" />
      </button>
    </div>
  );
};

const ScreenCard: React.FC = () => {
  const [card, setCard] = useState<Card | null>(null);

  useEffect(() => {
    commands
      .getScreenCard()
      .then(setCard)
      .catch((e) => console.error("Failed to load the screen card:", e));
  }, []);

  if (!card) return null;
  return card.kind === "confirm" ? (
    <ConfirmCard card={card} />
  ) : (
    <NoticeCard card={card} />
  );
};

export default ScreenCard;
