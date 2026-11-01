import React from "react";
import { useTranslation } from "react-i18next";
import { Shield } from "lucide-react";
import { useSettings } from "../../../hooks/useSettings";
import { ToggleSwitch } from "../../ui/ToggleSwitch";

/** The badge shown wherever it matters that Privileged mode is on. */
export const PrivilegedChip: React.FC = () => {
  const { t } = useTranslation();
  return (
    <span className="inline-flex items-center gap-1 rounded-full bg-logo-primary/20 px-2 py-0.5 text-xs font-medium">
      <Shield width={12} height={12} />
      {t("settings.privacy.privileged.chip")}
    </span>
  );
};

/** The one switch, here and on the Assistant page. */
export const PrivilegedToggle: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  return (
    <ToggleSwitch
      checked={getSetting("privileged_mode") ?? false}
      onChange={(enabled) => updateSetting("privileged_mode", enabled)}
      isUpdating={isUpdating("privileged_mode")}
      label={t("settings.privacy.privileged.toggle.label")}
      description={t("settings.privacy.privileged.toggle.description")}
      descriptionMode="tooltip"
      grouped
    />
  );
};
