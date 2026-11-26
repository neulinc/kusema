import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { commands, type BedrockTest } from "@/bindings";
import { useSettingsStore } from "@/stores/settingsStore";
import { Dropdown } from "../../ui/Dropdown";
import { SettingContainer } from "../../ui/SettingContainer";
import { Button } from "../../ui/Button";

/** Commercial regions with Amazon Bedrock's Mantle endpoint (AWS
 * endpoints-region-availability, checked 2026-11-26), with AWS's names. */
const REGIONS: [string, string][] = [
  ["us-east-1", "US East (N. Virginia)"],
  ["us-east-2", "US East (Ohio)"],
  ["us-west-2", "US West (Oregon)"],
  ["eu-central-1", "Europe (Frankfurt)"],
  ["eu-north-1", "Europe (Stockholm)"],
  ["eu-south-1", "Europe (Milan)"],
  ["eu-west-1", "Europe (Ireland)"],
  ["eu-west-2", "Europe (London)"],
  ["ap-northeast-1", "Asia Pacific (Tokyo)"],
  ["ap-south-1", "Asia Pacific (Mumbai)"],
  ["ap-southeast-2", "Asia Pacific (Sydney)"],
  ["ap-southeast-3", "Asia Pacific (Jakarta)"],
  ["sa-east-1", "South America (São Paulo)"],
];
const GOVCLOUD: [string, string][] = [
  ["us-gov-east-1", "AWS GovCloud (US-East)"],
  ["us-gov-west-1", "AWS GovCloud (US-West)"],
];
const OTHER = "other";

export const BEDROCK_ID = "bedrock_mantle";

const regionOf = (baseUrl: string): string =>
  baseUrl.match(/^https:\/\/bedrock-mantle\.([a-z0-9-]+)\.api\.aws/)?.[1] ??
  "us-east-1";

/** Kusema: Amazon Bedrock's region, what it uses, and a connection test. */
export const BedrockSettings: React.FC<{ baseUrl: string }> = ({ baseUrl }) => {
  const { t } = useTranslation();
  const region = regionOf(baseUrl);
  const isGov = GOVCLOUD.some(([id]) => id === region);
  const [showOther, setShowOther] = useState(isGov);
  const [busy, setBusy] = useState(false);
  const [test, setTest] = useState<BedrockTest | null>(null);

  useEffect(() => setTest(null), [region]);

  const choose = async (next: string) => {
    if (next === OTHER) {
      setShowOther(true);
      return;
    }
    if (next === region) return;
    setBusy(true);
    await commands.setBedrockRegion(next);
    const store = useSettingsStore.getState();
    await store.refreshSettings();
    await store.fetchPostProcessModels(BEDROCK_ID);
    setBusy(false);
  };

  const runTest = async () => {
    setBusy(true);
    setTest(null);
    const result = await commands.testBedrockConnection();
    if (result.status === "ok") setTest(result.data);
    setBusy(false);
  };

  const regionOptions = [
    ...REGIONS.map(([id, name]) => ({ value: id, label: `${name} · ${id}` })),
    {
      value: OTHER,
      label: t("settings.postProcessing.api.bedrock.otherRegions"),
    },
  ];

  return (
    <>
      <SettingContainer
        title={t("settings.postProcessing.api.bedrock.region")}
        description={t("settings.postProcessing.api.bedrock.regionDescription")}
        descriptionMode="tooltip"
        layout="horizontal"
        grouped={true}
      >
        <Dropdown
          options={regionOptions}
          selectedValue={isGov ? OTHER : region}
          onSelect={(value) => void choose(value)}
          disabled={busy}
        />
      </SettingContainer>
      {showOther && (
        <SettingContainer
          title={t("settings.postProcessing.api.bedrock.otherRegions")}
          description={t("settings.postProcessing.api.bedrock.govCloud")}
          descriptionMode="inline"
          layout="horizontal"
          grouped={true}
        >
          <Dropdown
            options={GOVCLOUD.map(([id, name]) => ({
              value: id,
              label: `${name} · ${id}`,
            }))}
            selectedValue={isGov ? region : ""}
            placeholder={t("settings.postProcessing.api.bedrock.chooseRegion")}
            onSelect={(value) => void choose(value)}
            disabled={busy}
          />
        </SettingContainer>
      )}
      <div className="flex flex-col gap-2 px-4 py-3">
        <p className="m-0 text-[13px]/5 text-ink-2">
          {t("settings.postProcessing.api.bedrock.help")}
        </p>
        <p className="m-0 text-[13px]/5 text-ink-2">
          {t("settings.postProcessing.api.bedrock.openWeight")}
        </p>
        <p className="m-0 text-[13px]/5 text-ink-2">
          {t("settings.postProcessing.api.bedrock.claudeAccess")}
        </p>
        <p className="m-0 text-[13px]/5 text-ink-2">
          {t("settings.postProcessing.api.bedrock.webSearch")}
        </p>
        <div className="flex flex-wrap items-center gap-3">
          <Button
            variant="secondary"
            size="sm"
            onClick={() => void runTest()}
            disabled={busy}
          >
            {busy
              ? t("settings.postProcessing.api.bedrock.testing")
              : t("settings.postProcessing.api.bedrock.test")}
          </Button>
          {test && !test.problem_key && (
            <span role="status" className="text-[13px]/5 text-ink-2">
              {t(
                test.model_answered
                  ? "settings.postProcessing.api.bedrock.okModel"
                  : "settings.postProcessing.api.bedrock.ok",
                { count: test.models },
              )}
            </span>
          )}
          {test?.problem_key && (
            <span
              role="alert"
              className="text-[13px]/5 text-error [overflow-wrap:anywhere]"
            >
              {t(test.problem_key, {
                name: test.problem_name ?? "",
                defaultValue: test.problem ?? "",
              })}
            </span>
          )}
        </div>
      </div>
    </>
  );
};
