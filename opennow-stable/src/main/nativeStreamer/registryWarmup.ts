import { spawn } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, unlinkSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { app, type BrowserWindow } from "electron";
import { resolveNativeStreamerExecutableCandidates } from "./executableDiscovery";
import { createNativeStreamerRuntimeEnvironment, nativeStreamerPlatformKey } from "./runtime";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

export type GStreamerScanStatus = "scanning" | "finished" | "failed";

/**
 * Hard ceiling for a background registry scan. A fresh `gst-inspect` registry
 * build can take 30-90s on a cold cache, but it must never hang forever: a
 * wedged child leaves `scanInProgress` set and the renderer toast stuck on
 * "scanning" with no resolution. On timeout the child is killed and the status
 * settles to "failed" so the UI converges (the next launch or clear retries).
 */
const SCAN_TIMEOUT_MS = 120_000;
export type GStreamerScanReason = "first-scan" | "driver-update";

export interface GStreamerScanSnapshot {
  /** Whether a background registry scan is currently running. */
  scanInProgress: boolean;
  /** Most recent scan status (null when no scan has run this process). */
  lastStatus: GStreamerScanStatus | null;
  /** Reason of the most recent scan. */
  lastReason: GStreamerScanReason | null;
}

export interface GStreamerRegistryPaths {
  registryDir: string;
  driverVersionPath: string;
  registryPath: string;
}

export function getGStreamerRegistryPaths(): GStreamerRegistryPaths {
  const registryDir = join(app.getPath("userData"), "native-streamer", "gstreamer");
  const platformKey = nativeStreamerPlatformKey(process.platform, process.arch);
  return {
    registryDir,
    driverVersionPath: join(registryDir, "gpu-driver-version.txt"),
    registryPath: join(registryDir, `${platformKey}-registry.bin`),
  };
}

let lastScanStatus: { status: GStreamerScanStatus; reason: GStreamerScanReason } | null = null;
let scanInProgress = false;

export function getGStreamerScanSnapshot(): GStreamerScanSnapshot {
  return {
    scanInProgress,
    lastStatus: lastScanStatus?.status ?? null,
    lastReason: lastScanStatus?.reason ?? null,
  };
}

function sendScanStatus(getMainWindow: () => BrowserWindow | null): void {
  const mainWindow = getMainWindow();
  if (!mainWindow || mainWindow.isDestroyed()) return;
  mainWindow.webContents.send("gstreamer-scan-status", lastScanStatus);
}

/**
 * Re-send the most recent scan status to a freshly loaded renderer. Called on
 * `did-finish-load` so a window that mounts mid-scan (or after a scan finished
 * before its listener attached) always converges on the real state instead of
 * showing a stuck "scanning" toast forever.
 */
export function replayGStreamerScanStatus(getMainWindow: () => BrowserWindow | null): void {
  if (lastScanStatus) {
    sendScanStatus(getMainWindow);
  }
}

/**
 * Warm up the GStreamer plugin registry in the background so the first stream
 * start is ~1s instead of 60-90s. Decides whether a scan is needed (missing
 * registry on first launch, or GPU driver changed), spawns `gst-inspect` once,
 * and notifies the renderer so it can show a status toast.
 *
 * Safe to call more than once: while a scan is in flight it is a no-op, so the
 * "Clear GStreamer cache" settings action can request an immediate rebuild
 * without double-spawning.
 */
