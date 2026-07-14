import React, { useState, useEffect } from "react";
import { useTranslation } from "react-i18next";
import { SettingContainer } from "../../ui/SettingContainer";
import { Input } from "../../ui/Input";
import { useSettings } from "../../../hooks/useSettings";

interface RemoteServerTokenProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

export const RemoteServerToken: React.FC<RemoteServerTokenProps> = React.memo(
  ({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting } = useSettings();

    const token = getSetting("remote_server_token") || "";
    const [localToken, setLocalToken] = useState(token);

    useEffect(() => {
      setLocalToken(token);
    }, [token]);

    const handleBlur = () => {
      if (localToken !== token) {
        updateSetting("remote_server_token", localToken || null);
      }
    };

    return (
      <SettingContainer
        title={t("settings.modelSettings.remoteToken.label", "API Token")}
        description={t(
          "settings.modelSettings.remoteToken.description",
          "Optional API token for authentication with the remote server",
        )}
        descriptionMode={descriptionMode}
        grouped={grouped}
      >
        <Input
          type="password"
          value={localToken}
          onChange={(e) => setLocalToken(e.target.value)}
          onBlur={handleBlur}
          placeholder="Optional"
          className="w-full max-w-[200px]"
        />
      </SettingContainer>
    );
  },
);
