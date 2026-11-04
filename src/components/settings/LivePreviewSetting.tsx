import React from "react";
import { useTranslation } from "react-i18next";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { useSettings } from "../../hooks/useSettings";

/** Kusema live preview: the words in the pill as you speak. */
export const LivePreviewSetting: React.FC<{ grouped?: boolean }> = ({
  grouped = true,
}) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const autoOff = getSetting("live_preview_auto_off") ?? false;
  return (
    <>
      <ToggleSwitch
        checked={getSetting("live_preview") ?? true}
        onChange={(on) => updateSetting("live_preview", on)}
        isUpdating={isUpdating("live_preview")}
        label={t("settings.livePreview.label")}
        description={t("settings.livePreview.description")}
        descriptionMode="tooltip"
        grouped={grouped}
      />
      {autoOff && (
        <p className="m-0 px-4 pb-3 text-xs text-ink-2">
          {t("settings.livePreview.autoOff")}
        </p>
      )}
    </>
  );
};