export async function warmUpGStreamerRegistry(options: {
  getMainWindow: () => BrowserWindow | null;
}): Promise<{ reason: "idle" | GStreamerScanReason; started: boolean }> {
  if (process.platform !== "win32") {
    return { reason: "idle", started: false };
  }
  if (scanInProgress) {
    return { reason: "idle", started: false };
  }

  try {
    const candidates = resolveNativeStreamerExecutableCandidates({
      platform: process.platform,
      arch: process.arch,
      resourcesPath: process.resourcesPath,
      appPath: app.getAppPath(),
      mainDir: __dirname,
      isPackaged: app.isPackaged,
      envExecutablePath: process.env.OPENNOW_NATIVE_STREAMER,
      getConfiguredPath: () => "",
      cacheContext: {
        appVersion: app.getVersion(),
        isPackaged: app.isPackaged,
        platform: process.platform,
        resourcesPath: process.resourcesPath,
        tempDirectory: app.getPath("temp"),
        userDataPath: app.getPath("userData"),
      },
    });

    if (candidates.length === 0) {
      return { reason: "idle", started: false };
    }
    const exePath = candidates[0];
    const runtimeRoot = join(dirname(exePath), "gstreamer");
    const gstInspect = join(runtimeRoot, "bin", "gst-inspect-1.0.exe");

    if (!existsSync(gstInspect)) {
      return { reason: "idle", started: false };
    }

    const { registryDir, driverVersionPath, registryPath } = getGStreamerRegistryPaths();

    let scanReason: "idle" | GStreamerScanReason = "idle";
    let currentDriver = "";

    try {
      const gpuInfo = await app.getGPUInfo("basic") as {
        gpuDevice?: Array<{ driverVersion?: string }>;
      };
      currentDriver = gpuInfo.gpuDevice?.[0]?.driverVersion || "";
    } catch (err) {
      console.warn("[Main] Failed to query GPU driver version:", err);
    }

    const registryExists = existsSync(registryPath);
    mkdirSync(registryDir, { recursive: true });

    if (!registryExists) {
      scanReason = "first-scan";
      if (currentDriver) writeFileSync(driverVersionPath, currentDriver, "utf8");
    } else if (currentDriver && existsSync(driverVersionPath)) {
      const lastDriver = readFileSync(driverVersionPath, "utf8").trim();
      if (lastDriver !== currentDriver.trim()) {
        console.log(`[Main] GPU Driver updated: ${lastDriver} -> ${currentDriver}. Resetting registry...`);
        scanReason = "driver-update";
        try { unlinkSync(registryPath); } catch {}
        writeFileSync(driverVersionPath, currentDriver, "utf8");
      }
    } else if (currentDriver) {
      // Missing version file, create it
      writeFileSync(driverVersionPath, currentDriver, "utf8");
    }

    const { env } = createNativeStreamerRuntimeEnvironment({
      executablePath: exePath,
      baseEnv: process.env,
      platform: process.platform,
      arch: process.arch,
      userDataPath: app.getPath("userData"),
      protocolVersion: 1,
      backendPreference: "gstreamer",
      videoBackendPreference: "auto",
      externalRendererEnabled: false,
      cloudGsyncMode: "auto",
      d3dFullscreenMode: "auto",
    });

    // Registry cache already exists and the GPU driver is unchanged — nothing
    // to rebuild. Skip the spawn entirely so subsequent launches stay ~1s.
    if (scanReason === "idle") {
      console.log("[Main] GStreamer registry cache is fresh; skipping background scan.");
      return { reason: "idle", started: false };
    }

    console.log(`[Main] GStreamer scan started (Reason: ${scanReason}).`);

    lastScanStatus = { status: "scanning", reason: scanReason };
    scanInProgress = true;
    sendScanStatus(options.getMainWindow);

    // stdout is ignored entirely: `gst-inspect` with no arguments dumps the
    // full plugin tree to stdout (megabytes). Piping it without ever draining
    // fills the pipe buffer and the child blocks forever — the classic
    // wedged-scan that left the "scanning" toast spinning with a
    // gst-inspect1.0.exe alive in Task Manager. We only need the exit code.
    const child = spawn(gstInspect, [], {
      env: { ...env, GST_REGISTRY_FORK: "no" },
      windowsHide: true,
      stdio: ["ignore", "ignore", "pipe"], // capture stderr only
    });

    let stderrOutput = "";
    child.stderr.on("data", (data) => {
      stderrOutput += data.toString();
    });

    const settle = (status: GStreamerScanStatus, detail: string): void => {
      scanInProgress = false;
      if (status === "failed") {
        console.error(`[Main] GStreamer background scan failed: ${detail}`);
      } else {
        console.log(`[Main] GStreamer background scan finished: ${detail}`);
      }
      lastScanStatus = { status, reason: scanReason };
      sendScanStatus(options.getMainWindow);
    };

    let settled = false;
    const timeout = setTimeout(() => {
      if (settled) return;
      settled = true;
      if (!child.killed) {
        child.kill();
      }
      settle(
        "failed",
        `timed out after ${SCAN_TIMEOUT_MS}ms (gst-inspect1.0.exe was killed)`,
      );
    }, SCAN_TIMEOUT_MS);

    child.on("exit", (code) => {
      if (settled) return;
      settled = true;
      clearTimeout(timeout);
      if (code !== 0) {
        settle("failed", `code ${code}. Stderr: ${stderrOutput}`);
      } else {
        settle("finished", `code ${code}`);
      }
    });

    return { reason: scanReason, started: true };
  } catch (err) {
    console.warn("[Main] Failed GStreamer background scan:", err);
    return { reason: "idle", started: false };
  }
}
