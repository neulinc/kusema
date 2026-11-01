import React from "react";
import { useTranslation } from "react-i18next";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { useSettings } from "../../hooks/useSettings";

interface ShowAllModelsProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

/** Swaps the Models page's short list for every model. */
export const ShowAllModels: React.FC<ShowAllModelsProps> = React.memo(
  ({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting, isUpdating } = useSettings();
    return (
      <ToggleSwitch
        checked={getSetting("show_all_models") ?? false}
        onChange={(enabled) => updateSetting("show_all_models", enabled)}
        isUpdating={isUpdating("show_all_models")}
        label={t("settings.advanced.showAllModels.label")}
        description={t("settings.advanced.showAllModels.description")}
        descriptionMode={descriptionMode}
        grouped={grouped}
      />
    );
  },
);
