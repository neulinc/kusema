import React, { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { open, save } from "@tauri-apps/plugin-dialog";
import { toast } from "sonner";
import { Xmark as X } from "iconoir-react";
import {
  commands,
  type ExportFormat,
  type Pack,
  type VocabEntry,
  type VocabularyView,
} from "@/bindings";
import { Button } from "../../ui/Button";
import { Input } from "../../ui/Input";
import { SettingContainer } from "../../ui/SettingContainer";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { ToggleSwitch } from "../../ui/ToggleSwitch";

/** Kusema vocabulary: packs, the user's own words, learned corrections,
 * and files to share them. */
export const VocabularySettings: React.FC = () => {
  const { t } = useTranslation();
  const [view, setView] = useState<VocabularyView | null>(null);
  const [written, setWritten] = useState("");
  const [soundsLike, setSoundsLike] = useState("");

  const refresh = useCallback(() => {
    commands
      .getVocabulary()
      .then(setView)
      .catch((e) => console.error("Failed to load the vocabulary:", e));
  }, []);

  useEffect(() => {
    refresh();
    const unlisten = listen("settings-changed", refresh);
    return () => {
      void unlisten.then((u) => u());
    };
  }, [refresh]);

  if (!view) return null;
  const own = view.entries.filter((e) => !(e.learned ?? false));
  const learned = view.entries.filter((e) => e.learned);

  const setPack = async (pack: Pack, enabled: boolean) => {
    const result = await commands.setVocabularyPack(pack, enabled);
    if (result.status === "error") toast.error(result.error);
    refresh();
  };

  const add = async () => {
    const spoken = soundsLike
      .split(",")
      .map((s) => s.trim())
      .filter(Boolean);
    const result = await commands.addVocabularyEntry(written, spoken);
    if (result.status === "error") {
      toast.error(result.error);
      return;
    }
    setWritten("");
    setSoundsLike("");
    refresh();
  };

  const remove = async (entry: VocabEntry) => {
    await commands.removeVocabularyEntry(entry.written);
    refresh();
  };

  const importFile = async () => {
    const path = await open({
      multiple: false,
      filters: [
        {
          name: t("settings.vocabulary.files.filterName"),
          extensions: ["json", "csv", "txt"],
        },
      ],
    });
    if (typeof path !== "string") return;
    const result = await commands.importVocabulary(path);
    if (result.status === "error") {
      toast.error(result.error);
      return;
    }
    toast.success(
      t("settings.vocabulary.files.imported", {
        added: result.data.added,
        existing: result.data.already_there,
      }),
    );
    refresh();
  };

  const exportFile = async (format: ExportFormat) => {
    const path = await save({
      defaultPath: `kusema-vocabulary.${format}`,
      filters: [
        {
          name: t("settings.vocabulary.files.filterName"),
          extensions: [format],
        },
      ],
    });
    if (!path) return;
    const result = await commands.exportVocabulary(path, format);
    if (result.status === "error") {
      toast.error(result.error);
      return;
    }
    toast.success(
      t("settings.vocabulary.files.exported", { number: result.data }),
    );
  };

  const entryRow = (entry: VocabEntry) => (
    <li
      key={entry.written}
      className="flex items-start justify-between gap-3 px-4 py-2"
    >
      <div className="min-w-0">
        <p className="font-medium break-words">{entry.written}</p>
        {(entry.sounds_like ?? []).length > 0 && (
          <p className="text-sm text-ink-2 break-words">
            {t("settings.vocabulary.soundsLikeList", {
              forms: (entry.sounds_like ?? []).join(", "),
            })}
          </p>
        )}
      </div>
      <button
        type="button"
        onClick={() => void remove(entry)}
        title={t("settings.vocabulary.remove", { word: entry.written })}
        aria-label={t("settings.vocabulary.remove", { word: entry.written })}
        className="rounded p-1 text-ink-3 hover:bg-wash hover:text-text"
      >
        <X className="h-4 w-4" />
      </button>
    </li>
  );

  return (
    <div className="max-w-3xl w-full mx-auto space-y-6">
      <SettingsGroup
        title={t("settings.vocabulary.packs.title")}
        description={t("settings.vocabulary.packs.description")}
      >
        <ToggleSwitch
          checked={view.packs.legal ?? true}
          onChange={(enabled) => void setPack("legal", enabled)}
          label={t("settings.vocabulary.packs.legal.label")}
          description={t("settings.vocabulary.packs.legal.description", {
            number: view.legal_count,
          })}
          descriptionMode="inline"
          grouped={true}
        />
        <ToggleSwitch
          checked={view.packs.medical ?? false}
          onChange={(enabled) => void setPack("medical", enabled)}
          label={t("settings.vocabulary.packs.medical.label")}
          description={t("settings.vocabulary.packs.medical.description", {
            number: view.medical_count,
          })}
          descriptionMode="inline"
          grouped={true}
        />
      </SettingsGroup>

      <SettingsGroup
        title={t("settings.vocabulary.own.title")}
        description={t("settings.vocabulary.own.description")}
      >
        <SettingContainer
          title={t("settings.vocabulary.own.add")}
          description={t("settings.vocabulary.own.addHelp")}
          descriptionMode="tooltip"
          grouped={true}
          layout="stacked"
        >
          <form
            className="flex flex-wrap items-center gap-2"
            onSubmit={(e) => {
              e.preventDefault();
              void add();
            }}
          >
            <Input
              variant="compact"
              value={written}
              onChange={(e) => setWritten(e.target.value)}
              placeholder={t("settings.vocabulary.own.writtenPlaceholder")}
              aria-label={t("settings.vocabulary.own.written")}
              className="w-48"
            />
            <Input
              variant="compact"
              value={soundsLike}
              onChange={(e) => setSoundsLike(e.target.value)}
              placeholder={t("settings.vocabulary.own.soundsLikePlaceholder")}
              aria-label={t("settings.vocabulary.own.soundsLike")}
              className="w-64"
            />
            <Button
              type="submit"
              variant="secondary"
              size="sm"
              disabled={!written.trim()}
            >
              {t("settings.vocabulary.own.addButton")}
            </Button>
          </form>
        </SettingContainer>
        {own.length === 0 ? (
          <p className="px-4 py-3 text-sm text-ink-3">
            {t("settings.vocabulary.own.empty")}
          </p>
        ) : (
          <ul className="divide-y divide-line">{own.map(entryRow)}</ul>
        )}
      </SettingsGroup>

      <SettingsGroup
        title={t("settings.vocabulary.learned.title")}
        description={t("settings.vocabulary.learned.description")}
      >
        {learned.length === 0 ? (
          <p className="px-4 py-3 text-sm text-ink-3">
            {t("settings.vocabulary.learned.empty")}
          </p>
        ) : (
          <ul className="divide-y divide-line">{learned.map(entryRow)}</ul>
        )}
      </SettingsGroup>

      <SettingsGroup
        title={t("settings.vocabulary.files.title")}
        description={t("settings.vocabulary.files.description")}
      >
        <div className="flex flex-wrap gap-2 px-4 py-3">
          <Button variant="secondary" size="sm" onClick={importFile}>
            {t("settings.vocabulary.files.import")}
          </Button>
          <Button
            variant="secondary"
            size="sm"
            onClick={() => void exportFile("json")}
          >
            {t("settings.vocabulary.files.exportJson")}
          </Button>
          <Button
            variant="secondary"
            size="sm"
            onClick={() => void exportFile("csv")}
          >
            {t("settings.vocabulary.files.exportCsv")}
          </Button>
        </div>
      </SettingsGroup>
    </div>
  );
};
