import React from "react";
import { useTranslation } from "react-i18next";
import type { AnswerTo } from "@/bindings";
import { useSettings } from "../../../hooks/useSettings";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { SettingContainer } from "../../ui/SettingContainer";
import { ToggleSwitch } from "../../ui/ToggleSwitch";
import { Dropdown } from "../../ui/Dropdown";
import { ShortcutInput } from "../ShortcutInput";

const ANSWER_TO: AnswerTo[] = ["speak", "type", "both"];

/** Kusema: typing questions, where answers go, and tutor style. */
export const AssistantAnswerSettings: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const answerTo = getSetting("assistant_answer_to") ?? "speak";

  return (
    <SettingsGroup
      title={t("settings.assistant.answers.title")}
      description={t("settings.assistant.answers.description")}
    >
      <ShortcutInput shortcutId="ask_box" grouped={true} />
      <SettingContainer
        title={t("settings.assistant.answers.answerTo.label")}
        description={t("settings.assistant.answers.answerTo.description")}
        descriptionMode="tooltip"
        grouped
      >
        <Dropdown
          options={ANSWER_TO.map((value) => ({
            value,
            label: t(`settings.assistant.answers.answerTo.${value}`),
          }))}
          selectedValue={answerTo}
          onSelect={(value) =>
            updateSetting("assistant_answer_to", value as AnswerTo)
          }
          disabled={isUpdating("assistant_answer_to")}
        />
      </SettingContainer>
      <ToggleSwitch
        checked={getSetting("assistant_speak_typed") ?? false}
        onChange={(on) => updateSetting("assistant_speak_typed", on)}
        isUpdating={isUpdating("assistant_speak_typed")}
        label={t("settings.assistant.answers.speakTyped.label")}
        description={t("settings.assistant.answers.speakTyped.description")}
        descriptionMode="tooltip"
        grouped
      />
      <ToggleSwitch
        checked={getSetting("assistant_tutor") ?? false}
        onChange={(on) => updateSetting("assistant_tutor", on)}
        isUpdating={isUpdating("assistant_tutor")}
        label={t("settings.assistant.answers.tutor.label")}
        description={t("settings.assistant.answers.tutor.description")}
        descriptionMode="inline"
        grouped
      />
      <p className="m-0 px-4 pb-3 text-xs text-ink-2">
        {t("settings.assistant.answers.tutor.note")}
      </p>
    </SettingsGroup>
  );
};
