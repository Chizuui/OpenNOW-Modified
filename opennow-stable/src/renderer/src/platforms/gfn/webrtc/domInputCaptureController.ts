import type { KeyboardLayout } from "@shared/gfn";

import {
  INPUT_MOUSE_ABS,
  INPUT_MOUSE_REL,
  codeMap,
  lockKeysStateFromEvent,
  mapKeyboardEvent,
  modifierFlags,
  toMouseButton,
  captureTimestampUs,
  type InputEncoder,
} from "../inputProtocol";
import { FULLSCREEN_KEYBOARD_LOCK_CODES } from "../keyboardLock";
import { GfnCursorOverlayController } from "../cursorChannel";
import {
  computeRelativeMouseDelta,
  MouseDeltaFilter,
  subsampleCoalescedPointerEvents,
} from "./mouseInput";

interface DomInputCaptureDependencies {
  videoElement: HTMLVideoElement;
  inputEncoder: InputEncoder;
  isInputReady: () => boolean;
  isInputBlocked: () => boolean;
  isNativeInputActive: () => boolean;
  isNativeElectronInputBridge: () => boolean;
  /** True while the native streamer owns OS RawInput capture (stacked sink). */
  isNativeStreamerInputOwned: () => boolean;
  shouldAutoFullscreen: () => boolean;
  getCurrentResolution: () => string;
  getKeyboardLayout: () => KeyboardLayout | undefined;
  getMicState: () => string;
  setWindowInputPaused: (paused: boolean) => void;
  recordSchedulingDelay: (delayMs: number) => void;
  refreshClipboardAvailability: () => Promise<boolean>;
  sendReliableSingleInput: (payload: Uint8Array) => void;
  sendReliable: (payload: Uint8Array) => void;
  sendInputPacket: (payload: Uint8Array, inputType: number) => void;
  onGamepadConnected: (event: GamepadEvent) => void;
  onGamepadDisconnected: (event: GamepadEvent) => void;
  log: (message: string) => void;
}

export interface MouseInputDiagnostics {
  flushBaseIntervalMs: number;
  flushIntervalMs: number;
  packetsPerSecond: number;
  residualMagnitude: number;
  /** EMA of the renderer-side event→send hop (ms) for addon / pointer-lock paths. */
  hopLatencyMs: number;
}

const MOUSE_FLUSH_FAST_MS = 0;
const MOUSE_FLUSH_NORMAL_MS = 0;
const MOUSE_FLUSH_SAFE_MS = 0;

function timestampUs(sourceTimestampMs?: number): bigint {
  return captureTimestampUs(sourceTimestampMs);
}

function parseResolution(resolution: string): { width: number; height: number } {
  const [rawWidth, rawHeight] = resolution.split("x");
  const width = Number.parseInt(rawWidth ?? "", 10);
  const height = Number.parseInt(rawHeight ?? "", 10);
  if (!Number.isFinite(width) || !Number.isFinite(height) || width <= 0 || height <= 0) {
    return { width: 1920, height: 1080 };
  }
  return { width, height };
}

export class DomInputCaptureController {
  private cursorOverlay: GfnCursorOverlayController | null = null;
  private inputCleanup: Array<() => void> = [];
  private readonly pressedKeys = new Set<number>();
  private pointerLockTarget: HTMLElement | null = null;
  private autoPointerLockInProgress = false;
  private pointerLockEscapeTimer: number | null = null;
  private pointerLockRelockTimer: number | null = null;
  private suppressNextSyntheticEscape = false;
  private syntheticEscapeSuppressionTimer: number | null = null;
  private keyboardLockState: "unknown" | "unsupported" | "locked" | "failed" = "unknown";
  private lastLockKeysState = -1;
  private mouseFlushTimer: number | null = null;
  private flushPendingMouseMovement: () => void = () => {};
  private pendingMouseDxFloat = 0;
  private pendingMouseDyFloat = 0;
  private pendingMouseAbs: { x: number; y: number; width: number; height: number } | null = null;
  private pendingMouseTimestampUs: bigint | null = null;
  private readonly mouseDeltaFilter = new MouseDeltaFilter();
  private mouseSensitivity = 1;
  private mouseAccelerationPercent = 1;
  private mouseFlushBaseIntervalMs = MOUSE_FLUSH_NORMAL_MS;
  private mouseFlushIntervalMs = MOUSE_FLUSH_NORMAL_MS;
  private mousePacketsSentInWindow = 0;
  private mousePacketsPerSecond = 0;
  private mousePacketRateWindowStartedAtMs = 0;
  private mouseFlushLastSendMs = 0;
  private mouseCoalescedBatchEntries = 0;
  private nativeCursorOverlayEnabled: boolean;

  // Native RawInput mode (Windows addon): when active, mouse comes from the OS
  // via injectNativeMouseEvent() instead of DOM/pointer-lock. The DOM mouse
  // handlers already gate on isPointerLockActive() (false in native mode), so
  // they stand down automatically — no double input.
  private nativeMouseActive = false;
  private queueNativeMouseMovement: (dx: number, dy: number) => void = () => {};
  private scheduleNativeMouseFlush: () => void = () => {};
  // Renderer-side hop latency diagnostic: time from a mouse event arriving in
  // this controller (addon delta or DOM pointer move) to the packet leaving on
  // the data channel. On the sink-native path this stays 0 — the mouse never
  // passes through the renderer.
  private mouseHopLatencyMs = 0;
  private mouseHopSamples = 0;
  private lastMouseEventArrivalMs: number | null = null;

  constructor(
    private readonly dependencies: DomInputCaptureDependencies,
    options: { mouseSensitivity: number; mouseAccelerationPercent: number; nativeCursorOverlay: boolean },
  ) {
    this.mouseSensitivity = options.mouseSensitivity;
    this.mouseAccelerationPercent = options.mouseAccelerationPercent;
    this.nativeCursorOverlayEnabled = options.nativeCursorOverlay;
  }

  setMouseSensitivity(value: number): void {
    this.mouseSensitivity = Math.max(0.01, Number.isFinite(value) ? value : 1);
  }

  setMouseAccelerationPercent(value: number): void {
    this.mouseAccelerationPercent = Math.max(1, Math.min(150, Math.round(Number.isFinite(value) ? value : 1)));
  }

  isNativeCursorOverlayEnabled(): boolean {
    return this.nativeCursorOverlayEnabled;
  }

  setNativeCursorOverlayEnabled(enabled: boolean): void {
    this.nativeCursorOverlayEnabled = enabled;
    if (!enabled) {
      this.cursorOverlay?.dispose();
      this.cursorOverlay = null;
      return;
    }
    if (!this.cursorOverlay) {
      this.cursorOverlay = new GfnCursorOverlayController(this.dependencies.videoElement);
      this.cursorOverlay.setFallbackResolution(parseResolution(this.dependencies.getCurrentResolution()));
      const lockElement = document.pointerLockElement;
      const pointerLockTarget = this.dependencies.videoElement.parentElement;
      this.cursorOverlay.setPointerLocked(
        lockElement === this.dependencies.videoElement || lockElement === pointerLockTarget,
      );
    }
  }

  setFallbackResolution(resolution: string): void {
    this.cursorOverlay?.setFallbackResolution(parseResolution(resolution));
  }

  handleCursorMessage(bytes: Uint8Array): boolean {
    return this.cursorOverlay?.handleMessage(bytes) ?? false;
  }

  suppressNextSyntheticEscapeOnPointerLockLoss(durationMs = 1000): void {
    this.clearSyntheticEscapeSuppression();
    this.suppressNextSyntheticEscape = true;
    this.syntheticEscapeSuppressionTimer = window.setTimeout(() => {
      this.clearSyntheticEscapeSuppression();
    }, Math.max(0, durationMs));
  }

  detach(): void {
    for (const cleanup of this.inputCleanup.splice(0)) {
      cleanup();
    }
    this.cursorOverlay?.dispose();
    this.cursorOverlay = null;
    this.flushPendingMouseMovement = () => {};
  }

  flushPendingMovement(): void {
    this.flushPendingMouseMovement();
  }

  reset(): void {
    this.detach();
    if (this.mouseFlushTimer !== null) {
      window.clearTimeout(this.mouseFlushTimer);
      this.mouseFlushTimer = null;
    }
    this.clearSyntheticEscapeSuppression();
    this.pendingMouseDxFloat = 0;
    this.pendingMouseDyFloat = 0;
    this.pendingMouseAbs = null;
    this.pendingMouseTimestampUs = null;
    this.mouseDeltaFilter.reset();
    this.mouseFlushLastSendMs = 0;
    this.mouseCoalescedBatchEntries = 0;
    this.mouseFlushBaseIntervalMs = MOUSE_FLUSH_NORMAL_MS;
    this.mouseFlushIntervalMs = MOUSE_FLUSH_NORMAL_MS;
    this.mousePacketsSentInWindow = 0;
    this.mousePacketsPerSecond = 0;
    this.mousePacketRateWindowStartedAtMs = 0;
    this.lastLockKeysState = -1;
  }

