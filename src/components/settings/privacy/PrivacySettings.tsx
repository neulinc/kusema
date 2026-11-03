import React, { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { save } from "@tauri-apps/plugin-dialog";
import { toast } from "sonner";
import {
  commands,
  type DestinationCount,
  type LedgerEntry,
  type LedgerFilter,
  type PrivacyOverview,
  type Purpose,
  type RequestResult,
} from "@/bindings";
import { useSettings } from "../../../hooks/useSettings";
import { Button } from "../../ui/Button";
import { Dropdown } from "../../ui/Dropdown";
import { SettingContainer } from "../../ui/SettingContainer";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { ToggleSwitch } from "../../ui/ToggleSwitch";
import { ShortcutInput } from "../ShortcutInput";
import { PrivilegedChip, PrivilegedToggle } from "./PrivilegedToggle";

const PURPOSES: Purpose[] = [
  "assistant_question",
  "screen_question",
  "post_processing",
  "voice_edit",
  "provider_models",
  "model_download",
  "voice_download",
  "update_check",
];
const RESULTS: RequestResult[] = ["sent", "blocked", "failed"];
const RETENTION_DAYS = [7, 30, 90, 365];
const ALL = "";
const SECONDS_PER_DAY = 24 * 60 * 60;
const PERIOD_DAYS = [1, 7, 30];
const LOOPBACK = ["localhost", "127.0.0.1", "::1", "[::1]"];

const formatBytes = (bytes: number): string => {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
};

const useDestinationName = () => {
  const { t } = useTranslation();
  return (host: string) =>
    LOOPBACK.includes(host.toLowerCase())
      ? t("settings.privacy.thisComputer", { host })
      : host;
};

const DestinationTable: React.FC<{
  title: string;
  rows: DestinationCount[];
}> = ({ title, rows }) => {
  const { t } = useTranslation();
  const name = useDestinationName();
  return (
    <div className="px-4 py-3">
      <p className="text-sm font-medium mb-2">{title}</p>
      {rows.length === 0 ? (
        <p className="text-sm text-ink-3">
          {t("settings.privacy.nothingSent")}
        </p>
      ) : (
        <table className="w-full text-sm">
          <thead className="text-ink-3 text-left">
            <tr>
              <th className="font-normal pb-1">
                {t("settings.privacy.columns.destination")}
              </th>
              <th className="font-normal pb-1 text-right">
                {t("settings.privacy.results.sent")}
              </th>
              <th className="font-normal pb-1 text-right">
                {t("settings.privacy.results.blocked")}
              </th>
              <th className="font-normal pb-1 text-right">
                {t("settings.privacy.columns.size")}
              </th>
            </tr>
          </thead>
          <tbody>
            {rows.map((row) => (
              <tr key={row.host}>
                <td className="py-0.5 pr-2 break-all">{name(row.host)}</td>
                <td className="py-0.5 text-right">{row.sent}</td>
                <td className="py-0.5 text-right">{row.blocked}</td>
                <td className="py-0.5 text-right whitespace-nowrap">
                  {formatBytes(row.bytes)}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
};

/** What has left this computer, and the switch that stops it. */
export const PrivacySettings: React.FC = () => {
  const { t, i18n } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const name = useDestinationName();

  const [overview, setOverview] = useState<PrivacyOverview | null>(null);
  const [entries, setEntries] = useState<LedgerEntry[]>([]);
  const [purpose, setPurpose] = useState(ALL);
  const [result, setResult] = useState(ALL);
  const [host, setHost] = useState(ALL);
  const [days, setDays] = useState(ALL);

  const refresh = useCallback(async () => {
    const filter: LedgerFilter = {
      purpose: purpose === ALL ? null : (purpose as Purpose),
      result: result === ALL ? null : (result as RequestResult),
      host: host === ALL ? null : host,
      since:
        days === ALL
          ? null
          : Math.floor(Date.now() / 1000) - Number(days) * SECONDS_PER_DAY,
    };
    const [overviewResult, listResult] = await Promise.all([
      commands.getPrivacyOverview(),
      commands.getPrivacyLedger(filter),
    ]);
    if (overviewResult.status === "ok") setOverview(overviewResult.data);
    if (listResult.status === "ok") setEntries(listResult.data);
  }, [purpose, result, host, days]);

  useEffect(() => {
    refresh();
    const unlisten = listen("privacy-ledger-changed", () => refresh());
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [refresh]);

  const exportCsv = async () => {
    const path = await save({
      defaultPath: "kusema-privacy-ledger.csv",
      filters: [{ name: "CSV", extensions: ["csv"] }],
    });
    if (!path) return;
    const exported = await commands.exportPrivacyLedger(path);
    if (exported.status === "ok") {
      toast.success(
        t("settings.privacy.list.exported", { count: exported.data }),
      );
    } else {
      toast.error(exported.error);
    }
  };

  const clear = async () => {
    const cleared = await commands.clearPrivacyLedger();
    if (cleared.status === "error") toast.error(cleared.error);
  };

  const purposeLabel = (value: Purpose) =>
    t(`settings.privacy.purposes.${value}`);
  const resultLabel = (value: RequestResult) =>
    t(`settings.privacy.results.${value}`);
  const allOption = { value: ALL, label: t("settings.privacy.list.all") };
  const hosts = (overview?.last_7_days ?? []).map((row) => row.host);
  const privileged = getSetting("privileged_mode") ?? false;
  const hotkeyEnabled = getSetting("privileged_hotkey_enabled") ?? false;
  const timeFormat = new Intl.DateTimeFormat(i18n.language, {
    dateStyle: "medium",
    timeStyle: "short",
  });

  return (
    <div className="max-w-3xl w-full mx-auto space-y-6">
      <div className="rounded-lg border border-line px-4 py-4">
        <p className="text-lg font-semibold">
          {t("settings.privacy.audio.title")}
        </p>
        <p className="text-sm text-ink-2 mt-1">
          {t("settings.privacy.audio.description")}
        </p>
      </div>

      <SettingsGroup
        title={t("settings.privacy.privileged.title")}
        description={t("settings.privacy.privileged.description")}
      >
        <PrivilegedToggle />
        <ToggleSwitch
          checked={getSetting("privileged_allow_lan") ?? false}
          onChange={(enabled) => updateSetting("privileged_allow_lan", enabled)}
          isUpdating={isUpdating("privileged_allow_lan")}
          label={t("settings.privacy.privileged.allowLan.label")}
          description={t("settings.privacy.privileged.allowLan.description")}
          descriptionMode="tooltip"
          grouped
        />
        <ToggleSwitch
          checked={getSetting("privileged_block_downloads") ?? false}
          onChange={(enabled) =>
            updateSetting("privileged_block_downloads", enabled)
          }
          isUpdating={isUpdating("privileged_block_downloads")}
          label={t("settings.privacy.privileged.blockDownloads.label")}
          description={t(
            "settings.privacy.privileged.blockDownloads.description",
          )}
          descriptionMode="tooltip"
          grouped
        />
        <ToggleSwitch
          checked={hotkeyEnabled}
          onChange={(enabled) =>
            updateSetting("privileged_hotkey_enabled", enabled)
          }
          isUpdating={isUpdating("privileged_hotkey_enabled")}
          label={t("settings.privacy.privileged.hotkey.label")}
          description={t("settings.privacy.privileged.hotkey.description")}
          descriptionMode="tooltip"
          grouped
        />
        {hotkeyEnabled && (
          <ShortcutInput shortcutId="toggle_privileged" grouped={true} />
        )}
      </SettingsGroup>

      <SettingsGroup title={t("settings.privacy.counts.title")}>
        <DestinationTable
          title={t("settings.privacy.counts.today")}
          rows={overview?.today ?? []}
        />
        <DestinationTable
          title={t("settings.privacy.counts.week")}
          rows={overview?.last_7_days ?? []}
        />
      </SettingsGroup>

      <SettingsGroup
        title={t("settings.privacy.list.title")}
        description={t("settings.privacy.list.description")}
      >
        <div className="px-4 py-3 grid grid-cols-2 gap-2">
          <Dropdown
            selectedValue={purpose}
            onSelect={setPurpose}
            options={[
              { ...allOption, label: t("settings.privacy.list.allPurposes") },
              ...PURPOSES.map((value) => ({
                value,
                label: purposeLabel(value),
              })),
            ]}
          />
          <Dropdown
            selectedValue={result}
            onSelect={setResult}
            options={[
              { ...allOption, label: t("settings.privacy.list.allResults") },
              ...RESULTS.map((value) => ({
                value,
                label: resultLabel(value),
              })),
            ]}
          />
          <Dropdown
            selectedValue={host}
            onSelect={setHost}
            options={[
              {
                ...allOption,
                label: t("settings.privacy.list.allDestinations"),
              },
              ...hosts.map((value) => ({ value, label: name(value) })),
            ]}
          />
          <Dropdown
            selectedValue={days}
            onSelect={setDays}
            options={[
              { ...allOption, label: t("settings.privacy.list.anyTime") },
              ...PERIOD_DAYS.map((value) => ({
                value: String(value),
                label: t("settings.privacy.list.lastDays", { count: value }),
              })),
            ]}
          />
        </div>
        <div className="px-4 pb-3">
          {entries.length === 0 ? (
            <p className="text-sm text-ink-3 py-2">
              {t("settings.privacy.nothingSent")}
            </p>
          ) : (
            <table className="w-full text-sm">
              <thead className="text-ink-3 text-left">
                <tr>
                  <th className="font-normal pb-1">
                    {t("settings.privacy.columns.time")}
                  </th>
                  <th className="font-normal pb-1">
                    {t("settings.privacy.columns.destination")}
                  </th>
                  <th className="font-normal pb-1">
                    {t("settings.privacy.columns.purpose")}
                  </th>
                  <th className="font-normal pb-1">
                    {t("settings.privacy.columns.data")}
                  </th>
                  <th className="font-normal pb-1 text-right">
                    {t("settings.privacy.columns.size")}
                  </th>
                  <th className="font-normal pb-1 text-right">
                    {t("settings.privacy.columns.result")}
                  </th>
                </tr>
              </thead>
              <tbody>
                {entries.map((entry) => (
                  <tr key={entry.id} className="align-top">
                    <td className="py-1 pr-2 whitespace-nowrap">
                      {timeFormat.format(entry.timestamp * 1000)}
                    </td>
                    <td className="py-1 pr-2 break-all">{name(entry.host)}</td>
                    <td className="py-1 pr-2">{purposeLabel(entry.purpose)}</td>
                    <td className="py-1 pr-2">
                      {t(`settings.privacy.data.${entry.data_kind}`)}
                    </td>
                    <td className="py-1 text-right whitespace-nowrap">
                      {formatBytes(entry.bytes)}
                    </td>
                    <td
                      className={`py-1 pl-2 text-right ${
                        entry.result === "blocked" ? "font-medium" : ""
                      }`}
                    >
                      {resultLabel(entry.result)}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </div>
        <SettingContainer
          title={t("settings.privacy.list.retention.title")}
          description={t("settings.privacy.list.retention.description")}
          descriptionMode="tooltip"
          grouped={true}
        >
          <Dropdown
            selectedValue={String(getSetting("ledger_retention_days") ?? 30)}
            onSelect={(value) =>
              updateSetting("ledger_retention_days", Number(value))
            }
            options={RETENTION_DAYS.map((value) => ({
              value: String(value),
              label: t("settings.privacy.list.retention.days", {
                count: value,
              }),
            }))}
          />
        </SettingContainer>
        <div className="px-4 py-3 flex gap-2 justify-end">
          <Button variant="secondary" size="sm" onClick={exportCsv}>
            {t("settings.privacy.list.export")}
          </Button>
          <Button variant="danger-ghost" size="sm" onClick={clear}>
            {t("settings.privacy.list.clear")}
          </Button>
        </div>
      </SettingsGroup>
      {privileged && (
        <div className="flex justify-center">
          <PrivilegedChip />
        </div>
      )}
    </div>
  );
};
