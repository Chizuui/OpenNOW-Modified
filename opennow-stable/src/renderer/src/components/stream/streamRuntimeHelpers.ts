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

export type RecordingStrategy = "raw-track" | "canvas-downscale";

export interface RecordingStrategyChoice {
  strategy: RecordingStrategy;
  mimeType: string;
  /** True when the chosen encoder runs on hardware (power-efficient). */
  hwAccelerated: boolean;
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
 * "raw-track": the raw WebRTC video track goes straight into MediaRecorder
 * (GFN-native model). "canvas-downscale": a downscaled canvas track bounds
 * the encode cost.
 *
 * Raw-track is ONLY chosen when `avc1` is supported AND the encoder is
 * hardware (power-efficient per MediaCapabilities). `isTypeSupported` alone
 * is not enough: on machines without Media Foundation / VideoToolbox / VAAPI
 * hardware AVC encode, Chromium silently falls back to the software OpenH264
 * encoder, and a full-resolution software re-encode of a 1080p60 stream
 * saturates the CPU that also runs the WebRTC decoder — the stream drops to
 * single-digit FPS while recording (the field report). Software encodes
 * instead go through the bounded 720p30 canvas downscale.
 */
export async function selectRecordingStrategy(
  isTypeSupported: (mimeType: string) => boolean,
  encodeProbe: RecordingEncodeProbe = defaultRecordingEncodeProbe,
): Promise<RecordingStrategyChoice> {
  const mimeType = selectRecordingMimeType(isTypeSupported);
  if (!mimeType.includes("avc1")) {
    return { strategy: "canvas-downscale", mimeType, hwAccelerated: false };
  }
  try {
    const info = await encodeProbe({
      contentType: mimeType,
      width: 1920,
      height: 1080,
      bitrate: 12_000_000,
      framerate: 60,
    });
    if (info?.powerEfficient) {
      return { strategy: "raw-track", mimeType, hwAccelerated: true };
    }
  } catch {
    // Probe unavailable (old runtime / sandbox) — fall through to the safe
    // canvas path rather than risk a software full-res encode.
  }
  return { strategy: "canvas-downscale", mimeType, hwAccelerated: false };
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