  getMouseDiagnostics(): MouseInputDiagnostics {
    return {
      flushBaseIntervalMs: this.mouseFlushBaseIntervalMs,
      flushIntervalMs: this.mouseFlushIntervalMs,
      packetsPerSecond: this.mousePacketsPerSecond,
      residualMagnitude: Math.hypot(this.pendingMouseDxFloat, this.pendingMouseDyFloat),
      hopLatencyMs: Math.round(this.mouseHopLatencyMs * 100) / 100,
    };
  }

  /** Pin the mouse coalesce window to its base interval. Deliberately never
   *  adaptive: growing the window under reliable-channel pressure made input
   *  latency track the network — the mouse slowed down exactly when the
   *  stream struggled. Native RawInput mouse stays at immediate flush (0 ms). */
  pinFlushIntervalToBase(): void {
    this.mouseFlushIntervalMs = this.nativeMouseActive ? 0 : this.mouseFlushBaseIntervalMs;
  }

  clearSyntheticEscapeSuppression(): void {
    this.suppressNextSyntheticEscape = false;
    if (this.syntheticEscapeSuppressionTimer !== null) {
      window.clearTimeout(this.syntheticEscapeSuppressionTimer);
      this.syntheticEscapeSuppressionTimer = null;
    }
  }

  private consumeSyntheticEscapeSuppression(): boolean {
    if (!this.suppressNextSyntheticEscape) {
      return false;
    }
    this.clearSyntheticEscapeSuppression();
    return true;
  }

  async requestPointerLockCompat(
    lockTarget: HTMLElement,
    options?: { unadjustedMovement?: boolean },
  ): Promise<void> {
    const maybePromise = lockTarget.requestPointerLock(options as any) as unknown;
    if (maybePromise && typeof (maybePromise as Promise<void>).then === "function") {
      await (maybePromise as Promise<void>);
    }
  }

  private syncLockKeysState(event: KeyboardEvent): void {
    const state = lockKeysStateFromEvent(event);
    if (state === this.lastLockKeysState) {
      return;
    }
    this.lastLockKeysState = state;
    if (!this.dependencies.isInputReady()) {
      return;
    }
    this.dependencies.sendReliableSingleInput(this.dependencies.inputEncoder.encodeLockKeysSync(state));
  }

  private requestEscapeKeyboardLock(): void {
    // Match the official GFN web client: keyboard.lock() only works (and is only
    // requested) in fullscreen. This is what actually keeps Escape from
    // releasing the pointer lock — so the stream forces fullscreen on entry.
    if (!document.fullscreenElement) {
      if (this.keyboardLockState === "locked") {
        this.keyboardLockState = "unknown";
      }
      return;
    }

    const nav = navigator as any;
    if (!nav.keyboard?.lock) {
      if (this.keyboardLockState !== "unsupported") {
        this.keyboardLockState = "unsupported";
        this.dependencies.log("Keyboard Lock API unavailable; Escape may release pointer lock");
      }
      return;
    }

    void Promise.resolve(nav.keyboard.lock(FULLSCREEN_KEYBOARD_LOCK_CODES))
      .then(() => {
        if (this.keyboardLockState !== "locked") {
          this.keyboardLockState = "locked";
          this.dependencies.log("Keyboard lock active for fullscreen stream");
        }
      })
      .catch((error: unknown) => {
        this.keyboardLockState = "failed";
        this.dependencies.log(`Keyboard Escape lock failed: ${String(error)}`);
      });
  }

  private async requestPointerLockWithOptionalFullscreen(
    lockTarget: HTMLElement,
    ensureFullscreen: boolean,
  ): Promise<void> {
    if (ensureFullscreen && !document.fullscreenElement) {
      // Use DOM fullscreen (requestFullscreen) FIRST, not Electron window
      // fullscreen. navigator.keyboard.lock() — the only thing that stops Escape
      // from releasing pointer lock — requires document.fullscreenElement to be
      // set, which Electron's BrowserWindow.setFullscreen() does NOT do. This is
      // exactly what the official GFN web client relies on. Fall back to the
      // native window fullscreen only if the DOM request is rejected.
      try {
        await document.documentElement.requestFullscreen();
      } catch (error) {
        this.dependencies.log(`DOM fullscreen request failed: ${String(error)}`);
        if (typeof window.openNow?.setFullscreen === "function") {
          try {
            await window.openNow.setFullscreen(true);
          } catch (nativeError) {
            this.dependencies.log(`Native fullscreen request failed: ${String(nativeError)}`);
          }
        }
      }
    }

    this.requestEscapeKeyboardLock();

    try {
      await this.requestPointerLockCompat(lockTarget, { unadjustedMovement: true });
      this.dependencies.log("Pointer lock acquired with unadjustedMovement=true (raw/unaccelerated)");
    } catch (err) {
      const domErr = err as DOMException;
      if (domErr?.name === "NotSupportedError") {
        this.dependencies.log("unadjustedMovement not supported, falling back to standard pointer lock (accelerated)");
        await this.requestPointerLockCompat(lockTarget);
      } else {
        throw err;
      }
    }
  }

  async attemptAutoPointerLock(ensureFullscreen = true): Promise<void> {
    if (this.autoPointerLockInProgress) return;
    // Native RawInput capture is active: the addon owns the cursor (confines +
    // hides it) and feeding raw deltas directly. Requesting DOM pointer lock on
    // top would fight the addon's ClipCursor — never auto-lock in native mode.
    if (this.isNativeMouseActive()) return;
    this.autoPointerLockInProgress = true;
    try {
      const target = this.pointerLockTarget ?? this.dependencies.videoElement;
      if (!target) return;
      const lockElement = document.pointerLockElement;
      if (lockElement === target || lockElement === this.dependencies.videoElement) {
        return;
      }

      try {
        await this.requestPointerLockWithOptionalFullscreen(target, ensureFullscreen);
        this.dependencies.log("Auto pointer lock acquired");
        return;
      } catch (err) {
        // Fallback to a simpler request if the guarded method fails
        try {
          await this.requestPointerLockCompat(target, { unadjustedMovement: true });
          this.dependencies.log("Auto pointer lock acquired (fallback)");
          return;
        } catch {
          this.dependencies.log(`Auto pointer lock failed: ${String(err)}`);
        }
      }
    } finally {
      this.autoPointerLockInProgress = false;
    }
  }

  private shouldSendSyntheticEscapeOnPointerLockLoss(): boolean {
    if (document.visibilityState !== "visible") {
      return false;
    }
    if (typeof document.hasFocus === "function" && !document.hasFocus()) {
      return false;
    }
    return true;
  }

  releasePressedKeys(reason: string): void {
    if (this.pressedKeys.size === 0 || !this.dependencies.isInputReady()) {
      this.pressedKeys.clear();
      return;
    }

    this.dependencies.log(`Releasing ${this.pressedKeys.size} key(s): ${reason}`);
    for (const vk of this.pressedKeys) {
      const payload = this.dependencies.inputEncoder.encodeKeyUp({
        keycode: vk,
        scancode: 0,
        modifiers: 0,
        timestampUs: timestampUs(),
      });
      this.dependencies.sendReliableSingleInput(payload);
    }
    this.pressedKeys.clear();
  }

  private sendKeyPacket(vk: number, scancode: number, modifiers: number, isDown: boolean): void {
    const payload = isDown
      ? this.dependencies.inputEncoder.encodeKeyDown({
        keycode: vk,
        scancode,
        modifiers,
        timestampUs: timestampUs(),
      })
      : this.dependencies.inputEncoder.encodeKeyUp({
        keycode: vk,
        scancode,
        modifiers,
        timestampUs: timestampUs(),
      });
    this.dependencies.sendReliableSingleInput(payload);
  }

  public sendAntiAfkPulse(): boolean {
    if (!this.dependencies.isInputReady()) {
      return false;
    }

    this.sendKeyPacket(codeMap.F13.vk, codeMap.F13.scancode, 0, true);
    window.setTimeout(() => this.sendKeyPacket(codeMap.F13.vk, codeMap.F13.scancode, 0, false), 50);
    return true;
  }

  public sendPasteShortcut(useMeta: boolean): boolean {
    if (!this.dependencies.isInputReady()) {
      return false;
    }

    const modifier = useMeta
      ? { ...codeMap.MetaLeft, flag: 0x08 }
      : { ...codeMap.ControlLeft, flag: 0x02 };

    this.sendKeyPacket(modifier.vk, modifier.scancode, modifier.flag, true);
    this.sendKeyPacket(codeMap.KeyV.vk, codeMap.KeyV.scancode, modifier.flag, true);
    this.sendKeyPacket(codeMap.KeyV.vk, codeMap.KeyV.scancode, modifier.flag, false);
    this.sendKeyPacket(modifier.vk, modifier.scancode, 0, false);
    return true;
  }

