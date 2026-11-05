import React from "react";
import { Trans, useTranslation } from "react-i18next";
import { useSettings } from "@/hooks/useSettings";
import { useOsType } from "@/hooks/useOsType";
import { keyLabel, triggerWord } from "@/lib/kusemaKey";
import { Keycaps } from "./Keycaps";

/**
 * The two rules: hold the Kusema key to talk (double-tap for continuous
 * dictation), and say "Kusema" first to ask, edit or look at the screen.
 * At the end of setup, from the tray, and atop "What can I say?".
 */
export const KusemaKeysCard: React.FC<{ className?: string }> = ({
  className = "",
}) => {
  const { t } = useTranslation();
  const { getSetting } = useSettings();
  const os = useOsType();
  const label = keyLabel(getSetting("kusema_key"), t);
  const trigger = triggerWord(getSetting("voice_edit_trigger"));
  const key = <Keycaps binding="" labels={[label]} osType={os} size="line" />;

  return (
    <section
      aria-label={t("kusemaKey.card.title")}
      className={`rounded-xl bg-hover p-[14px] ${className}`}
    >
      <ul className="m-0 flex list-none flex-col gap-[10px] p-0">
        <li>
          <Trans i18nKey="kusemaKey.card.hold" components={{ key }} />
        </li>
        <li>
          <Trans i18nKey="kusemaKey.card.doubleTap" components={{ key }} />
        </li>
        <li>
          <Trans
            i18nKey="kusemaKey.card.askOrType"
            values={{ trigger }}
            components={{
              space: (
                <Keycaps
                  binding=""
                  labels={[t("kusemaKey.space")]}
                  osType={os}
                  size="line"
                />
              ),
              t: <Keycaps binding="" labels={["T"]} osType={os} size="line" />,
              q: <q className="font-semibold" />,
            }}
          />
        </li>
      </ul>
    </section>
  );
};
