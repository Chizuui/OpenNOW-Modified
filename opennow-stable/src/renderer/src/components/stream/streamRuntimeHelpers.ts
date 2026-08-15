import { normalizeShortcut } from "../../shortcuts";

export const RECORDING_MIME_TYPES = [
  "video/mp4;codecs=avc1.42E01E,mp4a.40.2",
  "video/mp4;codecs=avc1",
  "video/mp4",
  "video/webm;codecs=h264",
  "video/webm;codecs=vp8",
  "video/webm",
] as const;

export function getShortcutConflictError(
  rawValue: string,
  reservedShortcuts: readonly (string | undefined)[],
): string | null {
  const trimmed = rawValue.trim();
  if (!trimmed) {
    return "Shortcut cannot be empty.";
  }

  const normalized = normalizeShortcut(trimmed);
  if (!normalized.valid) {
    return "Invalid shortcut format.";
  }

  const reserved = reservedShortcuts
    .filter((value): value is string => typeof value === "string" && value.trim().length > 0)
    .map((value) => normalizeShortcut(value))
    .filter((parsed) => parsed.valid)
    .map((parsed) => parsed.canonical);

  return reserved.includes(normalized.canonical)
    ? "Shortcut conflicts with an existing binding."
    : null;
}

export function selectRecordingMimeType(
  isTypeSupported: (mimeType: string) => boolean,
): string {
  return RECORDING_MIME_TYPES.find(isTypeSupported) ?? "video/webm";
}

export type RecordingStrategy = "encoded-transform" | "raw-track" | "canvas-downscale";

export const DEFAULT_RECORD_CAP_WIDTH = 1280;
export const DEFAULT_RECORD_CAP_HEIGHT = 720;
export const DEFAULT_RECORD_CAP_FPS = 30;
export const MAX_RECORD_CAP_FPS = 60;

export const RECORD_CAP_BY_RESOLUTION: Record<string, { width: number; height: number }> = {
  "1440p": { width: 2560, height: 1440 },
  "1080p": { width: 1920, height: 1080 },
  "720p": { width: 1280, height: 720 },
};

/** The maximum picture size/rate the user asked the recording to stay within. */
export interface RecordingCap {
  width: number;
  height: number;
  fps: number;
}

/** The live stream as the <video> element sees it at record-start time. */
export interface StreamInfo {
  width: number;
  height: number;
  /** Received frame rate from the track's settings; `null` when unknown. */
  fps: number | null;
}

/**
 * Resolve the user's recordingResolution/recordingFps settings into the cap
 * every recording path is bounded by. The canvas path draws into a canvas of
 * at most this size; the raw-track path is only taken when the stream already
 * fits inside it (see selectRecordingStrategy) — otherwise the user's "720p30"
 * setting is silently meaningless, which is exactly how a full-resolution
 * re-encode used to starve the decode pipeline.
 */
export function resolveRecordCap(resolution: string, fps: number): RecordingCap {
  const { width, height } =
    RECORD_CAP_BY_RESOLUTION[resolution] ?? { width: DEFAULT_RECORD_CAP_WIDTH, height: DEFAULT_RECORD_CAP_HEIGHT };
  const cappedFps =
    Number.isFinite(fps) && fps > 0
      ? Math.min(MAX_RECORD_CAP_FPS, Math.round(fps))
      : DEFAULT_RECORD_CAP_FPS;
  return { width, height, fps: cappedFps };
}

export interface RecordingStrategyChoice {
  strategy: RecordingStrategy;
  mimeType: string;
  /** True when the chosen encoder runs on hardware (power-efficient). */
  hwAccelerated: boolean;
  /** Short human-readable reason for the choice (logged + surfaced in the UI). */
  reason: string;
}

/**
 * MediaCapabilities encode probe, injected so tests can stub it. Defaults to
 * `navigator.mediaCapabilities.encodingInfo` for a MediaRecorder session.
 */
