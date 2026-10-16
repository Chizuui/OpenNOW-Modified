export type VideoCodec = "H264" | "H265" | "AV1";
/**
 * User-facing codec selection. `"auto"` lets the client pick the best codec
 * the device can actually decode at stream start (mirrors GFN web's
 * "Auto (AV1)" dropdown). Concrete values are explicit user choices and are
 * forced even when the device reports them unsupported.
 */
export type CodecPreference = "auto" | VideoCodec;
/**
 * User-facing fallback codec selection (web mode). When the requested/auto
 * codec cannot be negotiated on a given server (e.g. AV1 on some YES edges),
 * the stream falls back to a decodable codec. `"auto"` keeps every supported
 * GFN primary as a fallback (GFN-web behavior); a concrete value pins that
 * codec first so it wins whenever the primary codec fails.
 */
export type FallbackCodecPreference = "auto" | VideoCodec;
export type VideoAccelerationPreference = "auto" | "hardware" | "software";
export type StreamClientMode = "web" | "native";
/**
 * User-selectable jitter buffer aggressiveness (web mode).
 * - `"low"`: minimal buffering, lowest latency, most sensitive to jitter spikes.
 * - `"balanced"`: modest floor (~2 frames) plus RTT-adaptive growth (default).
 * - `"smooth"": larger floor that absorbs jitter spikes at the cost of latency.
 */
export type JitterBufferMode = "low" | "balanced" | "smooth";

export const JITTER_BUFFER_MODES: readonly JitterBufferMode[] = ["low", "balanced", "smooth"] as const;

/** Normalize an unknown persisted value to a valid jitter buffer mode. */
export function normalizeJitterBufferMode(raw: unknown): JitterBufferMode {
  return JITTER_BUFFER_MODES.includes(raw as JitterBufferMode)
    ? (raw as JitterBufferMode)
    : "balanced";
}

/**
 * Server-side dynamic streaming mode (`vqos.dynamicStreamingMode` in the NVST
 * SDP / `requestedStreamingFeatures.dynamicStreamingMode` in the CloudMatch
 * session request), mirroring the official client's DynamicStreamingMode enum:
 * - 0: off — the server keeps the requested resolution/FPS (no DRC/DFC).
 * - 1: prefer FPS — the server may lower resolution to protect frame rate.
 * - 2: prefer resolution — client-informed DFC lowers FPS before resolution.
 * - 3: dynamic — official default; DRC + DFC adapt both.
 */
export type DynamicStreamingMode = 0 | 1 | 2 | 3;

export const DYNAMIC_STREAMING_MODES: readonly DynamicStreamingMode[] = [0, 1, 2, 3] as const;

/** Normalize an unknown persisted value to a valid dynamic streaming mode. */
export function normalizeDynamicStreamingMode(raw: unknown): DynamicStreamingMode {
  if (raw === null || raw === undefined) {
    return 3;
  }
  const value = Number(raw);
  if (!Number.isFinite(value)) {
    return 3;
  }
  const mode = Math.trunc(value);
  return mode >= 0 && mode <= 3 ? (mode as DynamicStreamingMode) : 3;
}
/**
 * How the server-side session should present launched games.
 * Mirrors the official client's AppLaunchMode: TV/console clients request
 * "gamepadFriendly" so launchers (e.g. Steam) start in big picture mode.
 */
export type AppLaunchMode = "default" | "gamepadFriendly" | "touchFriendly";
export type NativeQueueMode = "auto" | "fixed" | "adaptive" | "vrr";

/** Color quality (bit depth + chroma subsampling), matching Rust ColorQuality enum */
export type ColorQuality = "8bit_420" | "8bit_444" | "10bit_420" | "10bit_444";

/** Helper: get CloudMatch bitDepth value (0 = 8-bit, 1 = 10-bit) */
export function colorQualityBitDepth(cq: ColorQuality): number {
  return cq.startsWith("10bit") ? 1 : 0;
}

