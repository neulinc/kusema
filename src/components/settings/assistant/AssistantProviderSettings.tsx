import React from "react";
import { useTranslation } from "react-i18next";
import { useSettings } from "../../../hooks/useSettings";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { SettingContainer } from "../../ui/SettingContainer";
import { Button } from "../../ui/Button";
import { Alert } from "../../ui/Alert";

/** Providers that run locally or behind a gateway and may not need a key. */
const KEY_OPTIONAL_PROVIDERS = ["custom"];

/** The provider and model Assistant mode uses (shared with Post Process),
 * with a shortcut to where they're configured. */
export const AssistantProviderSettings: React.FC<{
  onOpenPostProcessing: () => void;
}> = ({ onOpenPostProcessing }) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();

  const enabled = getSetting("post_process_enabled") ?? false;
  const providerId = getSetting("post_process_provider_id") ?? "";
  const provider = (getSetting("post_process_providers") ?? []).find(
    (p) => p.id === providerId,
  );
  const model = getSetting("post_process_models")?.[providerId] ?? "";
  const hasKey = !!getSetting("post_process_api_keys")?.[providerId];
  const needsKey = !KEY_OPTIONAL_PROVIDERS.includes(providerId);

  if (!enabled) {
    return (
      <SettingsGroup title={t("settings.assistant.provider.title")}>
        <div className="flex flex-col gap-3 p-4">
          <p className="text-sm">{t("settings.assistant.provider.disabled")}</p>
          <div>
            <Button
              variant="primary"
              size="sm"
              onClick={() => updateSetting("post_process_enabled", true)}
              disabled={isUpdating("post_process_enabled")}
            >
              {t("settings.assistant.provider.enable")}
            </Button>
          </div>
        </div>
      </SettingsGroup>
    );
  }

  const problem = !provider
    ? t("settings.assistant.provider.noProvider")
    : needsKey && !hasKey
      ? t("settings.assistant.provider.noKey", { provider: provider.label })
      : !model
        ? t("settings.assistant.provider.noModel")
        : null;

  return (
    <SettingsGroup title={t("settings.assistant.provider.title")}>
      <SettingContainer
        title={t("settings.assistant.provider.provider")}
        description={t("settings.assistant.provider.description")}
        descriptionMode="tooltip"
        grouped
      >
        <span className="text-sm">{provider?.label ?? providerId}</span>
      </SettingContainer>
      <SettingContainer
        title={t("settings.assistant.provider.model")}
        description={t("settings.assistant.provider.modelDescription")}
        descriptionMode="tooltip"
        grouped
      >
        <span className="text-sm select-text">
          {model || t("settings.assistant.provider.notSet")}
        </span>
      </SettingContainer>
      {problem && (
        <div className="px-4 pt-3">
          <Alert variant="warning">{problem}</Alert>
        </div>
      )}
      <div className="flex justify-end p-3">
        <Button variant="secondary" size="sm" onClick={onOpenPostProcessing}>
          {t("settings.assistant.provider.open")}
        </Button>
      </div>
    </SettingsGroup>
  );
};
