import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { openUrl } from "@tauri-apps/plugin-opener";
import { commands } from "@/bindings";
import { privacyNote } from "@/lib/cloudProviders";
import { useSettingsStore } from "@/stores/settingsStore";
import { Button } from "../../ui/Button";
import { TextLink } from "../../ui/TextLink";

type Check =
  | { state: "idle" }
  | { state: "checking" }
  | { state: "ok"; count: number }
  | { state: "failed"; error: string };

/** Kusema: under a cloud provider's key, what it does with your text (with a
 * link to its terms), and a button that checks the key by listing models. */
export const CloudProviderNote: React.FC<{
  providerId: string;
  hasKey: boolean;
  /** Amazon Bedrock has its own "Test connection". */
  showTest?: boolean;
}> = ({ providerId, hasKey, showTest = true }) => {
  const { t } = useTranslation();
  const note = privacyNote(providerId);
  const [check, setCheck] = useState<Check>({ state: "idle" });

  useEffect(() => setCheck({ state: "idle" }), [providerId]);

  const testKey = async () => {
    setCheck({ state: "checking" });
    const result = await commands.fetchPostProcessModels(providerId);
    if (result.status === "ok") {
      const store = useSettingsStore.getState();
      store.setPostProcessModelOptions(providerId, result.data);
      // A working key may have picked a recommended model.
      await store.refreshSettings();
      setCheck({ state: "ok", count: result.data.length });
    } else {
      setCheck({ state: "failed", error: result.error });
    }
  };

  return (
    <div className="flex flex-col gap-2 px-4 py-3">
      {note && (
        <p className="m-0 text-[13px]/5 text-ink-2">
          {t(`settings.postProcessing.api.privacy.${note.key}`)}{" "}
          <TextLink onClick={() => openUrl(note.url).catch(console.error)}>
            {t("settings.postProcessing.api.privacy.details")}
          </TextLink>
        </p>
      )}
      {showTest && (
        <div className="flex flex-wrap items-center gap-3">
          <Button
            variant="secondary"
            size="sm"
            onClick={() => void testKey()}
            disabled={!hasKey || check.state === "checking"}
          >
            {check.state === "checking"
              ? t("settings.postProcessing.api.testKey.checking")
              : t("settings.postProcessing.api.testKey.button")}
          </Button>
          {check.state === "ok" && (
            <span role="status" className="text-[13px]/5 text-ink-2">
              {t("settings.postProcessing.api.testKey.ok", {
                count: check.count,
              })}
            </span>
          )}
          {check.state === "failed" && (
            <span
              role="alert"
              className="text-[13px]/5 text-error [overflow-wrap:anywhere]"
            >
              {t("settings.postProcessing.api.testKey.failed", {
                error: check.error,
              })}
            </span>
          )}
        </div>
      )}
    </div>
  );
};
