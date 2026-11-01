import React, { useEffect, useId, useState } from "react";
import { useTranslation } from "react-i18next";
import { openUrl } from "@tauri-apps/plugin-opener";
import { commands, type LocalAiStatus } from "@/bindings";
import { useSettings } from "@/hooks/useSettings";
import { Button } from "../../ui/Button";
import { TextLink } from "../../ui/TextLink";
import { useOnboardingStore } from "../onboardingStore";
import { FIELD_CLASS, SetupScreen } from "./SetupScreen";
import { assistantPlan } from "./flow";

const OLLAMA_SETUP_URL = "https://ollama.com/download";
/** The provider that points at a server on this computer. */
const LOCAL_PROVIDER = "custom";
/** Set up here by the local choice, or only available on a Mac. */
const NOT_CLOUD = [LOCAL_PROVIDER, "apple_intelligence"];

type Choice = "local" | "cloud" | "skip";

interface SetupAssistantProps {
  progress: { index: number; count: number };
  download: React.ReactNode;
  onBack: () => void;
  onContinue: () => void;
}

const ModelSelect: React.FC<{
  models: string[];
  value: string;
  onChange: (model: string) => void;
}> = ({ models, value, onChange }) => {
  const { t } = useTranslation();
  const id = useId();
  return (
    <div className="flex flex-col gap-[6px]">
      <label htmlFor={id} className="font-medium">
        {t("onboarding.setup.assistant.model")}
      </label>
      <select
        id={id}
        className={FIELD_CLASS}
        value={value}
        onChange={(ev) => onChange(ev.target.value)}
      >
        {models.map((model) => (
          <option key={model} value={model}>
            {model}
          </option>
        ))}
      </select>
    </div>
  );
};

