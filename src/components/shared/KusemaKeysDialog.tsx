import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { Dialog } from "../ui/Dialog";
import { Button } from "../ui/Button";
import { KusemaKeysCard } from "./KusemaKeysCard";

/** The tray's "Kusema Keys": the card, over whatever page is open. */
export const KusemaKeysDialog: React.FC = () => {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  useEffect(() => {
    const off = listen("show-kusema-keys", () => setOpen(true));
    return () => {
      void off.then((fn) => fn());
    };
  }, []);
  return (
    <Dialog
      open={open}
      onOpenChange={setOpen}
      title={t("kusemaKey.card.title")}
      closeLabel={t("voiceCommands.close")}
      footer={
        <Button variant="secondary" size="sm" onClick={() => setOpen(false)}>
          {t("voiceCommands.close")}
        </Button>
      }
    >
      <KusemaKeysCard />
    </Dialog>
  );
};
