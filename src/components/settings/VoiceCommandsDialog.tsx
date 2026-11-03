import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { commands, type CommandGroup, type CommandInfo } from "@/bindings";
import { Dialog } from "../ui/Dialog";
import { Button } from "../ui/Button";
import { useSettings } from "../../hooks/useSettings";
import { formatKeyCombination } from "../../lib/utils/keyboard";
import { useOsType } from "../../hooks/useOsType";

const GROUPS: CommandGroup[] = ["select", "move", "edit"];

/** "What can I say?": every voice editing command, from the backend's list. */
export const VoiceCommandsDialog: React.FC<{
  open: boolean;
  onOpenChange: (open: boolean) => void;
}> = ({ open, onOpenChange }) => {
  const { t } = useTranslation();
  const { getSetting } = useSettings();
  const osType = useOsType();
  const [rows, setRows] = useState<CommandInfo[]>([]);

  useEffect(() => {
    if (open && rows.length === 0) {
      commands
        .getVoiceCommands()
        .then(setRows)
        .catch((e) => console.error("Failed to load the voice commands:", e));
    }
  }, [open, rows.length]);

  const binding = getSetting("bindings")?.voice_edit?.current_binding ?? "";
  const shortcut = binding ? formatKeyCombination(binding, osType) : "";
  const trigger = (getSetting("voice_edit_trigger") ?? "").trim();

  return (
    <Dialog
      open={open}
      onOpenChange={onOpenChange}
      title={t("voiceCommands.title")}
      description={t("voiceCommands.intro", { shortcut })}
      closeLabel={t("voiceCommands.close")}
      className="max-w-2xl"
      footer={
        <Button
          variant="secondary"
          size="sm"
          onClick={() => onOpenChange(false)}
        >
          {t("voiceCommands.close")}
        </Button>
      }
    >
      <div className="space-y-5 text-sm">
        {GROUPS.map((group) => (
          <section key={group} className="space-y-2">
            <h3 className="font-semibold">
              {t(`voiceCommands.groups.${group}`)}
            </h3>
            <ul className="space-y-2">
              {rows
                .filter((row) => row.group === group)
                .map((row) => (
                  <li key={row.id}>
                    <p>
                      {row.phrases.map((phrase, i) => (
                        <React.Fragment key={phrase}>
                          {i > 0 && <span className="text-ink-3"> · </span>}
                          <q className="font-medium">{phrase}</q>
                        </React.Fragment>
                      ))}
                    </p>
                    <p className="text-ink-2">
                      {t(`voiceCommands.commands.${row.id}`)}
                      {row.in_dictation && (
                        <span className="text-ink-3">
                          {" "}
                          ({t("voiceCommands.inDictation")})
                        </span>
                      )}
                    </p>
                  </li>
                ))}
            </ul>
          </section>
        ))}
        <section className="space-y-2">
          <h3 className="font-semibold">{t("voiceCommands.groups.ai")}</h3>
          <p className="text-ink-2">{t("voiceCommands.ai.description")}</p>
          <p>{t("voiceCommands.ai.examples")}</p>
          <p className="text-ink-2">
            {trigger
              ? t("voiceCommands.trigger", { trigger })
              : t("voiceCommands.triggerOff")}
          </p>
          <p className="text-ink-3">{t("voiceCommands.ai.privileged")}</p>
        </section>
        <p className="text-ink-2">{t("voiceCommands.selectFirst")}</p>
        <p className="text-ink-3">{t("voiceCommands.support")}</p>
        <p className="text-ink-3">{t("voiceCommands.english")}</p>
      </div>
    </Dialog>
  );
};

/** A button that opens the list, for any settings page. */
export const VoiceCommandsButton: React.FC = () => {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  return (
    <>
      <Button variant="secondary" size="sm" onClick={() => setOpen(true)}>
        {t("settings.voiceEdit.whatCanISay")}
      </Button>
      <VoiceCommandsDialog open={open} onOpenChange={setOpen} />
    </>
  );
};