/** Helper: get CloudMatch chromaFormat value (0 = 4:2:0, 1 = 4:4:4) */
export function colorQualityChromaFormat(cq: ColorQuality): number {
  return cq.endsWith("444") ? 1 : 0;
}

/** Helper: does this color quality mode require HEVC or AV1? */
export function colorQualityRequiresHevc(cq: ColorQuality): boolean {
  return cq !== "8bit_420";
}

export const USER_FACING_VIDEO_CODEC_OPTIONS: readonly VideoCodec[] = ["H264", "H265", "AV1"];
export const USER_FACING_COLOR_QUALITY_OPTIONS: readonly ColorQuality[] = ["8bit_420", "8bit_444", "10bit_420", "10bit_444"];

/** GFN-web ordering for the codec dropdown: Auto, AV1, H.264, H.265. */
export const CODEC_PREFERENCE_OPTIONS: readonly CodecPreference[] = ["auto", "AV1", "H264", "H265"];

/** Fallback-codec dropdown ordering: Auto, then most-compatible-first. */
export const FALLBACK_CODEC_PREFERENCE_OPTIONS: readonly FallbackCodecPreference[] = [
  "auto",
  "H264",
  "H265",
  "AV1",
];

/** Auto-resolution priority (GFN web auto-picks AV1 first). */
export const AUTO_CODEC_PREFERENCE_ORDER: readonly VideoCodec[] = ["AV1", "H264", "H265"];

export function isSupportedUserFacingCodec(codec: VideoCodec): boolean {
  return USER_FACING_VIDEO_CODEC_OPTIONS.includes(codec);
}

/** Normalize an unknown persisted value to a valid codec preference (defaults to "auto"). */
export function normalizeCodecPreference(raw: unknown): CodecPreference {
  return raw === "auto" || isSupportedUserFacingCodec(raw as VideoCodec)
    ? (raw as CodecPreference)
    : "auto";
}

/** Normalize an unknown persisted fallback-codec value (defaults to "auto"). */
export function normalizeFallbackCodecPreference(raw: unknown): FallbackCodecPreference {
  return raw === "auto" || isSupportedUserFacingCodec(raw as VideoCodec)
    ? (raw as FallbackCodecPreference)
    : "auto";
}

export function normalizeStreamPreferences(codec: CodecPreference, colorQuality: ColorQuality): {
  codec: CodecPreference;
  colorQuality: ColorQuality;
  migrated: boolean;
} {
  const normalizedCodec = normalizeCodecPreference(codec);
  const normalizedColorQuality = USER_FACING_COLOR_QUALITY_OPTIONS.includes(colorQuality)
    ? colorQuality
    : USER_FACING_COLOR_QUALITY_OPTIONS[0];
  // "auto" can resolve to H265/AV1 which support 10-bit modes, so only a
  // concrete H264 choice is pinned back to 8-bit 4:2:0.
  const codecCompatibleColorQuality = normalizedCodec === "H264" ? "8bit_420" : normalizedColorQuality;

  return {
    codec: normalizedCodec,
    colorQuality: codecCompatibleColorQuality,
    migrated: normalizedCodec !== codec || codecCompatibleColorQuality !== colorQuality,
  };
}

/** Helper: is this a 10-bit (HDR-capable) mode? */
export function colorQualityIs10Bit(cq: ColorQuality): boolean {
  return cq.startsWith("10bit");
}

export interface StreamingFeatures {
  reflex?: boolean;
  bitDepth?: number;
  cloudGsync?: boolean;
  chromaFormat?: number;
  enabledL4S?: boolean;
  trueHdr?: boolean;
}

export interface NativeTransitionDiagnostics {
  disableDynamicSplitEncodeUpdates?: boolean;
  forceQueueMode?: NativeQueueMode;
  disableTransitionFlushEscalation?: boolean;
}
