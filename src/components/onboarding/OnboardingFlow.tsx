import React, { useEffect, useLayoutEffect, useRef } from "react";
import { modelMatchesLanguage } from "@handy-computer/models";
import { useTranslation } from "react-i18next";
import { commands, type ChineseScript } from "@/bindings";
import { useSettings } from "@/hooks/useSettings";
import {
  STEP_ORDER,
  clearOnboardingProgress,
  useOnboardingStore,
  type OnboardingStep,
} from "./onboardingStore";
import { usePermissions } from "./usePermissions";
import type { ChineseScriptChoice } from "@/lib/utils/languageSuggestions";
import { isChinese } from "@/lib/utils/languageSuggestions";
import { Permissions } from "./screens/Permissions";
import { SetupAssistant } from "./setup/SetupAssistant";
import { SetupDone } from "./setup/SetupDone";
import { SetupMicrophone } from "./setup/SetupMicrophone";
import { SetupPractice } from "./setup/SetupPractice";
import { SetupWelcome } from "./setup/SetupWelcome";
import {
  resolveStep,
  setupModelChoice,
  stepAfter,
  stepBefore,
} from "./setup/flow";
import { Button } from "../ui/Button";
import { useOsType } from "@/hooks/useOsType";
import { DownloadStatus } from "./DownloadStatus";
import { useOnboardingModels } from "./useOnboardingModels";
import { useModelStore } from "@/stores/modelStore";
import { downloadPick } from "./downloadPick";

interface OnboardingFlowProps {
  /** Debug preview: inert (no prompts, downloads or setting writes). */
  preview?: boolean;
  /** The screen a preview opens on. */
  previewStep?: OnboardingStep;
  /** Called after the last screen, to show Settings. */
  onComplete: () => void;
}

// The backend's chinese_script values for the script picked on Welcome.
const SCRIPT_SETTING: Record<ChineseScriptChoice, ChineseScript> = {
  "zh-Hans": "simplified",
  "zh-Hant": "traditional_tw",
  "zh-HK": "traditional_hk",
};