/** Optional: where Ask Kusema gets its answers. */
export const SetupAssistant: React.FC<SetupAssistantProps> = ({
  progress,
  download,
  onBack,
  onContinue,
}) => {
  const { t } = useTranslation();
  const {
    getSetting,
    setPostProcessProvider,
    updatePostProcessBaseUrl,
    updatePostProcessApiKey,
    updatePostProcessModel,
  } = useSettings();
  const preview = useOnboardingStore((s) => s.preview);
  const [choice, setChoice] = useState<Choice | null>(null);
  const [saving, setSaving] = useState(false);

  /* ---------- on this computer ---------- */

  const [checking, setChecking] = useState(false);
  const [local, setLocal] = useState<LocalAiStatus | null>(null);
  const [localModel, setLocalModel] = useState("");

  const lookForLocal = async () => {
    if (preview) return;
    setChecking(true);
    const result = await commands.detectLocalAi();
    const status =
      result.status === "ok"
        ? result.data
        : { found: false, models: [], base_url: "" };
    setLocal(status);
    setLocalModel(status.models[0] ?? "");
    setChecking(false);
  };

  useEffect(() => {
    if (choice === "local" && !local) lookForLocal();
    // Looks once on choosing; "Check again" looks again.
  }, [choice]);

  /* ---------- a cloud provider ---------- */

  const providers = (getSetting("post_process_providers") ?? []).filter(
    (p) => !NOT_CLOUD.includes(p.id),
  );
  const [providerId, setProviderId] = useState("");
  const provider = providers.find((p) => p.id === providerId) ?? providers[0];
  const [apiKey, setApiKey] = useState("");
  const [cloudModels, setCloudModels] = useState<string[]>([]);
  const [cloudModel, setCloudModel] = useState("");
  const [keyError, setKeyError] = useState("");
  const providerField = useId();
  const keyField = useId();

  const pickProvider = (id: string) => {
    setProviderId(id);
    setCloudModels([]);
    setCloudModel("");
    setKeyError("");
  };

  const checkKey = async () => {
    if (preview || !provider) return;
    setChecking(true);
    setKeyError("");
    await updatePostProcessApiKey(provider.id, apiKey.trim());
    const result = await commands.fetchPostProcessModels(provider.id);
    if (result.status === "ok" && result.data.length) {
      setCloudModels(result.data);
      setCloudModel(result.data[0]);
      // Saved in Credential Manager now; no reason to keep it in the page.
      setApiKey("");
    } else {
      setCloudModels([]);
      setCloudModel("");
      setKeyError(t("onboarding.setup.assistant.cloud.keyFailed"));
    }
    setChecking(false);
  };

  /* ---------- leaving ---------- */

  const plan = assistantPlan(
    choice,
    localModel,
    provider?.id ?? "",
    cloudModel,
  );

  // AI cleanup stays off: choosing a model here only answers questions.
  const proceed = async () => {
    if (!preview && plan.kind !== "none") {
      setSaving(true);
      if (plan.kind === "local") {
        await updatePostProcessBaseUrl(LOCAL_PROVIDER, local?.base_url ?? "");
        await updatePostProcessModel(LOCAL_PROVIDER, plan.model);
        await setPostProcessProvider(LOCAL_PROVIDER);
      } else {
        await updatePostProcessModel(plan.providerId, plan.model);
        await setPostProcessProvider(plan.providerId);
      }
      setSaving(false);
    }
    onContinue();
  };

  const radio = (value: Choice) => (
    <label
      key={value}
      className={`flex cursor-pointer items-start gap-[10px] rounded-lg border px-[12px] py-[10px] focus-within:ring-2 focus-within:ring-logo-primary ${choice === value ? "border-background-ui bg-wash" : "border-line"}`}
    >
      <input
        type="radio"
        name="assistant-choice"
        className="mt-[5px] shrink-0 accent-background-ui"
        checked={choice === value}
        onChange={() => setChoice(value)}
      />
      <span className="min-w-0">
        <span className="block font-medium">
          {t(`onboarding.setup.assistant.${value}.title`)}
        </span>
        <span className="block text-small text-ink-2">
          {t(`onboarding.setup.assistant.${value}.description`)}
        </span>
      </span>
    </label>
  );

  return (
    <SetupScreen
      progress={progress}
      title={t("onboarding.setup.assistant.title")}
      sub={t("onboarding.setup.assistant.sub")}
      onBack={onBack}
      hint={download}
      primary={
        <Button variant="primary" size="lg" onClick={proceed} disabled={saving}>
          {t(
            plan.kind === "none" && choice !== "skip" && choice !== null
              ? "onboarding.setup.assistant.continueWithout"
              : "onboarding.shell.continue",
          )}
        </Button>
      }
    >
      <fieldset className="m-0 flex flex-col gap-[8px] border-0 p-0">
        <legend className="sr-only">
          {t("onboarding.setup.assistant.title")}
        </legend>
        {radio("local")}
        {radio("cloud")}
        {radio("skip")}
      </fieldset>

      {choice === "local" && (
        <div className="flex flex-col gap-[8px]">
          <p role="status" className="m-0 text-small text-ink-2">
            {checking
              ? t("onboarding.setup.assistant.local.checking")
              : !local
                ? ""
                : !local.found
                  ? t("onboarding.setup.assistant.local.notFound")
                  : local.models.length
                    ? t("onboarding.setup.assistant.local.found")
                    : t("onboarding.setup.assistant.local.noModels")}
          </p>
          {local?.found && local.models.length > 0 && (
            <ModelSelect
              models={local.models}
              value={localModel}
              onChange={setLocalModel}
            />
          )}
          {local && !checking && !local.models.length && (
            <div className="flex flex-wrap items-center gap-[12px]">
              {!local.found && (
                <TextLink
                  onClick={() => openUrl(OLLAMA_SETUP_URL).catch(console.error)}
                >
                  {t("onboarding.setup.assistant.local.instructions")}
                </TextLink>
              )}
              <Button variant="secondary" size="sm" onClick={lookForLocal}>
                {t("onboarding.setup.assistant.local.again")}
              </Button>
            </div>
          )}
        </div>
      )}

      {choice === "cloud" && (
        <div className="flex flex-col gap-[8px]">
          <div className="flex flex-col gap-[6px]">
            <label htmlFor={providerField} className="font-medium">
              {t("onboarding.setup.assistant.cloud.provider")}
            </label>
            <select
              id={providerField}
              className={FIELD_CLASS}
              value={provider?.id ?? ""}
              onChange={(ev) => pickProvider(ev.target.value)}
            >
              {providers.map((p) => (
                <option key={p.id} value={p.id}>
                  {p.label}
                </option>
              ))}
            </select>
          </div>
          <div className="flex flex-col gap-[6px]">
            <label htmlFor={keyField} className="font-medium">
              {t("onboarding.setup.assistant.cloud.key")}
            </label>
            <div className="flex flex-wrap gap-[8px]">
              <input
                id={keyField}
                type="password"
                autoComplete="off"
                spellCheck={false}
                className={`${FIELD_CLASS} min-w-[160px] flex-1 cursor-text select-text`}
                value={apiKey}
                onChange={(ev) => setApiKey(ev.target.value)}
              />
              <Button
                variant="secondary"
                size="md"
                onClick={checkKey}
                disabled={checking || !apiKey.trim()}
              >
                {t("onboarding.setup.assistant.cloud.check")}
              </Button>
            </div>
          </div>
          <p
            role="status"
            className={`m-0 text-small ${keyError ? "text-warning" : "text-ink-2"}`}
          >
            {keyError ||
              (checking
                ? t("onboarding.setup.assistant.cloud.checking")
                : cloudModels.length
                  ? t("onboarding.setup.assistant.cloud.saved")
                  : t("onboarding.setup.assistant.cloud.where"))}
          </p>
          {cloudModels.length > 0 && (
            <ModelSelect
              models={cloudModels}
              value={cloudModel}
              onChange={setCloudModel}
            />
          )}
        </div>
      )}

      <p className="m-0 text-small text-ink-2">
        {t("onboarding.setup.assistant.privileged")}
      </p>
    </SetupScreen>
  );
};
