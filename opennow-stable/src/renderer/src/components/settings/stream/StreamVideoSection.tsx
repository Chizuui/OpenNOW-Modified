import { Monitor } from "lucide-react";
import { type JSX } from "react";
import type { EntitledResolution, Settings } from "@shared/gfn";
import type { CodecTestResult } from "../../../lib/codecDiagnostics";
import { useTranslation } from "../../../i18n";
import type { SettingsChangeHandler } from "./streamSettingsTypes";
import { StreamQualityControls } from "./StreamQualityControls";
import { SessionProxySettings } from "./SessionProxySettings";
import { VideoShaderControls } from "./VideoShaderControls";
import { StreamSessionOptions } from "./StreamSessionOptions";

interface StreamVideoSectionProps {
  settings: Settings;
  showAll: boolean;
  handleChange: SettingsChangeHandler;
  handlePreview: SettingsChangeHandler;
  codecResults: CodecTestResult[] | null;
  entitledResolutions: EntitledResolution[];
  subscriptionInfoLoaded: boolean;
  subscriptionLoading: boolean;
  onBlockingOverlayChange?: (blocking: boolean) => void;
  /** Reveal the codec diagnostics panel (used by the codec dropdown's "See why"). */
  onOpenCodecDiagnostics?: () => void;
}

export function StreamVideoSection({
  settings,
  showAll,
  handleChange,
  handlePreview,
  codecResults,
  entitledResolutions,
  subscriptionInfoLoaded,
  subscriptionLoading,
  onBlockingOverlayChange,
  onOpenCodecDiagnostics,
}: StreamVideoSectionProps): JSX.Element {
  const { t } = useTranslation();

  return (
    <section className="settings-section">
      {showAll && <div className="settings-section-context">{t("settings.sections.stream")}</div>}
      <div className="settings-section-header">
        <Monitor size={18} />
        <h2>{t("settings.video.title")}</h2>
      </div>
      <div className="settings-rows">
        <StreamQualityControls
          settings={settings}
          handleChange={handleChange}
          handlePreview={handlePreview}
          codecResults={codecResults}
          entitledResolutions={entitledResolutions}
          subscriptionInfoLoaded={subscriptionInfoLoaded}
          subscriptionLoading={subscriptionLoading}
          onOpenCodecDiagnostics={onOpenCodecDiagnostics}
        />
        <SessionProxySettings
          settings={settings}
          handleChange={handleChange}
          onBlockingOverlayChange={onBlockingOverlayChange}
        />
        <StreamSessionOptions settings={settings} handleChange={handleChange} />
        <VideoShaderControls
          settings={settings}
          handleChange={handleChange}
          handlePreview={handlePreview}
        />
      </div>
    </section>
  );
}
