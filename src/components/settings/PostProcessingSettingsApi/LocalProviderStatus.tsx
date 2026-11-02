import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { openUrl } from "@tauri-apps/plugin-opener";
import { commands, type PostProcessProvider } from "@/bindings";
import { setUpUrl } from "@/lib/localProviders";
import { useSettingsStore } from "@/stores/settingsStore";
import { TextLink } from "../../ui/TextLink";

/**
 * Whether Ollama or LM Studio is running here. Not running is said plainly,
 * with how to set it up, instead of an error. Checks again when `check` changes.
 */
export const LocalProviderStatus: React.FC<{
  provider: PostProcessProvider;
  check: number;
}> = ({ provider, check }) => {
  const { t } = useTranslation();
  const [state, setState] = useState<
    "checking" | "notRunning" | "noModels" | "ok"
  >("checking");

  useEffect(() => {
    let gone = false;
    setState("checking");
    commands.detectLocalProvider(provider.id).then((result) => {
      if (gone) return;
      if (result.status !== "ok" || !result.data.found) {
        setState("notRunning");
        return;
      }
      useSettingsStore
        .getState()
        .setPostProcessModelOptions(provider.id, result.data.models);
      setState(result.data.models.length ? "ok" : "noModels");
    });
    return () => {
      gone = true;
    };
  }, [provider.id, check]);

  if (state === "ok" || state === "checking") return null;
  const url = setUpUrl(provider.id);
  return (
    <div role="status" className="px-4 pb-3 text-sm text-ink-2">
      <p className="m-0">
        {t(
          state === "notRunning"
            ? "settings.postProcessing.api.local.notRunning"
            : "settings.postProcessing.api.local.noModels",
          { name: provider.label },
        )}
      </p>
      {url && (
        <TextLink onClick={() => openUrl(url).catch(console.error)}>
          {t("settings.postProcessing.api.local.howTo", {
            name: provider.label,
          })}
        </TextLink>
      )}
    </div>
  );
};