  public sendText(text: string): number {
    if (!this.dependencies.isInputReady() || !text) {
      return 0;
    }

    const chunks = this.dependencies.inputEncoder.encodeTextInput(text);
    for (const chunk of chunks) {
      this.dependencies.sendReliable(chunk);
    }

    return Array.from(text).length;
  }

  /** Toggle native RawInput mouse mode. When active, the DOM mouse handlers
   *  stand down (they gate on pointer lock, which native mode does not use) and
   *  movement/buttons/wheel arrive via injectNativeMouseEvent(). */
  setNativeMouseActive(active: boolean): void {
    this.nativeMouseActive = active;
    if (active) {
      // Raw deltas must reach the encoder on the next task turn, never wait on
      // a coalesce window — 1:1 latency like the official GFN app.
      this.mouseFlushIntervalMs = 0;
    } else {
      this.flushPendingMovement();
    }
  }

  isNativeMouseActive(): boolean {
    return this.nativeMouseActive;
  }

  /** Feed one raw mouse event from the native addon into the encode pipeline. */
  injectNativeMouseEvent(ev: { kind: number; dx: number; dy: number; button: number; state: number; wheel: number }): void {
    if (!this.nativeMouseActive || !this.dependencies.isInputReady() || this.dependencies.isInputBlocked()) {
      return;
    }
    this.lastMouseEventArrivalMs = performance.now();
    if (ev.kind === 0) {
      // Relative move.
      if (ev.dx !== 0 || ev.dy !== 0) {
        this.queueNativeMouseMovement(ev.dx, ev.dy);
        this.scheduleNativeMouseFlush();
      }
      return;
    }
    if (ev.kind === 1) {
      // Button. Native RawInput order (0=L 1=R 2=M 3=X1 4=X2) mapped to GFN
      // button ids (1=Left 2=Middle 3=Right 4=Back/X1 5=Forward/X2). NOTE: do
      // not reuse toMouseButton() here — that expects DOM button order, which
      // swaps middle/right relative to RawInput.
      const NATIVE_TO_GFN_BUTTON: Record<number, number> = { 0: 1, 1: 3, 2: 2, 3: 4, 4: 5 };
      const gfnButton = NATIVE_TO_GFN_BUTTON[ev.button];
      if (gfnButton === undefined) {
        return;
      }
      const payload = ev.state === 1
        ? this.dependencies.inputEncoder.encodeMouseButtonDown({ button: gfnButton, timestampUs: timestampUs() })
        : this.dependencies.inputEncoder.encodeMouseButtonUp({ button: gfnButton, timestampUs: timestampUs() });
      this.dependencies.sendReliableSingleInput(payload);
      return;
    }
    if (ev.kind === 2) {
      // Wheel. Native gives signed notches * 120; forward the raw signed value
      // clamped to int16 like the DOM wheel path.
      const delta = Math.max(-32768, Math.min(32767, Math.round(ev.wheel)));
      if (delta !== 0) {
        const payload = this.dependencies.inputEncoder.encodeMouseWheel({ delta, timestampUs: timestampUs() });
        this.dependencies.sendReliableSingleInput(payload);
      }
    }
  }