export type RecordingEncodeProbe = (
  config: {
    contentType: string;
    width: number;
    height: number;
    bitrate: number;
    framerate: number;
  },
) => Promise<{ powerEfficient?: boolean } | undefined>;

/**
 * Pick the recording path for web mode.
 *
 * "encoded-transform": the receiver-side encoded transform captures the
 * stream's bitstream pre-decode and muxes it offline (GFN parity). Zero
 * re-encode — the recording can never compete with the decoder, so the
 * user's recording cap does NOT apply ("jangan di cap": the capture is
 * whatever the stream is). Taken whenever the runtime supports receiver
 * encoded transforms and the session's negotiated codec is capturable.
 *
 * "raw-track": the raw WebRTC video track goes straight into MediaRecorder
 * (no canvas, no draw loop). "canvas-downscale": a downscaled canvas track
 * bounds the encode cost to the user's recording cap.
 *
 * Raw-track is chosen ONLY when ALL of these hold:
 * 1. an AVC (avc1) container is supported — the format MediaRecorder can
 *    hardware-encode on Windows/macOS/Linux;
 * 2. the live stream already fits inside the user's recording cap
 *    (resolution AND frame rate). A full-resolution re-encode of a 1080p60
 *    stream competes with the WebRTC decoder for the same GPU/CPU even with a
 *    hardware encoder (the field report: "recording makes the stream lag" on
 *    a device that is otherwise fine) — so when the stream exceeds the cap,
 *    the encode is bounded to the cap instead, honoring the user's
 *    recordingResolution/recordingFps settings;
 * 3. the encoder at stream resolution is hardware (power-efficient per
 *    MediaCapabilities). `isTypeSupported` alone is not enough: on machines
 *    without Media Foundation / VideoToolbox / VAAPI hardware AVC encode,
 *    Chromium silently falls back to the software OpenH264 encoder, and a
 *    full-resolution software re-encode saturates the CPU that also runs the
 *    WebRTC decoder.
 *
 * Everything else goes through the bounded canvas downscale.
 */
export async function selectRecordingStrategy(
  isTypeSupported: (mimeType: string) => boolean,
  stream: StreamInfo,
  cap: RecordingCap,
  encodedTransformAvailable: boolean,
  encodeProbe: RecordingEncodeProbe = defaultRecordingEncodeProbe,
): Promise<RecordingStrategyChoice> {
  if (encodedTransformAvailable) {
    // mimeType placeholder — the caller replaces it with the codec-specific
    // container (video/mp4 or video/webm) derived from the receivers.
    return {
      strategy: "encoded-transform",
      mimeType: "video/mp4",
      hwAccelerated: true,
      reason: "receiver encoded transform available — bitstream captured pre-decode, zero re-encode (cap not applied)",
    };
  }
  const mimeType = selectRecordingMimeType(isTypeSupported);
  if (!mimeType.includes("avc1")) {
    return {
      strategy: "canvas-downscale",
      mimeType,
      hwAccelerated: false,
      reason: "no avc1 container — hardware encode unavailable, bounded canvas used",
    };
  }
  const streamWithinCap =
    stream.width > 0 &&
    stream.height > 0 &&
    stream.width <= cap.width &&
    stream.height <= cap.height &&
    // Unknown frame rate is treated as within-cap: the resolution gate above
    // is the dominant cost factor, and an unknown rate should not force the
    // downscale path on its own.
    (stream.fps === null || stream.fps <= cap.fps);
  if (!streamWithinCap) {
    const streamLabel =
      stream.width > 0 && stream.height > 0
        ? `${stream.width}x${stream.height}@${stream.fps ?? "?"}`
        : "unknown resolution";
    return {
      strategy: "canvas-downscale",
      mimeType,
      hwAccelerated: false,
      reason: `stream ${streamLabel} exceeds recording cap ${cap.width}x${cap.height}@${cap.fps} — encode bounded to cap`,
    };
  }
  try {
    const info = await encodeProbe({
      contentType: mimeType,
      width: stream.width,
      height: stream.height,
      bitrate: 12_000_000,
      framerate: stream.fps ?? cap.fps,
    });
    if (info?.powerEfficient) {
      return {
        strategy: "raw-track",
        mimeType,
        hwAccelerated: true,
        reason: `hardware AVC encoder and stream ${stream.width}x${stream.height}@${stream.fps ?? cap.fps} within cap ${cap.width}x${cap.height}@${cap.fps}`,
      };
    }
    return {
      strategy: "canvas-downscale",
      mimeType,
      hwAccelerated: false,
      reason: "AVC encoder not power-efficient at stream resolution — bounded canvas used",
    };
  } catch {
    // Probe unavailable (old runtime / sandbox) — fall through to the safe
    // canvas path rather than risk a software full-res encode.
    return {
      strategy: "canvas-downscale",
      mimeType,
      hwAccelerated: false,
      reason: "encoder probe unavailable — bounded canvas used",
    };
  }
}

