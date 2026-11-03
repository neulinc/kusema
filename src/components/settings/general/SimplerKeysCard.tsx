import React from "react";
import { useTranslation } from "react-i18next";
import { commands } from "@/bindings";
import { useSettings } from "@/hooks/useSettings";
import { Button } from "../../ui/Button";
import { KusemaKeysCard } from "../../shared/KusemaKeysCard";

/** Shown once after the move to the Kusema key, for people who had the old keys. */
export const SimplerKeysCard: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting } = useSettings();
  if (!getSetting("simpler_keys_card")) return null;
  const classic = getSetting("classic_shortcuts") ?? false;

  return (
    <section
      aria-labelledby="simpler-keys-title"
      className="rounded-lg border border-background-ui p-[16px]"
    >
      <h2
        id="simpler-keys-title"
        className="m-0 mb-[6px] text-base font-semibold"
      >
        {t("kusemaKey.news.title")}
      </h2>
      <p className="m-0 mb-[10px] text-small text-ink-2">
        {t(classic ? "kusemaKey.news.keptClassic" : "kusemaKey.news.body")}
      </p>
      <KusemaKeysCard />
      <div className="mt-[10px] flex justify-end">
        <Button
          variant="primary"
          size="sm"
          onClick={() => void commands.dismissSimplerKeysCard()}
        >
          {t("kusemaKey.news.gotIt")}
        </Button>
      </div>
    </section>
  );
};
