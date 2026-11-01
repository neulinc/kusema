import React from "react";
import { Trans, useTranslation } from "react-i18next";
import type { OSType } from "@/lib/utils/keyboard";
import { useSettings } from "@/hooks/useSettings";
import { Button } from "../../ui/Button";
import { Keycaps } from "../../shared/Keycaps";
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
  const done = e.lessons.hold || e.lessons.tap;
  const kind = e.result.kind;

  const status =
    e.phase === "listening"
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
    e.phase === "idle" &&
    (kind === "silent" || kind === "noWords" || kind === "nothingTyped");

  return (
    <SetupScreen
      progress={progress}
      title={t("onboarding.setup.practice.title")}
      // The cursor belongs in the box, so the first try lands there.
      focusTitle={false}
      onBack={onBack}
      hint={download}
      primary={
        <Button
          variant={done ? "primary" : "outline"}
          size="lg"
          onClick={onContinue}
        >
          {t(done ? "onboarding.shell.continue" : "onboarding.try.skip")}
        </Button>
      }
    >
      <p id="practice-how" className="m-0 text-center">
        <Trans
          i18nKey="onboarding.setup.practice.how"
          components={{
            keys: (
              <Keycaps
                binding={e.shortcut}
                osType={os}
                size="line"
                pressed={e.keysDown}
              />
            ),
          }}
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
          i18nKey="onboarding.setup.practice.continuous"
          components={{
            keys: <Keycaps binding={continuous} osType={os} size="line" />,
          }}
        />
      </p>
    </SetupScreen>
  );
};
