/**
 * Extract codec name from codecId string (e.g., "VP09" -> "VP9", "AV1X" -> "AV1")
 */
export function normalizeCodecName(codecId: string): string {
  const upper = codecId.toUpperCase();

  if (upper.startsWith("H264") || upper === "H264") {
    return "H264";
  }
  if (upper.startsWith("H265") || upper === "H265" || upper.startsWith("HEVC")) {
    return "H265";
  }
  if (upper.startsWith("AV1")) {
    return "AV1";
  }
  if (upper.startsWith("VP9") || upper.startsWith("VP09")) {
    return "VP9";
  }
  if (upper.startsWith("VP8")) {
    return "VP8";
  }

  return codecId;
}

/** Map WebRTC codec mimeType (or codecId fallback) to a display codec label. */
export function codecLabelFromMimeType(mimeType: string, codecId?: string): string {
  if (mimeType.includes("H264")) {
    return "H264";
  }
  if (mimeType.includes("H265") || mimeType.includes("HEVC")) {
    return "H265";
  }
  if (mimeType.includes("AV1")) {
    return "AV1";
  }
  if (mimeType.includes("VP9")) {
    return "VP9";
  }
  if (mimeType.includes("VP8")) {
    return "VP8";
  }
  if (codecId) {
    return normalizeCodecName(codecId);
  }
  return mimeType || "Unknown";
}

/**
 * Detect GPU type using browser APIs
 * Uses WebGL renderer string to identify GPU vendor/model
 */
export function detectGpuType(): string {
  try {
    const canvas = document.createElement("canvas");
    const gl = canvas.getContext("webgl2") || canvas.getContext("webgl");
    if (!gl) {
      return "Unknown";
    }

    const debugInfo = gl.getExtension("WEBGL_debug_renderer_info");
    if (debugInfo) {
      const vendor = gl.getParameter(debugInfo.UNMASKED_VENDOR_WEBGL);
      const renderer = gl.getParameter(debugInfo.UNMASKED_RENDERER_WEBGL);

      // Clean up renderer string - extract main GPU name
      let gpuName = renderer;

      // Remove common prefixes/suffixes for cleaner display
      gpuName = gpuName
        .replace(/\(R\)/g, "")
        .replace(/\(TM\)/g, "")
        .replace(/NVIDIA /i, "")
        .replace(/AMD /i, "")
        .replace(/Intel /i, "")
        .replace(/Microsoft Corporation - /i, "")
        .replace(/D3D12 /i, "")
        .replace(/Direct3D11 /i, "")
        .replace(/OpenGL Engine/i, "")
        .trim();

      // Limit length
      if (gpuName.length > 30) {
        gpuName = gpuName.substring(0, 27) + "...";
      }

      return gpuName || vendor || "Unknown";
    }
    return "Unknown";
  } catch {
    return "Unknown";
  }
}

/**
 * EWMA smoothing factor for the HUD jitter readout. WebRTC reports the raw
 * RFC 3550 interarrival jitter of the incoming video stream; per-packet
 * arrival variance makes that value jump between polls, so the HUD applies
 * an exponential moving average — the same spirit as the native streamer's
 * preference for rtpsession's `avg-jitter` over the raw `jitter` field.
 * 0.35 per ~1s poll stays responsive to sustained changes while clipping
 * single-sample spikes.
 */
export const JITTER_EWMA_ALPHA = 0.35;

/**
 * Inputs for the BWE-anomaly warning decision. See `shouldWarnBweLow`.
 */
export interface BweLowWarnCheck {
  /** Receiver bandwidth estimate (kbps) from availableIncomingBitrate. */
  availableKbps: number;
  /** Measured link RTT (ms). */
  rttMs: number;
  /** Measured packet loss (%). */
  packetLossPercent: number;
  /** Measured receive bitrate (kbps) — proves the stream is actually flowing. */
  measuredBitrateKbps: number;
  /** Milliseconds since the session started (BWE-ramp grace). */
  sessionAgeMs: number;
  /** One-shot guard: only warn once per session. */
  alreadyWarned: boolean;
  /** Below this estimate (kbps) the server is under-using the link. */
  floorKbps: number;
  /** BWE ramp grace after session start — a slow ramp is not a false positive. */
  graceMs: number;
  /** RTT (ms) below which the link counts as healthy. */
  healthyRttMs: number;
  /** Packet loss (%) below which the link counts as healthy. */
  healthyLossPct: number;
}

