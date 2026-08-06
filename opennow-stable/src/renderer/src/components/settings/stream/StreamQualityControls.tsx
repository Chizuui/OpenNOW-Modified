import { useCallback, useEffect, useMemo, type JSX, type ReactNode } from "react";
import type {
  CodecPreference,
  ColorQuality,
  EntitledResolution,
  Settings,
} from "@shared/gfn";
import {
  CODEC_PREFERENCE_OPTIONS,
  colorQualityRequiresHevc,
  expandEntitledStreamResolutions,
  getSafeFallbackEntitledResolutions,
  JITTER_BUFFER_MODES,
  resolveEntitledStreamProfile,
} from "@shared/gfn";
import {
  isCodecUsableForStream,
  resolveEffectiveCodec,
  type CodecTestResult,
} from "../../../lib/codecDiagnostics";
import { useTranslation } from "../../../i18n";
import { MotionSpinner } from "../../MotionSpinner";
import { SelectDropdown, type SelectDropdownOption } from "../../ui/SelectDropdown";
import { SettingRange } from "../SettingRange";
import {
  colorQualityOptions,
  getFpsForResolution,
  groupResolutions,
  inferAspectRatioFromResolution,
  STATIC_FPS_PRESETS,
  STATIC_RESOLUTION_PRESETS,
} from "../settingsFormatters";
import type { SettingsChangeHandler } from "./streamSettingsTypes";

interface StreamQualityControlsProps {
  settings: Settings;
  handleChange: SettingsChangeHandler;
  handlePreview: SettingsChangeHandler;
  codecResults: CodecTestResult[] | null;
  entitledResolutions: EntitledResolution[];
  subscriptionInfoLoaded: boolean;
  subscriptionLoading: boolean;
  /** Reveal the codec diagnostics panel (codec dropdown's "See why" action). */
  onOpenCodecDiagnostics?: () => void;
}

