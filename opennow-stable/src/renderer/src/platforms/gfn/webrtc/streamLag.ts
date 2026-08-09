import type { StreamLagReason } from "./streamDiagnosticsTypes";

export interface ClassifyStreamLagReasonParams {
  nativeInputActive: boolean;
  nativeRendererActive: boolean;
  framesReceived: number;
  framesDecoded: number;
  decodeTimeMs: number;
  decodeFps: number;
  renderFps: number;
  rttMs: number;
  packetLossPercent: number;
  jitterMs: number;
  jitterBufferDelayMs: number;
  inputQueueBufferedBytes: number;
  inputQueueDropCount: number;
  decoderPressureActive: boolean;
  decoderPressureReason: string;
  decoderBacklogFrames: number;
  dropRatePercent: number;
  backpressureThresholdBytes: number;
}

/** RTT (ms) at or above this is considered a spike worth surfacing in the HUD. */
export const RTT_SPIKE_MIN_MS = 80;
/** RTT must at least double vs the previous sample to be called a spike. */
export const RTT_SPIKE_MULTIPLIER = 2;
/**
 * Packet loss (%) above which the HUD banner should appear. Sub-threshold loss
 * (1 packet in 10k = 0.01%) is normal noise on real links — the rest of the
 * app only warns at ≥0.15% (getPacketLossColor) and flags network lag at ≥1%.
 */
export const PACKET_LOSS_BANNER_PERCENT = 0.15;

/**
 * Packet loss (%) above which the NATIVE HUD banner appears, driven by the
 * server-reported stats_channel field. The server's own measurement is a
 * cleaner send-side signal than the receiver-computed WebRTC getStats loss
 * (no duplicate/RTX/NACK-counting noise), so the native threshold is lower
 * than the web one. Healthy native sessions sit at ~0.02-0.05%; 0.1% is
 * 2-5× that floor and still far below anything concerning, so it fires only
 * on real degradation instead of never firing like the 0.15% web threshold
 * would for the server's cleaner numbers.
 */
export const NATIVE_PACKET_LOSS_BANNER_PERCENT = 0.1;

/**
 * True when the current RTT jumped sharply versus the previous sample — a
 * sudden "ping tinggi banget tiba-tiba" event, as opposed to a gradual rise.
 * Mirrors the log-based spike detector in GfnWebRtcClient.collectStats so the
 * HUD and the exported log agree on what counts as a spike.
 */
export function isRttSpike(previousRttMs: number, currentRttMs: number): boolean {
  return (
    currentRttMs >= RTT_SPIKE_MIN_MS
    && previousRttMs > 0
    && currentRttMs >= previousRttMs * RTT_SPIKE_MULTIPLIER
  );
}

/** Classify overlay lag warnings using sustained pressure signals, not timer jitter or normal decode times. */
export function classifyStreamLagReason(
  params: ClassifyStreamLagReasonParams,
): { reason: StreamLagReason; detail: string } {
  if (params.nativeInputActive || params.nativeRendererActive) {
    return {
      reason: "stable",
      detail: "Native streamer input bridge active",
    };
  }

  const networkSignals: string[] = [];
  if (params.packetLossPercent >= 1) networkSignals.push(`${params.packetLossPercent.toFixed(1)}% loss`);
  if (params.rttMs >= 75) networkSignals.push(`RTT ${params.rttMs.toFixed(0)}ms`);
  if (params.jitterMs >= 12) networkSignals.push(`jitter ${params.jitterMs.toFixed(1)}ms`);
  // The client keeps an intentional jitter buffer floor (30-100ms, RTT-scaled),
  // so a small buffer reading is expected and healthy. Only a buffer that has
  // grown well past even the maximum floor indicates real congestion.
  if (params.jitterBufferDelayMs >= 150) networkSignals.push(`buffer ${params.jitterBufferDelayMs.toFixed(1)}ms`);
  if (networkSignals.length > 0) {
    return {
      reason: "network",
      detail: networkSignals.join(" · "),
    };
  }

  const severeDecoderStall = params.framesReceived > 100 && params.framesDecoded === 0;
  if (params.decoderPressureActive || severeDecoderStall) {
    const detailParts: string[] = [];
    if (severeDecoderStall) detailParts.push("frames received but not decoded");
    if (params.decoderPressureReason === "decode_saturated" && params.decodeTimeMs > 0) {
      detailParts.push(`decode ${params.decodeTimeMs.toFixed(1)}ms`);
    }
    if (params.decoderBacklogFrames >= 45) detailParts.push(`backlog ${params.decoderBacklogFrames}`);
    if (params.dropRatePercent >= 6) detailParts.push(`${params.dropRatePercent.toFixed(1)}% drops`);
    if (detailParts.length === 0 && params.decoderPressureReason !== "stable") {
      detailParts.push(params.decoderPressureReason.replace(/_/g, " "));
    }
    return {
      reason: "decoder",
      detail: detailParts.join(" · ") || "decode pressure",
    };
  }

  if (
    params.inputQueueDropCount > 0
    || params.inputQueueBufferedBytes >= params.backpressureThresholdBytes
  ) {
    const detailParts: string[] = [];
    if (params.inputQueueDropCount > 0) detailParts.push(`drops ${params.inputQueueDropCount}`);
    if (params.inputQueueBufferedBytes >= params.backpressureThresholdBytes) {
      detailParts.push(`buffered ${(params.inputQueueBufferedBytes / 1024).toFixed(1)}KB`);
    }
    return {
      reason: "input_backpressure",
      detail: detailParts.join(" · "),
    };
  }

  if (params.renderFps > 0 && params.decodeFps > 0) {
    const renderGap = params.decodeFps - params.renderFps;
    const renderGapPercent = renderGap / params.decodeFps;
    // Absolute fps gaps are misleading at 120/240fps streams — require a large relative drop.
    const renderPressure =
      params.renderFps < 30
      || (renderGap >= 20 && renderGapPercent >= 0.2);
    if (renderPressure) {
      return {
        reason: "render",
        detail: `render ${params.renderFps}fps vs decode ${params.decodeFps}fps`,
      };
    }
  }

  return {
    reason: params.decodeFps > 0 || params.renderFps > 0 ? "stable" : "unknown",
    detail: params.decodeFps > 0 || params.renderFps > 0
      ? "No dominant lag source detected"
      : "Waiting for stream stats",
  };
}