/**
 * Whether to warn that the receiver's bandwidth estimate is stuck BELOW the
 * bitrate floor the client negotiates (`vqos.bw.minimumBitrateKbps`, ~4000
 * kbps) while the link is healthy. On a healthy path the estimate should sit
 * far above the floor; a stuck-low value usually means the server is not
 * ramping its encoder — and the first thing to check is whether transport-cc
 * was negotiated, because the server BWE is blind without TWCC feedback (the
 * native streamer's ~3.4 Mbps root cause). Pure + unit-tested.
 */
export function shouldWarnBweLow(check: BweLowWarnCheck): boolean {
  if (check.alreadyWarned) return false;
  // No real estimate yet (0 / placeholder) or above the floor: nothing to say.
  if (check.availableKbps <= 0 || check.availableKbps >= check.floorKbps) return false;
  // Only on a healthy link — a low estimate under congestion is expected.
  if (check.rttMs <= 0 || check.rttMs >= check.healthyRttMs) return false;
  if (check.packetLossPercent >= check.healthyLossPct) return false;
  // Only once the stream is actually receiving (BWE is meaningless before).
  if (check.measuredBitrateKbps <= 0) return false;
  // Allow the startup BWE ramp to finish before judging it stuck.
  if (check.sessionAgeMs < check.graceMs) return false;
  return true;
}

/**
 * Exponential moving average of a raw WebRTC jitter reading (ms). The first
 * sample is returned as-is so the readout appears immediately instead of
 * ramping up from 0; afterwards it is blended with the previous EWMA so a
 * one-off spike does not move the HUD much while a sustained shift still
 * lands within a few polls.
 */
export function smoothJitterMs(
  rawJitterMs: number,
  prevEwmaMs: number,
  alpha: number = JITTER_EWMA_ALPHA,
): number {
  if (prevEwmaMs <= 0) {
    return Math.max(0, rawJitterMs);
  }
  return alpha * Math.max(0, rawJitterMs) + (1 - alpha) * prevEwmaMs;
}

/** Average jitter buffer delay in ms from cumulative WebRTC inbound-rtp counters. */
export function averageJitterBufferDelayMs(
  jitterBufferDelaySeconds: number,
  jitterBufferEmittedCount: number,
): number | null {
  if (jitterBufferEmittedCount <= 0) {
    return null;
  }
  return Math.round((jitterBufferDelaySeconds / jitterBufferEmittedCount) * 1000 * 10) / 10;
}

export interface IntervalFrameRates {
  /** Frames received from the network per second (≈ server-sent rate). */
  receiveFps: number;
  /** Frames decoded locally per second. */
  decodeFps: number;
  /** Average milliseconds to decode one frame over the interval. */
  decodeTimeMs: number;
}

export interface IntervalFrameRateParams {
  framesReceived: number;
  framesDecoded: number;
  totalDecodeTime: number;
  prevFramesReceived: number;
  prevFramesDecoded: number;
  prevTotalDecodeTime: number;
  timeDeltaMs: number;
  prevReceiveFps: number;
  prevDecodeFps: number;
  prevDecodeTimeMs: number;
}

/**
 * Frame rates from per-interval WebRTC inbound-rtp deltas. `receiveFps` is
 * what the server sent, `decodeFps` is what the local decoder produced.
 * Chromium's raw `framesPerSecond` is a coarse sliding window that dips
 * (30/45/0) on static frames and lags behind the real rate, so deltas over
 * the ~1s poll window are both accurate and stable.
 *
 * Behavior on quiet intervals:
 * - Nothing new arrived (static frame): both rates keep their last value.
 * - Frames arriving but none decoded: decodeFps drops to 0 — the decoder is
 *   genuinely behind (stall/backlog), so the HUD must not show a stale rate.
 */