async function defaultRecordingEncodeProbe(
  config: {
    contentType: string;
    width: number;
    height: number;
    bitrate: number;
    framerate: number;
  },
): Promise<{ powerEfficient?: boolean } | undefined> {
  if (!navigator.mediaCapabilities?.encodingInfo) {
    return undefined;
  }
  const result = await navigator.mediaCapabilities.encodingInfo({
    type: "record",
    video: {
      contentType: config.contentType,
      width: config.width,
      height: config.height,
      bitrate: config.bitrate,
      framerate: config.framerate,
    },
  });
  return { powerEfficient: result.powerEfficient };
}

/**
 * Bound the user's recording bitrate per strategy. Canvas-downscale caps at
 * 12 Mbps (720p30 never benefits from more). Raw-track records at stream
 * resolution, so the user's explicit choice is honored up to the settings
 * slider's ceiling. `null` (auto) returns `undefined` — Chromium's
 * conservative resolution-based default is left untouched.
 */
export function clampRecordingBitrate(
  recordingBitrateMbps: number | null,
  strategy: RecordingStrategy,
): number | undefined {
  if (recordingBitrateMbps === null) return undefined;
  const max = strategy === "raw-track" ? 75 : 12;
  return Math.max(1, Math.min(max, Math.round(recordingBitrateMbps))) * 1_000_000;
}

export interface ThumbnailSize {
  width: number;
  height: number;
}

/**
 * How many frames the recording fell short of its target frame rate.
 *
 * The web-mode recorder draws at most one video frame per record-period
 * (canvas downscale + MediaRecorder). When the main thread cannot keep up
 * (drawImage + encode saturate the CPU, the weak-device case), draws happen
 * less often than the target and the recording plays back with repeated
 * frames. This counts the deficit against the wall-clock target; stream-side
 * stalls show up here too, which is the user-facing truth — the recording IS
 * choppier than its nominal rate either way.
 */
export function computeRecordingFrameShortfall(
  drawnFrames: number,
  elapsedMs: number,
  recordFps: number,
): number {
  if (!Number.isFinite(elapsedMs) || elapsedMs <= 0 || !(recordFps > 0)) {
    return 0;
  }
  const expectedFrames = Math.ceil((elapsedMs / 1000) * recordFps);
  return Math.max(0, expectedFrames - Math.max(0, Math.round(drawnFrames)));
}

export function fitThumbnailSize(
  width: number,
  height: number,
  maxWidth = 320,
  maxHeight = 180,
): ThumbnailSize {
  let fittedWidth = width;
  let fittedHeight = height;

  if (fittedWidth > maxWidth) {
    fittedHeight = Math.round((maxWidth / fittedWidth) * fittedHeight);
    fittedWidth = maxWidth;
  }
  if (fittedHeight > maxHeight) {
    fittedWidth = Math.round((maxHeight / fittedHeight) * fittedWidth);
    fittedHeight = maxHeight;
  }

  return { width: fittedWidth, height: fittedHeight };
}
