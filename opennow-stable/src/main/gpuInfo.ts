import type { App } from "electron";
import type { GpuBackendInfo } from "@shared/gfn";

/**
 * Parse a Chromium `video*AcceleratorSupportedProfiles` string into a list of
 * codec names. Example input:
 *   "H264: 16x16 to 4096x4096 pixels, HEVC: 16x16 to 8192x4352 pixels, AV1: ..."
 * HEVC is mapped to the app's "H265" naming.
 */
export function parseAcceleratedProfiles(profiles: string | undefined): string[] {
  if (!profiles) {
    return [];
  }
  const codecs: string[] = [];
  for (const part of profiles.split(",")) {
    const name = part.trim().split(":")[0]?.trim().toUpperCase();
    if (!name) continue;
    const normalized = name === "HEVC" ? "H265" : name;
    if (!codecs.includes(normalized)) {
      codecs.push(normalized);
    }
  }
  return codecs;
}

const FEATURE_HARDWARE_ACCELERATED = "hardware_accelerated";

function featureToBoolean(value: string | undefined): boolean | null {
  if (value === FEATURE_HARDWARE_ACCELERATED) return true;
  if (value === "disabled" || value === "unavailable" || value === "software_rendering") return false;
  return null;
}

interface RawGpuDevice {
  active?: boolean;
  deviceString?: string;
  vendorString?: string;
  driverVersion?: string;
}

interface RawGpuInfo {
  gpuDevice?: RawGpuDevice[];
  auxAttributes?: {
    videoDecodeAcceleratorSupportedProfiles?: string;
    videoEncodeAcceleratorSupportedProfiles?: string;
  };
}

interface RawGpuFeatureStatus {
  video_decode?: string;
  video_encode?: string;
}

export const EMPTY_GPU_BACKEND_INFO: GpuBackendInfo = {
  gpuName: null,
  vendorName: null,
  driverVersion: null,
  decodeAccelerated: null,
  encodeAccelerated: null,
  hardwareDecodeCodecs: [],
  hardwareEncodeCodecs: [],
};

/**
 * Normalize the raw Chromium GPUInfo + GPUFeatureStatus objects into the
 * renderer-facing `GpuBackendInfo`. Falls back to the active GPU device.
 */
export function collectGpuBackendInfo(
  rawInfo: unknown,
  rawFeatureStatus: unknown,
): GpuBackendInfo {
  const info = (rawInfo ?? {}) as RawGpuInfo;
  const featureStatus = (rawFeatureStatus ?? {}) as RawGpuFeatureStatus;

  const activeDevice =
    info.gpuDevice?.find((device) => device.active === true) ?? info.gpuDevice?.[0];

  return {
    gpuName: activeDevice?.deviceString ?? null,
    vendorName: activeDevice?.vendorString ?? null,
    driverVersion: activeDevice?.driverVersion ?? null,
    decodeAccelerated: featureToBoolean(featureStatus.video_decode),
    encodeAccelerated: featureToBoolean(featureStatus.video_encode),
    hardwareDecodeCodecs: parseAcceleratedProfiles(
      info.auxAttributes?.videoDecodeAcceleratorSupportedProfiles,
    ),
    hardwareEncodeCodecs: parseAcceleratedProfiles(
      info.auxAttributes?.videoEncodeAcceleratorSupportedProfiles,
    ),
  };
}

let cachedGpuBackendInfo: GpuBackendInfo | null = null;

/**
 * Fetch the GPU backend snapshot once per app session and cache it —
 * `app.getGPUInfo("complete")` is expensive and the values are stable while
 * the app runs. `getGPUFeatureStatus()` is only reliable after the GPU process
 * has initialized (the `gpu-info-update` event), so the first invocation after
 * app ready is safe for the user-triggered codec test.
 */
export async function getGpuBackendInfo(app: App): Promise<GpuBackendInfo> {
  if (cachedGpuBackendInfo) {
    return cachedGpuBackendInfo;
  }
  const [rawInfo, rawFeatureStatus] = await Promise.all([
    app.getGPUInfo("complete"),
    Promise.resolve(app.getGPUFeatureStatus()),
  ]);
  cachedGpuBackendInfo = collectGpuBackendInfo(rawInfo, rawFeatureStatus);
  return cachedGpuBackendInfo;
}

export function resetGpuBackendInfoCache(): void {
  cachedGpuBackendInfo = null;
}
