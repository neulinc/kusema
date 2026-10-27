import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { commands, type Voice } from "@/bindings";
import { useSettings } from "../../../hooks/useSettings";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { SettingContainer } from "../../ui/SettingContainer";
import { ToggleSwitch } from "../../ui/ToggleSwitch";
import { Dropdown } from "../../ui/Dropdown";
import { Slider } from "../../ui/Slider";
import { Button } from "../../ui/Button";

const MIN_RATE = 0.5;
const MAX_RATE = 2.0;

export const AssistantVoiceSettings: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const [voices, setVoices] = useState<Voice[]>([]);
  const [voicesError, setVoicesError] = useState<string | null>(null);

  const speakReplies = getSetting("assistant_speak_replies") ?? true;
  const voice = getSetting("assistant_voice") ?? null;
  const rate = getSetting("assistant_speech_rate") ?? 1;

  useEffect(() => {
    commands
      .getTtsVoices()
      .then((result) => {
        if (result.status === "ok") {
          setVoices(result.data);
        } else {
          setVoicesError(result.error);
        }
      })
      .catch((e) => setVoicesError(String(e)));
  }, []);

  const voiceOptions = [
    { value: "", label: t("settings.assistant.voice.systemDefault") },
    ...voices.map((v) => ({
      value: v.id,
      label: v.language ? `${v.name} (${v.language})` : v.name,
    })),
  ];

  const handlePreview = async () => {
    const result = await commands.previewTtsVoice(
      t("settings.assistant.voice.previewText"),
      voice,
      rate,
    );
    if (result.status === "error") {
      console.error("Failed to preview voice:", result.error);
    }
  };

  return (
    <SettingsGroup title={t("settings.assistant.voice.title")}>
      <ToggleSwitch
        checked={speakReplies}
        onChange={(enabled) =>
          updateSetting("assistant_speak_replies", enabled)
        }
        isUpdating={isUpdating("assistant_speak_replies")}
        label={t("settings.assistant.voice.speakReplies.label")}
        description={t("settings.assistant.voice.speakReplies.description")}
        descriptionMode="tooltip"
        grouped
      />
      <SettingContainer
        title={t("settings.assistant.voice.voice.label")}
        description={
          voicesError ?? t("settings.assistant.voice.voice.description")
        }
        descriptionMode="tooltip"
        grouped
        disabled={!speakReplies}
      >
        <Dropdown
          options={voiceOptions}
          selectedValue={voice ?? ""}
          onSelect={(value) =>
            updateSetting("assistant_voice", value === "" ? null : value)
          }
          disabled={!speakReplies || isUpdating("assistant_voice")}
        />
      </SettingContainer>
      <Slider
        value={rate}
        onChange={(value) => updateSetting("assistant_speech_rate", value)}
        min={MIN_RATE}
        max={MAX_RATE}
        step={0.05}
        label={t("settings.assistant.voice.rate.label")}
        description={t("settings.assistant.voice.rate.description")}
        descriptionMode="tooltip"
        grouped
        disabled={!speakReplies}
        formatValue={(value) => `${value.toFixed(2)}×`}
        onReset={() => updateSetting("assistant_speech_rate", 1)}
      />
      <SettingContainer
        title={t("settings.assistant.voice.preview.label")}
        description={t("settings.assistant.voice.preview.description")}
        descriptionMode="tooltip"
        grouped
      >
        <Button variant="secondary" size="sm" onClick={handlePreview}>
          {t("settings.assistant.voice.preview.button")}
        </Button>
      </SettingContainer>
    </SettingsGroup>
  );
};
