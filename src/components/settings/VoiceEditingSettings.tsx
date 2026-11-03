import React from "react";
import { useTranslation } from "react-i18next";
import { useSettings } from "../../hooks/useSettings";
import { SettingContainer } from "../ui/SettingContainer";
import { SettingsGroup } from "../ui/SettingsGroup";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { ShortcutInput } from "./ShortcutInput";
import { VoiceCommandsButton } from "./VoiceCommandsDialog";

/** Kusema voice editing: its shortcut, the AI preview and the command list. */
export const VoiceEditingSettings: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();

  return (
    <SettingsGroup
      title={t("settings.voiceEdit.title")}
      description={t("settings.voiceEdit.description")}
    >
      <ShortcutInput shortcutId="voice_edit" grouped={true} />
      <ToggleSwitch
        checked={getSetting("voice_edit_preview") ?? true}
        onChange={(enabled) => updateSetting("voice_edit_preview", enabled)}
        isUpdating={isUpdating("voice_edit_preview")}
        label={t("settings.voiceEdit.preview.label")}
        description={t("settings.voiceEdit.preview.description")}
        descriptionMode="tooltip"
        grouped={true}
      />
      <SettingContainer
        title={t("voiceCommands.title")}
        description={t("settings.voiceEdit.description")}
        descriptionMode="tooltip"
        grouped={true}
      >
        <VoiceCommandsButton />
      </SettingContainer>
    </SettingsGroup>
  );
};
