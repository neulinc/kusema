import React, { useEffect, useId, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { commands } from "@/bindings";
import { useSettings } from "@/hooks/useSettings";
import { Button } from "../../ui/Button";
import { useOnboardingStore } from "../onboardingStore";
import { FIELD_CLASS, SetupScreen } from "./SetupScreen";
import { MicCheck } from "../../shared/MicCheck";

/** On the meter's 0 to 1 scale: louder than a quiet room, about speech level. */
const HEARD_LEVEL = 0.35;
/** Silent for this long: say so. */
const QUIET_AFTER_MS = 6000;

interface SetupMicrophoneProps {
  progress: { index: number; count: number };
  download: React.ReactNode;
  onBack: () => void;
  onContinue: () => void;
}

/** Pick a microphone and see it working. */
export const SetupMicrophone: React.FC<SetupMicrophoneProps> = ({
  progress,
  download,
  onBack,
  onContinue,
}) => {
  const { t } = useTranslation();
  const { audioDevices, refreshAudioDevices, getSetting, updateSetting } =
    useSettings();
  const preview = useOnboardingStore((s) => s.preview);
  const fieldId = useId();
  const [level, setLevel] = useState(0);
  const [heard, setHeard] = useState(false);
  const [quiet, setQuiet] = useState(false);
  const [failed, setFailed] = useState(false);
  // Bumped to reopen the meter on the newly picked microphone.
  const [opened, setOpened] = useState(0);

  const saved = getSetting("selected_microphone");
  const selected = !saved || saved === "default" ? "Default" : saved;

  // The list isn't loaded until onboarding ends.
  useEffect(() => {
    if (!preview) refreshAudioDevices();
  }, [preview, refreshAudioDevices]);

  useEffect(() => {
    if (preview) return;
    let gone = false;
    setHeard(false);
    setQuiet(false);
    setFailed(false);
    setLevel(0);
    const off = listen<number>("mic-test-level", ({ payload }) => {
      setLevel(payload);
      if (payload >= HEARD_LEVEL) {
        setHeard(true);
        setQuiet(false);
      }
    });
    commands.startMicTest().then((result) => {
      if (!gone && result.status === "error") setFailed(true);
    });
    const timer = setTimeout(
      () => setHeard((was) => (setQuiet(!was), was)),
      QUIET_AFTER_MS,
    );
    return () => {
      gone = true;
      clearTimeout(timer);
      off.then((stop) => stop());
    };
  }, [preview, opened]);

  // Only on leaving: a new microphone replaces the running meter by itself.
  useEffect(() => {
    if (preview) return;
    return () => {
      commands.stopMicTest();
    };
  }, [preview]);

  const pick = async (name: string) => {
    await updateSetting("selected_microphone", name);
    setOpened((n) => n + 1);
  };

  const percent = Math.round(level * 100);
  const status = failed
    ? t("onboarding.setup.microphone.failed")
    : heard
      ? t("onboarding.setup.microphone.heard")
      : quiet
        ? t("onboarding.setup.microphone.quiet")
        : "";

  return (
    <SetupScreen
      progress={progress}
      title={t("onboarding.setup.microphone.title")}
      sub={t("onboarding.setup.microphone.sub")}
      onBack={onBack}
      hint={download}
      primary={
        <Button variant="primary" size="lg" onClick={onContinue}>
          {t("onboarding.shell.continue")}
        </Button>
      }
    >
      <div className="flex flex-col gap-[6px]">
        <label htmlFor={fieldId} className="font-medium">
          {t("onboarding.setup.microphone.label")}
        </label>
        <select
          id={fieldId}
          className={FIELD_CLASS}
          value={selected}
          onChange={(ev) => pick(ev.target.value)}
        >
          {/* Until the list loads, so the selection always has an option. */}
          {!audioDevices.some((d) => d.name === selected) && (
            <option value={selected}>{selected}</option>
          )}
          {audioDevices.map((device) => (
            <option key={device.index} value={device.name}>
              {device.name}
            </option>
          ))}
        </select>
      </div>
      <div
        role="meter"
        aria-label={t("onboarding.setup.microphone.meter")}
        aria-valuemin={0}
        aria-valuemax={100}
        // Tens, so a screen reader isn't told about every flicker.
        aria-valuenow={Math.round(percent / 10) * 10}
        className="h-[12px] w-full overflow-hidden rounded-full border border-line bg-wash"
      >
        <div
          className="h-full bg-background-ui"
          style={{ width: `${percent}%` }}
        />
      </div>
      <p
        role="status"
        className={`m-0 min-h-[1.6em] text-small ${failed || quiet ? "text-warning" : "text-moss"}`}
      >
        {status}
      </p>
      <p className="m-0 rounded-lg bg-wash px-[12px] py-[10px] text-small text-ink-2">
        {t("onboarding.setup.microphone.tip")}
      </p>
      {/* Kusema: the microphone check; the meter reopens when it's done. */}
      {!preview && (
        <div className="-mx-4">
          <MicCheck onFinished={() => setOpened((n) => n + 1)} />
        </div>
      )}
    </SetupScreen>
  );
};
