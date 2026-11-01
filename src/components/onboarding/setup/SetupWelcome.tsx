import React, { useId, useMemo } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../ui/Button";
import { AppLanguageMenu } from "../AppLanguageMenu";
import { useOnboardingStore } from "../onboardingStore";
import { useSettings } from "@/hooks/useSettings";
import { CURATED_ASR } from "@/lib/models/catalog";
import { languageName } from "@/lib/constants/languages";
import { getSupportedLanguage } from "@/i18n";
import { FIELD_CLASS, SetupScreen } from "./SetupScreen";
import { OTHER_LANGUAGES_MODEL, languageChoices } from "./flow";

interface SetupWelcomeProps {
  progress: { index: number; count: number };
  download: React.ReactNode;
  onContinue: () => void;
}

/** What Kusema is, what stays private, and the language they'll dictate in. */
export const SetupWelcome: React.FC<SetupWelcomeProps> = ({
  progress,
  download,
  onContinue,
}) => {
  const { t, i18n } = useTranslation();
  const { updateSetting } = useSettings();
  const { languages, preview, set } = useOnboardingStore();
  const fieldId = useId();
  const ui = i18n.language;

  const choices = useMemo(() => {
    const other = CURATED_ASR.find((m) => m.id === OTHER_LANGUAGES_MODEL);
    return languageChoices(other?.languages ?? [], (code) =>
      languageName(code, ui),
    );
  }, [ui]);

  const uiLanguage = getSupportedLanguage(ui) || "en";
  const changeUiLanguage = (code: string) => {
    i18n.changeLanguage(code);
    if (!preview) updateSetting("app_language", code);
  };

  return (
    <SetupScreen
      progress={progress}
      title={t("onboarding.setup.welcome.title")}
      sub={t("onboarding.setup.welcome.privacy")}
      pill={<AppLanguageMenu value={uiLanguage} onChange={changeUiLanguage} />}
      hint={download}
      primary={
        <Button variant="primary" size="lg" onClick={onContinue}>
          {t("onboarding.shell.continue")}
        </Button>
      }
    >
      <div className="flex flex-col gap-[6px]">
        <label htmlFor={fieldId} className="font-medium">
          {t("onboarding.setup.welcome.language")}
        </label>
        <select
          id={fieldId}
          className={FIELD_CLASS}
          value={languages[0] ?? "en"}
          onChange={(ev) => set({ languages: [ev.target.value] })}
        >
          {choices.map((code) => (
            <option key={code} value={code}>
              {languageName(code, ui)}
            </option>
          ))}
        </select>
      </div>
    </SetupScreen>
  );
};
