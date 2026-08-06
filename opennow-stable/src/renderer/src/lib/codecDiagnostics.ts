import {
  AUTO_CODEC_PREFERENCE_ORDER,
  normalizeStreamPreferences,
  type CodecPreference,
  type ColorQuality,
  type VideoCodec,
} from "@shared/gfn";

export interface CodecTestResult {
  codec: string;
  webrtcSupported: boolean;
  decodeSupported: boolean;
  hwAccelerated: boolean;
  encodeSupported: boolean;
  encodeHwAccelerated: boolean;
  decodeVia: string;
  encodeVia: string;
  profiles: string[];
}

const CODEC_TEST_CONFIGS: {
  name: VideoCodec;
  webrtcMime: string;
  decodeContentType: string;
  encodeContentType: string;
}[] = [
  {
    name: "H264",
    webrtcMime: "video/H264",
    decodeContentType: 'video/mp4; codecs="avc1.42E01E"',
    encodeContentType: 'video/mp4; codecs="avc1.42E01E"',
  },
  {
    name: "H265",
    webrtcMime: "video/H265",
    decodeContentType: 'video/mp4; codecs="hev1.1.6.L93.B0"',
    encodeContentType: 'video/mp4; codecs="hev1.1.6.L93.B0"',
  },
  {
    name: "AV1",
    webrtcMime: "video/AV1",
    decodeContentType: 'video/mp4; codecs="av01.0.08M.08"',
    encodeContentType: 'video/mp4; codecs="av01.0.08M.08"',
  },
];

export const CODEC_TEST_RESULTS_STORAGE_KEY = "opennow.codec-test-results.v1";

