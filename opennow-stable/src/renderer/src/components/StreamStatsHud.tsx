import { useMemo, useState } from "react";
import { AnimatePresence, m, useReducedMotion } from "motion/react";
import { AlertTriangle } from "lucide-react";
import type { JSX } from "react";
import type { StatsOverlayPosition } from "@shared/gfn";
import type { StreamLagReason } from "../platforms/gfn/webrtcClient";
import type { StreamDiagnosticsStore } from "../utils/streamDiagnosticsStore";
import { useStreamDiagnosticsStore } from "../utils/streamDiagnosticsStore";
import {
  formatBitrate,
  formatServerLocation,
  getPacketLossColor,
  getRttColor,
} from "../utils/streamDiagnosticsFormat";
import {
  disclosureTransition,
  getStatusPulseMotion,
  surfaceRevealTransition,
} from "./MotionProvider";
import { useTranslation } from "../i18n";

function getLagReasonLabel(reason: StreamLagReason): string {
  switch (reason) {
    case "network":
      return "Network";
    case "decoder":
      return "Decode";
    case "input_backpressure":
      return "Input";
    case "render":
      return "Render";
    case "stable":
      return "Stable";
    default:
      return "Unknown";
  }
}

export interface StreamStatsHudProps {
  diagnosticsStore: StreamDiagnosticsStore;
  mode: "compact" | "full";
  position: StatsOverlayPosition;
  gstreamerEnabled: boolean;
  serverRegion?: string;
  userSelectedRegionName?: string;
  sessionTimeRemainingText: string | null;
  hintsVisible?: boolean;
}

