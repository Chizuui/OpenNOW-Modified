/**
 * Display refresh-rate detection mirroring the official GFN web/PC client:
 * count requestAnimationFrame callbacks over a ~2s window (4s timeout) and
 * derive the display Hz. Used to auto-fill the default stream FPS so a
 * high-refresh display gets a 120/240 FPS session (and the server-side
 * GAME FPS stat can exceed 60) exactly like GFN web.
 */

/** Storage key marking that the FPS auto-detection decision was already made. */
const FPS_AUTO_RESOLVED_KEY = "opennow.fps-auto-detected.v1";

/**
 * Map a measured display refresh rate to the best GFN stream FPS tier.
 * Thresholds mirror the official client's `eE(h) { return h >= 117 }` gate
 * for 120 FPS and its balanced display-mode list (90, 120, 240 tiers).
 * Any measurement <= 0 or below 90 Hz keeps 60 FPS.
 */
export function recommendedStreamFps(refreshRate: number): number {
  if (!Number.isFinite(refreshRate)) return 60;
  if (refreshRate >= 233) return 240;
  if (refreshRate >= 117) return 120;
  if (refreshRate >= 90) return 90;
  return 60;
}

/**
 * Whether the auto-detect feature should upgrade the stream FPS: only when
 * the user is still on the untouched default FPS (i.e. never made an explicit
 * choice) AND the display supports a higher tier. Explicitly chosen values
 * (including an explicit 60) are always respected.
 */
export function shouldAutoUpgradeStreamFps(
  currentFps: number,
  defaultFps: number,
  recommendedFps: number,
): boolean {
  return currentFps === defaultFps && recommendedFps > currentFps && Number.isFinite(recommendedFps);
}

/**
 * Convert a rAF callback count over a window into a refresh rate in Hz.
 * Mirrors the official client's `Math.floor(z.length / (h / 1e3))`: the count
 * includes the first callback at the window start AND the last one that crosses
 * the window edge (+up to 1), so FLOOR (not round) absorbs that overcount and
 * never flips a tier boundary (e.g. a 116 Hz display must stay 116, not 117).
 */
export function framesToRefreshRate(sampleCount: number, windowMs: number): number {
  if (!Number.isFinite(sampleCount) || sampleCount <= 0 || !Number.isFinite(windowMs) || windowMs <= 0) {
    return 0;
  }
  return Math.floor(sampleCount / (windowMs / 1000));
}

/**
 * Measure the display refresh rate by counting rAF callbacks over `scanMs`.
 * Returns 0 when the measurement fails, times out, the document is hidden, or
 * the window is being moved/resized mid-scan (the official client restarts the
 * scan in that case; we treat a moved window as "unknown" and skip).
 */
export function detectDisplayRefreshRate(
  scanMs = 2000,
  timeoutMs = 4000,
): Promise<number> {
  return new Promise((resolve) => {
    if (
      typeof window === "undefined"
      || typeof window.requestAnimationFrame !== "function"
      || document.visibilityState === "hidden"
    ) {
      resolve(0);
      return;
    }

    let timedOut = false;
    let finished = false;
    const timeout = window.setTimeout(() => {
      timedOut = true;
      if (!finished) {
        finished = true;
        resolve(0);
      }
    }, timeoutMs);

    const startX = window.screenX;
    const startY = window.screenY;
    const startWidth = window.innerWidth;
    const startHeight = window.innerHeight;

    let sampleCount = 0;
    let scanStartMs = 0;

    const finish = (fps: number): void => {
      if (finished) {
        return;
      }
      finished = true;
      window.clearTimeout(timeout);
      resolve(fps >= 30 && fps <= 500 ? fps : 0);
    };

    const tick = (now: number): void => {
      if (finished) {
        return;
      }
      const windowMoved =
        window.screenX !== startX
        || window.screenY !== startY
        || window.innerWidth !== startWidth
        || window.innerHeight !== startHeight;
      if (windowMoved) {
        finish(0);
        return;
      }
      if (scanStartMs === 0) {
        scanStartMs = now;
      }
      sampleCount += 1;
      if (now - scanStartMs >= scanMs) {
        finish(framesToRefreshRate(sampleCount, scanMs));
        return;
      }
      try {
        window.requestAnimationFrame(tick);
      } catch {
        finish(0);
      }
    };

    try {
      window.requestAnimationFrame(tick);
    } catch {
      finish(0);
    }
  });
}

/** Whether the one-time FPS auto-detection decision was already made. */
export function hasResolvedAutoFps(): boolean {
  try {
    return window.localStorage.getItem(FPS_AUTO_RESOLVED_KEY) === "1";
  } catch {
    return false;
  }
}

/** Persist the one-time FPS auto-detection decision (survives restarts). */
export function markFpsAutoResolved(): void {
  try {
    window.localStorage.setItem(FPS_AUTO_RESOLVED_KEY, "1");
  } catch {
    // best effort
  }
}
