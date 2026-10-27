import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { commands, type KokoroStatus } from "@/bindings";
import { SettingContainer } from "../../ui/SettingContainer";
import { Button } from "../../ui/Button";

interface KokoroDownloadProgress {
  downloaded: number;
  total: number;
  percentage: number;
}

/** Download, cancel or remove the Kokoro voices. Calls `onChange` when
 * they become available or go away, so the voice list can refresh. */
export const KokoroVoices: React.FC<{ onChange: () => void }> = ({
  onChange,
}) => {
  const { t } = useTranslation();
  const [status, setStatus] = useState<KokoroStatus | null>(null);
  const [percentage, setPercentage] = useState(0);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    commands.getKokoroStatus().then(setStatus);
    const unlistenStatus = listen<KokoroStatus>(
      "kokoro-status-changed",
      (event) => {
        setStatus(event.payload);
        onChange();
      },
    );
    const unlistenProgress = listen<KokoroDownloadProgress>(
      "kokoro-download-progress",
      (event) => setPercentage(event.payload.percentage),
    );
    return () => {
      unlistenStatus.then((fn) => fn());
      unlistenProgress.then((fn) => fn());
    };
  }, [onChange]);

  if (!status?.supported) {
    return null;
  }

  const handleDownload = async () => {
    setError(null);
    setPercentage(0);
    const result = await commands.downloadKokoro();
    if (result.status === "error") {
      setError(result.error);
    }
  };

  const handleDelete = async () => {
    setError(null);
    const result = await commands.deleteKokoro();
    if (result.status === "error") {
      setError(result.error);
    }
  };

  let action: React.ReactNode;
  if (status.downloading) {
    action = (
      <div className="flex items-center gap-3">
        <span className="text-sm tabular-nums">
          {t("settings.assistant.voice.kokoro.downloading", {
            percentage: Math.min(100, Math.round(percentage)),
          })}
        </span>
        <Button
          variant="secondary"
          size="sm"
          onClick={() => commands.cancelKokoroDownload()}
        >
          {t("settings.assistant.voice.kokoro.cancel")}
        </Button>
      </div>
    );
  } else if (status.downloaded) {
    action = (
      <Button variant="danger-ghost" size="sm" onClick={handleDelete}>
        {t("settings.assistant.voice.kokoro.delete")}
      </Button>
    );
  } else {
    action = (
      <Button variant="primary" size="sm" onClick={handleDownload}>
        {t("settings.assistant.voice.kokoro.download", {
          size: status.size_mb,
        })}
      </Button>
    );
  }

  return (
    <SettingContainer
      title={t("settings.assistant.voice.kokoro.label")}
      description={
        error ??
        (status.downloaded
          ? t("settings.assistant.voice.kokoro.installed")
          : t("settings.assistant.voice.kokoro.description"))
      }
      descriptionMode="tooltip"
      grouped
    >
      {action}
    </SettingContainer>
  );
};
