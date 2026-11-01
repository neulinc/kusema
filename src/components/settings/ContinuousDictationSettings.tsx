import React from "react";
import { useTranslation } from "react-i18next";
import { useSettings } from "../../hooks/useSettings";
import { Button } from "../ui/Button";
import { SettingContainer } from "../ui/SettingContainer";
import { SettingsGroup } from "../ui/SettingsGroup";
import { Slider } from "../ui/Slider";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { ShortcutInput } from "./ShortcutInput";

const BINDING = "dictate_continuous";
/** Numpad + as each keyboard implementation spells it. */
const DRAGON_KEY = { handy_keys: "keypadplus", tauri: "NumpadAdd" };

/** Continuous dictation: its shortcut, how pauses are read, when it stops. */
export const ContinuousDictationSettings: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, updateBinding, isUpdating } =
    useSettings();

  const implementation = getSetting("keyboard_implementation") ?? "handy_keys";
  const dragonKey = DRAGON_KEY[implementation];
  const usingDragonKey =
    getSetting("bindings")?.[BINDING]?.current_binding === dragonKey;

  return (
    <SettingsGroup
      title={t("settings.dictation.title")}
      description={t("settings.dictation.description")}
    >
      <ShortcutInput shortcutId={BINDING} grouped={true} />
      <SettingContainer
        title={t("settings.dictation.dragonKey.title")}
        description={t("settings.dictation.dragonKey.description")}
        descriptionMode="tooltip"
        grouped={true}
      >
        <Button
          variant="secondary"
          size="sm"
          disabled={usingDragonKey}
          onClick={() => updateBinding(BINDING, dragonKey)}
        >
          {t("settings.dictation.dragonKey.button")}
        </Button>
      </SettingContainer>
      <Slider
        value={getSetting("dictation_pause_ms") ?? 700}
        onChange={(ms) => updateSetting("dictation_pause_ms", ms)}
        min={300}
        max={3000}
        step={100}
        label={t("settings.dictation.pause.title")}
        description={t("settings.dictation.pause.description")}
        descriptionMode="tooltip"
        grouped={true}
        formatValue={(ms) =>
          t("settings.dictation.pause.seconds", {
            seconds: (ms / 1000).toFixed(1),
          })
        }
      />
      <Slider
        value={getSetting("dictation_auto_stop_minutes") ?? 3}
        onChange={(minutes) =>
          updateSetting("dictation_auto_stop_minutes", minutes)
        }
        min={1}
        max={30}
        step={1}
        label={t("settings.dictation.autoStop.title")}
        description={t("settings.dictation.autoStop.description")}
        descriptionMode="tooltip"
        grouped={true}
        formatValue={(minutes) =>
          t("settings.dictation.autoStop.minutes", { minutes })
        }
      />
      <ToggleSwitch
        checked={getSetting("dictation_voice_commands") ?? true}
        onChange={(enabled) =>
          updateSetting("dictation_voice_commands", enabled)
        }
        isUpdating={isUpdating("dictation_voice_commands")}
        label={t("settings.dictation.voiceCommands.label")}
        description={t("settings.dictation.voiceCommands.description")}
        descriptionMode="tooltip"
        grouped
      />
    </SettingsGroup>
  );
};
