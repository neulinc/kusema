import React from "react";
import { useTranslation } from "react-i18next";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { useSettings } from "../../hooks/useSettings";
import { MicCheck } from "../shared/MicCheck";

/** Settings → Sound: the microphone check, and its use against room noise. */
export const MicCheckSettings: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  return (
    <>
      <div>
        <div className="px-4 pt-3">
          <h3 className="m-0 text-sm font-medium">{t("micCheck.title")}</h3>
        </div>
        <MicCheck />
      </div>
      <ToggleSwitch
        checked={getSetting("adaptive_vad") ?? true}
        onChange={(on) => updateSetting("adaptive_vad", on)}
        isUpdating={isUpdating("adaptive_vad")}
        label={t("micCheck.adaptive.label")}
        description={t("micCheck.adaptive.description")}
        descriptionMode="tooltip"
        grouped
      />
    </>
  );
};
