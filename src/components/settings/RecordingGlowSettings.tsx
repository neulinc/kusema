import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { commands, type GlowColor, type GlowPalette } from "@/bindings";
import { useSettings } from "../../hooks/useSettings";
import { Button } from "../ui/Button";
import { Dropdown } from "../ui/Dropdown";
import { SettingContainer } from "../ui/SettingContainer";
import { SettingsGroup } from "../ui/SettingsGroup";
import { Slider } from "../ui/Slider";
import { ToggleSwitch } from "../ui/ToggleSwitch";

const PALETTES: GlowPalette[] = ["standard", "color_blind"];

/** The glow around the screen while listening, and the session's sounds. */
export const RecordingGlowSettings: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const palette = getSetting("glow_palette") ?? "standard";
  const enabled = getSetting("glow_enabled") ?? true;
  const [colors, setColors] = useState<GlowColor[]>([]);

  useEffect(() => {
    commands.getGlowColors().then(setColors).catch(console.error);
  }, [palette]);

  return (
    <SettingsGroup
      title={t("settings.glow.title")}
      description={t("settings.glow.description")}
    >
      <ToggleSwitch
        checked={enabled}
        onChange={(on) => updateSetting("glow_enabled", on)}
        isUpdating={isUpdating("glow_enabled")}
        label={t("settings.glow.enabled.label")}
        description={t("settings.glow.enabled.description")}
        descriptionMode="tooltip"
        grouped
      />
      <Slider
        value={getSetting("glow_thickness") ?? 24}
        onChange={(value) => updateSetting("glow_thickness", value)}
        min={8}
        max={48}
        step={2}
        label={t("settings.glow.thickness.label")}
        description={t("settings.glow.thickness.description")}
        descriptionMode="tooltip"
        grouped
        formatValue={(value) => `${Math.round(value)}`}
        disabled={!enabled}
      />
      <Slider
        value={getSetting("glow_intensity") ?? 80}
        onChange={(value) => updateSetting("glow_intensity", value)}
        min={30}
        max={100}
        step={5}
        label={t("settings.glow.intensity.label")}
        description={t("settings.glow.intensity.description")}
        descriptionMode="tooltip"
        grouped
        formatValue={(value) => `${Math.round(value)}%`}
        disabled={!enabled}
      />
      <SettingContainer
        title={t("settings.glow.palette.label")}
        description={t("settings.glow.palette.description")}
        descriptionMode="tooltip"
        grouped
      >
        <Dropdown
          options={PALETTES.map((value) => ({
            value,
            label: t(`settings.glow.palette.${value}`),
          }))}
          selectedValue={palette}
          onSelect={(value) =>
            updateSetting("glow_palette", value as GlowPalette)
          }
          disabled={!enabled}
        />
      </SettingContainer>
      <ul className="m-0 flex list-none flex-col gap-[6px] px-4 py-3">
        {colors.map(({ mode, color, dashed }) => (
          <li key={mode} className="flex items-center gap-[10px]">
            <span
              aria-hidden="true"
              className={`h-0 w-[28px] shrink-0 border-t-[5px] ${dashed ? "border-dashed" : "border-solid"}`}
              style={{ borderColor: color }}
            />
            <span className="min-w-0 flex-1 text-sm">
              {t(`settings.glow.modes.${mode}`)}
            </span>
            <Button
              variant="secondary"
              size="sm"
              aria-label={t("settings.glow.previewLabel", {
                mode: t(`settings.glow.modes.${mode}`),
              })}
              onClick={() => commands.previewRecordingGlow(mode)}
            >
              {t("settings.glow.preview")}
            </Button>
          </li>
        ))}
      </ul>
      <ToggleSwitch
        checked={getSetting("dictation_sounds") ?? true}
        onChange={(on) => updateSetting("dictation_sounds", on)}
        isUpdating={isUpdating("dictation_sounds")}
        label={t("settings.glow.sounds.label")}
        description={t("settings.glow.sounds.description")}
        descriptionMode="tooltip"
        grouped
      />
    </SettingsGroup>
  );
};
