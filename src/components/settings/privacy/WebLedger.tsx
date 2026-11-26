import React, { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { commands, type WebEntry } from "@/bindings";
import { SettingsGroup } from "../../ui/SettingsGroup";

const formatBytes = (bytes: number): string => {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
};

/** Kusema web search: every search and page read, with what was searched
 * for or which address, newest first. */
export const WebLedger: React.FC = () => {
  const { t, i18n } = useTranslation();
  const [entries, setEntries] = useState<WebEntry[]>([]);
  const timeFormat = new Intl.DateTimeFormat(i18n.language, {
    dateStyle: "short",
    timeStyle: "short",
  });

  const refresh = useCallback(async () => {
    const result = await commands.getWebLedger(null);
    if (result.status === "ok") setEntries(result.data);
  }, []);

  useEffect(() => {
    void refresh();
    const unlisten = listen("privacy-ledger-changed", () => void refresh());
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, [refresh]);

  return (
    <SettingsGroup
      title={t("settings.privacy.web.title")}
      description={t("settings.privacy.web.description")}
    >
      <p className="m-0 px-4 pt-3 text-[13px]/5 text-ink-2">
        {t("settings.privacy.web.local")}
      </p>
      <div className="px-4 py-3">
        {entries.length === 0 ? (
          <p className="m-0 text-sm text-ink-3">
            {t("settings.privacy.web.empty")}
          </p>
        ) : (
          <table className="w-full text-sm">
            <thead className="text-ink-3 text-left">
              <tr>
                <th className="font-normal pb-1">
                  {t("settings.privacy.columns.time")}
                </th>
                <th className="font-normal pb-1">
                  {t("settings.privacy.web.columns.what")}
                </th>
                <th className="font-normal pb-1">
                  {t("settings.privacy.web.columns.provider")}
                </th>
                <th className="font-normal pb-1 text-right">
                  {t("settings.privacy.web.columns.found")}
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
                  <td className="py-1 pr-2 [overflow-wrap:anywhere]">
                    <span className="text-ink-3">
                      {t(`settings.privacy.web.kinds.${entry.kind}`)}:{" "}
                    </span>
                    {entry.detail}
                  </td>
                  <td className="py-1 pr-2 break-all">{entry.provider}</td>
                  <td className="py-1 text-right whitespace-nowrap">
                    {entry.kind === "search"
                      ? (entry.results ?? 0)
                      : formatBytes(entry.bytes_received ?? 0)}
                  </td>
                  <td
                    className={`py-1 pl-2 text-right ${
                      entry.result === "blocked" ? "font-medium" : ""
                    }`}
                  >
                    {t(`settings.privacy.results.${entry.result}`)}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
    </SettingsGroup>
  );
};
