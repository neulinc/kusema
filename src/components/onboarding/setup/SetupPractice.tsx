import React, { useEffect, useRef, useState } from "react";
import { Trans, useTranslation } from "react-i18next";
import { events } from "@/bindings";
import { keycapLabel, type OSType } from "@/lib/utils/keyboard";
import { useSettings } from "@/hooks/useSettings";
import { Button } from "../../ui/Button";
import { Keycaps } from "../../shared/Keycaps";
import { keyLabel } from "@/lib/kusemaKey";
import { useTryEngine } from "../useTryEngine";
import { SetupScreen } from "./SetupScreen";

interface SetupPracticeProps {
  progress: { index: number; count: number };
  os: OSType;
  /** The speech model is on disk. */
  ready: boolean;
  download: React.ReactNode;
  onBack: () => void;
  onContinue: () => void;
}

/** The first real dictation, into a box on the screen. */
export const SetupPractice: React.FC<SetupPracticeProps> = ({
  progress,
  os,
  ready,
  download,
  onBack,
  onContinue,
}) => {
  const { t } = useTranslation();
  const e = useTryEngine({ os });
  const { getSetting } = useSettings();
  const continuous =
    getSetting("bindings")?.dictate_continuous?.current_binding ?? "";
  // Kusema: the Kusema key is what's practised, unless it's off.
  const useKey = getSetting("kusema_key_enabled") ?? true;
  const keyName = keyLabel(getSetting("kusema_key"), t);
  const keyCaps = (pressed?: boolean) =>
    useKey ? (
      <Keycaps
        binding=""
        labels={[keyName]}
        osType={os}
        size="line"
        pressed={pressed}
      />
    ) : (
      <Keycaps
        binding={pressed === undefined ? continuous : e.shortcut}
        osType={os}
        size="line"
        pressed={pressed}
      />
    );
  const kind = e.result.kind;

  // Continuous dictation never types into Kusema's own windows, but each piece
  // lands in its History entry, so the box shows that instead.
  const [continuousTry, setContinuousTry] = useState<
    "none" | "running" | "ended"
  >("none");
  const [continuousWords, setContinuousWords] = useState(false);
  const running = useRef(false);
  const done = e.lessons.hold || e.lessons.tap || continuousWords;
  useEffect(() => {
    const offStatus = events.dictationStatusEvent.listen(({ payload }) => {
      running.current = payload.phase !== "ended";
      setContinuousTry(running.current ? "running" : "ended");
    });
    const offHistory = events.historyUpdatePayload.listen(({ payload }) => {
      if (!running.current || !e.field.current) return;
      if (payload.action !== "added" && payload.action !== "updated") return;
      e.field.current.value = payload.entry.transcription_text;
      setContinuousWords(true);
    });
    // A hold after a session has ended is the practice this screen asks for.
    const offState = events.recordingStateEvent.listen(({ payload }) => {
      if (payload.phase === "started")
        setContinuousTry((was) => (was === "ended" ? "none" : was));
    });
    return () => {
      offStatus.then((off) => off());
      offHistory.then((off) => off());
      offState.then((off) => off());
    };
  }, []);
  const status =
    continuousTry === "running"
      ? useKey
        ? t("onboarding.setup.practice.continuousRunningKey", { key: keyName })
        : t("onboarding.setup.practice.continuousRunning", {
            stop: keycapLabel(continuous, os),
          })
      : continuousTry === "ended"
        ? t("onboarding.setup.practice.continuousEnded")
        : e.phase === "listening"
          ? t("onboarding.setup.practice.listening")
          : e.phase === "typing"
            ? t("onboarding.setup.practice.typing")
            : kind === "silent" || kind === "noWords" || kind === "nothingTyped"
              ? `${t(`onboarding.try.${kind}`)} ${t(`onboarding.try.${kind}Sub`)}`
              : done
                ? t("onboarding.setup.practice.success")
                : !ready
                  ? t("onboarding.setup.practice.waiting")
                  : "";
  const problem =
    continuousTry === "none" &&
    e.phase === "idle" &&
    (kind === "silent" || kind === "noWords" || kind === "nothingTyped");
  // Moving on would leave it listening, ready to type into the next window.
  const locked = continuousTry === "running";

  return (
    <SetupScreen
      progress={progress}
      title={t("onboarding.setup.practice.title")}
      // The cursor belongs in the box, so the first try lands there.
      focusTitle={false}
      onBack={locked ? undefined : onBack}
      hint={download}
      primary={
        <Button
          variant={done ? "primary" : "outline"}
          size="lg"
          onClick={onContinue}
          disabled={locked}
        >
          {t(done ? "onboarding.shell.continue" : "onboarding.try.skip")}
        </Button>
      }
    >
      <p id="practice-how" className="m-0 text-center">
        <Trans
          i18nKey="onboarding.setup.practice.how"
          components={{ keys: keyCaps(e.keysDown) }}
        />
      </p>
      <blockquote
        id="practice-sentence"
        className="m-0 rounded-lg bg-wash px-[12px] py-[10px] text-center font-medium"
      >
        {t("onboarding.setup.practice.sentence")}
      </blockquote>
      <textarea
        ref={e.field}
        rows={3}
        aria-label={t("onboarding.setup.practice.box")}
        aria-describedby="practice-how practice-sentence"
        placeholder={t("onboarding.setup.practice.placeholder")}
        spellCheck={false}
        className="w-full resize-none rounded-lg border border-line-strong bg-background px-[12px] py-[10px] text-body text-text select-text focus:outline-none focus-visible:ring-2 focus-visible:ring-logo-primary"
      />
      <p
        role="status"
        className={`m-0 min-h-[1.6em] text-small ${problem ? "text-warning" : done ? "text-moss" : "text-ink-2"}`}
      >
        {status}
      </p>
      <p className="m-0 text-small text-ink-2">
        <Trans
          i18nKey={
            useKey
              ? "onboarding.setup.practice.continuousKey"
              : "onboarding.setup.practice.continuous"
          }
          components={{ keys: keyCaps() }}
        />
      </p>
    </SetupScreen>
  );
};
