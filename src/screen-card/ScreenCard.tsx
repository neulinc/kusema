import React, { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { AlertTriangle, X } from "lucide-react";
import { commands, type ScreenCard as Card } from "@/bindings";
import { Button } from "@/components/ui/Button";

type Confirm = Extract<Card, { kind: "confirm" }>;
type Notice = Extract<Card, { kind: "notice" }>;
type Edit = Extract<Card, { kind: "edit" }>;
type Correct = Extract<Card, { kind: "correct" }>;
type Offer = Extract<Card, { kind: "offer" }>;

/** Kusema key: add a misheard first word as a way the trigger is heard? */
const OfferCard: React.FC<{ card: Offer }> = ({ card }) => {
  const { t } = useTranslation();
  const answered = useRef(false);
  const answer = useCallback((add: boolean) => {
    if (answered.current) return;
    answered.current = true;
    void commands.answerScreenCard(add);
  }, []);
  useEffect(() => {
    ready();
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
  return (
    <div className="flex h-full flex-col gap-3 bg-background p-4 text-text">
      <h1 className="m-0 text-base font-semibold">
        {t("screenCard.offer.title", {
          word: card.word,
          trigger: card.trigger,
        })}
      </h1>
      <p className="m-0 text-sm text-ink-2">
        {t("screenCard.offer.body", {
          word: card.word,
          trigger: card.trigger,
        })}
      </p>
      <div className="mt-auto flex justify-end gap-2">
        <Button variant="secondary" size="sm" onClick={() => answer(false)}>
          {t("screenCard.offer.no")}
        </Button>
        <Button
          variant="primary"
          size="sm"
          autoFocus
          onClick={() => answer(true)}
        >
          {t("screenCard.offer.yes")}
        </Button>
      </div>
    </div>
  );
};

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
        <p className="text-sm text-ink-2 select-text">
          {card.key ? t(card.key, { name: card.name ?? "" }) : card.message}
        </p>
        {card.setup && (
          <Button
            variant="secondary"
            size="sm"
            onClick={() => {
              void commands.openSetupPage(card.setup!);
              void commands.dismissScreenCard();
            }}
          >
            {t("overlay.setUp")}
          </Button>
        )}
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

/** Kusema voice editing: the selection and the AI's version of it. */
const EditCard: React.FC<{ card: Edit }> = ({ card }) => {
  const { t } = useTranslation();
  const [secondsLeft, setSecondsLeft] = useState(card.timeoutSecs);
  const answered = useRef(false);

  const answer = useCallback((apply: boolean) => {
    if (answered.current) return;
    answered.current = true;
    void commands.answerScreenCard(apply);
  }, []);

  useEffect(() => {
    ready();
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
    const timer = setInterval(
      () => setSecondsLeft((s) => Math.max(0, s - 1)),
      1000,
    );
    const onFocus = () => document.getElementById("edit-card-apply")?.focus();
    window.addEventListener("focus", onFocus);
    return () => {
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("focus", onFocus);
      clearInterval(timer);
    };
  }, [answer]);

  const block = (label: string, text: string, strong: boolean) => (
    <section className="flex min-h-0 flex-1 flex-col gap-1">
      <h2 className="text-xs font-medium uppercase tracking-wide text-ink-3">
        {label}
      </h2>
      <div
        className={`min-h-0 flex-1 overflow-auto whitespace-pre-wrap rounded-md border border-line p-2 text-sm select-text ${
          strong ? "bg-background" : "bg-wash text-ink-2"
        }`}
      >
        {text}
      </div>
    </section>
  );

  return (
    <div className="flex h-full flex-col gap-3 bg-background p-4 text-text select-none">
      <div className="flex items-baseline justify-between gap-2">
        <h1 className="text-sm font-semibold">{t("screenCard.edit.title")}</h1>
        <span className="text-xs text-ink-3 tabular-nums">
          {t("screenCard.autoCancel", { seconds: secondsLeft })}
        </span>
      </div>
      <p className="line-clamp-2 text-sm select-text">
        {t("screenCard.edit.instruction", { instruction: card.instruction })}
      </p>
      {block(t("screenCard.edit.original"), card.original, false)}
      {block(t("screenCard.edit.replacement"), card.replacement, true)}
      <div className="space-y-1">
        <p className="text-xs text-ink-2">
          {t("screenCard.edit.by", { provider: card.provider })}
        </p>
        {card.richText && (
          <p className="text-xs text-warning">
            {t("screenCard.edit.richText")}
          </p>
        )}
        <p className="text-xs text-ink-3">{t("screenCard.edit.keys")}</p>
      </div>
      <div className="flex justify-end gap-2">
        <Button variant="secondary" size="sm" onClick={() => answer(false)}>
          {t("screenCard.edit.cancel")}
        </Button>
        <Button
          id="edit-card-apply"
          variant="primary"
          size="sm"
          autoFocus
          onClick={() => answer(true)}
        >
          {t("screenCard.edit.apply")}
        </Button>
      </div>
    </div>
  );
};

/** Kusema vocabulary: the last dictated text, to fix by hand. */
const CorrectCard: React.FC<{ card: Correct }> = ({ card }) => {
  const { t } = useTranslation();
  const [text, setText] = useState(card.text);
  const [secondsLeft, setSecondsLeft] = useState(card.timeoutSecs);
  const answered = useRef(false);
  const boxRef = useRef<HTMLTextAreaElement>(null);

  const answer = useCallback((value: string | null) => {
    if (answered.current) return;
    answered.current = true;
    void commands.answerCorrectionCard(value);
  }, []);

  useEffect(() => {
    ready();
    const timer = setInterval(
      () => setSecondsLeft((s) => Math.max(0, s - 1)),
      1000,
    );
    const onFocus = () => {
      const box = boxRef.current;
      if (box && document.activeElement !== box) {
        box.focus();
        box.setSelectionRange(box.value.length, box.value.length);
      }
    };
    window.addEventListener("focus", onFocus);
    return () => {
      clearInterval(timer);
      window.removeEventListener("focus", onFocus);
    };
  }, []);

  return (
    <div className="flex h-full flex-col gap-3 bg-background p-4 text-text">
      <div className="flex items-baseline justify-between gap-2">
        <h1 className="text-sm font-semibold">
          {t("screenCard.correct.title")}
        </h1>
        <span className="text-xs text-ink-3 tabular-nums">
          {t("screenCard.autoCancel", { seconds: secondsLeft })}
        </span>
      </div>
      <textarea
        ref={boxRef}
        value={text}
        autoFocus
        aria-label={t("screenCard.correct.title")}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && !e.shiftKey) {
            e.preventDefault();
            answer(text);
          } else if (e.key === "Escape") {
            e.preventDefault();
            answer(null);
          }
        }}
        className="min-h-0 flex-1 resize-none rounded-md border border-line bg-background p-2 text-sm focus:outline-none focus:ring-1 focus:ring-logo-primary"
      />
      <p className="text-xs text-ink-3">{t("screenCard.correct.help")}</p>
      <div className="flex justify-end gap-2">
        <Button variant="secondary" size="sm" onClick={() => answer(null)}>
          {t("screenCard.correct.cancel")}
        </Button>
        <Button variant="primary" size="sm" onClick={() => answer(text)}>
          {t("screenCard.correct.save")}
        </Button>
      </div>
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
  if (card.kind === "edit") return <EditCard card={card} />;
  if (card.kind === "correct") return <CorrectCard card={card} />;
  if (card.kind === "offer") return <OfferCard card={card} />;
  return card.kind === "confirm" ? (
    <ConfirmCard card={card} />
  ) : (
    <NoticeCard card={card} />
  );
};

export default ScreenCard;
