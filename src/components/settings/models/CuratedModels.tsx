import React from "react";
import { useTranslation } from "react-i18next";
import type { ModelInfo } from "@/bindings";
import { Button } from "@/components/ui/Button";
import { downloadStateOf } from "@/lib/models/downloadState";
import { splitCurated } from "@/lib/models/curated";
import { repoIdOf } from "@/lib/models/retired";
import { formatModelSize } from "@/lib/utils/format";
import { useModelStore } from "@/stores/modelStore";
import RetiredModelsNotice from "./RetiredModelsNotice";
import { cancelDownload, pickModel, usePlace } from "./modelEntries";

const Row: React.FC<{
  model: ModelInfo;
  name: string;
  description?: string;
  recommended?: boolean;
}> = ({ model, name, description, recommended }) => {
  const { t } = useTranslation();
  const place = usePlace(model.id);
  const percent = useModelStore((s) => {
    const d = downloadStateOf(s, model.id);
    return d.state === "downloading"
      ? Math.min(99, Math.floor(d.progress * 100))
      : 0;
  });

  const action = (() => {
    switch (place) {
      case "active":
        return (
          <span className="text-sm font-medium text-moss">
            {t("settings.models.curated.inUse")}
          </span>
        );
      case "loading":
      case "checking":
        return (
          <span role="status" className="text-sm text-ink-2">
            {t("settings.models.curated.gettingReady")}
          </span>
        );
      case "downloading":
        return (
          <div className="flex items-center gap-[10px]">
            <span role="status" className="text-sm tabular-nums text-ink-2">
              {t("settings.models.curated.downloading", { percent })}
            </span>
            <Button
              variant="secondary"
              size="sm"
              aria-label={t("settings.models.curated.cancelLabel", { name })}
              onClick={() => cancelDownload(model.id)}
            >
              {t("settings.models.curated.cancel")}
            </Button>
          </div>
        );
      case "ready":
        return (
          <Button
            variant="secondary"
            size="sm"
            aria-label={t("settings.models.curated.useLabel", { name })}
            onClick={() => pickModel(model.id)}
          >
            {t("settings.models.curated.use")}
          </Button>
        );
      case "remote":
        return (
          <Button
            variant="secondary"
            size="sm"
            aria-label={t("settings.models.curated.downloadLabel", { name })}
            onClick={() => pickModel(model.id)}
          >
            {t("settings.models.curated.download", {
              size: formatModelSize(model.size_mb),
            })}
          </Button>
        );
    }
  })();

  return (
    <li className="flex flex-wrap items-center justify-between gap-x-[16px] gap-y-[6px] px-[16px] py-[12px]">
      <div className="min-w-0 flex-[1_1_260px]">
        <div className="flex flex-wrap items-center gap-[8px]">
          <span className="font-medium">{name}</span>
          {recommended && (
            <span className="rounded-full bg-wash px-[8px] text-xs text-ink-2">
              {t("settings.models.curated.recommended")}
            </span>
          )}
        </div>
        {description && <p className="m-0 text-sm text-ink-2">{description}</p>}
      </div>
      {action}
    </li>
  );
};

/** The Models page's short list, in plain language. */
export const CuratedModels: React.FC = () => {
  const { t } = useTranslation();
  const loading = useModelStore((s) => s.loading);
  const models = useModelStore((s) => s.models);

  if (loading) {
    return (
      <div className="max-w-3xl w-full mx-auto">
        <div className="flex items-center justify-center py-16">
          <div className="w-8 h-8 border-2 border-logo-primary border-t-transparent rounded-full animate-spin" />
        </div>
      </div>
    );
  }

  const { curated, others } = splitCurated(models, repoIdOf);
  const listClass =
    "m-0 flex list-none flex-col divide-y divide-line rounded-lg border border-line p-0";

  return (
    <div className="mx-auto w-full max-w-3xl space-y-[16px] overflow-y-auto">
      <h1 className="m-0 px-[12px] text-xl font-semibold">
        {t("settings.models.title")}
      </h1>
      <div className="mx-[12px] empty:hidden">
        <RetiredModelsNotice />
      </div>
      <ul className={`mx-[12px] ${listClass}`}>
        {curated.map(({ key, model }, i) => (
          <Row
            key={model.id}
            model={model}
            name={t(`settings.models.curated.rows.${key}.name`)}
            description={t(`settings.models.curated.rows.${key}.description`)}
            recommended={i === 0}
          />
        ))}
      </ul>
      {others.length > 0 && (
        <section className="mx-[12px]">
          <h2 className="m-0 mb-[8px] text-sm font-medium text-ink-2">
            {t("settings.models.curated.others")}
          </h2>
          <ul className={listClass}>
            {others.map((model) => (
              <Row key={model.id} model={model} name={model.name} />
            ))}
          </ul>
        </section>
      )}
      <p className="m-0 px-[12px] text-sm text-ink-2">
        {t("settings.models.curated.showAllHint")}
      </p>
    </div>
  );
};