/** First-run onboarding. Progress persists, so a relaunch or quit resumes. */
export const OnboardingFlow: React.FC<OnboardingFlowProps> = ({
  preview = false,
  previewStep,
  onComplete,
}) => {
  const { i18n, t } = useTranslation();
  const store = useOnboardingStore();
  const { settings, updateSetting } = useSettings();
  const permissions = usePermissions(preview);
  const osType = useOsType();
  const models = useOnboardingModels(store.languages);
  const currentModel = useModelStore((s) => s.currentModel);
  const selectModel = useModelStore((s) => s.selectModel);

  const startModel = (id: string) => {
    if (!preview) downloadPick(models, id, store.downloadId);
    store.set({
      modelId: id,
      downloadId: id,
    });
  };

  // The model downloads from the moment setup opens, and follows the language.
  // Reads the store since reset() runs first; a quit's download picks back up.
  const language = store.languages[0] ?? "en";
  const modelStarted = useRef(false);
  useEffect(() => {
    if (preview || !models.offered.length) return;
    const { downloadId } = useOnboardingStore.getState();
    const chosen = setupModelChoice({
      language,
      current: downloadId,
      onDisk: models.offered
        .filter((m) => models.infoFor(m)?.is_downloaded)
        .map((m) => m.id),
      hears: (id, code) => {
        const model = models.offered.find((m) => m.id === id);
        return !!model && modelMatchesLanguage(model, code);
      },
    });
    if (chosen !== downloadId) {
      modelStarted.current = true;
      startModel(chosen);
      return;
    }
    // Once: a download just started still reads as not started.
    if (modelStarted.current) return;
    modelStarted.current = true;
    const model = models.offered.find((m) => m.id === chosen);
    if (model && models.downloadState(model).state === "none")
      models.startDownload(model);
  }, [models.offered.length, language]);

  // Select the picked model as soon as it's on disk, so Try it can use it.
  const downloaded = models.offered.find((m) => m.id === store.downloadId);
  const downloadedInfo = downloaded && models.infoFor(downloaded);
  useEffect(() => {
    if (preview || !downloadedInfo?.is_downloaded) return;
    if (currentModel !== downloadedInfo.id) selectModel(downloadedInfo.id);
  }, [
    preview,
    downloadedInfo?.id,
    downloadedInfo?.is_downloaded,
    currentModel,
    selectModel,
  ]);

  // Practice teaches holding, and the tray's tap, which only hold_or_toggle supports;
  // settings migrated from push_to_talk may still be Toggle or PushToTalk.
  const activationSet = useRef(false);
  const activation = settings?.shortcut_activation;
  useEffect(() => {
    if (preview || !activation || activationSet.current) return;
    activationSet.current = true;
    if (activation !== "hold_or_toggle")
      updateSetting("shortcut_activation", "hold_or_toggle");
  }, [preview, activation]);

  // Before first paint so a preview never shows the saved step.
  useLayoutEffect(() => {
    store.reset(preview);
    if (preview && previewStep) store.set({ step: previewStep });
    return () => {
      if (preview) store.reset(false);
    };
    // Once per mount.
  }, []);

  // Restore the UI language a preview may have changed on Welcome.
  useEffect(() => {
    if (!preview) return;
    const before = i18n.language;
    return () => {
      if (i18n.language !== before) i18n.changeLanguage(before);
    };
  }, [preview]);

  if (!permissions.checked) return null;

  const steps = STEP_ORDER.filter(
    (s) => s !== "permissions" || permissions.needed || store.welcomeBack,
  );
  const step = resolveStep(steps, store.step);
  const progress = { index: steps.indexOf(step), count: steps.length };
  // Reaching returnTo ends the detour.
  const go = (to: OnboardingStep) =>
    store.set(
      to === store.returnTo ? { step: to, returnTo: null } : { step: to },
    );
  /** Next screen, or back to All set after a detour from its panel. */
  const onward = () => (store.returnTo ? go(store.returnTo) : next());
  const next = () => {
    const after = stepAfter(steps, step);
    if (after) go(after);
    else finish();
  };

  const back = () => {
    const before = stepBefore(steps, step);
    if (before) go(before);
  };

  const finish = async () => {
    // Saved only now: each login item change posts a macOS notification.
    if (!preview) {
      await updateSetting("autostart_enabled", store.atLogin);
      await commands.activateFirstWins();
      await commands.completeOnboarding();
    }
    onComplete();
    // After Settings replaces the flow, so Welcome doesn't flash.
    if (!preview) clearOnboardingProgress();
  };

  const downloadNow = preview
    ? ({ state: "ready" } as const)
    : models.downloadState(downloaded);
  const modelReady = downloadNow.state === "ready";

  const saveLanguages = () => {
    if (preview) return;
    const { languages, script } = useOnboardingStore.getState();
    // The model's own Language stays as is; effective_language uses these.
    updateSetting("spoken_languages", languages);
    if (languages.some(isChinese))
      updateSetting("chinese_script", SCRIPT_SETTING[script]);
  };

  const failed = downloadNow.state === "failed";
  const download = (
    <div className="flex flex-wrap items-center justify-end gap-[8px]">
      <DownloadStatus models={models} model={downloaded} />
      {failed && downloaded && (
        <Button
          variant="secondary"
          size="sm"
          onClick={() => startModel(downloaded.id)}
        >
          {t("onboarding.setup.retryDownload")}
        </Button>
      )}
    </div>
  );

  switch (step) {
    case "welcome":
      return (
        <SetupWelcome
          progress={progress}
          download={download}
          onContinue={() => {
            saveLanguages();
            onward();
          }}
        />
      );
    case "permissions":
      return (
        <Permissions
          permissions={permissions}
          progress={progress}
          onBack={back}
          onContinue={next}
        />
      );
    case "microphone":
      return (
        <SetupMicrophone
          progress={progress}
          download={download}
          onBack={back}
          onContinue={next}
        />
      );
    case "practice":
      return (
        <SetupPractice
          progress={progress}
          os={osType}
          ready={modelReady}
          download={download}
          onBack={back}
          onContinue={next}
        />
      );
    case "assistant":
      return (
        <SetupAssistant
          progress={progress}
          download={download}
          onBack={back}
          onContinue={next}
        />
      );
    case "done":
      return (
        <SetupDone
          progress={progress}
          os={osType}
          download={download}
          onBack={back}
          onDone={finish}
        />
      );
  }
};
