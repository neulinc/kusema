import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  commands,
  type WebSearchMode,
  type WebSearchProvider,
  type WebSearchSettings as Web,
} from "@/bindings";
import { useSettings } from "../../../hooks/useSettings";
import { BUILTIN_SEARCH, useAdminPolicy } from "@/lib/cloudProviders";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { SettingContainer } from "../../ui/SettingContainer";
import { ToggleSwitch } from "../../ui/ToggleSwitch";
import { Dropdown } from "../../ui/Dropdown";
import { Input } from "../../ui/Input";
import { Button } from "../../ui/Button";
import { Alert } from "../../ui/Alert";
import { TextLink } from "../../ui/TextLink";

/** Neulinc's page about hosting SearXNG for a firm. No tracking parameters. */
export const NEULINC_SEARXNG_URL = "https://neulinc.com/kusema";
/** Where Brave's pricing and keys are (checked 2026-11-26). */
const BRAVE_API_URL = "https://brave.com/search/api/";

const DEFAULT_WEB: Web = {
  enabled: false,
  mode: "when_asked",
  provider: "brave",
  searxng_url: "",
  confirm_searches: false,
  paste_sources: true,
};

type Test =
  | { state: "idle" }
  | { state: "testing" }
  | { state: "ok"; count: number }
  | { state: "failed"; error: string };

