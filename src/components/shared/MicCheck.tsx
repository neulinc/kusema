import React, { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { commands, type MicReport } from "@/bindings";
import { Button } from "../ui/Button";
import { QUIET_SECONDS, READ_SECONDS, tipKeys } from "@/lib/micCheck";

type Phase = "idle" | "quiet" | "reading" | "result" | "failed";

interface MicCheckProps {
  /** The check is over (or closed): the caller's level meter can reopen. */
  onFinished?: () => void;
}

/** Kusema microphone check: a few seconds of quiet, a sentence read aloud,
 * then what would help, in plain words. Measured on this computer; the
 * audio is kept only in memory, for "Play my test back". */
export const MicCheck: React.FC<MicCheckProps> = ({ onFinished }) => {
  const { t } = useTranslation();
  const [phase, setPhase] = useState<Phase>("idle");
  const [left, setLeft] = useState(QUIET_SECONDS);
  const [level, setLevel] = useState(0);
  const [report, setReport] = useState<MicReport | null>(null);
  const [adapted, setAdapted] = useState(false);
  const timers = useRef<ReturnType<typeof setTimeout>[]>([]);
  const finishing = useRef(false);

  const clearTimers = () => {
    timers.current.forEach(clearTimeout);
    timers.current = [];
  };

  // Closing the page mid-check stops it and forgets the audio.
  useEffect(
    () => () => {
      clearTimers();
      void commands.cancelMicCheck();
    },
    [],
  );

  useEffect(() => {
    if (phase !== "quiet" && phase !== "reading") return;
    const off = listen<number>("mic-test-level", ({ payload }) =>
      setLevel(payload),
    );
    return () => {
      void off.then((stop) => stop());
    };
  }, [phase]);

  const finish = async () => {
    if (finishing.current) return;
    finishing.current = true;
    clearTimers();
    const result = await commands.finishMicCheck();
    finishing.current = false;
    if (result.status === "ok") {
      setReport(result.data);
      const status = await commands.getMicProfile();
      setAdapted(status.gate_db !== null);
      setPhase("result");
    } else {
      setPhase("failed");
    }
    onFinished?.();
  };

  const start = async () => {
    clearTimers();
    setReport(null);
    setLevel(0);
    const result = await commands.startMicCheck();
    if (result.status === "error") {
      setPhase("failed");
      return;
    }
    setPhase("quiet");
    setLeft(QUIET_SECONDS);
    for (let s = 1; s < QUIET_SECONDS; s++) {
      timers.current.push(
        setTimeout(() => setLeft(QUIET_SECONDS - s), s * 1000),
      );
    }
    timers.current.push(
      setTimeout(() => {
        void commands.markMicCheckSpeech();
        setPhase("reading");
        timers.current.push(
          setTimeout(() => void finish(), READ_SECONDS * 1000),
        );
      }, QUIET_SECONDS * 1000),
    );
  };

  const cancel = () => {
    clearTimers();
    void commands.cancelMicCheck();
    setPhase("idle");
    onFinished?.();
  };

  const meter = (
    <div
      role="meter"
      aria-label={t("micCheck.meter")}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={Math.round(level * 10) * 10}
      className="h-[8px] w-full overflow-hidden rounded-full border border-line bg-wash"
    >
      <div
        className="h-full bg-background-ui"
        style={{ width: `${Math.round(level * 100)}%` }}
      />
    </div>
  );

  return (
    <div className="flex flex-col gap-3 px-4 py-3" aria-live="polite">
      {phase === "idle" && (
        <>
          <p className="m-0 text-sm text-ink-2">{t("micCheck.intro")}</p>
          <div>
            <Button variant="secondary" size="sm" onClick={() => void start()}>
              {t("micCheck.start")}
            </Button>
          </div>
        </>
      )}

      {phase === "quiet" && (
        <>
          <p className="m-0 text-sm font-medium" role="status">
            {t("micCheck.quiet", { seconds: left })}
          </p>
          {meter}
          <div>
            <Button variant="secondary" size="sm" onClick={cancel}>
              {t("micCheck.cancel")}
            </Button>
          </div>
        </>
      )}

      {phase === "reading" && (
        <>
          <p className="m-0 text-sm font-medium" role="status">
            {t("micCheck.read")}
          </p>
          <p className="m-0 rounded-lg bg-wash px-3 py-2 text-base">
            {t("micCheck.sentence")}
          </p>
          {meter}
          <div className="flex gap-2">
            <Button variant="primary" size="sm" onClick={() => void finish()}>
              {t("micCheck.done")}
            </Button>
            <Button variant="secondary" size="sm" onClick={cancel}>
              {t("micCheck.cancel")}
            </Button>
          </div>
        </>
      )}

      {phase === "result" && report && (
        <>
          <ul className="m-0 flex list-none flex-col gap-1 p-0" role="status">
            {tipKeys(report.tips).map((key) => (
              <li key={key} className="text-sm">
                {t(key)}
              </li>
            ))}
          </ul>
          <p className="m-0 text-xs text-ink-3">
            {t("micCheck.numbers", {
              noise: Math.round(report.noise_db),
              speech: Math.round(report.speech_db),
              snr: Math.round(report.snr_db),
            })}
          </p>
          {adapted && (
            <p className="m-0 text-xs text-ink-2">{t("micCheck.adapted")}</p>
          )}
          <div className="flex flex-wrap gap-2">
            <Button
              variant="secondary"
              size="sm"
              onClick={() => void commands.playMicCheck()}
            >
              {t("micCheck.play")}
            </Button>
            <Button variant="secondary" size="sm" onClick={() => void start()}>
              {t("micCheck.again")}
            </Button>
          </div>
        </>
      )}

      {phase === "failed" && (
        <>
          <p className="m-0 text-sm text-warning" role="status">
            {t("micCheck.failed")}
          </p>
          <div>
            <Button variant="secondary" size="sm" onClick={() => void start()}>
              {t("micCheck.again")}
            </Button>
          </div>
        </>
      )}

      <details className="text-xs text-ink-2">
        <summary className="cursor-pointer">{t("micCheck.anyMic")}</summary>
        <ul className="mt-1 mb-0 ps-4">
          <li>{t("micCheck.look.boom")}</li>
          <li>{t("micCheck.look.usb")}</li>
          <li>{t("micCheck.look.wired")}</li>
        </ul>
      </details>
    </div>
  );
};
