import { BrowserWindow, ipcMain, app, Menu } from "electron";
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

// Native raw-mouse addon (Windows RawInput / macOS CGEventTap / Linux X11
// grab). Captures raw mouse + confines the cursor so Escape never releases
// anything (we don't use browser pointer-lock). Events arrive via an N-API
// ThreadSafeFunction callback — no UDP, no keyboard hook. On Linux Wayland or
// missing macOS permissions grabMouse() returns false and the renderer falls
// back to DOM pointer lock.
interface NativeMouseEvent {
  kind: number;   // 0 = move, 1 = button, 2 = wheel
  dx: number;
  dy: number;
  button: number; // 0=L 1=R 2=M 3=X1 4=X2
  state: number;  // 1=down 0=up
  wheel: number;  // signed notches * 120
}
interface OpennowInputAddon {
  grabMouse(hwnd: Buffer, onEvent: (ev: NativeMouseEvent) => void): boolean;
  releaseMouse(): void;
}

const requireModule = createRequire(import.meta.url);
let opennowInput: OpennowInputAddon | null = null;

try {
  const isDev = !app.isPackaged;
  const modulePath = isDev
    ? resolve(app.getAppPath(), "build/Release/opennow_input.node")
    : join(process.resourcesPath, "opennow_input.node");
  if (existsSync(modulePath)) {
    opennowInput = requireModule(modulePath) as OpennowInputAddon;
    console.log("[NativeInput] Loaded opennow_input raw-mouse addon");
  } else {
    console.warn(`[NativeInput] opennow_input addon not found at: ${modulePath} (falling back to DOM pointer lock)`);
  }
} catch (error) {
  console.error("[NativeInput] Failed to load opennow_input addon:", error);
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
    // Windows: a transparent shell is only needed while the GFN-style stacked
    // native renderer is enabled (the native video window sits behind the
    // BrowserWindow and must show through). Keeping the window opaque by
    // default preserves the DWM maximize/snap animation and Win+Arrow
    // shortcuts, which transparent windows on Windows lose. The `transparent`
    // flag is fixed at window creation, so enabling Stacked Render mode only
    // takes effect on the next app launch.
    ...(process.platform === "win32" && settings.nativeStackedRenderer === true
      ? { transparent: true, backgroundColor: "#00000000", hasShadow: false }
      : { backgroundColor: "#0f172a" }),
    // Frameless window with a custom title bar (GFN-style chrome). On macOS
    // keep the native traffic lights via a hidden title bar instead.
    ...(process.platform === "darwin"
      ? {
          titleBarStyle: "hidden" as const,
          trafficLightPosition: { x: 12, y: 10 },
        }
      : { frame: false }),
    webPreferences: {
      preload: preloadPath,
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: false,
      // Keep the renderer at full frame rate even when the window is
      // backgrounded or occluded (e.g. Discord share / screen capture).
      backgroundThrottling: false,
    },
  });
  deps.setMainWindow(window);

  const emitMaximizeState = (maximized: boolean): void => {
    if (!window.isDestroyed()) {
      window.webContents.send(IPC_CHANNELS.WINDOW_MAXIMIZE_STATE_CHANGED, maximized);
    }
  };
  window.on("maximize", () => emitMaximizeState(true));
  window.on("unmaximize", () => emitMaximizeState(false));

  window.webContents.on("render-process-gone", (_event, details) => {
    console.error("[Main] Renderer process gone:", details);
    captureMainException(new Error(`Renderer process gone: ${details.reason}`), {
      reason: details.reason,
      exit_code: details.exitCode,
    });
  });
  window.webContents.on("console-message", (_event, level, message, line, sourceId) => {
    // level 1 = console.log: forward it too so WebRTC/stream diagnostics
    // (Network stats, bwe, SDP builder lines) show up in exported logs.
    if (level < 1) return;
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
    },
  );

  // Native raw-mouse grab/release. On grab, hand the window's native handle to
  // the addon; every raw mouse event is forwarded to the renderer via IPC
  // (no UDP). Returns whether native capture actually started so the renderer
  // can fall back to DOM pointer lock when the addon is unavailable.
  ipcMain.handle(IPC_CHANNELS.NATIVE_MOUSE_GRAB, (): boolean => {
    if (!opennowInput) return false;
    const mainWindow = deps.getMainWindow();
    if (!mainWindow || mainWindow.isDestroyed()) return false;
    try {
      const handle = mainWindow.getNativeWindowHandle();
      return opennowInput.grabMouse(handle, (nativeEvent: NativeMouseEvent) => {
        const target = deps.getMainWindow();
        if (target && !target.isDestroyed()) {
          target.webContents.send(IPC_CHANNELS.NATIVE_MOUSE_EVENT, nativeEvent);
        }
      });
    } catch (err) {
      console.error("[NativeInput] grabMouse failed:", err);
      return false;
    }
  });

  ipcMain.handle(IPC_CHANNELS.NATIVE_MOUSE_RELEASE, (): void => {
    if (!opennowInput) return;
    try {
      opennowInput.releaseMouse();
    } catch (err) {
      console.error("[NativeInput] releaseMouse failed:", err);
    }
  });

  // Frameless window (frame: false): Alt+Space normally opens the OS system
  // menu, which is how keyboard users minimize/maximize/close a window. Restore
  // it manually with a native popup menu. Never intercept while a stream session
  // is active — the key belongs to the game there (and the native streamer's
  // raw input owns the session keyboard while captured).
  const openFramelessWindowSystemMenu = (): void => {
    const activeWindow = deps.getMainWindow();
    if (!activeWindow || activeWindow.isDestroyed()) return;
    const isMaximized = activeWindow.isMaximized();
    const isMinimized = activeWindow.isMinimized();
    const systemMenu = Menu.buildFromTemplate([
      {
        label: "Restore",
        enabled: isMaximized || isMinimized,
        click: () => {
          if (activeWindow.isMaximized()) activeWindow.unmaximize();
          if (activeWindow.isMinimized()) activeWindow.restore();
        },
      },
      { type: "separator" },
      { label: "Minimize", click: () => activeWindow.minimize() },
      {
        label: "Maximize",
        enabled: !isMaximized && !isMinimized,
        click: () => activeWindow.maximize(),
      },
      { type: "separator" },
      { label: "Close", click: () => activeWindow.close() },
    ]);
    systemMenu.popup({ window: activeWindow });
  };

  // Intercept Escape early to avoid Chromium exiting fullscreen before the
  // renderer can forward the key to the remote session. Keep a short fullscreen
  // grace window after pointer lock drops so rapid repeated Escape presses cannot
  // win the race before the renderer re-locks the pointer.
  window.webContents.on("before-input-event", (event, input) => {
    try {
      const mainWindow = deps.getMainWindow();
      // Dev-only: the default application menu (and its F12 / Ctrl+Shift+I /
      // Cmd+Option+I accelerators) is removed or stripped, so restore devtools
      // access here. Never in packaged builds — the key belongs to the game
      // during a stream there.
      if (!app.isPackaged && input.type === "keyDown" && !deps.getStreamInputActive()) {
        const isF12 = input.key === "F12";
        const isCtrlShiftI = input.control && input.shift && input.key.toLowerCase() === "i";
        const isCmdOptI = process.platform === "darwin" && input.meta && input.alt && input.key.toLowerCase() === "i";
        if (isF12 || isCtrlShiftI || isCmdOptI) {
          event.preventDefault();
          window.webContents.toggleDevTools();
          return;
        }
      }
      if (
        process.platform !== "darwin" &&
        input.type === "keyDown" &&
        input.alt &&
        !input.control &&
        !input.meta &&
        !deps.getStreamInputActive() &&
        !deps.getPointerLockActive() &&
        !(mainWindow && !mainWindow.isDestroyed() && mainWindow.isFullScreen()) &&
        input.key === " "
      ) {
        event.preventDefault();
        openFramelessWindowSystemMenu();
        return;
      }
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
        // No per-tap debounce: every physical Escape press forwards to the
        // game, matching GFN web where Escape taps are safe. The disconnect
        // risk of rapid presses was the fullscreen-exit race (Chromium leaving
        // fullscreen on each press), which this guard already prevents via
        // preventDefault + the hold state machine — OS auto-repeat is filtered
        // to "hold-repeat" and never becomes a tap, so a stuck/spammed key
        // still yields at most one tap per physical press-release cycle.
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
        opennowInput.releaseMouse();
      } catch {
        // ignore
      }
    }
  });
}
