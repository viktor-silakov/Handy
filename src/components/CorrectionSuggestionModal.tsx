import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "./ui/Button";
import { Input } from "./ui/Input";
import type { CorrectionPair } from "@/bindings";

interface CorrectionSuggestionModalProps {
  suggestion: CorrectionPair | null;
  isSaving: boolean;
  onClose: () => void;
  onSave: (entry: CorrectionPair) => Promise<void>;
}

export const CorrectionSuggestionModal: React.FC<
  CorrectionSuggestionModalProps
> = ({ suggestion, isSaving, onClose, onSave }) => {
  const { t } = useTranslation();
  const [wrong, setWrong] = useState("");
  const [correct, setCorrect] = useState("");

  useEffect(() => {
    setWrong(suggestion?.wrong ?? "");
    setCorrect(suggestion?.correct ?? "");
  }, [suggestion]);

  if (!suggestion) {
    return null;
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/45 px-4">
      <div className="w-full max-w-md rounded-2xl border border-mid-gray/20 bg-background shadow-2xl">
        <div className="flex flex-col gap-4 p-5">
          <div className="space-y-1">
            <h2 className="text-lg font-semibold">
              {t("settings.correctionDictionary.modal.title")}
            </h2>
            <p className="text-sm text-text/70">
              {t("settings.correctionDictionary.modal.description")}
            </p>
          </div>

          <div className="grid grid-cols-1 gap-2">
            <Input
              value={wrong}
              onChange={(event) => setWrong(event.target.value)}
              placeholder={t("settings.correctionDictionary.fields.wrong")}
              disabled={isSaving}
            />
            <Input
              value={correct}
              onChange={(event) => setCorrect(event.target.value)}
              placeholder={t("settings.correctionDictionary.fields.correct")}
              disabled={isSaving}
            />
          </div>

          <div className="flex justify-end gap-2">
            <Button variant="secondary" onClick={onClose} disabled={isSaving}>
              {t("settings.correctionDictionary.modal.dismiss")}
            </Button>
            <Button
              onClick={() => onSave({ wrong, correct })}
              disabled={!wrong.trim() || !correct.trim() || isSaving}
            >
              {t("settings.correctionDictionary.modal.save")}
            </Button>
          </div>
        </div>
      </div>
    </div>
  );
};
