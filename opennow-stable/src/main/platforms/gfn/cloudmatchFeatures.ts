import type { AppLaunchMode, CodecPreference, SessionCreateRequest, StreamSettings, VideoCodec } from "@shared/gfn";
import { DEFAULT_MINIMUM_FPS_FOR_REFLEX_WITHOUT_VRR } from "@shared/cloudGsync";

import type { CloudMatchRequest } from "./types";

// Wire values used by cloudmatch session requests. Matches the official
// client's mapping: Default -> 1, GamepadFriendly -> 2, TouchFriendly -> 3.
const APP_LAUNCH_MODE_WIRE_VALUES: Record<AppLaunchMode, number> = {
  default: 1,
  gamepadFriendly: 2,
  touchFriendly: 3,
};

export function appLaunchModeWireValue(mode: AppLaunchMode | undefined): number {
  return APP_LAUNCH_MODE_WIRE_VALUES[mode ?? "default"];
}

export function buildRequestedStreamingFeatures(
  settings: StreamSettings,
  bitDepth: number,
  chromaFormat: number,
  _hdrEnabled: boolean,
  /** Codecs this client can actually decode (hardware-aware, WebRTC-receivable). */
  supportedCodecs?: readonly VideoCodec[],
): CloudMatchRequest["sessionRequestData"]["requestedStreamingFeatures"] {
  const cloudGsync = settings.enableCloudGsync;

  return {
    reflex: shouldRequestReflex(settings),
    bitDepth,
    cloudGsync,
    enabledL4S: settings.enableL4S,
    supportedHidDevices: 0,
    profile: 0,
    fallbackToLogicalResolution: false,
    chromaFormat,
    prefilterMode: 0,
    prefilterSharpness: 0,
    prefilterNoiseReduction: 0,
    hudStreamingMode: 0,
    // ── Aligned with the official client's requestedStreamingFeatures ──
    // Bitrate preference in Kbps (official multiplies the Mbps setting by 1000).
    maxBitrateKbps: Math.round(settings.maxBitrateMbps * 1000),
    // Official wire codec id (H264=1, H265=2, AV1=3, 0 when "auto"), resolved
    // down the official preference ladder against the codecs this client can
    // actually decode (the official bundle intersects its preference ladder
    // with its hardware-aware decode capability list before sending).
    codec: resolveRequestedCodecWireValue(
      codecWireValue(settings.codec),
      (supportedCodecs ?? []).map(codecWireValue),
    ),
    // No client vsync preference in the fork (GFN streams client-side vsync off).
    vsync: false,
    // Official client default (desiredFeatures.dynamicStreamingMode ?? 3).
    // Aligned with nvstOffer's dynamicStreamingMode:3 so the server runs the
    // same dynamic-streaming profile as the web app. The renderer handles
    // mid-session track (SSRC) replacement, so mode 3 is safe.
    dynamicStreamingMode: 3,
    // STEREO (matches audioMode: 2); official mapping: stereo=2, 5.1=6, 7.1=8.
    audioChannelCount: 2,
  };
}

/**
 * Official wire codec ids for requestedStreamingFeatures (mirrors the
 * official client's `sr()`: H264=1, H265=2, AV1=3, unknown/auto=0).
 */
export function codecWireValue(codec: CodecPreference): number {
  switch (codec) {
    case "H264":
      return 1;
    case "H265":
      return 2;
    case "AV1":
      return 3;
    default:
      return 0; // "auto"
  }
}

/**
 * Official codec preference ladders (wire ids), verified against the
 * play.geforcenow.com bundle: the requested codec is intersected with the
 * codecs the client can actually decode — AV1→[3,2,1], H265→[2,1], H264→[1],
 * auto→[0] (auto leaves the choice to the server).
 */
const OFFICIAL_CODEC_LADDERS: Record<number, readonly number[]> = {
  0: [0],
  1: [1],
  2: [2, 1],
  3: [3, 2, 1],
};

/**
 * Resolve a requested codec wire id down the official preference ladder
 * against the wire ids of the codecs this client can decode. With no
 * capability data the explicit preference is kept unchanged (legacy behavior).
 */
export function resolveRequestedCodecWireValue(
  preferenceWireValue: number,
  supportedCodecWireValues: readonly number[],
): number {
  const ladder = OFFICIAL_CODEC_LADDERS[preferenceWireValue] ?? [preferenceWireValue];
  if (supportedCodecWireValues.length === 0) {
    return ladder[0];
  }
  const supported = new Set(supportedCodecWireValues);
  return ladder.find((value) => supported.has(value)) ?? ladder[0];
}

export function shouldRequestReflex(settings: StreamSettings): boolean {
  if (typeof settings.cloudGsyncResolution?.reflexEnabled === "boolean") {
    return settings.cloudGsyncResolution.reflexEnabled;
  }

  const reflexMinimum =
    settings.cloudGsyncResolution?.capabilities.minimumFpsForReflexWithoutVrr
    ?? DEFAULT_MINIMUM_FPS_FOR_REFLEX_WITHOUT_VRR;
  return settings.enableCloudGsync || settings.fps >= reflexMinimum;
}

export function shouldEnableInGameSettingsPersistence(
  input: Pick<SessionCreateRequest, "enablePersistingInGameSettings" | "supportsInGameSettingsPersistence">,
): boolean {
  return (
    input.enablePersistingInGameSettings === true &&
    input.supportsInGameSettingsPersistence === true
  );
}