export function StreamQualityControls({
  settings,
  handleChange,
  handlePreview,
  codecResults,
  entitledResolutions,
  subscriptionInfoLoaded,
  subscriptionLoading,
  onOpenCodecDiagnostics,
}: StreamQualityControlsProps): JSX.Element {
  const { t } = useTranslation();
  const effectiveEntitledResolutions = useMemo(() => {
    const baseResolutions = entitledResolutions.length > 0
      ? entitledResolutions
      : subscriptionInfoLoaded
        ? getSafeFallbackEntitledResolutions()
        : [];
    return expandEntitledStreamResolutions(baseResolutions);
  }, [entitledResolutions, subscriptionInfoLoaded]);
  const useEntitledStreamOptions = effectiveEntitledResolutions.length > 0;
  const resolutionGroups = useMemo(
    () => useEntitledStreamOptions ? groupResolutions(effectiveEntitledResolutions) : [],
    [effectiveEntitledResolutions, useEntitledStreamOptions],
  );
  const dynamicFpsOptions = useMemo(
    () => useEntitledStreamOptions
      ? getFpsForResolution(effectiveEntitledResolutions, settings.resolution)
      : [],
    [effectiveEntitledResolutions, settings.resolution, useEntitledStreamOptions],
  );
  const resolvedEntitledProfile = useMemo(
    () => resolveEntitledStreamProfile(effectiveEntitledResolutions, {
      resolution: settings.resolution,
      fps: settings.fps,
    }),
    [effectiveEntitledResolutions, settings.fps, settings.resolution],
  );
  const resolutionOptions = useMemo<SelectDropdownOption[]>(
    () => useEntitledStreamOptions
      ? resolutionGroups.flatMap((group) => group.resolutions.map((resolution) => ({
          value: resolution.value,
          label: resolution.label,
          group: group.category,
        })))
      : STATIC_RESOLUTION_PRESETS.map((resolution) => ({
          value: resolution.value,
          label: resolution.label,
        })),
    [resolutionGroups, useEntitledStreamOptions],
  );

  const handleResolutionChange = useCallback((resolution: string): void => {
    handleChange("resolution", resolution);
    const aspectRatio = inferAspectRatioFromResolution(resolution);
    if (settings.aspectRatio !== aspectRatio) {
      handleChange("aspectRatio", aspectRatio);
    }
  }, [handleChange, settings.aspectRatio]);

  useEffect(() => {
    if (!useEntitledStreamOptions || !resolvedEntitledProfile) return;

    if (resolvedEntitledProfile.resolution !== settings.resolution) {
      handleResolutionChange(resolvedEntitledProfile.resolution);
    }
    if (resolvedEntitledProfile.fps !== settings.fps) {
      handleChange("fps", resolvedEntitledProfile.fps);
    }
  }, [
    handleChange,
    handleResolutionChange,
    resolvedEntitledProfile,
    settings.fps,
    settings.resolution,
    useEntitledStreamOptions,
  ]);

  const handleColorQualityChange = useCallback((colorQuality: ColorQuality): void => {
    if (colorQualityRequiresHevc(colorQuality) && settings.codec === "H264") {
      handleChange("codec", "H265");
    }
    handleChange("colorQuality", colorQuality);
  }, [handleChange, settings.codec]);

  const handleCodecChange = useCallback((codec: CodecPreference): void => {
    handleChange("codec", codec);
    // Only an explicit H264 pick is pinned to 8-bit 4:2:0; "auto" can resolve
    // to H265/AV1 which support the 10-bit / 4:4:4 color modes.
    if (codec === "H264" && settings.colorQuality !== "8bit_420") {
      handleChange("colorQuality", "8bit_420");
    }
  }, [handleChange, settings.colorQuality]);

  const autoPickedCodec = useMemo(() => resolveEffectiveCodec("auto"), []);

  // GFN-web-style codec dropdown: Auto first (with the resolved codec in
  // parentheses), then the concrete codecs. Codecs the device cannot decode are
  // disabled, grouped under an "Unsupported" header with a "See why" action
  // that reveals the codec diagnostics panel.
  const codecPreferenceOptions = useMemo<Array<{
    value: string;
    label: ReactNode;
    disabled?: boolean;
    group?: string;
  }>>(() => {
    const supported: Array<{ value: string; label: ReactNode; group?: string }> = [];
    const unsupported: Array<{ value: string; label: ReactNode; disabled?: boolean; group?: string }> = [];

    for (const preference of CODEC_PREFERENCE_OPTIONS) {
      if (preference === "auto") {
        supported.push({
          value: "auto",
          label: t("settings.video.codecAutoPick", { codec: autoPickedCodec }),
        });
        continue;
      }
      const usable = isCodecUsableForStream(preference, codecResults);
      if (usable) {
        supported.push({ value: preference, label: preference });
      } else {
        unsupported.push({
          value: preference,
          // Dimmed under the localized "Unsupported" group header (GFN-web style);
          // hover shows the device-specific reason and "See why" opens diagnostics.
          label: <span title={t("settings.video.codecUnsupportedReason")}>{preference}</span>,
          disabled: true,
          group: t("settings.video.codecUnsupported"),
        });
      }
    }

    if (unsupported.length > 0) {
      unsupported.push({
        value: "__see_why__",
        label: t("settings.video.codecSeeWhy"),
        group: t("settings.video.codecUnsupported"),
      });
    }

    return [...supported, ...unsupported];
  }, [autoPickedCodec, codecResults, t]);

  const handleCodecPreferenceChange = useCallback((value: string): void => {
    if (value === "__see_why__") {
      onOpenCodecDiagnostics?.();
      return;
    }
    handleCodecChange(value as CodecPreference);
  }, [handleCodecChange, onOpenCodecDiagnostics]);

  return (
    <>
      <div className="settings-row">
        <label className="settings-label" htmlFor="settings-stream-resolution">
          <span className="settings-label-title">
            {t("settings.video.resolution")}
            {subscriptionLoading && <MotionSpinner size={12} className="settings-loading-icon" />}
          </span>
        </label>
        <div className="settings-row-control">
          <SelectDropdown
            id="settings-stream-resolution"
            value={settings.resolution}
            options={resolutionOptions}
            onChange={handleResolutionChange}
            menuClassName="select-dropdown__menu--grouped"
          />
        </div>
      </div>

      <div className="settings-row">
        <label className="settings-label">{t("settings.video.fps")}</label>
        <div className="settings-row-control">
          <div className="settings-chip-row">
            {(useEntitledStreamOptions
              ? dynamicFpsOptions.map((value) => ({ value }))
              : STATIC_FPS_PRESETS
            ).map((preset) => (
              <button
                key={preset.value}
                className={`settings-chip ${settings.fps === preset.value ? "active" : ""}`}
                aria-pressed={settings.fps === preset.value}
                onClick={() => {
                  handleChange("fps", preset.value);
                }}
              >
                <span>{preset.value}</span>
              </button>
            ))}
          </div>
        </div>
      </div>

      <div className="settings-row">
        <label className="settings-label" htmlFor="settings-stream-codec">{t("settings.video.codec")}</label>
        <div className="settings-row-control">
          <SelectDropdown
            id="settings-stream-codec"
            value={settings.codec}
            options={codecPreferenceOptions}
            onChange={handleCodecPreferenceChange}
            ariaLabel={t("settings.video.codec")}
          />
          <span className="settings-subtle-hint">
            {settings.codec === "auto"
              ? t("settings.video.codecAutoHint")
              : t("settings.video.codecManualHint")}
          </span>
        </div>
      </div>

      <div className="settings-row">
        <label className="settings-label">{t("settings.video.colorDepth")}</label>
        <div className="settings-row-control">
          <div className="settings-chip-row">
            {colorQualityOptions.map((option) => {
              const needsHevc = colorQualityRequiresHevc(option.value);
              const colorDescription = option.value === "8bit_420"
                ? t("settings.colorQuality.mostCompatible")
                : option.value === "8bit_444"
                  ? t("settings.colorQuality.sharperChroma")
                  : option.value === "10bit_420"
                    ? t("settings.colorQuality.higherBitDepth")
                    : t("settings.colorQuality.highestChromaAndBitDepth");
              return (
                <button
                  key={option.value}
                  className={`settings-chip ${settings.colorQuality === option.value ? "active" : ""}`}
                  aria-pressed={settings.colorQuality === option.value}
                  onClick={() => handleColorQualityChange(option.value)}
                  title={needsHevc
                    ? t("settings.colorQuality.requiresH265OrAv1Title", {
                        description: colorDescription,
                      })
                    : colorDescription}
                >
                  <span>{option.label}</span>
                </button>
              );
            })}
          </div>
          {colorQualityRequiresHevc(settings.colorQuality) && settings.codec === "H264" && (
            <span className="settings-input-hint">
              {t("settings.video.requiresH265OrAv1")}
            </span>
          )}
        </div>
      </div>

      <div className="settings-row settings-row--column">
        <div className="settings-row-top">
          <label className="settings-label" htmlFor="settings-stream-max-bitrate">
            {t("settings.video.maxBitrate")}
          </label>
          <span className="settings-value-badge">{settings.maxBitrateMbps} Mbps</span>
        </div>
        <SettingRange
          id="settings-stream-max-bitrate"
          className="settings-slider"
          min={5}
          max={150}
          step={5}
          value={settings.maxBitrateMbps}
          onPreview={(value) => handlePreview("maxBitrateMbps", value)}
          onCommit={(value) => handleChange("maxBitrateMbps", value)}
        />
      </div>

      <div className="settings-row settings-row--column">
        <div className="settings-row-top">
          <label className="settings-label" htmlFor="settings-stream-recording-bitrate">
            {t("settings.video.recordingBitrate")}
          </label>
          <span className="settings-value-badge">
            {settings.recordingBitrateMbps === null
              ? t("app.labels.auto")
              : `${settings.recordingBitrateMbps} Mbps`}
          </span>
        </div>
        <div className="settings-chip-row">
          <button
            type="button"
            className={`settings-chip ${settings.recordingBitrateMbps === null ? "active" : ""}`}
            aria-pressed={settings.recordingBitrateMbps === null}
            onClick={() => handleChange("recordingBitrateMbps", null)}
          >
            <span>{t("app.labels.auto")}</span>
          </button>
          <button
            type="button"
            className={`settings-chip ${settings.recordingBitrateMbps !== null ? "active" : ""}`}
            aria-pressed={settings.recordingBitrateMbps !== null}
            onClick={() => {
              handleChange("recordingBitrateMbps", settings.recordingBitrateMbps ?? 75);
            }}
          >
            <span>{t("settings.video.customBitrate")}</span>
          </button>
        </div>
        <SettingRange
          id="settings-stream-recording-bitrate"
          className="settings-slider"
          min={5}
          max={200}
          step={5}
          value={settings.recordingBitrateMbps ?? 75}
          disabled={settings.recordingBitrateMbps === null}
          onPreview={(value) => handlePreview("recordingBitrateMbps", value)}
          onCommit={(value) => handleChange("recordingBitrateMbps", value)}
        />
        <span className="settings-subtle-hint">
          {t("settings.video.recordingBitrateHint")}
        </span>
      </div>

      <div className="settings-row settings-row--column">
        <div className="settings-row-top">
          <label className="settings-label">{t("settings.video.jitterBuffer")}</label>
        </div>
        <div className="settings-chip-row">
          {JITTER_BUFFER_MODES.map((mode) => {
            const label = mode === "low"
              ? t("settings.video.jitterBufferLow")
              : mode === "smooth"
                ? t("settings.video.jitterBufferSmooth")
                : t("settings.video.jitterBufferBalanced");
            return (
              <button
                key={mode}
                type="button"
                className={`settings-chip ${settings.jitterBufferMode === mode ? "active" : ""}`}
                aria-pressed={settings.jitterBufferMode === mode}
                onClick={() => handleChange("jitterBufferMode", mode)}
              >
                <span>{label}</span>
              </button>
            );
          })}
        </div>
        <span className="settings-subtle-hint">
          {t("settings.video.jitterBufferHint")}
        </span>
      </div>
    </>
  );
}