/** Kusema web search: off by default; when on, how and with what. */
export const WebSearchSettings: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const policy = useAdminPolicy();
  const web = getSetting("web_search") ?? DEFAULT_WEB;
  const privileged = getSetting("privileged_mode") ?? false;
  const aiProviderId = getSetting("post_process_provider_id") ?? "";
  const builtinName = BUILTIN_SEARCH[aiProviderId];
  const aiLabel =
    (getSetting("post_process_providers") ?? []).find(
      (p) => p.id === aiProviderId,
    )?.label ?? aiProviderId;

  const [hasKey, setHasKey] = useState(false);
  const [keyDraft, setKeyDraft] = useState("");
  const [urlDraft, setUrlDraft] = useState(web.searxng_url);
  const [test, setTest] = useState<Test>({ state: "idle" });
  useEffect(() => setUrlDraft(web.searxng_url), [web.searxng_url]);
  useEffect(() => {
    void commands.hasBraveSearchKey().then(setHasKey);
  }, []);
  useEffect(() => setTest({ state: "idle" }), [web.provider]);

  const save = (change: Partial<Web>) =>
    updateSetting("web_search", { ...web, ...change });
  const updating = isUpdating("web_search");

  if (policy.disable_web_access) {
    return (
      <SettingsGroup
        title={t("settings.assistant.web.title")}
        description={t("settings.assistant.web.description")}
      >
        <div className="px-4 py-3">
          <Alert variant="info">{t("settings.assistant.web.adminOff")}</Alert>
        </div>
      </SettingsGroup>
    );
  }

  const saveKey = async () => {
    const key = keyDraft.trim();
    if (!key) return;
    await commands.setBraveSearchKey(key);
    setKeyDraft("");
    setHasKey(true);
    setTest({ state: "idle" });
  };

  const removeKey = async () => {
    await commands.setBraveSearchKey("");
    setHasKey(false);
    setTest({ state: "idle" });
  };

  const saveUrl = () => {
    const url = urlDraft.trim().replace(/\/+$/, "");
    if (url !== web.searxng_url) save({ searxng_url: url });
  };

  const runTest = async () => {
    setTest({ state: "testing" });
    const result = await commands.testWebSearch();
    setTest(
      result.status === "ok"
        ? { state: "ok", count: result.data }
        : { state: "failed", error: result.error },
    );
  };

  const adminServer = policy.searxng_url;
  const providerOptions = [
    { value: "brave", label: t("settings.assistant.web.provider.brave") },
    { value: "searxng", label: t("settings.assistant.web.provider.searxng") },
    {
      value: "builtin",
      label: builtinName
        ? t("settings.assistant.web.provider.builtin", { name: builtinName })
        : t("settings.assistant.web.provider.builtinAny"),
    },
  ];
  const canTest =
    (web.provider === "brave" && hasKey) ||
    (web.provider === "searxng" && !!(adminServer || web.searxng_url));

  return (
    <SettingsGroup
      title={t("settings.assistant.web.title")}
      description={t("settings.assistant.web.description")}
    >
      <ToggleSwitch
        checked={web.enabled}
        onChange={(enabled) => save({ enabled })}
        isUpdating={updating}
        label={t("settings.assistant.web.enabled.label")}
        description={t("settings.assistant.web.enabled.description")}
        descriptionMode="tooltip"
        grouped
      />
      {privileged && web.enabled && (
        <div className="px-4 py-3">
          <Alert variant="info">{t("settings.assistant.web.privileged")}</Alert>
        </div>
      )}
      {web.enabled && (
        <>
          <SettingContainer
            title={t("settings.assistant.web.mode.label")}
            description={t("settings.assistant.web.mode.description")}
            descriptionMode="tooltip"
            grouped
          >
            <Dropdown
              options={[
                {
                  value: "when_asked",
                  label: t("settings.assistant.web.mode.whenAsked"),
                },
                {
                  value: "automatic",
                  label: t("settings.assistant.web.mode.automatic"),
                },
              ]}
              selectedValue={web.mode}
              onSelect={(mode) => save({ mode: mode as WebSearchMode })}
              disabled={updating}
            />
          </SettingContainer>
          <p className="m-0 px-4 pb-3 text-[13px]/5 text-ink-2">
            {t(
              web.mode === "automatic"
                ? "settings.assistant.web.mode.automaticNote"
                : "settings.assistant.web.mode.whenAskedNote",
            )}{" "}
            {t("settings.assistant.web.limits")}
          </p>

          <SettingContainer
            title={t("settings.assistant.web.provider.label")}
            description={t("settings.assistant.web.provider.description")}
            descriptionMode="tooltip"
            grouped
          >
            <Dropdown
              options={providerOptions}
              selectedValue={web.provider}
              onSelect={(provider) =>
                save({ provider: provider as WebSearchProvider })
              }
              disabled={updating}
            />
          </SettingContainer>

          {web.provider === "brave" && (
            <>
              <SettingContainer
                title={t("settings.assistant.web.brave.key")}
                description={t("settings.assistant.web.brave.keyDescription")}
                descriptionMode="tooltip"
                grouped
              >
                {hasKey ? (
                  <div className="flex items-center gap-2">
                    <span className="text-[13px] text-ink-2">
                      {t("settings.assistant.web.brave.saved")}
                    </span>
                    <Button
                      variant="secondary"
                      size="sm"
                      onClick={() => void removeKey()}
                    >
                      {t("settings.assistant.web.brave.remove")}
                    </Button>
                  </div>
                ) : (
                  <div className="flex items-center gap-2">
                    <Input
                      type="password"
                      variant="compact"
                      value={keyDraft}
                      onChange={(e) => setKeyDraft(e.target.value)}
                      onKeyDown={(e) => {
                        if (e.key === "Enter") void saveKey();
                      }}
                      placeholder={t(
                        "settings.assistant.web.brave.placeholder",
                      )}
                      aria-label={t("settings.assistant.web.brave.key")}
                      className="w-56"
                    />
                    <Button
                      variant="secondary"
                      size="sm"
                      onClick={() => void saveKey()}
                      disabled={!keyDraft.trim()}
                    >
                      {t("settings.assistant.web.brave.save")}
                    </Button>
                  </div>
                )}
              </SettingContainer>
              <p className="m-0 px-4 pb-3 text-[13px]/5 text-ink-2">
                {t("settings.assistant.web.brave.pricing")}{" "}
                <TextLink
                  onClick={() => openUrl(BRAVE_API_URL).catch(console.error)}
                >
                  {t("settings.assistant.web.brave.getKey")}
                </TextLink>
              </p>
            </>
          )}

          {web.provider === "searxng" && (
            <>
              <SettingContainer
                title={t("settings.assistant.web.searxng.label")}
                description={t("settings.assistant.web.searxng.description")}
                descriptionMode="tooltip"
                grouped
              >
                <Input
                  variant="compact"
                  value={adminServer ?? urlDraft}
                  onChange={(e) => setUrlDraft(e.target.value)}
                  onBlur={saveUrl}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") saveUrl();
                  }}
                  disabled={!!adminServer}
                  placeholder="https://search.example.com"
                  aria-label={t("settings.assistant.web.searxng.label")}
                  className="w-64"
                />
              </SettingContainer>
              <p className="m-0 px-4 pb-3 text-[13px]/5 text-ink-2">
                {adminServer ? (
                  t("settings.assistant.web.searxng.adminSet")
                ) : (
                  <>
                    {t("settings.assistant.web.searxng.neulinc")}{" "}
                    <TextLink
                      onClick={() =>
                        openUrl(NEULINC_SEARXNG_URL).catch(console.error)
                      }
                    >
                      {t("settings.assistant.web.searxng.learnMore")}
                    </TextLink>
                  </>
                )}
              </p>
            </>
          )}

          {web.provider === "builtin" && (
            <div className="px-4 pb-3">
              {builtinName ? (
                <p className="m-0 text-[13px]/5 text-ink-2">
                  {t(`settings.assistant.web.builtin.pricing.${aiProviderId}`)}
                  {web.confirm_searches && (
                    <>
                      {" "}
                      {t("settings.assistant.web.builtin.cantConfirm", {
                        name: builtinName,
                      })}
                    </>
                  )}
                </p>
              ) : (
                <Alert variant="warning">
                  {t("settings.assistant.web.builtin.unavailable", {
                    name: aiLabel,
                  })}
                </Alert>
              )}
            </div>
          )}

          {web.provider !== "builtin" && (
            <div className="flex flex-wrap items-center gap-3 px-4 pb-3">
              <Button
                variant="secondary"
                size="sm"
                onClick={() => void runTest()}
                disabled={!canTest || test.state === "testing" || privileged}
              >
                {test.state === "testing"
                  ? t("settings.assistant.web.test.testing")
                  : t("settings.assistant.web.test.button")}
              </Button>
              {test.state === "ok" && (
                <span role="status" className="text-[13px]/5 text-ink-2">
                  {t("settings.assistant.web.test.ok", { count: test.count })}
                </span>
              )}
              {test.state === "failed" && (
                <span
                  role="alert"
                  className="text-[13px]/5 text-error [overflow-wrap:anywhere]"
                >
                  {t("settings.assistant.web.test.failed", {
                    error: test.error,
                  })}
                </span>
              )}
            </div>
          )}

          <ToggleSwitch
            checked={web.confirm_searches}
            onChange={(confirm_searches) => save({ confirm_searches })}
            isUpdating={updating}
            label={t("settings.assistant.web.confirm.label")}
            description={t("settings.assistant.web.confirm.description")}
            descriptionMode="tooltip"
            grouped
          />
          <ToggleSwitch
            checked={web.paste_sources}
            onChange={(paste_sources) => save({ paste_sources })}
            isUpdating={updating}
            label={t("settings.assistant.web.pasteSources.label")}
            description={t("settings.assistant.web.pasteSources.description")}
            descriptionMode="tooltip"
            grouped
          />
          <p className="m-0 px-4 py-3 text-[13px]/5 text-ink-2">
            {t("settings.assistant.web.links")}
          </p>
        </>
      )}
    </SettingsGroup>
  );
};
