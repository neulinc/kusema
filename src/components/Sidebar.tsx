import React from "react";
import { useTranslation } from "react-i18next";
import {
  Settings as Cog,
  Flask as FlaskConical,
  ClockRotateRight as History,
  InfoCircle as Info,
  Sparks as Sparkles,
  Cpu,
  ChatBubble as MessageCircle,
  ShieldCheck as Shield,
  Book as BookOpen,
  Microphone,
} from "iconoir-react";
import HandyTextLogo from "./icons/HandyTextLogo";
import { useSettings } from "../hooks/useSettings";
import {
  GeneralSettings,
  AdvancedSettings,
  HistorySettings,
  AboutSettings,
  PostProcessingSettings,
  ModelsSettings,
  PrivacySettings,
} from "./settings";
import { VocabularySettings } from "./settings/vocabulary/VocabularySettings";

export type SidebarSection = keyof typeof SECTIONS_CONFIG;

interface IconProps {
  width?: number | string;
  height?: number | string;
  size?: number | string;
  className?: string;
  [key: string]: any;
}

interface SectionConfig {
  labelKey: string;
  icon: React.ComponentType<IconProps>;
  /** `null` when App renders the section itself (Debug, with its handlers). */
  component: React.ComponentType | null;
  enabled: (settings: any) => boolean;
}

export const SECTIONS_CONFIG = {
  general: {
    labelKey: "sidebar.general",
    icon: Microphone,
    component: GeneralSettings,
    enabled: () => true,
  },
  assistant: {
    labelKey: "sidebar.assistant",
    icon: MessageCircle,
    // Rendered by App, which passes it navigation.
    component: null,
    enabled: () => true,
  },
  privacy: {
    labelKey: "sidebar.privacy",
    icon: Shield,
    component: PrivacySettings,
    enabled: () => true,
  },
  vocabulary: {
    labelKey: "sidebar.vocabulary",
    icon: BookOpen,
    component: VocabularySettings,
    enabled: () => true,
  },
  history: {
    labelKey: "sidebar.history",
    icon: History,
    component: HistorySettings,
    enabled: () => true,
  },
  models: {
    labelKey: "sidebar.models",
    icon: Cpu,
    component: ModelsSettings,
    enabled: () => true,
  },
  advanced: {
    labelKey: "sidebar.advanced",
    icon: Cog,
    component: AdvancedSettings,
    enabled: () => true,
  },
  postprocessing: {
    labelKey: "sidebar.postProcessing",
    icon: Sparkles,
    component: PostProcessingSettings,
    // Also where a model chosen in setup is changed.
    enabled: (settings) =>
      (settings?.post_process_enabled ||
        !!settings?.post_process_models?.[
          settings?.post_process_provider_id
        ]) ??
      false,
  },
  debug: {
    labelKey: "sidebar.debug",
    icon: FlaskConical,
    component: null,
    enabled: (settings) => settings?.debug_mode ?? false,
  },
  about: {
    labelKey: "sidebar.about",
    icon: Info,
    component: AboutSettings,
    enabled: () => true,
  },
} as const satisfies Record<string, SectionConfig>;

interface SidebarProps {
  activeSection: SidebarSection;
  onSectionChange: (section: SidebarSection) => void;
}

export const Sidebar: React.FC<SidebarProps> = ({
  activeSection,
  onSectionChange,
}) => {
  const { t } = useTranslation();
  const { settings } = useSettings();

  const availableSections = Object.entries(SECTIONS_CONFIG)
    .filter(([_, config]) => config.enabled(settings))
    .map(([id, config]) => ({ id: id as SidebarSection, ...config }));

  return (
    <nav
      aria-label={t("sidebar.label")}
      className="flex flex-col w-44 h-full shrink-0 bg-sidebar border-e border-line items-center px-2"
    >
      <HandyTextLogo width={112} className="m-4" />
      <div className="flex flex-col w-full items-stretch gap-0.5 pt-2">
        {availableSections.map((section) => {
          const Icon = section.icon;
          const isActive = activeSection === section.id;

          return (
            <button
              key={section.id}
              type="button"
              aria-current={isActive ? "page" : undefined}
              className={`flex gap-2.5 items-center h-9 px-2.5 w-full rounded-lg cursor-pointer text-start transition-colors duration-150 motion-reduce:transition-none focus:outline-none focus-visible:ring-2 focus-visible:ring-accent ${
                isActive
                  ? "bg-selected text-text"
                  : "text-ink-2 hover:bg-hover hover:text-text"
              }`}
              onClick={() => onSectionChange(section.id)}
            >
              <Icon
                width={20}
                height={20}
                strokeWidth={1.5}
                aria-hidden="true"
                className="shrink-0"
              />
              <p
                className="text-sm font-medium truncate"
                title={t(section.labelKey)}
              >
                {t(section.labelKey)}
              </p>
            </button>
          );
        })}
      </div>
    </nav>
  );
};
