import React from "react";
import { useTranslation } from "react-i18next";
import { Globe } from "iconoir-react";
import { commands, type WebSource } from "@/bindings";

const hostOf = (url: string): string => {
  try {
    return new URL(url).hostname.replace(/^www\./, "");
  } catch {
    return url;
  }
};

/** Kusema web search: the pages an answer drew on, numbered as the answer
 * cites them ([1], [2]...). Each opens in the browser. */
export const WebSources: React.FC<{ sources: WebSource[] }> = ({ sources }) => {
  const { t } = useTranslation();
  if (sources.length === 0) return null;
  return (
    <div className="mt-2 border-t border-line pt-2">
      <p className="m-0 mb-1 text-xs font-medium text-ink-2">
        {t("askBox.sources")}
      </p>
      <ol className="m-0 flex list-none flex-col gap-1 p-0">
        {sources.map((source, i) => (
          <li key={source.url} className="flex min-w-0 gap-1 text-xs">
            <span className="shrink-0 text-ink-2">[{i + 1}]</span>
            <button
              type="button"
              onClick={() =>
                void commands.openWebSource(source.url).catch(console.error)
              }
              title={source.url}
              className="min-w-0 cursor-pointer truncate rounded text-start text-accent underline decoration-line underline-offset-2 hover:decoration-current focus:outline-none focus-visible:ring-2 focus-visible:ring-accent"
            >
              {source.title}
              <span className="ms-1 text-ink-2 no-underline">
                · {hostOf(source.url)}
              </span>
            </button>
          </li>
        ))}
      </ol>
    </div>
  );
};

/** The "Search the web" switch beside a question box: a globe, the label
 * and an on/off indicator, so it's seen whenever web search is on. */
export const SearchToggle: React.FC<{
  on: boolean;
  onChange: (on: boolean) => void;
  disabled?: boolean;
}> = ({ on, onChange, disabled }) => {
  const { t } = useTranslation();
  return (
    <button
      type="button"
      role="switch"
      aria-checked={on}
      disabled={disabled}
      onClick={() => onChange(!on)}
      title={t("askBox.searchWebHint")}
      className={`flex shrink-0 cursor-pointer items-center gap-1.5 rounded-lg border px-2.5 py-1.5 text-xs font-medium transition-colors focus:outline-none focus-visible:ring-2 focus-visible:ring-accent disabled:cursor-default disabled:opacity-50 ${
        on
          ? "border-accent bg-selected text-text"
          : "border-line-strong bg-wash text-text hover:bg-hover"
      }`}
    >
      <Globe
        className={`h-3.5 w-3.5 ${on ? "text-accent" : "text-ink-2"}`}
        aria-hidden
      />
      {t("askBox.searchWeb")}
      <span
        aria-hidden
        className={`relative ms-0.5 inline-block h-3.5 w-6 rounded-full transition-colors ${
          on ? "bg-accent" : "bg-line-strong"
        }`}
      >
        <span
          className={`absolute top-0.5 h-2.5 w-2.5 rounded-full bg-surface transition-all ${
            on ? "start-3" : "start-0.5"
          }`}
        />
      </span>
    </button>
  );
};

/** Under an answer that didn't use the web, while web search is on: asks
 * the same question again, searching (with all the usual checks). */
export const SearchThisButton: React.FC<{
  onClick: () => void;
  disabled?: boolean;
}> = ({ onClick, disabled }) => {
  const { t } = useTranslation();
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      className="mt-2 flex items-center gap-1 rounded text-xs text-ink-2 hover:text-text focus:outline-none focus-visible:ring-2 focus-visible:ring-accent disabled:opacity-50"
    >
      <Globe className="h-3.5 w-3.5" aria-hidden />
      {t("askBox.searchThis")}
    </button>
  );
};
