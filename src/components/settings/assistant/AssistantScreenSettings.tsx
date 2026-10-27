import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { useSettings } from "../../../hooks/useSettings";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { SettingContainer } from "../../ui/SettingContainer";
import { ToggleSwitch } from "../../ui/ToggleSwitch";
import { Dropdown } from "../../ui/Dropdown";
import { Input } from "../../ui/Input";
import { Alert } from "../../ui/Alert";
import { ShortcutInput } from "../ShortcutInput";

/** Providers that run locally or behind a gateway and may not need a key. */
const KEY_OPTIONAL_PROVIDERS = ["custom"];
/** Can't take images (or run Assistant mode at all). */
const UNSUPPORTED_PROVIDERS = ["apple_intelligence"];
const SAME_AS_ASSISTANT = "";

/** "Ask about my screen": its shortcut, the confirmation card, and which
 * provider screenshots go to. */
export const AssistantScreenSettings: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();

  const confirm = getSetting("assistant_screen_confirm") ?? true;
  const screenProviderId = getSetting("assistant_screen_provider_id") ?? null;
  const screenModel = getSetting("assistant_screen_model") ?? null;
  const providers = getSetting("post_process_providers") ?? [];
  const models = getSetting("post_process_models") ?? {};
  const keys = getSetting("post_process_api_keys") ?? {};
  const assistantProvider = providers.find(
    (p) => p.id === getSetting("post_process_provider_id"),
  );

  const screenProvider = providers.find((p) => p.id === screenProviderId);
  const effectiveProvider = screenProvider ?? assistantProvider;
  const [modelDraft, setModelDraft] = useState(screenModel ?? "");
  useEffect(() => setModelDraft(screenModel ?? ""), [screenModel]);

  const providerOptions = [
    {
      value: SAME_AS_ASSISTANT,
      label: assistantProvider
        ? t("settings.assistant.screen.provider.sameNamed", {
            provider: assistantProvider.label,
          })
        : t("settings.assistant.screen.provider.same"),
    },
    ...providers
      .filter((p) => !UNSUPPORTED_PROVIDERS.includes(p.id))
      .map((p) => ({ value: p.id, label: p.label })),
  ];

  const missingKey =
    screenProvider &&
    !KEY_OPTIONAL_PROVIDERS.includes(screenProvider.id) &&
    !keys[screenProvider.id];

  const saveModel = () => {
    const trimmed = modelDraft.trim();
    if (trimmed !== (screenModel ?? "")) {
      updateSetting("assistant_screen_model", trimmed === "" ? null : trimmed);
    }
  };

  return (
    <SettingsGroup
      title={t("settings.assistant.screen.title")}
      description={t("settings.assistant.screen.description")}
    >
      <ShortcutInput shortcutId="assistant_screen" grouped={true} />
      <ToggleSwitch
        checked={confirm}
        onChange={(enabled) =>
          updateSetting("assistant_screen_confirm", enabled)
        }
        isUpdating={isUpdating("assistant_screen_confirm")}
        label={t("settings.assistant.screen.confirm.label")}
        description={t("settings.assistant.screen.confirm.description")}
        descriptionMode="tooltip"
        grouped
      />
      {!confirm && (
        <div className="px-4 py-3">
          <Alert variant="warning">
            {t("settings.assistant.screen.confirm.offWarning", {
              provider:
                effectiveProvider?.label ??
                t("settings.assistant.screen.provider.yourProvider"),
            })}
          </Alert>
        </div>
      )}
      <SettingContainer
        title={t("settings.assistant.screen.provider.label")}
        description={t("settings.assistant.screen.provider.description")}
        descriptionMode="tooltip"
        grouped
      >
        <Dropdown
          options={providerOptions}
          selectedValue={screenProvider ? screenProvider.id : SAME_AS_ASSISTANT}
          onSelect={(value) =>
            updateSetting(
              "assistant_screen_provider_id",
              value === SAME_AS_ASSISTANT ? null : value,
            )
          }
          disabled={isUpdating("assistant_screen_provider_id")}
        />
      </SettingContainer>
      {screenProvider && (
        <SettingContainer
          title={t("settings.assistant.screen.model.label")}
          description={t("settings.assistant.screen.model.description")}
          descriptionMode="tooltip"
          grouped
        >
          <Input
            variant="compact"
            value={modelDraft}
            onChange={(e) => setModelDraft(e.target.value)}
            onBlur={saveModel}
            onKeyDown={(e) => {
              if (e.key === "Enter") saveModel();
            }}
            placeholder={
              models[screenProvider.id] ||
              t("settings.assistant.screen.model.placeholder")
            }
            className="w-56"
          />
        </SettingContainer>
      )}
      {missingKey && (
        <div className="px-4 py-3">
          <Alert variant="warning">
            {t("settings.assistant.provider.noKey", {
              provider: screenProvider.label,
            })}
          </Alert>
        </div>
      )}
    </SettingsGroup>
  );
};