  install(videoElement: HTMLVideoElement): void {
    this.detach();

    const pointerLockTarget = (videoElement.parentElement as HTMLElement | null) ?? videoElement;
    const originalPointerLockTargetTabIndex = pointerLockTarget.getAttribute("tabindex");
    if (this.isNativeCursorOverlayEnabled()) {
      this.cursorOverlay = new GfnCursorOverlayController(videoElement);
      this.cursorOverlay.setFallbackResolution(parseResolution(this.dependencies.getCurrentResolution()));
    } else {
      this.cursorOverlay = null;
    }
    if (originalPointerLockTargetTabIndex === null) {
      pointerLockTarget.tabIndex = -1;
    }
    const focusPointerLockTarget = (): void => {
      try {
        pointerLockTarget.focus({ preventScroll: true });
      } catch {
        pointerLockTarget.focus();
      }
    };
    const isPointerLockActive = (): boolean => {
      const lockElement = document.pointerLockElement;
      return lockElement === pointerLockTarget || lockElement === videoElement;
    };
    this.cursorOverlay?.setPointerLocked(isPointerLockActive());

    // Mirror mode: tracks whether the HW cursor is over the stream viewport.
    // Dual-source: coarse window focus/blur sets the initial state and handles
    // cases where the cursor was already inside when the stream started;
    // mouseenter/mouseleave on pointerLockTarget refines it for sub-window
    // boundaries (overlays, toolbars, multi-monitor cursor exit without blur).
    let mouseInStreamView = document.hasFocus();
    let lastAbsX: number | null = null;
    let lastAbsY: number | null = null;
    // Prevent repeated auto-lock attempts within the same focus session.
    let autoLockPending = false;

    // Track an approximate server-side absolute pointer position (in server
    // pixels — the remote stream's resolution) so we can align the server cursor
    // to the hardware cursor when transitioning from mirror -> pointer-lock.
    // `null` means unknown; when unknown we assume server cursor equals HW cursor on first entry.
    let simulatedAbsX: number | null = null;
    let simulatedAbsY: number | null = null;
    // When a document-level entry event triggers tryAutoLock, we store the
    // entry absolute coordinates here so tryAutoLock can align before locking.
    let pendingEntryAbsX: number | null = null;
    let pendingEntryAbsY: number | null = null;

    const onPointerLockTargetMouseEnter = (): void => {
      mouseInStreamView = true;
      lastAbsX = null;
      lastAbsY = null;
      tryAutoLock();
    };

    const onPointerLockTargetMouseLeave = (): void => {
      mouseInStreamView = false;
      lastAbsX = null;
      lastAbsY = null;
      autoLockPending = false;
    };

    const hasPointerRawUpdate = "onpointerrawupdate" in videoElement;
    const hasCoalescedEvents =
      typeof PointerEvent !== "undefined" && "getCoalescedEvents" in PointerEvent.prototype;
    const pointerMoveEventName: "pointerrawupdate" | "pointermove" | null = hasPointerRawUpdate
      ? "pointerrawupdate"
      : (typeof PointerEvent !== "undefined" ? "pointermove" : null);
    this.mouseFlushBaseIntervalMs = hasPointerRawUpdate
      ? MOUSE_FLUSH_FAST_MS
      : hasCoalescedEvents
        ? MOUSE_FLUSH_NORMAL_MS
        : MOUSE_FLUSH_SAFE_MS;
    this.mouseFlushIntervalMs = this.mouseFlushBaseIntervalMs;
    const mouseInitNow = performance.now();
    this.mouseFlushLastSendMs = mouseInitNow;
    this.mouseCoalescedBatchEntries = 0;
    this.pendingMouseDxFloat = 0;
    this.pendingMouseDyFloat = 0;
    this.pendingMouseAbs = null;
    this.pendingMouseTimestampUs = null;
    this.mousePacketsPerSecond = 0;
    this.mousePacketsSentInWindow = 0;
    this.mousePacketRateWindowStartedAtMs = mouseInitNow;
    this.mouseDeltaFilter.reset();
    this.mouseDeltaFilter.setRelaxedForRawInput(hasPointerRawUpdate);
    this.dependencies.log(
      `Mouse input mode: ${pointerMoveEventName ?? "mousemove"}, coalesced=${hasCoalescedEvents ? "yes" : "no"}, flush=${this.mouseFlushIntervalMs}ms`,
    );

    const pointerScaleCache = {
      rectWidth: 0,
      rectHeight: 0,
      scaleX: 1,
      scaleY: 1,
      serverWidth: 0,
      serverHeight: 0,
      resolution: "",
    };
    const getPointerScale = (): typeof pointerScaleCache => {
      const rect = pointerLockTarget.getBoundingClientRect();
      const resolution = this.dependencies.getCurrentResolution() ?? "";
      if (
        pointerScaleCache.rectWidth === rect.width
        && pointerScaleCache.rectHeight === rect.height
        && pointerScaleCache.resolution === resolution
      ) {
        return pointerScaleCache;
      }

      let serverWidth = rect.width;
      let serverHeight = rect.height;
      const resMatch = /^([0-9]+)x([0-9]+)$/.exec(resolution);
      if (resMatch) {
        serverWidth = parseInt(resMatch[1], 10) || serverWidth;
        serverHeight = parseInt(resMatch[2], 10) || serverHeight;
      }

      pointerScaleCache.rectWidth = rect.width;
      pointerScaleCache.rectHeight = rect.height;
      pointerScaleCache.serverWidth = serverWidth;
      pointerScaleCache.serverHeight = serverHeight;
      pointerScaleCache.scaleX = rect.width > 0 ? serverWidth / rect.width : 1;
      pointerScaleCache.scaleY = rect.height > 0 ? serverHeight / rect.height : 1;
      pointerScaleCache.resolution = resolution;
      return pointerScaleCache;
    };

    const updateMousePacketRate = (): void => {
      const now = performance.now();
      if (this.mousePacketRateWindowStartedAtMs <= 0) {
        this.mousePacketRateWindowStartedAtMs = now;
      }
      const elapsed = now - this.mousePacketRateWindowStartedAtMs;
      if (elapsed >= 1000) {
        this.mousePacketsPerSecond = Math.round((this.mousePacketsSentInWindow * 1000) / elapsed);
        this.mousePacketsSentInWindow = 0;
        this.mousePacketRateWindowStartedAtMs = now;
      }
    };

    let pointerRawStuckCount = 0;
    let lastPointerClientX = Number.NaN;
    let lastPointerClientY = Number.NaN;

    const hasPendingMouseMovement = (): boolean =>
      this.pendingMouseAbs !== null
      || Math.abs(this.pendingMouseDxFloat) >= 0.5
      || Math.abs(this.pendingMouseDyFloat) >= 0.5;

    const markServerCursorAt = (abs: { x: number; y: number; width: number; height: number }): void => {
      // An absolute packet pins the server cursor exactly; keep the simulated
      // server-pixel baseline in sync for the pointer-lock entry alignment path.
      const { serverWidth, serverHeight } = getPointerScale();
      simulatedAbsX = Math.round((abs.x / abs.width) * serverWidth);
      simulatedAbsY = Math.round((abs.y / abs.height) * serverHeight);
    };

    const flushMouse = (forceReliable = false): boolean => {
      const tickNow = performance.now();
      if (!this.dependencies.isInputReady() || !hasPendingMouseMovement()) {
        return false;
      }

      // A batch can hold both an absolute position (queued while the overlay
      // cursor was visible) and relative deltas accumulated after the cursor
      // was hidden mid-batch. Send the absolute packet first, then the
      // relative deltas, preserving event order like the official client's
      // mixed batch encoding — never discard queued relative movement.
      const batchTimestampUs = this.pendingMouseTimestampUs ?? timestampUs();
      let sentAny = false;

      // Compute the relative part first (without consuming it) so a mixed
      // abs+rel pair can be detected up front. The partially reliable channel
      // is unordered, so a dependent pair must travel on the ordered reliable
      // channel or the relative delta could arrive before the absolute pin
      // and be overwritten by it.
      // Raw 1:1 deltas for every renderer source (addon + DOM pointer lock):
      // computeRelativeMouseDelta deliberately applies no server-width ÷
      // window-width normalization — raw-input games calibrate sensitivity on
      // raw counts, so window-size scaling would make the feel depend on the
      // window size and break muscle memory (local play has no such scaling
      // either). Absolute positioning (pointer-lock entry alignment, cursor
      // overlay) still uses getPointerScale below.
      const relPart = computeRelativeMouseDelta(
        this.pendingMouseDxFloat,
        this.pendingMouseDyFloat,
      );
      const mixedBatch = this.pendingMouseAbs !== null && relPart !== null;

      if (this.pendingMouseAbs !== null) {
        const abs = this.pendingMouseAbs;
        this.pendingMouseAbs = null;
        const payload = this.dependencies.inputEncoder.encodeMouseAbsolute({
          ...abs,
          timestampUs: batchTimestampUs,
        });
        if (mixedBatch || forceReliable) {
          this.dependencies.sendReliable(payload);
        } else {
          this.dependencies.sendInputPacket(payload, INPUT_MOUSE_ABS);
        }
        this.mousePacketsSentInWindow += 1;
        markServerCursorAt(abs);
        sentAny = true;
      }

      if (relPart !== null) {
        this.pendingMouseDxFloat = relPart.residualX;
        this.pendingMouseDyFloat = relPart.residualY;

        const payload = this.dependencies.inputEncoder.encodeMouseMove({
          dx: relPart.dxServer,
          dy: relPart.dyServer,
          timestampUs: batchTimestampUs,
        });
        if (mixedBatch || forceReliable) {
          this.dependencies.sendReliable(payload);
        } else {
          this.dependencies.sendInputPacket(payload, INPUT_MOUSE_REL);
        }
        this.mousePacketsSentInWindow += 1;

        if (simulatedAbsX !== null && simulatedAbsY !== null) {
          simulatedAbsX += relPart.dxServer;
          simulatedAbsY += relPart.dyServer;
        }
        sentAny = true;
      }

      if (!sentAny) {
        return false;
      }

      const expectedSendAt = this.mouseFlushLastSendMs + this.mouseFlushIntervalMs;
      this.dependencies.recordSchedulingDelay(Math.max(0, tickNow - expectedSendAt));
      this.pendingMouseTimestampUs = null;
      this.mouseCoalescedBatchEntries = 0;
      this.mouseFlushLastSendMs = tickNow;
      updateMousePacketRate();
      // Hop latency diagnostic: event arrival → packet on the wire. EMA over
      // samples so a single scheduling hiccup doesn't skew the shown value.
      if (this.lastMouseEventArrivalMs !== null) {
        const hopMs = Math.max(0, tickNow - this.lastMouseEventArrivalMs);
        this.lastMouseEventArrivalMs = null;
        this.mouseHopLatencyMs = this.mouseHopSamples === 0
          ? hopMs
          : this.mouseHopLatencyMs * 0.9 + hopMs * 0.1;
        this.mouseHopSamples += 1;
      }
      return true;
    };

    this.flushPendingMouseMovement = () => {
      try {
        flushMouse();
      } catch (err) {
        this.dependencies.log(`Mouse flush failed (non-fatal): ${String(err)}`);
      }
    };

    /** Official GFN dl(): schedule cl() after the coalesce interval elapses. */
    const scheduleMouseBatchFlush = (): void => {
      if (this.mouseFlushTimer !== null) {
        return;
      }

      const now = performance.now();
      const elapsed = now - this.mouseFlushLastSendMs;
      if (this.mouseFlushIntervalMs <= 0 || elapsed >= this.mouseFlushIntervalMs) {
        flushMouse();
        if (hasPendingMouseMovement()) {
          // Defer the follow-up flush instead of rescheduling synchronously:
          // when flushMouse cannot drain the batch (e.g. input not ready while
          // the quick menu / pause overlay is open) a sync reschedule recurses
          // until "Maximum call stack size exceeded". A timer lets the loop
          // yield and keeps pending deltas until input becomes ready again.
          this.mouseFlushTimer = window.setTimeout(() => {
            this.mouseFlushTimer = null;
            scheduleMouseBatchFlush();
          }, 0);
        }
        return;
      }

      this.mouseFlushTimer = window.setTimeout(() => {
        this.mouseFlushTimer = null;
        try {
          flushMouse();
        } catch (err) {
          this.dependencies.log(`Mouse flush tick failed (non-fatal): ${String(err)}`);
        } finally {
          if (hasPendingMouseMovement()) {
            scheduleMouseBatchFlush();
          }
        }
      }, Math.max(0, this.mouseFlushIntervalMs - elapsed));
    };

    /** Official GFN Cp(): after wm(), flush when the mouse batch transitions empty -> non-empty. */
    const afterPointerMovement = (): void => {
      if (!hasPendingMouseMovement()) {
        return;
      }
      const elapsed = performance.now() - this.mouseFlushLastSendMs;
      if (this.mouseFlushIntervalMs <= 0 || elapsed >= this.mouseFlushIntervalMs) {
        flushMouse();
        if (hasPendingMouseMovement()) {
          // Same deferred follow-up as scheduleMouseBatchFlush — never reschedule
          // synchronously or an undrainable batch overflows the stack.
          this.mouseFlushTimer = window.setTimeout(() => {
            this.mouseFlushTimer = null;
            scheduleMouseBatchFlush();
          }, 0);
        }
      } else {
        scheduleMouseBatchFlush();
      }
    };

    // Native RawInput movement path. Mirrors queueMouseMovement's sensitivity +
    // acceleration but is NOT gated on pointer lock (native mode has none), and
    // skips the DOM delta filter / cursor overlay (raw OS deltas are already
    // clean). Feeds the same pending-delta batch + flush pipeline.
    this.queueNativeMouseMovement = (dx: number, dy: number): void => {
      if (!this.dependencies.isInputReady()) {
        return;
      }
      let adjustedDx = dx * this.mouseSensitivity;
      let adjustedDy = dy * this.mouseSensitivity;
      if (this.mouseAccelerationPercent > 1) {
        const speed = Math.hypot(adjustedDx, adjustedDy);
        const strength = (this.mouseAccelerationPercent - 1) / 149;
        const accelFactor = 1 + Math.min(0.6 * strength, (speed / 50) * strength);
        adjustedDx *= accelFactor;
        adjustedDy *= accelFactor;
      }
      this.pendingMouseDxFloat += adjustedDx;
      this.pendingMouseDyFloat += adjustedDy;
      if (this.pendingMouseTimestampUs === null) {
        this.pendingMouseTimestampUs = timestampUs();
      }
      this.mouseCoalescedBatchEntries += 1;
    };
    this.scheduleNativeMouseFlush = scheduleMouseBatchFlush;

    const tryAutoLock = (): void => {
      try {
        if (document?.body?.dataset?.sidebarOpen === "1") {
          return;
        }
      } catch {}

      // Never lock while the window is not actually focused (mid alt-tab, or
      // focus was lost without a blur event yet) — the request would fail with
      // WrongDocumentError and the retry loop would fight the user's switch.
      if (typeof document.hasFocus === "function" && !document.hasFocus()) {
        return;
      }

      if (
        autoLockPending
        || isPointerLockActive()
        || this.isNativeMouseActive()
        || this.dependencies.isNativeStreamerInputOwned()
        || !mouseInStreamView
        || !this.dependencies.isInputReady()
      ) {
        return;
      }
      autoLockPending = true;

      // Align server cursor to current HW cursor (if we have an entry position)
      // before requesting pointer lock so the transition appears smooth.
      try {
        const targetAbsX = pendingEntryAbsX ?? lastAbsX;
        const targetAbsY = pendingEntryAbsY ?? lastAbsY;
        // Consume pending entry coords
        pendingEntryAbsX = null;
        pendingEntryAbsY = null;

        if (typeof targetAbsX === "number" && typeof targetAbsY === "number") {
          const targetRect = pointerLockTarget.getBoundingClientRect();
          this.cursorOverlay?.setClientPosition(targetRect.left + targetAbsX, targetRect.top + targetAbsY);
          const overlayAbs = this.cursorOverlay?.isCursorVisible()
            ? this.cursorOverlay.getAbsolutePosition()
            : null;
          const { scaleX, scaleY, serverWidth, serverHeight } = getPointerScale();

          if (overlayAbs) {
            // Overlay cursor is visible: pin the server cursor with one
            // absolute packet instead of simulating relative moves.
            const movePayload = this.dependencies.inputEncoder.encodeMouseAbsolute({
              ...overlayAbs,
              timestampUs: timestampUs(),
            });
            this.dependencies.sendReliable(movePayload);
            markServerCursorAt(overlayAbs);
          } else {
            // Translate the element-local target into server pixels.
            const targetServerX = Math.round(targetAbsX * scaleX);
            const targetServerY = Math.round(targetAbsY * scaleY);

            if (simulatedAbsX === null || simulatedAbsY === null) {
              // No baseline known: assume server cursor is centered and move from
              // center -> target in server pixels so remote cursor matches HW cursor.
              const baselineXServer = Math.round(serverWidth / 2);
              const baselineYServer = Math.round(serverHeight / 2);
              const dx = Math.round(targetServerX - baselineXServer);
              const dy = Math.round(targetServerY - baselineYServer);
              if (dx !== 0 || dy !== 0) {
                const movePayload = this.dependencies.inputEncoder.encodeMouseMove({
                  dx: Math.max(-32768, Math.min(32767, dx)),
                  dy: Math.max(-32768, Math.min(32767, dy)),
                  timestampUs: timestampUs(),
                });
                this.dependencies.sendReliable(movePayload);
              }
              // Record simulated baseline in server pixels.
              simulatedAbsX = targetServerX;
              simulatedAbsY = targetServerY;
            } else {
              // sim values are stored in server pixels now; compute server delta.
              const dx = Math.round(targetServerX - simulatedAbsX);
              const dy = Math.round(targetServerY - simulatedAbsY);
              if (dx !== 0 || dy !== 0) {
                const movePayload = this.dependencies.inputEncoder.encodeMouseMove({
                  dx: Math.max(-32768, Math.min(32767, dx)),
                  dy: Math.max(-32768, Math.min(32767, dy)),
                  timestampUs: timestampUs(),
                });
                this.dependencies.sendReliable(movePayload);
                simulatedAbsX += dx;
                simulatedAbsY += dy;
              }
            }
          }
        }
      } catch (err) {
        this.dependencies.log(`Pointer lock alignment failed (non-fatal): ${String(err)}`);
      }

      // Force fullscreen on re-lock too, so keyboard.lock() stays engaged and
      // Escape keeps reaching the game without dropping the pointer (GFN parity).
      void this.attemptAutoPointerLock(true)
        .catch(() => {})
        .finally(() => {
          autoLockPending = false;
        });
    };

    const queueMouseMovement = (dx: number, dy: number, eventTimestampMs: number): void => {
      // The native streamer owns RawInput capture on the stacked sink window:
      // it feeds the data channel directly, so forwarding DOM deltas here too
      // would double-input the game (mouse runs ~2×). The sink also drops the
      // DOM pointer lock on arm, but gate on the flag as defense in depth.
      if (
        !this.dependencies.isInputReady()
        || !isPointerLockActive()
        || this.dependencies.isNativeStreamerInputOwned()
      ) {
        return;
      }
      this.lastMouseEventArrivalMs = performance.now();

      if (!this.mouseDeltaFilter.update(dx, dy, eventTimestampMs)) {
        return;
      }

      // Apply user-configured sensitivity, then optional software acceleration.
      let adjustedDx = this.mouseDeltaFilter.getX() * this.mouseSensitivity;
      let adjustedDy = this.mouseDeltaFilter.getY() * this.mouseSensitivity;

      if (this.mouseAccelerationPercent > 1) {
        const speed = Math.hypot(adjustedDx, adjustedDy);
        const strength = (this.mouseAccelerationPercent - 1) / 149;
        // Gentle curve: low-speed precision, high-speed turn boost (caps at +60% at 150%).
        const accelFactor = 1 + Math.min(0.6 * strength, (speed / 50) * strength);
        adjustedDx *= accelFactor;
        adjustedDy *= accelFactor;
      }

      this.cursorOverlay?.moveBy(adjustedDx, adjustedDy);

      // Official GFN local-cursor mode: while the client-rendered cursor is
      // visible, send absolute positions (type 5) that mirror the clamped
      // overlay position so the server cursor cannot drift from the overlay.
      // Relative deltas (type 7) remain for hidden-cursor/raw-input games.
      if (this.cursorOverlay?.isCursorVisible()) {
        const abs = this.cursorOverlay.getAbsolutePosition();
        if (abs) {
          // Deliver raw-input deltas queued before the cursor became
          // visible ahead of the absolute pin, in order, on the reliable
          // channel — never after it, where they would shift the server
          // cursor off the overlay.
          if (
            Math.abs(this.pendingMouseDxFloat) >= 0.5
            || Math.abs(this.pendingMouseDyFloat) >= 0.5
          ) {
            flushMouse(true);
          }
          this.pendingMouseDxFloat = 0;
          this.pendingMouseDyFloat = 0;
          this.pendingMouseAbs = abs;
          if (this.pendingMouseTimestampUs === null) {
            this.pendingMouseTimestampUs = timestampUs(eventTimestampMs);
          }
          this.mouseCoalescedBatchEntries += 1;
          return;
        }
      }

      this.pendingMouseDxFloat += adjustedDx;
      this.pendingMouseDyFloat += adjustedDy;
      if (this.pendingMouseTimestampUs === null) {
        this.pendingMouseTimestampUs = timestampUs(eventTimestampMs);
      }
      this.mouseCoalescedBatchEntries += 1;
    };

    const processRelativePointerSamples = (
      samples: readonly { movementX: number; movementY: number; timeStamp: number }[],
    ): void => {
      const hadBatch = hasPendingMouseMovement();
      const { events } = subsampleCoalescedPointerEvents(samples, this.mouseCoalescedBatchEntries);
      for (const sample of events) {
        queueMouseMovement(sample.movementX, sample.movementY, sample.timeStamp);
      }
      if (!hadBatch && hasPendingMouseMovement()) {
        afterPointerMovement();
      }
    };

    const onPointerMove = (event: PointerEvent) => {
      try {
        if (document?.body?.dataset?.sidebarOpen === "1") return;
      } catch {}
      if (this.dependencies.isInputBlocked()) return;
      if (event.pointerType && event.pointerType !== "mouse") {
        return;
      }

      if (isPointerLockActive()) {
        if (hasPointerRawUpdate && event.type === "pointerrawupdate") {
          if (event.movementX === 0 && event.movementY === 0) {
            const clientMoved =
              event.clientX !== lastPointerClientX || event.clientY !== lastPointerClientY;
            lastPointerClientX = event.clientX;
            lastPointerClientY = event.clientY;
            if (clientMoved && ++pointerRawStuckCount >= 8) {
              this.dependencies.log("pointerrawupdate stuck; switching to immediate mouse flush");
              this.mouseFlushIntervalMs = 0;
              pointerRawStuckCount = 0;
            }
          } else {
            pointerRawStuckCount = 0;
          }
        }

        const samples = hasCoalescedEvents ? event.getCoalescedEvents() : [];
        if (samples.length > 0) {
          processRelativePointerSamples(samples);
          return;
        }
        processRelativePointerSamples([event]);
      } else if (mouseInStreamView) {
        // Pointer lock disabled: keep local cursor tracking up to date without
        // forwarding mouse movement into the stream.
        const rect = pointerLockTarget.getBoundingClientRect();
        const absX = event.clientX - rect.left;
        const absY = event.clientY - rect.top;
        lastAbsX = absX;
        lastAbsY = absY;
      }
    };

    const onMouseMove = (event: MouseEvent) => {
      try {
        if (document?.body?.dataset?.sidebarOpen === "1") return;
      } catch {}
      if (this.dependencies.isInputBlocked()) return;
      if (isPointerLockActive()) {
        processRelativePointerSamples([event]);
      } else if (mouseInStreamView) {
        // Pointer lock disabled: keep local cursor tracking up to date without
        // forwarding mouse movement into the stream.
        const rect = pointerLockTarget.getBoundingClientRect();
        const absX = event.clientX - rect.left;
        const absY = event.clientY - rect.top;
        lastAbsX = absX;
        lastAbsY = absY;
      }
    };

    const onKeyDown = (event: KeyboardEvent) => {
      if (this.dependencies.isInputBlocked()) return;
      if (!this.dependencies.isInputReady()) {
        return;
      }

      const isEscapeEvent =
        event.key === "Escape"
        || event.key === "Esc"
        || event.code === "Escape"
        || event.keyCode === 27;

      // The native streamer owns OS RawInput keyboard capture on the stacked
      // sink: keys flow from the streamer, so forwarding them again here would
      // double-input the game. Escape is the exception — the streamer skips it
      // and Electron keeps intercepting + forwarding exactly one tap.
      if (this.dependencies.isNativeStreamerInputOwned() && !isEscapeEvent) {
        return;
      }

      this.syncLockKeysState(event);

      const mapped = mapKeyboardEvent(event, this.dependencies.getKeyboardLayout()) ?? (isEscapeEvent ? codeMap.Escape : null);

      // Keep browser from handling held keys (for example Tab focus traversal)
      // while streaming input is active.
      if (event.repeat) {
        if (isPointerLockActive() || mapped) {
          event.preventDefault();
        }
        return;
      }

      if (isPointerLockActive()) {
        event.preventDefault();
      }

      if (!mapped) {
        return;
      }

      if (this.pressedKeys.has(mapped.vk)) {
        event.preventDefault();
        return;
      }

      event.preventDefault();
      this.pressedKeys.add(mapped.vk);

      const eventTimestampUs = timestampUs(event.timeStamp);

      const payload = this.dependencies.inputEncoder.encodeKeyDown({
        keycode: mapped.vk,
        scancode: mapped.scancode,
        modifiers: modifierFlags(event),
        timestampUs: eventTimestampUs,
      });
      this.dependencies.sendReliableSingleInput(payload);
    };

    const onKeyUp = (event: KeyboardEvent) => {
      if (this.dependencies.isInputBlocked()) return;
      if (!this.dependencies.isInputReady()) {
        return;
      }

      const isEscapeEvent =
        event.key === "Escape"
        || event.key === "Esc"
        || event.code === "Escape"
        || event.keyCode === 27;

      // Same gate as onKeyDown: native streamer owns keys except Escape.
      if (this.dependencies.isNativeStreamerInputOwned() && !isEscapeEvent) {
        return;
      }

      this.syncLockKeysState(event);

      const isCapsLockToggle = event.code === "CapsLock";
      const mapped = mapKeyboardEvent(event, this.dependencies.getKeyboardLayout()) ?? (isEscapeEvent ? codeMap.Escape : null);
      if (!mapped && !isCapsLockToggle) {
        return;
      }

      event.preventDefault();
      const eventTimestampUs = timestampUs(event.timeStamp);
      const modifiers = modifierFlags(event);

      if (isCapsLockToggle) {
        // Official GFN gg(): CapsLock keyup sends synthetic keydown then keyup (vk 160).
        if (mapped && this.pressedKeys.has(mapped.vk)) {
          this.pressedKeys.delete(mapped.vk);
          this.dependencies.sendReliableSingleInput(this.dependencies.inputEncoder.encodeKeyUp({
            keycode: mapped.vk,
            scancode: mapped.scancode,
            modifiers,
            timestampUs: eventTimestampUs,
          }));
        }

        const capsVk = 0xa0;
        this.dependencies.sendReliableSingleInput(this.dependencies.inputEncoder.encodeKeyDown({
          keycode: capsVk,
          scancode: 0,
          modifiers,
          timestampUs: eventTimestampUs,
        }));
        this.pressedKeys.delete(capsVk);
        this.dependencies.sendReliableSingleInput(this.dependencies.inputEncoder.encodeKeyUp({
          keycode: capsVk,
          scancode: 0,
          modifiers,
          timestampUs: eventTimestampUs,
        }));
        return;
      }

      if (!mapped || !this.pressedKeys.has(mapped.vk)) {
        return;
      }

      event.preventDefault();
      this.pressedKeys.delete(mapped.vk);
      this.dependencies.sendReliableSingleInput(this.dependencies.inputEncoder.encodeKeyUp({
        keycode: mapped.vk,
        scancode: mapped.scancode,
        modifiers,
        timestampUs: eventTimestampUs,
      }));
    };

    const onMouseDown = (event: MouseEvent) => {
      if (this.dependencies.isInputBlocked()) return;
      if (!this.dependencies.isInputReady()) {
        return;
      }
      if (!isPointerLockActive()) {
        return;
      }
      event.preventDefault();
      const payload = this.dependencies.inputEncoder.encodeMouseButtonDown({
        button: toMouseButton(event.button),
        timestampUs: timestampUs(event.timeStamp),
      });
      // Official GFN client sends all mouse events on reliable channel (input_channel_v1)
      this.dependencies.sendReliableSingleInput(payload);
    };

    const onMouseUp = (event: MouseEvent) => {
      if (this.dependencies.isInputBlocked()) return;
      if (!this.dependencies.isInputReady()) {
        return;
      }
      if (!isPointerLockActive()) {
        return;
      }
      event.preventDefault();
      const payload = this.dependencies.inputEncoder.encodeMouseButtonUp({
        button: toMouseButton(event.button),
        timestampUs: timestampUs(event.timeStamp),
      });
      // Official GFN client sends all mouse events on reliable channel (input_channel_v1)
      this.dependencies.sendReliableSingleInput(payload);
    };

    const onWheel = (event: WheelEvent) => {
      if (this.dependencies.isInputBlocked()) return;
      if (!this.dependencies.isInputReady()) {
        return;
      }
      if (!isPointerLockActive()) {
        return;
      }
      event.preventDefault();
      // Official GFN client sends negated raw deltaY as int16 (no quantization to ±120).
      // Clamp to int16 range since browser deltaY can exceed it with fast scrolling.
      const delta = Math.max(-32768, Math.min(32767, Math.round(-event.deltaY)));
      const payload = this.dependencies.inputEncoder.encodeMouseWheel({
        delta,
        timestampUs: timestampUs(event.timeStamp),
      });
      this.dependencies.sendReliableSingleInput(payload);
    };

    const onClick = () => {
      focusPointerLockTarget();
      void this.requestPointerLockWithOptionalFullscreen(pointerLockTarget, this.dependencies.shouldAutoFullscreen()).catch(
        (err: DOMException) => {
          this.dependencies.log(`Pointer lock request failed: ${err.name}: ${err.message}`);
        },
      );
      videoElement.focus();
    };

    // Auto-lock on mouse enter or first move over video without requiring a click
    const onVideoAreaHover = () => {
      if (!isPointerLockActive() && this.dependencies.isInputReady()) {
        tryAutoLock();
      }
    };
    pointerLockTarget.addEventListener("mouseenter", onVideoAreaHover);

    const schedulePointerLockRetention = (reason: string): void => {
      if (this.pointerLockRelockTimer !== null) {
        return;
      }

      this.pointerLockRelockTimer = window.setTimeout(() => {
        this.pointerLockRelockTimer = null;

        // Never re-lock while the native streamer owns RawInput capture on the
        // sink window — re-acquiring here is what made the two paths fight
        // (arm → lock-loss → re-lock → overlap → double mouse speed).
        if (
          !this.dependencies.isInputReady()
          || !this.shouldSendSyntheticEscapeOnPointerLockLoss()
          || isPointerLockActive()
          || this.dependencies.isNativeStreamerInputOwned()
        ) {
          return;
        }

        const target = this.pointerLockTarget;
        if (!target) {
          return;
        }

        void this.requestPointerLockWithOptionalFullscreen(target, false)
          .then(() => {
            this.dependencies.log(`Pointer lock restored after ${reason}`);
          })
          .catch((error: unknown) => {
            this.dependencies.log(`Pointer lock restore failed after ${reason}: ${String(error)}`);
          });
      }, 75);
    };

    // Store lock target for pointer lock re-acquisition
    this.pointerLockTarget = pointerLockTarget;

    // Handle pointer lock changes — send synthetic Escape when lock is lost by browser
    // (matches official GFN client's "pointerLockEscape" feature)
    const onPointerLockChange = () => {
      if (isPointerLockActive()) {
        this.cursorOverlay?.setPointerLocked(true);
        // Pointer lock gained — cancel any pending synthetic Escape.
        // Reset absolute position tracking since we switch to relative movement.
        lastAbsX = null;
        lastAbsY = null;
        if (this.pointerLockEscapeTimer !== null) {
          window.clearTimeout(this.pointerLockEscapeTimer);
          this.pointerLockEscapeTimer = null;
        }
        if (this.pointerLockRelockTimer !== null) {
          window.clearTimeout(this.pointerLockRelockTimer);
          this.pointerLockRelockTimer = null;
        }
        this.clearSyntheticEscapeSuppression();
        // Try to acquire keyboard lock for low-level key capture (best-effort).
        try {
          this.requestEscapeKeyboardLock();
        } catch {}

        // Notify main process that pointer lock is active so native-level
        // interception (before-input-event) can act accordingly.
        try {
          (window as any).openNow?.notifyPointerLockChange?.(true);
        } catch {}
        return;
      }

      const suppressEscapeFullscreenGrace = this.suppressNextSyntheticEscape;
      this.cursorOverlay?.setPointerLocked(false);

      // Pointer lock was lost — reset mirror state so tracking resumes from the
      // current cursor position rather than from a stale last-known position.
      lastAbsX = null;
      lastAbsY = null;

      try {
        (window as any).openNow?.notifyPointerLockChange?.(false, suppressEscapeFullscreenGrace);
      } catch {}

      // Pointer lock was lost
      if (!this.dependencies.isInputReady()) return;

      // The native streamer owns RawInput capture on the stacked sink window:
      // the lock was dropped because the sink took over (not by the user). Do
      // not synthesize Escape and do not re-lock — the sink feeds the game
      // directly and the retention timer is gated on the same flag.
      if (this.dependencies.isNativeStreamerInputOwned()) {
        return;
      }

      if (this.consumeSyntheticEscapeSuppression()) {
        this.releasePressedKeys("pointer lock intentionally released");
        return;
      }

      if (!this.shouldSendSyntheticEscapeOnPointerLockLoss()) {
        this.releasePressedKeys("pointer lock lost while unfocused");
        return;
      }

      // VK 0x1B = 27 = Escape
      const escapeWasPressed = this.pressedKeys.has(0x1B);

      if (escapeWasPressed) {
        // Escape was already tracked as pressed — the normal keyup handler will fire
        // and send Escape keyup to the server. No synthetic needed, but Chromium
        // still released pointer lock, so restore it after keyup has a chance to run.
        schedulePointerLockRetention("tracked Escape");
        return;
      }

      // Escape was NOT tracked as pressed — browser intercepted it before our keydown fired.
      // Send synthetic Escape keydown+keyup after 50ms (matches official GFN client).
      // Also re-acquire pointer lock so the user stays in the game.
      this.pointerLockEscapeTimer = window.setTimeout(() => {
        this.pointerLockEscapeTimer = null;

        if (!this.dependencies.isInputReady()) return;

        if (!this.shouldSendSyntheticEscapeOnPointerLockLoss()) {
          this.releasePressedKeys("focus changed before synthetic Escape");
          return;
        }

        // Release all currently held keys first (matching official client's MS() function)
        this.releasePressedKeys("pointer lock lost before synthetic Escape");

        // Send synthetic Escape keydown + keyup
        this.dependencies.log("Sending synthetic Escape (pointer lock lost by browser)");
        const escDown = this.dependencies.inputEncoder.encodeKeyDown({
          keycode: 0x1B,
          scancode: codeMap.Escape.scancode,
          modifiers: 0,
          timestampUs: timestampUs(),
        });
        this.dependencies.sendReliableSingleInput(escDown);

        const escUp = this.dependencies.inputEncoder.encodeKeyUp({
          keycode: 0x1B,
          scancode: codeMap.Escape.scancode,
          modifiers: 0,
          timestampUs: timestampUs(),
        });
        this.dependencies.sendReliableSingleInput(escUp);

        schedulePointerLockRetention("synthetic Escape");
      }, 50);
    };

    const onWindowBlur = () => {
      // Don't release keys during microphone permission request
      // as getUserMedia() may cause brief window focus loss
      if (this.dependencies.getMicState() === "permission_pending") {
        this.dependencies.log("Window blur during mic permission - keeping keys pressed");
        return;
      }
      mouseInStreamView = false;
      lastAbsX = null;
      lastAbsY = null;
      this.releasePressedKeys("window blur");
      // Free the cursor for the app the user alt-tabbed to: Chromium on
      // Windows keeps pointer lock across focus loss, so without this the
      // mouse stays hidden and locked to the stream while working elsewhere.
      // Re-acquisition on return is handled by onWindowFocus -> tryAutoLock.
      if (document.pointerLockElement) {
        document.exitPointerLock();
      }
      // Pause forwarding while window is not focused (host overlay pause is separate).
      // In native mode the renderer sink can be a separate no-activate window,
      // so a focus transition is not enough reason to stop controller polling.
      // Native RawInput capture (this.isNativeMouseActive()) is an exception:
      // RIDEV_INPUTSINK keeps delivering events even while unfocused, so without
      // pausing here the game would receive mouse movement while the user
      // alt-tabs away.
      if (!this.dependencies.isNativeInputActive() || this.isNativeMouseActive()) {
        this.dependencies.setWindowInputPaused(true);
      }
    };

    const onVisibilityChange = () => {
      if (document.visibilityState !== "visible") {
        this.releasePressedKeys(`visibility ${document.visibilityState}`);
        this.dependencies.setWindowInputPaused(true);
        return;
      }

      this.dependencies.setWindowInputPaused(false);
    };

    const onWindowFocus = () => {
      this.dependencies.setWindowInputPaused(false);
      mouseInStreamView = true;
      lastAbsX = null;
      lastAbsY = null;
      focusPointerLockTarget();
      void this.dependencies.refreshClipboardAvailability();
      // Auto-lock: acquire pointer lock when the user switches back to the app.
      // Defer briefly so a still-in-flight focus/fullscreen transition (the
      // tail end of an alt-tab) cannot reject the request (WrongDocumentError).
      window.setTimeout(() => {
        if (typeof document.hasFocus === "function" && !document.hasFocus()) {
          return;
        }
        tryAutoLock();
      }, 150);
    };

    // Re-assert the keyboard lock on entering fullscreen. When leaving fullscreen,
    // keep the lock if the pointer is still locked (windowed play still needs
    // Escape held); only release it once we no longer own the input.
    const onFullscreenChange = () => {
      if (document.fullscreenElement) {
        this.requestEscapeKeyboardLock();
        return;
      }
      const pointerLocked =
        document.pointerLockElement === this.pointerLockTarget
        || document.pointerLockElement === this.dependencies.videoElement;
      if (pointerLocked) {
        // Still capturing input in a window — keep Escape locked.
        this.requestEscapeKeyboardLock();
        return;
      }
      const nav = navigator as any;
      if (nav.keyboard?.unlock) {
        try {
          nav.keyboard.unlock();
          this.keyboardLockState = "unknown";
        } catch {
          /* no-op */
        }
      }
    };

    // Add gamepad event listeners
    window.addEventListener("gamepadconnected", this.dependencies.onGamepadConnected);
    window.addEventListener("gamepaddisconnected", this.dependencies.onGamepadDisconnected);

    document.addEventListener("keydown", onKeyDown, true);
    document.addEventListener("keyup", onKeyUp, true);
    if (pointerMoveEventName) {
      document.addEventListener(pointerMoveEventName, onPointerMove as EventListener);
    } else {
      window.addEventListener("mousemove", onMouseMove);
    }
    // Use document capture for buttons/wheel in native internal mode so clicks
    // still reach us even if the native child HWND is topmost for a frame.
    const buttonTarget: HTMLElement | Document = this.dependencies.isNativeElectronInputBridge()
      ? document
      : pointerLockTarget;
    const buttonCapture = this.dependencies.isNativeElectronInputBridge();
    buttonTarget.addEventListener("mousedown", onMouseDown as EventListener, buttonCapture);
    buttonTarget.addEventListener("mouseup", onMouseUp as EventListener, buttonCapture);
    buttonTarget.addEventListener("wheel", onWheel as EventListener, {
      passive: false,
      capture: buttonCapture,
    } as AddEventListenerOptions);
    pointerLockTarget.addEventListener("mouseenter", onPointerLockTargetMouseEnter);
    pointerLockTarget.addEventListener("mouseleave", onPointerLockTargetMouseLeave);
    // Detect when the mouse enters the application window (from outside the
    // browsing context) and trigger auto pointer lock. We listen to
    // `pointerover` when PointerEvents are available and fall back to
    // `mouseover` for older environments. If `relatedTarget` is null or not
    // part of this document, the pointer came from outside the window. Only
    // attempt auto-lock when the pointer is actually over the stream viewport
    // (pointerLockTarget) to avoid accidental locks when the cursor enters
    // over chrome/UI areas.
    const onDocumentPointerEnterWindow = (ev: PointerEvent | MouseEvent) => {
      // Only care about physical mouse pointers
      if (typeof PointerEvent !== "undefined" && ev instanceof PointerEvent) {
        if (ev.pointerType && ev.pointerType !== "mouse") return;
      }

      const related = (ev as any).relatedTarget as Node | null | undefined;
      if (related && document.contains(related)) {
        // relatedTarget is still within this document — this is an intra-document
        // move, not an entry from outside the window.
        return;
      }

      // Only trigger auto-lock if the pointer is actually over the stream
      // viewport (pointerLockTarget). This prevents accidental locks when the
      // cursor enters the window over chrome/UI areas.
      const rect = pointerLockTarget.getBoundingClientRect();
      const clientX = (ev as MouseEvent).clientX;
      const clientY = (ev as MouseEvent).clientY;
      if (!Number.isFinite(clientX) || !Number.isFinite(clientY)) {
        return;
      }

      if (clientX < rect.left || clientX > rect.right || clientY < rect.top || clientY > rect.bottom) {
        return;
      }

      // Treat this as entering the stream/window area for auto-lock purposes
      mouseInStreamView = true;
      // Save entry absolute coords so tryAutoLock can align the server cursor
      // before requesting pointer lock.
      pendingEntryAbsX = clientX - rect.left;
      pendingEntryAbsY = clientY - rect.top;
      lastAbsX = null;
      lastAbsY = null;
      tryAutoLock();
    };

    // Fallback: some environments may not produce pointerover relatedTarget=null
    // when entering the native window. Listen for the first mousemove while we
    // believe the pointer is outside the window and treat that as an entry.
    const onFirstMouseMoveIntoWindow = (ev: MouseEvent | PointerEvent) => {
      if (mouseInStreamView) return;
      if (typeof PointerEvent !== "undefined" && ev instanceof PointerEvent) {
        if (ev.pointerType && ev.pointerType !== "mouse") return;
      }

      // Only consider it an entry if the cursor is over the stream viewport
      const rect = pointerLockTarget.getBoundingClientRect();
      const clientX = (ev as MouseEvent).clientX;
      const clientY = (ev as MouseEvent).clientY;
      if (!Number.isFinite(clientX) || !Number.isFinite(clientY)) return;
      if (clientX < rect.left || clientX > rect.right || clientY < rect.top || clientY > rect.bottom) return;

      mouseInStreamView = true;
      lastAbsX = null;
      lastAbsY = null;
      tryAutoLock();
      // remove this listener after first use
      document.removeEventListener("mousemove", onFirstMouseMoveIntoWindow as EventListener, true);
      if (typeof PointerEvent !== "undefined") {
        document.removeEventListener("pointermove", onFirstMouseMoveIntoWindow as EventListener, true);
      }
    };
    videoElement.addEventListener("click", onClick);
    if (typeof PointerEvent !== "undefined") {
      document.addEventListener("pointerover", onDocumentPointerEnterWindow, true);
      document.addEventListener("pointermove", onFirstMouseMoveIntoWindow as EventListener, true);
    } else {
      document.addEventListener("mouseover", onDocumentPointerEnterWindow, true);
      document.addEventListener("mousemove", onFirstMouseMoveIntoWindow as EventListener, true);
    }
    focusPointerLockTarget();
    document.addEventListener("pointerlockchange", onPointerLockChange);
    document.addEventListener("fullscreenchange", onFullscreenChange);
    window.addEventListener("blur", onWindowBlur);
    document.addEventListener("visibilitychange", onVisibilityChange);
    window.addEventListener("focus", onWindowFocus);

    this.inputCleanup.push(() => window.removeEventListener("gamepadconnected", this.dependencies.onGamepadConnected));
    this.inputCleanup.push(() => window.removeEventListener("gamepaddisconnected", this.dependencies.onGamepadDisconnected));
    this.inputCleanup.push(() => document.removeEventListener("keydown", onKeyDown, true));
    this.inputCleanup.push(() => document.removeEventListener("keyup", onKeyUp, true));
    if (pointerMoveEventName) {
      this.inputCleanup.push(() => document.removeEventListener(pointerMoveEventName, onPointerMove as EventListener));
    } else {
      this.inputCleanup.push(() => window.removeEventListener("mousemove", onMouseMove));
    }
    this.inputCleanup.push(() => {
      buttonTarget.removeEventListener("mousedown", onMouseDown as EventListener, buttonCapture);
      buttonTarget.removeEventListener("mouseup", onMouseUp as EventListener, buttonCapture);
      buttonTarget.removeEventListener("wheel", onWheel as EventListener, {
        capture: buttonCapture,
      } as EventListenerOptions);
    });
    this.inputCleanup.push(() => pointerLockTarget.removeEventListener("mouseenter", onPointerLockTargetMouseEnter));
    this.inputCleanup.push(() => pointerLockTarget.removeEventListener("mouseleave", onPointerLockTargetMouseLeave));
    if (typeof PointerEvent !== "undefined") {
      this.inputCleanup.push(() => document.removeEventListener("pointerover", onDocumentPointerEnterWindow, true));
      this.inputCleanup.push(() => document.removeEventListener("pointermove", onFirstMouseMoveIntoWindow as EventListener, true));
    } else {
      this.inputCleanup.push(() => document.removeEventListener("mouseover", onDocumentPointerEnterWindow, true));
      this.inputCleanup.push(() => document.removeEventListener("mousemove", onFirstMouseMoveIntoWindow as EventListener, true));
    }
    this.inputCleanup.push(() => videoElement.removeEventListener("click", onClick));
    this.inputCleanup.push(() => {
      if (originalPointerLockTargetTabIndex === null) {
        pointerLockTarget.removeAttribute("tabindex");
      } else {
        pointerLockTarget.setAttribute("tabindex", originalPointerLockTargetTabIndex);
      }
    });
    this.inputCleanup.push(() => document.removeEventListener("pointerlockchange", onPointerLockChange));
    this.inputCleanup.push(() => document.removeEventListener("fullscreenchange", onFullscreenChange));
    this.inputCleanup.push(() => window.removeEventListener("blur", onWindowBlur));
    this.inputCleanup.push(() => document.removeEventListener("visibilitychange", onVisibilityChange));
    this.inputCleanup.push(() => window.removeEventListener("focus", onWindowFocus));
    this.inputCleanup.push(() => {
      if (this.pointerLockEscapeTimer !== null) {
        window.clearTimeout(this.pointerLockEscapeTimer);
        this.pointerLockEscapeTimer = null;
      }
      if (this.pointerLockRelockTimer !== null) {
        window.clearTimeout(this.pointerLockRelockTimer);
        this.pointerLockRelockTimer = null;
      }
      this.clearSyntheticEscapeSuppression();
      this.releasePressedKeys("input cleanup");
      this.pendingMouseDxFloat = 0;
      this.pendingMouseDyFloat = 0;
      this.pendingMouseAbs = null;
      this.pendingMouseTimestampUs = null;
      this.mouseDeltaFilter.reset();
      this.pointerLockTarget = null;
      // Unlock keyboard on cleanup
      const nav = navigator as any;
      if (nav.keyboard?.unlock) {
        nav.keyboard.unlock();
      }
    });
  }

  /**
   * Query browser for supported video codecs via RTCRtpReceiver.getCapabilities.
   * Returns normalized names like "H264", "H265", "AV1", "VP9", "VP8".
   */
}
