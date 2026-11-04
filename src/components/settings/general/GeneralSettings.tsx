import React from "react";
import { useTranslation } from "react-i18next";
import { type } from "@tauri-apps/plugin-os";
import { MicrophoneSelector } from "../MicrophoneSelector";
import { ChannelSelector } from "../ChannelSelector";
import { MicCheckSettings } from "../MicCheckSettings";
import { KusemaKeySettings } from "../KusemaKeySettings";
import { SimplerKeysCard } from "./SimplerKeysCard";
import { ContinuousDictationSettings } from "../ContinuousDictationSettings";
import { VoiceEditingSettings } from "../VoiceEditingSettings";
import { CleanupSettings } from "../CleanupSettings";
import { RecordingGlowSettings } from "../RecordingGlowSettings";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { OutputDeviceSelector } from "../OutputDeviceSelector";
import { AudioFeedback } from "../AudioFeedback";
import { useSettings } from "../../../hooks/useSettings";
import { VolumeSlider } from "../VolumeSlider";
import { MuteWhileRecording } from "../MuteWhileRecording";
import { FirstWinsCard } from "./FirstWinsCard";
import { ModelSettingsCard } from "./ModelSettingsCard";
import { PreferredLanguages } from "../PreferredLanguages";
import { ChineseScriptSetting } from "../ChineseScript";
import { isChinese } from "@/lib/utils/languageSuggestions";

export const GeneralSettings: React.FC<{
  onOpenPostProcessing?: () => void;
}> = ({ onOpenPostProcessing }) => {
  const { t } = useTranslation();
  const { audioFeedbackEnabled, getSetting } = useSettings();
  const isWindows = type() === "windows";
  // Only for Chinese speakers, or anyone who already picked a script.
  const speaksChinese =
    (getSetting("spoken_languages") ?? []).some(isChinese) ||
    (getSetting("chinese_script") ?? "as_transcribed") !== "as_transcribed";
  return (
    <div className="max-w-3xl w-full mx-auto space-y-6">
      <SimplerKeysCard />
      <FirstWinsCard />
      <KusemaKeySettings />
      <ContinuousDictationSettings />
      {isWindows && <VoiceEditingSettings />}
      {isWindows && <RecordingGlowSettings />}
      <CleanupSettings onOpenPostProcessing={onOpenPostProcessing} />
      <SettingsGroup title={t("settings.languages.title")}>
        <PreferredLanguages descriptionMode="tooltip" grouped={true} />
        {speaksChinese && (
          <ChineseScriptSetting descriptionMode="tooltip" grouped={true} />
        )}
      </SettingsGroup>
      <ModelSettingsCard />
      <SettingsGroup title={t("settings.sound.title")}>
        <MicrophoneSelector descriptionMode="tooltip" grouped={true} />
        <ChannelSelector descriptionMode="tooltip" grouped={true} />
        <MicCheckSettings />
        <MuteWhileRecording descriptionMode="tooltip" grouped={true} />
        <AudioFeedback descriptionMode="tooltip" grouped={true} />
        <OutputDeviceSelector
          descriptionMode="tooltip"
          grouped={true}
          disabled={!audioFeedbackEnabled}
        />
        <VolumeSlider disabled={!audioFeedbackEnabled} />
      </SettingsGroup>
    </div>
  );
};
