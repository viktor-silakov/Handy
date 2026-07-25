import React from "react";
import { useTranslation } from "react-i18next";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { Slider } from "../ui/Slider";
import { useSettings } from "../../hooks/useSettings";
import { useOsType } from "../../hooks/useOsType";

interface RemoteDesktopPasteProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

export const RemoteDesktopPaste: React.FC<RemoteDesktopPasteProps> = React.memo(
  ({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting, isUpdating } = useSettings();
    const osType = useOsType();

    // Detection is macOS-only (frontmost bundle id via osascript).
    if (osType !== "macos") {
      return null;
    }

    const enabled = getSetting("remote_desktop_paste_optimization") ?? false;
    const delay = getSetting("remote_desktop_paste_delay_ms") ?? 500;

    return (
      <>
        <ToggleSwitch
          checked={enabled}
          onChange={(value) =>
            updateSetting("remote_desktop_paste_optimization", value)
          }
          isUpdating={isUpdating("remote_desktop_paste_optimization")}
          label={t("settings.advanced.remoteDesktopPaste.label")}
          description={t("settings.advanced.remoteDesktopPaste.description")}
          descriptionMode={descriptionMode}
          grouped={grouped}
        />
        {enabled && (
          <Slider
            value={delay}
            onChange={(value) =>
              updateSetting("remote_desktop_paste_delay_ms", value)
            }
            min={100}
            max={2000}
            step={50}
            label={t("settings.advanced.remoteDesktopPaste.delayLabel")}
            description={t(
              "settings.advanced.remoteDesktopPaste.delayDescription",
            )}
            descriptionMode={descriptionMode}
            grouped={grouped}
            formatValue={(v) => `${v}ms`}
          />
        )}
      </>
    );
  },
);
