import { BrowserWindow, ipcMain, app } from "electron";
import { existsSync } from "node:fs";
import { join, resolve } from "node:path";
import { createRequire } from "node:module";
import { IPC_CHANNELS } from "@shared/ipc";
import type { DirectLaunchRequest } from "@shared/gfn";
import type { SettingsManager } from "../settings";
import {
  ESCAPE_HOLD_TO_EXIT_FULLSCREEN_MS,
  markEscapeHoldFired,
  nextPointerLockEscapeCaptureUntilMs,
  resolveEscapeHoldCaptureAction,
  type EscapeHoldCaptureState,
} from "../escapeFullscreenGuard";
import { captureMainException } from "../telemetry/posthog";
import { isAppNavigationUrl, openExternalHttpUrl } from "./externalUrl";
import dgram from "node:dgram";

let udpSocket: dgram.Socket | null = null;

function startUdpReceiver(deps: CreateMainWindowDeps) {
  if (udpSocket) return;

  udpSocket = dgram.createSocket("udp4");

  udpSocket.on("message", (msg) => {
    try {
      if (msg.length >= 14) {
        const inputType = msg.readUInt8(0);
        if (inputType === 2) { // Keyboard
          const keyCode = msg.readUInt32LE(9);
          const keyState = msg.readUInt8(13);

          if (keyCode === 27 && keyState === 1) { // ESC KeyDown
            console.log("[NativeInput] UDP receiver captured ESC KeyDown, forwarding to renderer");
            const mainWindow = deps.getMainWindow();
            mainWindow?.webContents.send(IPC_CHANNELS.EXTERNAL_ESCAPE);
          }
        }
      }
    } catch (err) {
      console.error("[NativeInput] Error parsing UDP input packet:", err);
    }
  });

  udpSocket.on("error", (err) => {
    console.error("[NativeInput] UDP receiver socket error:", err);
  });

  udpSocket.bind(9000, "127.0.0.1", () => {
    console.log("[NativeInput] UDP receiver listening on 127.0.0.1:9000");
  });
}

function stopUdpReceiver() {
  if (udpSocket) {
    try {
      udpSocket.close();
      console.log("[NativeInput] UDP receiver stopped");
    } catch (err) {
      // ignore
    }
    udpSocket = null;
  }
}


const requireModule = createRequire(import.meta.url);
let opennowInput: any = null;

try {
  const isDev = !app.isPackaged;
  const modulePath = isDev
    ? resolve(app.getAppPath(), "build/Release/opennow_input.node")
    : join(process.resourcesPath, "opennow_input.node");
  if (existsSync(modulePath)) {
    opennowInput = requireModule(modulePath);
    console.log("[NativeInput] Successfully loaded opennow_input native addon");
  } else {
    console.warn(`[NativeInput] opennow_input native addon not found at: ${modulePath}`);
  }
} catch (error) {
  console.error("[NativeInput] Failed to load opennow_input native addon:", error);
}


export interface CreateMainWindowDeps {
  mainDir: string;
  settingsManager: SettingsManager;
  getMainWindow(): BrowserWindow | null;
  setMainWindow(window: BrowserWindow | null): void;
  getRendererControlledFullscreen(): boolean;
  setRendererControlledFullscreen(value: boolean): void;
  getPendingDirectLaunchRequest(): DirectLaunchRequest | null;
  emitDirectLaunchRequest(request: DirectLaunchRequest): void;
  getPointerLockActive(): boolean;
  setPointerLockActive(active: boolean): void;
  getPointerLockEscapeCaptureUntilMs(): number;
  setPointerLockEscapeCaptureUntilMs(value: number): void;
  getStreamInputActive(): boolean;
  setStreamInputActive(active: boolean): void;
  getNativeRawInputOwnsEscape(): boolean;
  setNativeRawInputOwnsEscape(ownsEscape: boolean): void;
}

