import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Xmark as X } from "iconoir-react";
import { commands } from "@/bindings";
import { useSettings } from "../../hooks/useSettings";
import { useOsType } from "../../hooks/useOsType";
import { formatKeyCombination } from "../../lib/utils/keyboard";
import { Button } from "../ui/Button";
import { Input } from "../ui/Input";
import { SettingContainer } from "../ui/SettingContainer";
import { SettingsGroup } from "../ui/SettingsGroup";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { ShortcutInput } from "./ShortcutInput";
import { VoiceCommandsButton } from "./VoiceCommandsDialog";

const BINDING = "voice_edit";
/** One key to hold, as each keyboard implementation spells it: Right Ctrl
 * where modifier-only shortcuts work, else Numpad −. */
const SIMPLE_KEY = { handy_keys: "ctrl_right", tauri: "NumpadSubtract" };

/** Kusema voice editing: its shortcut, the AI preview, the trigger word for
 * continuous dictation and the command list. */
export const VoiceEditingSettings: React.FC = () => {
  const { t } = useTranslation();
  const osType = useOsType();
  const { getSetting, updateSetting, updateBinding, isUpdating } =
    useSettings();

  const implementation = getSetting("keyboard_implementation") ?? "handy_keys";
  const simpleKey = SIMPLE_KEY[implementation];
  const usingSimpleKey =
    getSetting("bindings")?.[BINDING]?.current_binding === simpleKey;
  // Kusema: with the Kusema key, "Kusema, …" edits; this is for the classic key.
  const classic = getSetting("classic_shortcuts") ?? false;

  const saved = getSetting("voice_edit_trigger") ?? "";
  const variants = getSetting("trigger_variants") ?? [];
  const [trigger, setTrigger] = useState(saved);
  useEffect(() => setTrigger(saved), [saved]);
  const commitTrigger = () => {
    if (trigger.trim() !== saved) {
      void updateSetting("voice_edit_trigger", trigger.trim());
    }
  };

  return (
    <SettingsGroup
      title={t("settings.voiceEdit.title")}
      description={t("settings.voiceEdit.description")}
    >
      <ShortcutInput shortcutId={BINDING} grouped={true} />
      {classic && (
        <SettingContainer
          title={t("settings.voiceEdit.simpleKey.title")}
          description={t("settings.voiceEdit.simpleKey.description")}
          descriptionMode="tooltip"
          grouped={true}
        >
          <Button
            variant="secondary"
            size="sm"
            disabled={usingSimpleKey}
            onClick={() => updateBinding(BINDING, simpleKey)}
          >
            {t("settings.voiceEdit.simpleKey.button", {
              key: formatKeyCombination(simpleKey, osType),
            })}
          </Button>
        </SettingContainer>
      )}
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
        title={t("settings.voiceEdit.trigger.label")}
        description={t("settings.voiceEdit.trigger.description")}
        descriptionMode="tooltip"
        grouped={true}
      >
        <Input
          variant="compact"
          value={trigger}
          placeholder={t("settings.voiceEdit.trigger.placeholder")}
          aria-label={t("settings.voiceEdit.trigger.label")}
          onChange={(e) => setTrigger(e.target.value)}
          onBlur={commitTrigger}
          onKeyDown={(e) => {
            if (e.key === "Enter") commitTrigger();
          }}
          disabled={isUpdating("voice_edit_trigger")}
          className="w-40"
        />
      </SettingContainer>
      {variants.length > 0 && (
        <div className="flex flex-wrap items-center gap-1.5 px-4 pb-3">
          <span className="text-xs text-ink-2">
            {t("settings.voiceEdit.trigger.variants")}
          </span>
          {variants.map((variant) => (
            <span
              key={variant}
              className="flex items-center gap-1 rounded-full bg-wash px-2 py-0.5 text-xs"
            >
              {variant}
              <button
                type="button"
                onClick={() => void commands.removeTriggerVariant(variant)}
                aria-label={t("settings.voiceEdit.trigger.removeVariant", {
                  variant,
                })}
                className="text-ink-2 hover:text-text"
              >
                <X className="h-3 w-3" />
              </button>
            </span>
          ))}
        </div>
      )}
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