export function StreamStatsHud({
  diagnosticsStore,
  mode,
  position,
  gstreamerEnabled,
  serverRegion,
  userSelectedRegionName,
  sessionTimeRemainingText,
  hintsVisible = false,
}: StreamStatsHudProps): JSX.Element {
  const { t } = useTranslation();
  const reducedMotion = useReducedMotion();
  const statusPulseMotion = getStatusPulseMotion(reducedMotion);
  const stats = useStreamDiagnosticsStore(diagnosticsStore);
  const [advancedOpen, setAdvancedOpen] = useState(false);

  // ── KPI values (GFN parity) ──
  // GAME = frames decoded from stream (server-side game FPS);
  // STREAM = frames rendered on this device.
  // Prefer stats_channel gameFps > decodeFps > renderFps fallback.
  const gameFps = stats.gameFps !== undefined && stats.gameFps > 0
    ? String(stats.gameFps)
    : (stats.decodeFps !== undefined && stats.decodeFps > 0
      ? String(stats.decodeFps)
      : (stats.renderFps > 0 ? String(stats.renderFps) : "--"));
  const streamFps = stats.renderFps > 0 ? String(stats.renderFps) : "--";
  const rttColor = getRttColor(stats.rttMs);
  const pingText = stats.rttMs > 0 ? String(Math.round(stats.rttMs)) : "--";

  const gpuTitle = stats.gpuType && stats.gpuType !== "" ? stats.gpuType : t("stream.stats.title");
  const parsedRegionLabel = stats.serverLocationLabel && stats.serverLocationLabel !== "--"
    ? stats.serverLocationLabel
    : formatServerLocation(
        stats.serverZone,
        stats.serverRegion || serverRegion || "",
      );
  const regionLabel = parsedRegionLabel !== "--" ? parsedRegionLabel : (userSelectedRegionName || "--");

  // ── Network section ──
  // Packet loss shown as a percentage over the sampling interval (WebRTC raw
  // packetsLost can go negative from duplicates, so the percent is clamped ≥0).
  const packetLossPct = Math.max(0, stats.packetLossPercent);
  const packetLossColor = getPacketLossColor(packetLossPct);
  const packetLossText = `${packetLossPct.toFixed(2)}%`;
  const totalAvailableMbps = stats.targetBitrateKbps > 0
    ? formatBitrate(stats.targetBitrateKbps)
    : "--";
  const totalUsedText = stats.bitrateKbps > 0 ? formatBitrate(stats.bitrateKbps) : "--";
  const jitterText = stats.jitterMs > 0 ? `${stats.jitterMs.toFixed(1)}ms` : "--";

  // ── Stream section ──
  const resolutionText = stats.resolution && stats.resolution !== ""
    ? stats.resolution
    : (stats.nativeRendererActive ? "Native renderer" : "--");
  const codecText = [stats.codec, stats.colorCodec].filter((v) => v && v !== "").join(", ") || "--";

  const hasLagIssue = stats.lagReason !== "stable" && stats.lagReason !== "unknown";
  const hasPacketLoss = stats.packetLossPercent > 0;
  const hasIssues = hasLagIssue || hasPacketLoss;

  const advancedLines = useMemo(() => {
    const lines: string[] = [];
    lines.push(
      `Decode ${stats.decodeTimeMs.toFixed(1)}ms · Render ${stats.renderTimeMs.toFixed(1)}ms · JitterBuf ${stats.jitterBufferDelayMs.toFixed(1)}ms · Jitter ${stats.jitterMs.toFixed(1)}ms`,
    );
    lines.push(
      `Input queue ${(stats.inputQueueBufferedBytes / 1024).toFixed(1)}KB · peak ${(stats.inputQueuePeakBufferedBytes / 1024).toFixed(1)}KB · drops ${stats.inputQueueDropCount} · sched ${stats.inputQueueMaxSchedulingDelayMs.toFixed(1)}ms · residual ${stats.mouseResidualMagnitude.toFixed(2)}px`,
    );
    lines.push(
      `Mouse flush ${stats.mouseFlushIntervalMs.toFixed(0)}ms · ${stats.mousePacketsPerSecond}/s · PR ${stats.partiallyReliableInputOpen ? `${stats.mouseMoveTransport} · ${(stats.partiallyReliableInputQueueBufferedBytes / 1024).toFixed(1)}KB` : "off"}`,
    );
    lines.push(
      gstreamerEnabled
        ? `GStreamer enabled · ${stats.nativeRendererActive ? "in use" : "not active"}`
        : "GStreamer disabled · Chromium WebRTC",
    );
    const hwLine = [stats.hardwareAcceleration, stats.gpuType].filter(Boolean).join(" · ");
    if (hwLine) lines.push(hwLine);
    if (stats.decoderPressureActive || stats.decoderRecoveryAttempts > 0) {
      lines.push(
        `Decoder recovery ${stats.decoderPressureActive ? "active" : "idle"} · attempts ${stats.decoderRecoveryAttempts} · action ${stats.decoderRecoveryAction}`,
      );
    }
    if (stats.nativeTransitionSummary || stats.nativeQueueMode || stats.nativeCapsFramerate) {
      lines.push(
        `Native transition ${stats.nativeTransitionSummary ?? "none"} · queue ${stats.nativeQueueMode ?? "unknown"} · caps ${stats.nativeCapsFramerate ?? "unknown"}`,
      );
    }
    if (hasLagIssue) {
      lines.push(`Lag source ${getLagReasonLabel(stats.lagReason).toLowerCase()} · ${stats.lagReasonDetail}`);
    }
    return lines;
  }, [gstreamerEnabled, hasLagIssue, stats]);

  const kpiRow = (
    <div className="sv-stats-kpis">
      <div className="sv-stats-kpi-card">
        <span className="sv-stats-kpi-num">{gameFps}</span>
        <span className="sv-stats-kpi-unit">{t("stream.stats.fpsUnit")}</span>
        <span className="sv-stats-kpi-name">{t("stream.stats.game")}</span>
      </div>
      <div className="sv-stats-kpi-card">
        <span className="sv-stats-kpi-num">{streamFps}</span>
        <span className="sv-stats-kpi-unit">{t("stream.stats.fpsUnit")}</span>
        <span className="sv-stats-kpi-name">{t("stream.stats.stream")}</span>
      </div>
      <div className="sv-stats-kpi-card">
        <span className="sv-stats-kpi-num" style={{ color: rttColor }}>{pingText}</span>
        <span className="sv-stats-kpi-unit">{t("stream.stats.msUnit")}</span>
        <span className="sv-stats-kpi-name">{t("stream.stats.ping")}</span>
      </div>
    </div>
  );

  return (
    <m.aside
      className={[
        "sv-stats",
        `sv-stats--${mode}`,
        `sv-stats--pos-${position}`,
        hasIssues ? "sv-stats--warn" : "",
        hintsVisible ? "sv-stats--hints" : "",
      ]
        .filter(Boolean)
        .join(" ")}
      initial={{ opacity: 0, y: 8 }}
      animate={{ opacity: 1, y: 0 }}
      exit={{ opacity: 0, y: 6 }}
      transition={surfaceRevealTransition}
      aria-label={t("stream.stats.overlayLabel")}
    >
      <header className="sv-stats-head">
        <span className="sv-stats-head-accent" aria-hidden />
        <span className="sv-stats-head-title">{gpuTitle}</span>
        {hasIssues && (
          <m.span
            className="sv-stats-alert-dot"
            aria-hidden
            animate={statusPulseMotion.animate}
            transition={statusPulseMotion.transition}
          >
            <AlertTriangle size={12} />
          </m.span>
        )}
      </header>

      {kpiRow}

      {mode === "compact" ? (
        <div className="sv-stats-serverbar" title={regionLabel}>
          {regionLabel}
        </div>
      ) : (
        <div className="sv-stats-full">
          <section className="sv-stats-section">
            <h4 className="sv-stats-section-title">{t("stream.stats.network")}</h4>
            <p className="sv-stats-subhead">{t("stream.stats.stability")}</p>
            <div className="sv-stats-row">
              <span>{t("stream.stats.packetLoss")}</span>
              <span style={{ color: hasPacketLoss ? packetLossColor : undefined }}>{packetLossText}</span>
            </div>
            <div className="sv-stats-row">
              <span>Jitter</span>
              <span>{jitterText}</span>
            </div>
            <p className="sv-stats-subhead">{t("stream.stats.bandwidth")}</p>
            <div className="sv-stats-row">
              <span>{t("stream.stats.totalAvailable")}</span>
              <span>{totalAvailableMbps}</span>
            </div>
            <div className="sv-stats-row">
              <span>{t("stream.stats.totalUsed")}</span>
              <span>{totalUsedText}</span>
            </div>
          </section>

          <section className="sv-stats-section">
            <h4 className="sv-stats-section-title">{t("stream.stats.streamSection")}</h4>
            <div className="sv-stats-row">
              <span>{t("stream.stats.resolution")}</span>
              <span>{resolutionText}</span>
            </div>
            <div className="sv-stats-row">
              <span>{t("stream.stats.codec")}</span>
              <span>{codecText}</span>
            </div>
            <div className="sv-stats-row">
              <span>{t("stream.stats.serverLocation")}</span>
              <span>{regionLabel}</span>
            </div>
            {sessionTimeRemainingText && (
              <div className="sv-stats-row">
                <span>{t("stream.stats.timeRemainingShort")}</span>
                <span>{sessionTimeRemainingText}</span>
              </div>
            )}
          </section>

          {advancedLines.length > 0 && (
            <div className="sv-stats-advanced">
              <button
                type="button"
                className="sv-stats-advanced-toggle"
                onClick={() => setAdvancedOpen((v) => !v)}
                aria-expanded={advancedOpen}
              >
                {advancedOpen ? t("stream.stats.hideAdvanced") : t("stream.stats.showAdvanced")}
              </button>
              <AnimatePresence initial={false}>
                {advancedOpen && (
                  <m.div
                    key="advanced"
                    className="sv-stats-advanced-body"
                    initial={{ height: 0, opacity: 0 }}
                    animate={{ height: "auto", opacity: 1 }}
                    exit={{ height: 0, opacity: 0 }}
                    transition={disclosureTransition}
                  >
                    {advancedLines.map((line) => (
                      <p key={line} className="sv-stats-foot">{line}</p>
                    ))}
                  </m.div>
                )}
              </AnimatePresence>
            </div>
          )}
        </div>
      )}
    </m.aside>
  );
}