export function loadStoredCodecResults(): CodecTestResult[] | null {
  try {
    const raw = window.sessionStorage.getItem(CODEC_TEST_RESULTS_STORAGE_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as unknown;
    if (!Array.isArray(parsed)) return null;
    return parsed as CodecTestResult[];
  } catch {
    return null;
  }
}

export function saveStoredCodecResults(results: CodecTestResult[] | null): void {
  try {
    if (results && results.length > 0) {
      window.sessionStorage.setItem(CODEC_TEST_RESULTS_STORAGE_KEY, JSON.stringify(results));
      return;
    }
    window.sessionStorage.removeItem(CODEC_TEST_RESULTS_STORAGE_KEY);
  } catch {
  }
}

function isLinuxArmClient(): boolean {
  const platform = navigator.platform?.toLowerCase() ?? "";
  const ua = navigator.userAgent?.toLowerCase() ?? "";
  const linux = platform.includes("linux") || ua.includes("linux");
  const arm = /(aarch64|arm64|armv\d|arm)/.test(platform) || /(aarch64|arm64|armv\d|arm)/.test(ua);
  return linux && arm;
}

function guessDecodeBackend(hwAccelerated: boolean): string {
  if (!hwAccelerated) return "Software (CPU)";
  const platform = navigator.platform?.toLowerCase() ?? "";
  const ua = navigator.userAgent?.toLowerCase() ?? "";
  if (platform.includes("win") || ua.includes("windows")) return "D3D11 (GPU)";
  if (platform.includes("mac") || ua.includes("macintosh")) return "VideoToolbox (GPU)";
  if (platform.includes("linux") || ua.includes("linux")) {
    return isLinuxArmClient() ? "V4L2 (GPU)" : "VA-API (GPU)";
  }
  return "Hardware (GPU)";
}

function guessEncodeBackend(hwAccelerated: boolean): string {
  if (!hwAccelerated) return "Software (CPU)";
  const platform = navigator.platform?.toLowerCase() ?? "";
  const ua = navigator.userAgent?.toLowerCase() ?? "";
  if (platform.includes("win") || ua.includes("windows")) return "Media Foundation (GPU)";
  if (platform.includes("mac") || ua.includes("macintosh")) return "VideoToolbox (GPU)";
  if (platform.includes("linux") || ua.includes("linux")) {
    return isLinuxArmClient() ? "V4L2 (GPU)" : "VA-API (GPU)";
  }
  return "Hardware (GPU)";
}

export async function testCodecSupport(): Promise<CodecTestResult[]> {
  const results: CodecTestResult[] = [];
  const webrtcCaps = RTCRtpReceiver.getCapabilities?.("video");
  const webrtcCodecMimes = new Set(webrtcCaps?.codecs.map((codec) => codec.mimeType.toLowerCase()) ?? []);
  const webrtcProfiles = new Map<string, string[]>();

  if (webrtcCaps) {
    for (const codec of webrtcCaps.codecs) {
      const mime = codec.mimeType.toLowerCase();
      const sdpLine = (codec as unknown as Record<string, string>).sdpFmtpLine ?? "";
      if (!mime.includes("rtx") && !mime.includes("red") && !mime.includes("ulpfec")) {
        const existing = webrtcProfiles.get(mime) ?? [];
        if (sdpLine) existing.push(sdpLine);
        webrtcProfiles.set(mime, existing);
      }
    }
  }

  for (const config of CODEC_TEST_CONFIGS) {
    const webrtcSupported = webrtcCodecMimes.has(config.webrtcMime.toLowerCase());
    const profiles = webrtcProfiles.get(config.webrtcMime.toLowerCase()) ?? [];

    let decodeSupported = false;
    let hwAccelerated = false;
    try {
      const decodeResult = await navigator.mediaCapabilities.decodingInfo({
        type: "webrtc",
        video: {
          contentType: config.webrtcMime === "video/H265" ? "video/h265" : config.webrtcMime.toLowerCase(),
          width: 1920,
          height: 1080,
          framerate: 60,
          bitrate: 20_000_000,
        },
      });
      decodeSupported = decodeResult.supported;
      hwAccelerated = decodeResult.powerEfficient;
    } catch {
      try {
        const decodeResult = await navigator.mediaCapabilities.decodingInfo({
          type: "file",
          video: {
            contentType: config.decodeContentType,
            width: 1920,
            height: 1080,
            framerate: 60,
            bitrate: 20_000_000,
          },
        });
        decodeSupported = decodeResult.supported;
        hwAccelerated = decodeResult.powerEfficient;
      } catch {
      }
    }

    let encodeSupported = false;
    let encodeHwAccelerated = false;
    try {
      const encodeResult = await navigator.mediaCapabilities.encodingInfo({
        type: "webrtc",
        video: {
          contentType: config.webrtcMime === "video/H265" ? "video/h265" : config.webrtcMime.toLowerCase(),
          width: 1920,
          height: 1080,
          framerate: 60,
          bitrate: 20_000_000,
        },
      });
      encodeSupported = encodeResult.supported;
      encodeHwAccelerated = encodeResult.powerEfficient;
    } catch {
      try {
        const encodeResult = await navigator.mediaCapabilities.encodingInfo({
          type: "record",
          video: {
            contentType: config.encodeContentType,
            width: 1920,
            height: 1080,
            framerate: 60,
            bitrate: 20_000_000,
          },
        });
        encodeSupported = encodeResult.supported;
        encodeHwAccelerated = encodeResult.powerEfficient;
      } catch {
      }
    }

    // Keep `decodeSupported` as the raw mediaCapabilities answer: a codec can be
    // present in the WebRTC receiver capabilities yet not decodable on this
    // device (e.g. H.265 without the HEVC extension), which is exactly what
    // GFN web surfaces as "Unsupported". `webrtcSupported` stays as its own
    // signal and is only used as a fallback when no test results exist.
    results.push({
      codec: config.name,
      webrtcSupported,
      decodeSupported,
      hwAccelerated,
      encodeSupported,
      encodeHwAccelerated,
      decodeVia: decodeSupported ? guessDecodeBackend(hwAccelerated) : "Unsupported",
      encodeVia: encodeSupported ? guessEncodeBackend(encodeHwAccelerated) : "Unsupported",
      profiles,
    });
  }

  return results;
}

export type CodecDecodeBadgeState = "gpu" | "cpu" | "testing" | null;

function isLinuxClient(): boolean {
  const platform = navigator.platform?.toLowerCase() ?? "";
  const ua = navigator.userAgent?.toLowerCase() ?? "";
  return platform.includes("linux") || ua.includes("linux");
}

export function shouldShowLinuxHardwareCodecHint(results: CodecTestResult[] | null): boolean {
  if (!results || results.length === 0 || !isLinuxClient()) {
    return false;
  }

  return results.some((result) => result.decodeSupported && !result.hwAccelerated)
    || results.some((result) => result.encodeSupported && !result.encodeHwAccelerated)
    || results.some((result) => result.codec === "H265" && !result.decodeSupported);
}

export function getCodecDecodeBadgeState(
  codec: VideoCodec,
  codecResults: CodecTestResult[] | null,
  codecTesting: boolean,
): CodecDecodeBadgeState {
  const result = codecResults?.find((entry) => entry.codec === codec);
  if (!result) {
    return codecTesting ? "testing" : null;
  }
  if (!result.decodeSupported) {
    return null;
  }
  return result.hwAccelerated ? "gpu" : "cpu";
}

/**
 * Synchronous availability check via `RTCRtpReceiver.getCapabilities` — the
 * same source the WebRTC layer uses to decide which codecs it can receive.
 */
export function isWebRtcCodecAvailable(codec: VideoCodec): boolean {
  try {
    const caps = RTCRtpReceiver.getCapabilities?.("video");
    if (!caps) return false;
    const mime = ({ H264: "video/H264", H265: "video/H265", AV1: "video/AV1" } as const)[codec].toLowerCase();
    return caps.codecs.some((entry) => entry.mimeType.toLowerCase() === mime);
  } catch {
    return false;
  }
}

/**
 * Whether a codec is usable for streaming. Prefers the diagnostic test results
 * (decode + WebRTC readiness) and falls back to WebRTC receiver capabilities.
 */
export function isCodecUsableForStream(
  codec: VideoCodec,
  codecResults: CodecTestResult[] | null,
): boolean {
  const result = codecResults?.find((entry) => entry.codec === codec);
  if (result) {
    // A codec is usable for STREAMING only when it is BOTH decodable on this
    // device AND listed in the WebRTC receiver capabilities. `mediaCapabilities`
    // can report a codec as decodable (e.g. AV1 via software dav1d) while
    // WebRTC still cannot receive it; picking such a codec used to hard-filter
    // the offer down to it and the answer would reject the whole video m-line.
    return result.decodeSupported && result.webrtcSupported;
  }
  return isWebRtcCodecAvailable(codec);
}

/**
 * Return the saved concrete codec preference that should be migrated to
 * `"auto"` because it is not usable for streaming on this device. Returns
 * `null` when the preference is already `"auto"` or when no diagnostic test
 * results exist yet — a missing/empty result set must never trigger a
 * migration (avoids false positives while diagnostics are still loading or
 * failed).
 */
export function getCodecToMigrateToAuto(
  codecPreference: CodecPreference,
  codecResults: CodecTestResult[] | null,
): VideoCodec | null {
  if (codecPreference === "auto" || !codecResults || codecResults.length === 0) {
    return null;
  }
  return isCodecUsableForStream(codecPreference, codecResults) ? null : codecPreference;
}

/**
 * Resolve a codec preference into a concrete codec for session negotiation.
 * `"auto"` picks the first codec the device can actually decode (AV1 → H264 →
 * H265, mirroring GFN web's "Auto (AV1)"), falling back to H264. An explicit
 * user choice is always honored, even if the device reports it unsupported.
 */
export function resolveEffectiveCodec(preference: CodecPreference): VideoCodec {
  if (preference !== "auto") {
    return preference;
  }
  return AUTO_CODEC_PREFERENCE_ORDER.find((codec) => isWebRtcCodecAvailable(codec)) ?? "H264";
}

/**
 * Resolve a codec preference into a concrete, color-quality-compatible stream
 * profile. Re-runs the H264 → 8-bit 4:2:0 pinning against the *resolved* codec
 * so an "auto" pick that lands on H264 on a device without AV1/H265 still
 * negotiates SDR-compatible color, mirroring `normalizeStreamPreferences`.
 */
export function resolveStreamProfileCodec(
  codecPreference: CodecPreference,
  colorQuality: ColorQuality,
): { codec: VideoCodec; colorQuality: ColorQuality } {
  const resolved = resolveEffectiveCodec(codecPreference);
  const normalized = normalizeStreamPreferences(resolved, colorQuality);
  return {
    // `resolved` is concrete (never "auto"), so the normalization cannot turn
    // it into "auto" — the cast is safe.
    codec: normalized.codec as VideoCodec,
    colorQuality: normalized.colorQuality,
  };
}
