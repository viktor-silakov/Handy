import React, { useState, useEffect } from "react";
import { useTranslation } from "react-i18next";
import { SettingContainer } from "../../ui/SettingContainer";
import { Input } from "../../ui/Input";
import { useSettings } from "../../../hooks/useSettings";

interface RemoteServerUrlProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

export const RemoteServerUrl: React.FC<RemoteServerUrlProps> = React.memo(
  ({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting } = useSettings();

    const url = getSetting("remote_server_url") || "";
    const [localUrl, setLocalUrl] = useState(url);

    useEffect(() => {
      setLocalUrl(url);
    }, [url]);

    const handleBlur = () => {
      if (localUrl !== url) {
        updateSetting("remote_server_url", localUrl);
      }
    };

    return (
      <SettingContainer
        title={t("settings.modelSettings.remoteUrl.label", "Remote Server URL")}
        description={t(
          "settings.modelSettings.remoteUrl.description",
          "URL of the remote transcription server (e.g. http://localhost:3000)",
        )}
        descriptionMode={descriptionMode}
        grouped={grouped}
      >
        <Input
          type="url"
          value={localUrl}
          onChange={(e) => setLocalUrl(e.target.value)}
          onBlur={handleBlur}
          placeholder="http://localhost:3000"
          className="w-full max-w-[200px]"
        />
      </SettingContainer>
    );
  },
);
