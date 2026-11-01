import React from "react";
import { useTranslation } from "react-i18next";
import { Check } from "lucide-react";
import { commands } from "@/bindings";
import { useOsType } from "@/hooks/useOsType";
import { useSettings } from "@/hooks/useSettings";
import { Button } from "../../ui/Button";
import { Keycaps } from "../../shared/Keycaps";
import { showWinsCard, winRows, winsDone } from "./firstWins";

/** Three things to try after setup. Ticks itself; goes when it's dismissed. */
export const FirstWinsCard: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting } = useSettings();
  const os = useOsType();
  const wins = getSetting("first_wins");
  const bindings = getSetting("bindings");
  if (!wins || !showWinsCard(wins)) return null;

  const rows = winRows(wins);
  const done = winsDone(wins);

  return (
    <section
      aria-labelledby="first-wins-title"
      className="rounded-lg border border-line p-[16px]"
    >
      <div className="mb-[10px] flex flex-wrap items-center justify-between gap-[8px]">
        <h2 id="first-wins-title" className="m-0 text-base font-semibold">
          {t("settings.firstWins.title")}
        </h2>
        <Button
          variant="ghost"
          size="sm"
          onClick={() => commands.dismissFirstWins()}
        >
          {t("settings.firstWins.dismiss")}
        </Button>
      </div>
      <ul className="m-0 flex list-none flex-col gap-[8px] p-0">
        {rows.map((row) => (
          <li key={row.id} className="flex items-center gap-[10px]">
            <span
              aria-hidden="true"
              className={`grid size-[20px] shrink-0 place-items-center rounded-full border ${row.done ? "border-background-ui bg-background-ui text-white" : "border-line-strong"}`}
            >
              {row.done && <Check className="size-[13px]" />}
            </span>
            <span className={`min-w-0 flex-1 ${row.done ? "text-ink-2" : ""}`}>
              <span className="sr-only">
                {t(
                  row.done
                    ? "settings.firstWins.done"
                    : "settings.firstWins.notDone",
                )}
                {": "}
              </span>
              {t(`settings.firstWins.${row.id}`)}
            </span>
            <Keycaps
              binding={bindings?.[row.binding]?.current_binding ?? ""}
              osType={os}
              size="line"
            />
          </li>
        ))}
      </ul>
      <p role="status" className="m-0 mt-[10px] text-small text-ink-2">
        {done === rows.length
          ? t("settings.firstWins.allDone")
          : t("settings.firstWins.progress", {
              done,
              count: rows.length,
            })}
      </p>
    </section>
  );
};
