import React, { useId } from "react";
import { useTranslation } from "react-i18next";
import type { OSType } from "@/lib/utils/keyboard";
import { useSettings } from "@/hooks/useSettings";
import { Button } from "../../ui/Button";
import { Keycaps } from "../../shared/Keycaps";
import { useOnboardingStore } from "../onboardingStore";
import { SetupScreen } from "./SetupScreen";

/** Label key under onboarding.setup.done.keys, and the shortcut's id. */
const HOTKEYS = [
  ["dictate", "transcribe"],
  ["continuous", "dictate_continuous"],
  ["ask", "assistant"],
  ["askScreen", "assistant_screen"],
] as const;

interface SetupDoneProps {
  progress: { index: number; count: number };
  os: OSType;
  download: React.ReactNode;
  onBack: () => void;
  onDone: () => void;
}

/** The keys worth remembering, on one card. */
export const SetupDone: React.FC<SetupDoneProps> = ({
  progress,
  os,
  download,
  onBack,
  onDone,
}) => {
  const { t } = useTranslation();
  const { getSetting } = useSettings();
  const { atLogin, set } = useOnboardingStore();
  const bindings = getSetting("bindings");
  const atLoginId = useId();

  return (
    <SetupScreen
      progress={progress}
      title={t("onboarding.setup.done.title")}
      sub={t("onboarding.setup.done.sub")}
      onBack={onBack}
      hint={download}
      primary={
        <Button variant="primary" size="lg" onClick={onDone}>
          {t("onboarding.setup.done.finish")}
        </Button>
      }
    >
      <dl className="m-0 flex flex-col divide-y divide-line rounded-lg border border-line">
        {HOTKEYS.map(([label, id]) => (
          <div
            key={id}
            className="flex flex-wrap items-center justify-between gap-x-[16px] gap-y-[4px] px-[14px] py-[10px]"
          >
            <dt className="font-medium">
              {t(`onboarding.setup.done.keys.${label}`)}
            </dt>
            <dd className="m-0">
              <Keycaps
                binding={bindings?.[id]?.current_binding ?? ""}
                osType={os}
                size="line"
              />
            </dd>
          </div>
        ))}
      </dl>
      <p className="m-0 text-small text-ink-2">
        {t("onboarding.setup.done.wins")}
      </p>
      <label
        htmlFor={atLoginId}
        className="flex cursor-pointer items-center gap-[10px]"
      >
        <input
          id={atLoginId}
          type="checkbox"
          className="size-[16px] shrink-0 accent-background-ui focus:outline-none focus-visible:ring-2 focus-visible:ring-logo-primary"
          checked={atLogin}
          onChange={(ev) => set({ atLogin: ev.target.checked })}
        />
        {t("onboarding.setup.done.atLogin")}
      </label>
    </SetupScreen>
  );
};