export function computeIntervalFrameRates(params: IntervalFrameRateParams): IntervalFrameRates {
  if (params.timeDeltaMs <= 0) {
    return {
      receiveFps: params.prevReceiveFps,
      decodeFps: params.prevDecodeFps,
      decodeTimeMs: params.prevDecodeTimeMs,
    };
  }

  const receivedDelta = Math.max(0, params.framesReceived - params.prevFramesReceived);
  const decodedDelta = Math.max(0, params.framesDecoded - params.prevFramesDecoded);

  const receiveFps = receivedDelta > 0
    ? Math.round((receivedDelta * 1000) / params.timeDeltaMs)
    : params.prevReceiveFps;

  let decodeFps = params.prevDecodeFps;
  if (decodedDelta > 0) {
    decodeFps = Math.round((decodedDelta * 1000) / params.timeDeltaMs);
  } else if (receivedDelta > 0) {
    decodeFps = 0;
  }

  let decodeTimeMs = params.prevDecodeTimeMs;
  const decodeTimeDelta = params.totalDecodeTime - params.prevTotalDecodeTime;
  if (decodedDelta > 0 && decodeTimeDelta > 0) {
    decodeTimeMs = Math.round((decodeTimeDelta / decodedDelta) * 1000 * 10) / 10;
  }

  return { receiveFps, decodeFps, decodeTimeMs };
}

/**
 * The official GFN client translates the server's raw gpuType code (e.g.
 * "2080d / T10") into a friendly rig name via a server-provided gpuNameMap
 * (cloud variable `enableGpuNameMappingV2`). OpenNOW has no access to that
 * cloud variable, so mirror the observed map locally so the HUD shows the
 * same "GeForce RTX 3080"-style label the official client does instead of the
 * raw code. Keys are exact gpuType values captured from CloudMatch payloads;
 * unknown values pass through unchanged.
 */
const SERVER_GPU_NAME_MAP: Record<string, string> = {
  // Basic Rig (Performance tier, older / entry-class hardware)
  "1060b / T10-8": "Basic Rig",
  "1060bi / T10-8": "Basic Rig",
  "1060c / T10-8": "Basic Rig",
  "1080d / P40": "Basic Rig",
  "2080c / T10": "Basic Rig",
  "3050b / L40-6": "Basic Rig",
  "3050b / L40G-6": "Basic Rig",
  "3050b / L40S-6": "Basic Rig",
  "3050b / A10G-6": "Basic Rig",
  // GeForce RTX 20-series
  "2060c / L40G-8": "GeForce RTX 2060",
  "2080d / T10": "GeForce RTX",
  "2080h / T10": "GeForce RTX",
  // GeForce RTX 30-series
  "3060d / L40-24": "GeForce RTX",
  "3060d / L40-12": "GeForce RTX",
  "3060d / L40G-12": "GeForce RTX",
  "3060d / L40S-12": "GeForce RTX",
  "3060d / L40S-24": "GeForce RTX",
  "3060d / A10G-12": "GeForce RTX",
  "3080h / A10G": "GeForce RTX 3080",
  "3080p / A10Gx2": "GeForce RTX 3080",
  // GeForce RTX 40-series
  "4080h / L40": "GeForce RTX 4080",
  "4080h / L40G": "GeForce RTX 4080",
  "4080h / L40S": "GeForce RTX 4080",
  "4080p / L40x2": "GeForce RTX 4080",
  "4080p / L40Gx2": "GeForce RTX 4080",
  "4080p / L40Sx2": "GeForce RTX 4080",
  // GeForce RTX 50-series
  "5080h / B40": "GeForce RTX 5080",
};

/** Map a raw CloudMatch gpuType code to the official rig name (unknown → unchanged). */
export function mapServerGpuType(raw: string): string {
  const trimmed = raw.trim();
  if (!trimmed) {
    return "";
  }
  return SERVER_GPU_NAME_MAP[trimmed] ?? trimmed;
}
