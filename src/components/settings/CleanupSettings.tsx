import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { commands, type AiCleanupStatus } from "@/bindings";
import { useSettings } from "../../hooks/useSettings";
import { Button } from "../ui/Button";
import { SettingsGroup } from "../ui/SettingsGroup";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { FillerWordRemoval } from "./FillerWordRemoval";
import { PostProcessingToggle } from "./PostProcessingToggle";

/** A sentence on where AI cleanup would send text right now. */
const useAiStatusText = (status: AiCleanupStatus | null) => {
  const { t } = useTranslation();
  if (!status?.provider) return t("settings.cleanup.ai.status.noProvider");
  const provider = status.provider;
  if (status.blocked)
    return t("settings.cleanup.ai.status.blocked", { provider });
  if (!status.has_key)
    return t("settings.cleanup.ai.status.noKey", { provider });
  if (!status.model)
    return t("settings.cleanup.ai.status.noModel", { provider });
  return status.is_local
    ? t("settings.cleanup.ai.status.local", { provider, model: status.model })
    : t("settings.cleanup.ai.status.cloud", { provider, model: status.model });
};

/** Local cleanup, spoken punctuation and AI cleanup, in one place. */
export const CleanupSettings: React.FC<{
  onOpenPostProcessing?: () => void;
}> = ({ onOpenPostProcessing }) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const [status, setStatus] = useState<AiCleanupStatus | null>(null);

  const localCleanup = getSetting("local_cleanup_enabled") ?? true;
  const aiCleanup = getSetting("post_process_enabled") ?? false;
  // The status follows whatever the settings say now.
  const provider = getSetting("post_process_provider_id");
  const privileged = getSetting("privileged_mode");
  const models = getSetting("post_process_models");
  const keys = getSetting("post_process_api_keys");

  useEffect(() => {
    commands.getAiCleanupStatus().then(setStatus);
  }, [aiCleanup, provider, privileged, models, keys]);

  const statusText = useAiStatusText(status);

  return (
    <SettingsGroup
      title={t("settings.cleanup.title")}
      description={t("settings.cleanup.description")}
    >
      <ToggleSwitch
        checked={localCleanup}
        onChange={(enabled) => updateSetting("local_cleanup_enabled", enabled)}
        isUpdating={isUpdating("local_cleanup_enabled")}
        label={t("settings.cleanup.local.label")}
        description={t("settings.cleanup.local.description")}
        descriptionMode="tooltip"
        grouped
      />
      {localCleanup && (
        <>
          <ToggleSwitch
            checked={getSetting("spoken_punctuation_enabled") ?? true}
            onChange={(enabled) =>
              updateSetting("spoken_punctuation_enabled", enabled)
            }
            isUpdating={isUpdating("spoken_punctuation_enabled")}
            label={t("settings.cleanup.spokenPunctuation.label")}
            description={t("settings.cleanup.spokenPunctuation.description")}
            descriptionMode="tooltip"
            grouped
          />
          <FillerWordRemoval descriptionMode="tooltip" grouped={true} />
        </>
      )}
      <PostProcessingToggle descriptionMode="tooltip" grouped={true} />
      {aiCleanup && (
        <div className="px-4 py-3 space-y-2">
          <p className="text-sm text-ink-2">{statusText}</p>
          <p className="text-sm text-ink-3">
            {t("settings.cleanup.ai.recommendLocal")}
          </p>
          <Button variant="secondary" size="sm" onClick={onOpenPostProcessing}>
            {t("settings.cleanup.ai.setUp")}
          </Button>
        </div>
      )}
    </SettingsGroup>
  );
};
