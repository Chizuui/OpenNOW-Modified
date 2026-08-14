import type { VideoCodec } from "@shared/gfn";
import { USER_FACING_VIDEO_CODEC_OPTIONS } from "@shared/gfn";

export const codecOptions: VideoCodec[] = [...USER_FACING_VIDEO_CODEC_OPTIONS];
export const allResolutionOptions = ["1280x720", "1280x800", "1440x900", "1680x1050", "1920x1080", "1920x1200", "2560x1080", "2560x1440", "2560x1600", "3440x1440", "3840x2160", "3840x2400"];
export const fpsOptions = [30, 60, 120, 144, 165, 240];
export const aspectRatioOptions = ["16:9", "16:10", "21:9", "32:9"] as const;

const RESOLUTION_TO_ASPECT_RATIO: Record<string, string> = {
  "1280x720": "16:9",
  "1280x800": "16:10",
  "1440x900": "16:10",
  "1680x1050": "16:10",
  "1920x1080": "16:9",
  "1920x1200": "16:10",
  "2560x1080": "21:9",
  "2560x1440": "16:9",
  "2560x1600": "16:10",
  "3440x1440": "21:9",
  "3840x2160": "16:9",
  "3840x2400": "16:10",
  "5120x1440": "32:9",
};

export const getResolutionsByAspectRatio = (aspectRatio: string): string[] => {
  return allResolutionOptions.filter(res => RESOLUTION_TO_ASPECT_RATIO[res] === aspectRatio);
};
export const resolutionOptions = getResolutionsByAspectRatio("16:9");

/**
 * The stream-profile downgrade ladder used by the automatic network
 * downgrade (native `network-assessment` verdict = poor): step fps down one
 * rung (240 → 120 → 60 → 30); once fps is already minimal, step the
 * resolution down one rung (preserving aspect ratio where possible). This is
 * the renderer-side half of GFN's pre-stream "stream test" profile
 * rejection — instead of refusing to start, we step the running session down
 * so the stream stays watchable on a degraded link.
 */
export const FPS_DOWNGRADE_LADDER = [240, 120, 60, 30];

export function nextLowerFps(currentFps: number): number | null {
  return (
    FPS_DOWNGRADE_LADDER.find((fps) => fps < currentFps) ?? null
  );
}

export function nextLowerResolution(currentResolution: string): string | null {
  const index = allResolutionOptions.indexOf(currentResolution);
  // Unknown resolution: fall back to 1080p (the safest mid-rung); otherwise
  // step one rung down within the same aspect-ratio family so the picture
  // keeps its shape.
  if (index < 0) {
    return "1920x1080";
  }
  const aspect = RESOLUTION_TO_ASPECT_RATIO[currentResolution];
  const sameAspect = getResolutionsByAspectRatio(aspect ?? "16:9");
  const sameAspectIndex = sameAspect.indexOf(currentResolution);
  return sameAspectIndex > 0 ? sameAspect[sameAspectIndex - 1] : null;
}

/**
 * Present-limiter pacing modes exposed in the Stream Quality panel (the GFN
 * NVST p-f pacing framework analogue). `custom` is a fixed fps chosen from
 * `pacingFpsOptions` and serialized as its numeric string (e.g. `"120"`).
 */
export const pacingModeOptions = ["auto", "stream", "vrr", "off"] as const;
export const pacingFpsOptions = [60, 120, 144, 165, 240];

/**
 * Whether a pacing-mode value is a custom fps string (a number, e.g. "120")
 * rather than one of the named modes.
 */
export function isCustomPacingFps(mode: string): boolean {
  return /^\d+$/.test(mode);
}

/**
 * Next pacing mode in the named cycle (matches the Stream Quality panel chip
 * order): auto → stream → vrr → off → auto. A custom fps override sits outside
 * the named loop, so it steps to `off` (the end of the cycle) and the next
 * press wraps back to `auto`.
 */
export function cyclePacingMode(current: string): string {
  const index = pacingModeOptions.indexOf(
    current as (typeof pacingModeOptions)[number],
  );
  if (index >= 0) {
    return pacingModeOptions[(index + 1) % pacingModeOptions.length];
  }
  return "off";
}