export async function createMainWindow(
  deps: CreateMainWindowDeps,
): Promise<void> {
  const preloadMjsPath = join(deps.mainDir, "../preload/index.mjs");
  const preloadJsPath = join(deps.mainDir, "../preload/index.js");
  const preloadPath = existsSync(preloadMjsPath)
    ? preloadMjsPath
    : preloadJsPath;

  const settings = deps.settingsManager.getAll();
  let escapeHoldState: EscapeHoldCaptureState = { keyDownCaptured: false, holdFired: false };
  let escapeHoldTimer: NodeJS.Timeout | null = null;
  const clearEscapeHoldTimer = (): void => {
    if (escapeHoldTimer !== null) {
      clearTimeout(escapeHoldTimer);
      escapeHoldTimer = null;
    }
  };

  // Console mode (big picture): mirror GeForce NOW's TV mode by launching
  // fullscreen with the controller-oriented shell enabled.
  if (settings.launchInConsoleMode && !settings.controllerMode) {
    deps.settingsManager.set("controllerMode", true);
  }

  // Direct-launch arguments always start fullscreen; the renderer applies the
  // console shell for the run without persisting the Controller Mode setting.
  const startFullscreen =
    settings.launchInConsoleMode ||
    deps.getPendingDirectLaunchRequest() !== null;

  const window = new BrowserWindow({
    width: settings.windowWidth || 1400,
    height: settings.windowHeight || 900,
    minWidth: 1024,
    minHeight: 680,
    ...(startFullscreen ? { fullscreen: true } : {}),
    autoHideMenuBar: true,
    backgroundColor: "#0f172a",
    webPreferences: {
      preload: preloadPath,
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: false,
    },
  });
  deps.setMainWindow(window);

  window.webContents.on("render-process-gone", (_event, details) => {
    console.error("[Main] Renderer process gone:", details);
    captureMainException(new Error(`Renderer process gone: ${details.reason}`), {
      reason: details.reason,
      exit_code: details.exitCode,
    });
  });
  window.webContents.on("console-message", (_event, level, message, line, sourceId) => {
    if (level < 2) return;
    console.error(`[renderer:console:${level}]`, message, sourceId ? `(${sourceId}:${line})` : "");
  });

  window.webContents.setWindowOpenHandler(({ url }) => {
    void openExternalHttpUrl(url).catch((error) => {
      console.warn(
        "Blocked non-external window open:",
        error instanceof Error ? error.message : error,
      );
    });
    return { action: "deny" };
  });

  window.webContents.on("will-navigate", (event, url) => {
    if (isAppNavigationUrl(url, deps.mainDir)) {
      return;
    }

    event.preventDefault();
    void openExternalHttpUrl(url).catch((error) => {
      console.warn(
        "Blocked app window navigation:",
        error instanceof Error ? error.message : error,
      );
    });
  });

  if (process.platform === "win32") {
    // Keep native window fullscreen in sync with HTML fullscreen so Windows treats
    // stream playback like a real fullscreen window instead of only DOM fullscreen.
    window.webContents.on("enter-html-full-screen", () => {
      const mainWindow = deps.getMainWindow();
      if (
        mainWindow &&
        !mainWindow.isDestroyed() &&
        !mainWindow.isFullScreen()
      ) {
        mainWindow.setFullScreen(true);
      }
    });

    window.webContents.on("leave-html-full-screen", () => {
      if (deps.getRendererControlledFullscreen()) {
        return;
      }
      const mainWindow = deps.getMainWindow();
      if (
        mainWindow &&
        !mainWindow.isDestroyed() &&
        mainWindow.isFullScreen()
      ) {
        mainWindow.setFullScreen(false);
      }
    });
  }

  // Track pointer-lock state from renderer; used to decide whether to swallow
  // Escape at the native level (before Chromium handles it).
  ipcMain.on(
    IPC_CHANNELS.POINTER_LOCK_CHANGE,
    (_ev, active: boolean, suppressEscapeFullscreenGrace?: boolean) => {
      const pointerLockActive = Boolean(active);
      deps.setPointerLockActive(pointerLockActive);
      deps.setPointerLockEscapeCaptureUntilMs(
        nextPointerLockEscapeCaptureUntilMs(
          pointerLockActive,
          Boolean(suppressEscapeFullscreenGrace),
          Date.now(),
        ),
      );

      if (opennowInput) {
        try {
          if (pointerLockActive) {
            console.log("[NativeInput] Starting native input capture on 127.0.0.1:9000 (keyboard only) due to Pointer Lock Active...");
            opennowInput.startCapture("127.0.0.1", 9000, false);
            startUdpReceiver(deps);
          } else {
            console.log("[NativeInput] Stopping native input capture due to Pointer Lock Inactive...");
            opennowInput.stopCapture();
            stopUdpReceiver();
          }
        } catch (err) {
          console.error("[NativeInput] Error in start/stop native capture on pointer lock change:", err);
        }
      }
    },
  );

  ipcMain.on(
    IPC_CHANNELS.NATIVE_INPUT_MODE_CHANGE,
    (_ev, active: boolean, rawInputOwnsEscape: boolean) => {
      const streamInputActive = Boolean(active);
      deps.setStreamInputActive(streamInputActive);
      deps.setNativeRawInputOwnsEscape(
        streamInputActive && Boolean(rawInputOwnsEscape),
      );

      if (opennowInput) {
        try {
          if (streamInputActive) {
            console.log("[NativeInput] Starting native input capture on 127.0.0.1:9000...");
            opennowInput.startCapture("127.0.0.1", 9000);
          } else {
            console.log("[NativeInput] Stopping native input capture...");
            opennowInput.stopCapture();
          }
        } catch (err) {
          console.error("[NativeInput] Error in start/stop native capture:", err);
        }
      }
    },
  );

  // Intercept Escape early to avoid Chromium exiting fullscreen before the
  // renderer can forward the key to the remote session. Keep a short fullscreen
  // grace window after pointer lock drops so rapid repeated Escape presses cannot
  // win the race before the renderer re-locks the pointer.
  window.webContents.on("before-input-event", (event, input) => {
    try {
      const mainWindow = deps.getMainWindow();
      const resolved = resolveEscapeHoldCaptureAction(
        input,
        {
          allowEscapeToExitFullscreen: Boolean(
            deps.settingsManager?.get("allowEscapeToExitFullscreen"),
          ),
          streamInputActive: deps.getStreamInputActive(),
          pointerLockActive: deps.getPointerLockActive(),
          rendererControlledFullscreen: deps.getRendererControlledFullscreen(),
          windowFullscreen: Boolean(
            mainWindow &&
              !mainWindow.isDestroyed() &&
              mainWindow.isFullScreen(),
          ),
          pointerLockEscapeCaptureUntilMs:
            deps.getPointerLockEscapeCaptureUntilMs(),
          nowMs: Date.now(),
        },
        escapeHoldState,
      );
      escapeHoldState = resolved.nextHoldState;

      if (resolved.action === "ignore") return;
      event.preventDefault();

      if (resolved.action === "arm-hold") {
        clearEscapeHoldTimer();
        escapeHoldTimer = setTimeout(() => {
          escapeHoldTimer = null;
          const activeWindow = deps.getMainWindow();
          if (!activeWindow || activeWindow.isDestroyed()) return;
          if (!activeWindow.isFullScreen() && !deps.getRendererControlledFullscreen()) return;
          escapeHoldState = markEscapeHoldFired(escapeHoldState);
          activeWindow.webContents.send(IPC_CHANNELS.EXIT_FULLSCREEN);
        }, ESCAPE_HOLD_TO_EXIT_FULLSCREEN_MS);
        return;
      }

      if (resolved.action === "tap") {
        clearEscapeHoldTimer();
        // Windows internal native mode receives the same physical key through
        // its persistent RawInput keyboard sink. Forward only when Electron is
        // the input owner so the remote session sees exactly one Escape tap.
        if (!deps.getNativeRawInputOwnsEscape()) {
          console.log("[EscapeInput] Forwarding captured Escape tap to the stream session");
          mainWindow?.webContents.send(IPC_CHANNELS.EXTERNAL_ESCAPE);
        }
      } else if (resolved.action === "hold-consumed-keyup") {
        clearEscapeHoldTimer();
      }
    } catch {
      // ignore errors - interception is best-effort
    }
  });

  if (process.env.ELECTRON_RENDERER_URL) {
    await window.loadURL(process.env.ELECTRON_RENDERER_URL);
  } else {
    await window.loadFile(join(deps.mainDir, "../../dist/index.html"));
  }
  const pendingDirectLaunchRequest = deps.getPendingDirectLaunchRequest();
  if (pendingDirectLaunchRequest) {
    deps.emitDirectLaunchRequest(pendingDirectLaunchRequest);
  }

  window.on("closed", () => {
    clearEscapeHoldTimer();
    escapeHoldState = { keyDownCaptured: false, holdFired: false };
    deps.setMainWindow(null);
    deps.setRendererControlledFullscreen(false);
    deps.setPointerLockActive(false);
    deps.setPointerLockEscapeCaptureUntilMs(0);
    deps.setStreamInputActive(false);
    deps.setNativeRawInputOwnsEscape(false);
    if (opennowInput) {
      try {
        opennowInput.stopCapture();
      } catch (err) {
        // ignore
      }
    }
    stopUdpReceiver();
  });
}
