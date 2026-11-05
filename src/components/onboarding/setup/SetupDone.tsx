import React, { useId } from "react";
import { useTranslation } from "react-i18next";
import type { OSType } from "@/lib/utils/keyboard";
import { useSettings } from "@/hooks/useSettings";
import { askBoxCustomized, hiddenShortcut } from "@/lib/kusemaKey";
import { Button } from "../../ui/Button";
import { Keycaps } from "../../shared/Keycaps";
import { KusemaKeysCard } from "../../shared/KusemaKeysCard";
import { useOnboardingStore } from "../onboardingStore";
import { SetupScreen } from "./SetupScreen";

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
      <KusemaKeysCard />
      {!hiddenShortcut(
        "ask_box",
        getSetting("classic_shortcuts") ?? false,
        askBoxCustomized(bindings?.ask_box),
      ) && (
        <p className="m-0 flex flex-wrap items-center gap-x-[8px] gap-y-[4px] text-small">
          {t("onboarding.setup.done.typeToKusema")}
          <Keycaps
            binding={bindings?.ask_box?.current_binding ?? ""}
            osType={os}
            size="line"
          />
        </p>
      )}
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
