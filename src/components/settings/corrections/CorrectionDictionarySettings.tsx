import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Button } from "../../ui/Button";
import { Input } from "../../ui/Input";
import { SettingContainer } from "../../ui/SettingContainer";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { ToggleSwitch } from "../../ui/ToggleSwitch";
import { useSettings } from "../../../hooks/useSettings";
import { useOsType } from "../../../hooks/useOsType";
import type { CorrectionPair } from "@/bindings";
import {
  normalizeCorrectionDictionary,
  normalizeCorrectionPair,
  upsertCorrectionPair,
} from "@/lib/utils/correctionDictionary";

export const CorrectionDictionarySettings: React.FC = () => {
  const { t } = useTranslation();
  const osType = useOsType();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const dictionary =
    (getSetting("correction_dictionary") as CorrectionPair[] | undefined) ?? [];
  const trackingEnabled =
    getSetting("track_input_correction_suggestions") ?? false;
  const dictionaryUpdating = isUpdating("correction_dictionary");
  const trackingUpdating = isUpdating("track_input_correction_suggestions");
  const trackingAvailable = osType === "macos";
  const [newWrong, setNewWrong] = useState("");
  const [newCorrect, setNewCorrect] = useState("");
  const [drafts, setDrafts] = useState<CorrectionPair[]>(dictionary);

  useEffect(() => {
    setDrafts(dictionary);
  }, [dictionary]);

  const handleAddEntry = async () => {
    const normalized = normalizeCorrectionPair({
      wrong: newWrong,
      correct: newCorrect,
    });

    if (!normalized) {
      toast.error(t("settings.correctionDictionary.messages.invalid"));
      return;
    }

    const existing = dictionary.find(
      (entry) => entry.wrong.toLowerCase() === normalized.wrong.toLowerCase(),
    );

    if (
      existing &&
      existing.correct.toLowerCase() === normalized.correct.toLowerCase()
    ) {
      toast.error(
        t("settings.correctionDictionary.messages.duplicate", {
          wrong: normalized.wrong,
        }),
      );
      return;
    }

    await updateSetting(
      "correction_dictionary",
      upsertCorrectionPair(dictionary, normalized),
    );
    setNewWrong("");
    setNewCorrect("");
  };

  const handleSaveDraft = async (index: number) => {
    const normalized = normalizeCorrectionPair(drafts[index]);

    if (!normalized) {
      toast.error(t("settings.correctionDictionary.messages.invalid"));
      return;
    }

    const remainingEntries = dictionary.filter(
      (_, entryIndex) => entryIndex !== index,
    );
    const nextDictionary = normalizeCorrectionDictionary([
      ...remainingEntries,
      normalized,
    ]);

    await updateSetting("correction_dictionary", nextDictionary);
  };

  const handleRemoveDraft = async (index: number) => {
    await updateSetting(
      "correction_dictionary",
      dictionary.filter((_, entryIndex) => entryIndex !== index),
    );
  };

  return (
    <div className="max-w-3xl w-full mx-auto space-y-6">
      <SettingsGroup
        title={t("settings.correctionDictionary.title")}
        description={t("settings.correctionDictionary.description")}
      >
        <SettingContainer
          title={t("settings.correctionDictionary.add.title")}
          description={t("settings.correctionDictionary.add.description")}
          grouped={true}
          layout="stacked"
        >
          <div className="flex flex-col gap-2">
            <div className="grid grid-cols-1 sm:grid-cols-2 gap-2">
              <Input
                value={newWrong}
                onChange={(event) => setNewWrong(event.target.value)}
                placeholder={t("settings.correctionDictionary.fields.wrong")}
                disabled={dictionaryUpdating}
              />
              <Input
                value={newCorrect}
                onChange={(event) => setNewCorrect(event.target.value)}
                placeholder={t("settings.correctionDictionary.fields.correct")}
                disabled={dictionaryUpdating}
              />
            </div>
            <div className="flex justify-end">
              <Button
                onClick={handleAddEntry}
                disabled={
                  !newWrong.trim() || !newCorrect.trim() || dictionaryUpdating
                }
              >
                {t("settings.correctionDictionary.add.button")}
              </Button>
            </div>
          </div>
        </SettingContainer>

        <ToggleSwitch
          checked={trackingEnabled}
          onChange={(enabled) =>
            updateSetting("track_input_correction_suggestions", enabled)
          }
          disabled={!trackingAvailable}
          isUpdating={trackingUpdating}
          label={t("settings.correctionDictionary.suggestions.label")}
          description={t(
            trackingAvailable
              ? "settings.correctionDictionary.suggestions.description"
              : "settings.correctionDictionary.suggestions.descriptionUnavailable",
          )}
          grouped={true}
        />
      </SettingsGroup>

      <SettingsGroup title={t("settings.correctionDictionary.entriesTitle")}>
        {drafts.length === 0 ? (
          <div className="px-4 py-3 text-sm text-text/60">
            {t("settings.correctionDictionary.empty")}
          </div>
        ) : (
          drafts.map((entry: CorrectionPair, index: number) => (
            <div
              key={`${entry.wrong}-${index}`}
              className="px-4 py-3 flex flex-col gap-2"
            >
              <div className="grid grid-cols-1 sm:grid-cols-2 gap-2">
                <Input
                  value={entry.wrong}
                  onChange={(event) =>
                    setDrafts((current: CorrectionPair[]) =>
                      current.map(
                        (draft: CorrectionPair, draftIndex: number) =>
                          draftIndex === index
                            ? { ...draft, wrong: event.target.value }
                            : draft,
                      ),
                    )
                  }
                  placeholder={t("settings.correctionDictionary.fields.wrong")}
                  disabled={dictionaryUpdating}
                />
                <Input
                  value={entry.correct}
                  onChange={(event) =>
                    setDrafts((current: CorrectionPair[]) =>
                      current.map(
                        (draft: CorrectionPair, draftIndex: number) =>
                          draftIndex === index
                            ? { ...draft, correct: event.target.value }
                            : draft,
                      ),
                    )
                  }
                  placeholder={t(
                    "settings.correctionDictionary.fields.correct",
                  )}
                  disabled={dictionaryUpdating}
                />
              </div>
              <div className="flex justify-end gap-2">
                <Button
                  variant="secondary"
                  onClick={() => handleSaveDraft(index)}
                  disabled={dictionaryUpdating}
                >
                  {t("settings.correctionDictionary.actions.save")}
                </Button>
                <Button
                  variant="danger-ghost"
                  onClick={() => handleRemoveDraft(index)}
                  disabled={dictionaryUpdating}
                >
                  {t("settings.correctionDictionary.actions.remove")}
                </Button>
              </div>
            </div>
          ))
        )}
      </SettingsGroup>
    </div>
  );
};
