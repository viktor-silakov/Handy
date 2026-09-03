import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { useSettings } from "../../hooks/useSettings";
import { useOsType } from "../../hooks/useOsType";
import { Input } from "../ui/Input";
import { Button } from "../ui/Button";
import { SettingContainer } from "../ui/SettingContainer";

interface KeystrokeTypingWindowsProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

const normalizePattern = (pattern: string) =>
  pattern
    .replace(/[<>"']/g, "")
    .replace(/\s+/g, " ")
    .trim();

export const KeystrokeTypingWindows: React.FC<KeystrokeTypingWindowsProps> =
  React.memo(({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting, isUpdating } = useSettings();
    const osType = useOsType();
    const [newPattern, setNewPattern] = useState("");

    if (osType !== "macos") {
      return null;
    }

    const patterns: string[] =
      getSetting("keystroke_typing_window_patterns") || [];
    const normalizedPattern = normalizePattern(newPattern);

    const handleAddPattern = () => {
      if (normalizedPattern && normalizedPattern.length <= 100) {
        if (
          patterns.some(
            (p) => p.toLowerCase() === normalizedPattern.toLowerCase(),
          )
        ) {
          toast.error(
            t("settings.advanced.keystrokeTypingWindows.duplicate", {
              pattern: normalizedPattern,
            }),
          );
          return;
        }
        updateSetting("keystroke_typing_window_patterns", [
          ...patterns,
          normalizedPattern,
        ]);
        setNewPattern("");
      }
    };

    const handleRemovePattern = (patternToRemove: string) => {
      updateSetting(
        "keystroke_typing_window_patterns",
        patterns.filter((p) => p !== patternToRemove),
      );
    };

    const handleKeyPress = (e: React.KeyboardEvent) => {
      if (e.key === "Enter") {
        e.preventDefault();
        handleAddPattern();
      }
    };

    return (
      <>
        <SettingContainer
          title={t("settings.advanced.keystrokeTypingWindows.title")}
          description={t(
            "settings.advanced.keystrokeTypingWindows.description",
          )}
          descriptionMode={descriptionMode}
          grouped={grouped}
        >
          <div className="flex items-center gap-2">
            <Input
              type="text"
              className="max-w-48"
              value={newPattern}
              onChange={(e) => setNewPattern(e.target.value)}
              onKeyDown={handleKeyPress}
              placeholder={t(
                "settings.advanced.keystrokeTypingWindows.placeholder",
              )}
              variant="compact"
              disabled={isUpdating("keystroke_typing_window_patterns")}
            />
            <Button
              onClick={handleAddPattern}
              disabled={
                !normalizedPattern ||
                normalizedPattern.length > 100 ||
                isUpdating("keystroke_typing_window_patterns")
              }
              variant="primary"
              size="md"
            >
              {t("settings.advanced.keystrokeTypingWindows.add")}
            </Button>
          </div>
        </SettingContainer>
        {patterns.length > 0 && (
          <div
            className={`px-4 p-2 ${grouped ? "" : "rounded-lg border border-mid-gray/20"} flex flex-wrap gap-1`}
          >
            {patterns.map((pattern) => (
              <Button
                key={pattern}
                onClick={() => handleRemovePattern(pattern)}
                disabled={isUpdating("keystroke_typing_window_patterns")}
                variant="secondary"
                size="sm"
                className="inline-flex items-center gap-1 cursor-pointer"
                aria-label={t(
                  "settings.advanced.keystrokeTypingWindows.remove",
                  { pattern },
                )}
              >
                <span>{pattern}</span>
                <svg
                  className="w-3 h-3"
                  fill="none"
                  stroke="currentColor"
                  viewBox="0 0 24 24"
                >
                  <path
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    strokeWidth={2}
                    d="M6 18L18 6M6 6l12 12"
                  />
                </svg>
              </Button>
            ))}
          </div>
        )}
      </>
    );
  });
