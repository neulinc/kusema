import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Plus, Trash2, X } from "lucide-react";
import { commands, type AppStyle } from "@/bindings";
import { useSettings } from "../../../hooks/useSettings";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { ToggleSwitch } from "../../ui/ToggleSwitch";
import { Button } from "../../ui/Button";
import { Input } from "../../ui/Input";
import { Textarea } from "../../ui/Textarea";
import { addApp, newStyleId, removeApp } from "./appStyleRules";

const BUILT_IN = ["email", "formal", "casual"];

/** Kusema styles per app: which program gets which style, in plain words. */
export const AppStylesSettings: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const saved = getSetting("app_styles") ?? [];
  const enabled = getSetting("app_styles_enabled") ?? true;
  const [styles, setStyles] = useState<AppStyle[]>(saved);
  const [newApps, setNewApps] = useState<Record<string, string>>({});

  // The backend tidies what's saved; show its version.
  const savedKey = JSON.stringify(saved);
  useEffect(() => setStyles(JSON.parse(savedKey) as AppStyle[]), [savedKey]);

  const save = (next: AppStyle[]) => {
    setStyles(next);
    void commands.setAppStyles(next);
  };
  const edit = (id: string, change: Partial<AppStyle>) =>
    setStyles((all) => all.map((s) => (s.id === id ? { ...s, ...change } : s)));

  const builtInText = (id: string, part: "name" | "instruction") =>
    BUILT_IN.includes(id) ? t(`settings.appStyles.builtIn.${id}.${part}`) : "";

  const addStyle = () =>
    save([
      ...styles,
      {
        id: newStyleId(styles.map((s) => s.id)),
        name: t("settings.appStyles.newName"),
        apps: [],
        instruction: "",
      },
    ]);

  const submitApp = (style: AppStyle) => {
    const name = newApps[style.id] ?? "";
    const apps = addApp(style.apps ?? [], name);
    setNewApps((all) => ({ ...all, [style.id]: "" }));
    if (apps !== style.apps)
      save(styles.map((s) => (s.id === style.id ? { ...s, apps } : s)));
  };

  return (
    <SettingsGroup
      title={t("settings.appStyles.title")}
      description={t("settings.appStyles.description")}
    >
      <ToggleSwitch
        checked={enabled}
        onChange={(on) => updateSetting("app_styles_enabled", on)}
        isUpdating={isUpdating("app_styles_enabled")}
        label={t("settings.appStyles.enabled.label")}
        description={t("settings.appStyles.enabled.description")}
        descriptionMode="tooltip"
        grouped
      />
      {styles.map((style) => {
        const name = style.name ?? builtInText(style.id, "name");
        const instruction =
          style.instruction ?? builtInText(style.id, "instruction");
        return (
          <div
            key={style.id}
            className={`flex flex-col gap-2 border-t border-line px-4 py-3 ${
              enabled ? "" : "opacity-50"
            }`}
          >
            <div className="flex items-center gap-2">
              <Input
                variant="compact"
                value={name}
                aria-label={t("settings.appStyles.name")}
                onChange={(e) => edit(style.id, { name: e.target.value })}
                onBlur={() => save(styles)}
                disabled={!enabled}
                className="flex-1"
              />
              <button
                type="button"
                onClick={() => save(styles.filter((s) => s.id !== style.id))}
                aria-label={t("settings.appStyles.remove", { name })}
                title={t("settings.appStyles.remove", { name })}
                disabled={!enabled}
                className="rounded p-1 text-ink-2 hover:bg-wash hover:text-text focus:outline-none focus-visible:ring-2 focus-visible:ring-logo-primary"
              >
                <Trash2 className="h-4 w-4" />
              </button>
            </div>
            <Textarea
              value={instruction}
              rows={2}
              aria-label={t("settings.appStyles.instruction")}
              placeholder={t("settings.appStyles.instructionPlaceholder")}
              onChange={(e) => edit(style.id, { instruction: e.target.value })}
              onBlur={() => save(styles)}
              disabled={!enabled}
            />
            <div className="flex flex-wrap items-center gap-1.5">
              <span className="text-xs text-ink-2">
                {t("settings.appStyles.apps")}
              </span>
              {(style.apps ?? []).map((app) => (
                <span
                  key={app}
                  className="flex items-center gap-1 rounded-full bg-wash px-2 py-0.5 text-xs"
                >
                  {app}
                  <button
                    type="button"
                    onClick={() =>
                      save(
                        styles.map((s) =>
                          s.id === style.id
                            ? { ...s, apps: removeApp(s.apps ?? [], app) }
                            : s,
                        ),
                      )
                    }
                    aria-label={t("settings.appStyles.removeApp", { app })}
                    disabled={!enabled}
                    className="text-ink-2 hover:text-text"
                  >
                    <X className="h-3 w-3" />
                  </button>
                </span>
              ))}
              <Input
                variant="compact"
                value={newApps[style.id] ?? ""}
                placeholder={t("settings.appStyles.addAppPlaceholder")}
                aria-label={t("settings.appStyles.addApp")}
                onChange={(e) =>
                  setNewApps((all) => ({ ...all, [style.id]: e.target.value }))
                }
                onKeyDown={(e) => {
                  if (e.key === "Enter") {
                    e.preventDefault();
                    submitApp(style);
                  }
                }}
                onBlur={() => submitApp(style)}
                disabled={!enabled}
                className="w-40 text-xs"
              />
            </div>
          </div>
        );
      })}
      <div className="flex flex-wrap items-center gap-2 border-t border-line px-4 py-3">
        <Button
          variant="secondary"
          size="sm"
          onClick={addStyle}
          disabled={!enabled}
        >
          <Plus className="me-1 h-3.5 w-3.5" />
          {t("settings.appStyles.add")}
        </Button>
        <Button
          variant="secondary"
          size="sm"
          onClick={() => void commands.resetAppStyles()}
        >
          {t("settings.appStyles.reset")}
        </Button>
        <span className="text-xs text-ink-2">
          {t("settings.appStyles.neutral")}
        </span>
      </div>
    </SettingsGroup>
  );
};
