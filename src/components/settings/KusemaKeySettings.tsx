import React, { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { type } from "@tauri-apps/plugin-os";
import { commands, type KusemaKey, type KusemaKeyInfo } from "@/bindings";
import { useSettings } from "../../hooks/useSettings";
import { CLASSIC_SHORTCUTS, PRESETS, keyLabel } from "@/lib/kusemaKey";
import { SettingsGroup } from "../ui/SettingsGroup";
import { SettingContainer } from "../ui/SettingContainer";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { Dropdown } from "../ui/Dropdown";
import { Button } from "../ui/Button";
import { Alert } from "../ui/Alert";
import { KusemaKeysCard } from "../shared/KusemaKeysCard";
import { ShortcutInput } from "./ShortcutInput";
import { ShortcutActivationSetting } from "./ShortcutActivation";

const CUSTOM = "custom";

/** `kusema-key-captured`: the key pressed to become the Kusema key. */
interface CapturedKey {
  key: KusemaKey | null;
  refusal: "types" | "needed" | "moves" | null;
  cancelled: boolean;
}

/** The Kusema key, the Ask box's key, Cancel, and the classic shortcuts. */
export const KusemaKeySettings: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const isLinux = type() === "linux";
  const key = getSetting("kusema_key");
  const enabled = getSetting("kusema_key_enabled") ?? true;
  const classic = getSetting("classic_shortcuts") ?? false;
  const postProcess = getSetting("post_process_enabled") ?? false;
  const [info, setInfo] = useState<KusemaKeyInfo | null>(null);
  const [capturing, setCapturing] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);

  const refreshInfo = useCallback(() => {
    commands.getKusemaKeyInfo().then(setInfo).catch(console.error);
  }, []);
  useEffect(refreshInfo, [refreshInfo, key]);

  const choose = async (next: KusemaKey) => {
    setProblem(null);
    const result = await commands.setKusemaKey(next);
    if (result.status === "error") setProblem(result.error);
  };

  useEffect(() => {
    const off = listen<CapturedKey>("kusema-key-captured", (event) => {
      setCapturing(false);
      const captured = event.payload;
      if (captured.key) void choose(captured.key);
      else if (captured.refusal)
        setProblem(t(`kusemaKey.refusal.${captured.refusal}`));
      refreshInfo();
    });
    return () => {
      void off.then((fn) => fn());
      void commands.cancelKusemaKeyCapture();
    };
  }, [refreshInfo, t]);

  const capture = () => {
    setProblem(null);
    setCapturing(true);
    void commands.startKusemaKeyCapture();
  };

  const selected = key?.kind ?? "right_ctrl";
  const options = [
    ...PRESETS.map((kind) => ({
      value: kind,
      label: t(`kusemaKey.presets.${kind}`),
    })),
    ...(key?.kind === "custom"
      ? [{ value: CUSTOM, label: keyLabel(key, t) }]
      : []),
  ];

  return (
    <SettingsGroup
      title={t("kusemaKey.settings.title")}
      description={t("kusemaKey.settings.description")}
    >
      <div className="px-4 py-3">
        <KusemaKeysCard />
      </div>
      <ToggleSwitch
        checked={enabled}
        onChange={(on) => updateSetting("kusema_key_enabled", on)}
        isUpdating={isUpdating("kusema_key_enabled")}
        label={t("kusemaKey.settings.enabled.label")}
        description={t("kusemaKey.settings.enabled.description")}
        descriptionMode="tooltip"
        grouped
      />
      <SettingContainer
        title={t("kusemaKey.settings.key.label")}
        description={t("kusemaKey.settings.key.description")}
        descriptionMode="tooltip"
        grouped
      >
        <div className="flex flex-wrap items-center gap-2">
          <Dropdown
            options={options}
            selectedValue={selected}
            onSelect={(value) => {
              if (value !== CUSTOM) void choose({ kind: value } as KusemaKey);
            }}
            disabled={!enabled || capturing}
          />
          <Button
            variant="secondary"
            size="sm"
            onClick={
              capturing
                ? () =>
                    void commands
                      .cancelKusemaKeyCapture()
                      .then(() => setCapturing(false))
                : capture
            }
            disabled={!enabled}
          >
            {capturing
              ? t("kusemaKey.settings.key.pressNow")
              : t("kusemaKey.settings.key.press")}
          </Button>
        </div>
      </SettingContainer>
      {problem && (
        <div className="px-4 pb-3">
          <Alert variant="error">{problem}</Alert>
        </div>
      )}
      {info?.suggest_copilot && (
        <div className="px-4 pb-3">
          <Alert variant="info">
            <span className="me-2">
              {t("kusemaKey.settings.copilotSuggest")}
            </span>
            <Button
              variant="secondary"
              size="sm"
              onClick={() => void choose({ kind: "copilot" })}
            >
              {t("kusemaKey.settings.useCopilot")}
            </Button>
          </Alert>
        </div>
      )}
      {info?.is_laptop &&
        !info.suggest_copilot &&
        selected === "right_ctrl" && (
          <p className="m-0 px-4 pb-3 text-xs text-ink-2">
            {t("kusemaKey.settings.laptopHint")}
          </p>
        )}
      {/* T while holding opens the Ask box; its own key shows here only if
          it was changed, otherwise with the classic shortcuts. */}
      {!classic && <ShortcutInput shortcutId="ask_box" grouped={true} />}
      {!isLinux && <ShortcutInput shortcutId="cancel" grouped={true} />}
      <ToggleSwitch
        checked={classic}
        onChange={(on) => updateSetting("classic_shortcuts", on)}
        isUpdating={isUpdating("classic_shortcuts")}
        label={t("kusemaKey.settings.classic.label")}
        description={t("kusemaKey.settings.classic.description")}
        descriptionMode="tooltip"
        grouped
      />
      {classic && <ShortcutInput shortcutId="ask_box" grouped={true} />}
      {classic &&
        CLASSIC_SHORTCUTS.filter(
          (id) => id !== "transcribe_with_post_process" || postProcess,
        ).map((id) => <ShortcutInput key={id} shortcutId={id} grouped />)}
      {classic && (
        <ShortcutActivationSetting descriptionMode="tooltip" grouped={true} />
      )}
    </SettingsGroup>
  );
};
