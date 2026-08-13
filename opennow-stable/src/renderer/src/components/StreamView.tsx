import { useState, useEffect, useCallback, useRef } from "react";
import { createPortal } from "react-dom";
import { AnimatePresence } from "motion/react";
import type { JSX } from "react";
import { Maximize, Minimize, LogOut, AlertTriangle } from "lucide-react";
import { SessionStartedSplash } from "./SessionStartedSplash";
import { StreamStatsHud } from "./StreamStatsHud";
import type { StreamDiagnosticsStore } from "../utils/streamDiagnosticsStore";
import { useStreamDiagnosticsSelector } from "../utils/streamDiagnosticsStore";
import { getStoreDisplayName, getStoreIconComponent } from "./GameCard";
import { SessionElapsedIndicator } from "./ElapsedSessionIndicators";
import type { MicrophoneMode, StatsOverlayPosition, SubscriptionInfo, VideoShaderSettings } from "@shared/gfn";
import type { StatsOverlayMode } from "../lib/appTypes";
import { VideoShaderPipeline } from "../platforms/gfn/videoShaderPipeline";
import { formatShortcutForDisplay } from "../shortcuts";
import { useScreenshotGallery } from "../hooks/useScreenshotGallery";
import { useStreamMenuNavigation } from "../hooks/useStreamMenuNavigation";
import { useStreamRecorder } from "../hooks/useStreamRecorder";
import { formatSessionTimeRemaining, formatWarningSeconds } from "./stream/streamFormatters";
import { AntiAfkIndicator, MicrophoneIndicator, ProcessingIndicator, RecordingIndicator } from "./stream/StreamIndicators";
import { StreamTitleBar } from "./stream/StreamTitleBar";
import {
  hasVisibleStreamVideo,
  StreamEmptyState,
  StreamWaitingForVideo,
  VideoFocusOnReady,
} from "./stream/StreamEmptyStates";
import { StreamQuickMenu } from "./stream/quick-menu/StreamQuickMenu";
import { MotionSpinner } from "./MotionSpinner";

const ANTI_AFK_TOGGLE_ACK_MS = 5000;
const CONTROLLER_SIDEBAR_SHORTCUT_DISPLAY = "View + Menu";

interface StreamViewProps {
  videoRef: React.Ref<HTMLVideoElement>;
  audioRef: React.Ref<HTMLAudioElement>;
  diagnosticsStore: StreamDiagnosticsStore;
  statsMode: StatsOverlayMode;
  statsPosition: StatsOverlayPosition;
  showNativeStats?: boolean;
  nativeInputCaptureActive?: boolean;
  gstreamerEnabled: boolean;
  nativeExternalRenderer?: boolean;
  shortcuts: {
    toggleStats: string;
    togglePointerLock: string;
    toggleFullscreen: string;
    stopStream: string;
    toggleAntiAfk: string;
    toggleMicrophone?: string;
    screenshot: string;
    recording: string;
  };
  hideStreamButtons?: boolean;
  serverRegion?: string;
  userSelectedRegionName?: string;
  antiAfkEnabled: boolean;
  antiAfkAckNonce: number;
  showAntiAfkIndicator: boolean;
  exitPrompt: {
    open: boolean;
    gameTitle: string;
  };
  sessionStartedAtMs: number | null;
  isStreaming: boolean;
  sessionCounterEnabled: boolean;
  showSessionTimeRemainingInStatsOverlay: boolean;
  sessionTimeRemainingSeconds: number | null;
  sessionClockShowEveryMinutes: number;
  sessionClockShowDurationSeconds: number;
  streamWarning: {
    code: 1 | 2 | 3;
    message: string;
    tone: "warn" | "critical";
    secondsLeft?: number;
  } | null;
  isFullscreen: boolean;
  isConnecting: boolean;
  streamRevealComplete: boolean;
  gameTitle: string;
  recordingBitrateMbps: number | null;
  recordingResolution: string;
  recordingFps: number;
  onRecordingResolutionChange: (value: string) => void;
  onRecordingFpsChange: (value: number) => void;
  onRecordingBitrateMbpsChange: (value: number | null) => void;
  platformStore?: string;
  onToggleFullscreen: () => void;
  onConfirmExit: () => void;
  onCancelExit: () => void;
  onEndSession: () => void;
  onToggleMicrophone?: () => void;
  mouseSensitivity: number;
  onMouseSensitivityChange: (value: number) => void;
  mouseAcceleration: number;
  onMouseAccelerationChange: (value: number) => void;
  maxBitrateMbps: number;
  onMaxBitrateMbpsChange: (value: number) => void;
  onRequestPointerLock?: () => void;
  onReleasePointerLock?: () => void;
  onNativeInputPaused?: (paused: boolean) => void;
  microphoneMode: MicrophoneMode;
  onMicrophoneModeChange: (value: MicrophoneMode) => void;
  onScreenshotShortcutChange: (value: string) => void;
  onRecordingShortcutChange: (value: string) => void;
  onShowSessionTimeRemainingInStatsOverlayChange: (value: boolean) => void;
  onStatsPositionChange: (value: StatsOverlayPosition) => void;
  subscriptionInfo: SubscriptionInfo | null;
  micTrack?: MediaStreamTrack | null;
  className?: string;
  allowEscapeToExitFullscreen?: boolean;
  videoShader: VideoShaderSettings;
  onVideoShaderChange: (value: VideoShaderSettings) => void;
}

export function StreamView({
  videoRef,
  audioRef,
  diagnosticsStore,
  statsMode,
  statsPosition,
  showNativeStats = false,
  nativeInputCaptureActive = false,
  gstreamerEnabled,
  nativeExternalRenderer = false,
  shortcuts,
  serverRegion,
  userSelectedRegionName,
  antiAfkEnabled,
  antiAfkAckNonce,
  showAntiAfkIndicator,
  exitPrompt,
  sessionStartedAtMs,
  isStreaming,
  sessionCounterEnabled,
  showSessionTimeRemainingInStatsOverlay,
  sessionTimeRemainingSeconds,
  sessionClockShowEveryMinutes,
  sessionClockShowDurationSeconds,
  streamWarning,
  isFullscreen,
  isConnecting,
  streamRevealComplete,
  gameTitle,
  recordingBitrateMbps,
  recordingResolution,
  recordingFps,
  onRecordingResolutionChange,
  onRecordingFpsChange,
  onRecordingBitrateMbpsChange,
  platformStore,
  onToggleFullscreen,
  onConfirmExit,
  onCancelExit,
  onEndSession,
  onToggleMicrophone,
  mouseSensitivity,
  onMouseSensitivityChange,
  mouseAcceleration,
  onMouseAccelerationChange,
  maxBitrateMbps,
  onMaxBitrateMbpsChange,
  onRequestPointerLock,
  onReleasePointerLock,
  onNativeInputPaused,
  microphoneMode,
  onMicrophoneModeChange,
  onScreenshotShortcutChange,
  onRecordingShortcutChange,
  onShowSessionTimeRemainingInStatsOverlayChange,
  onStatsPositionChange,
  subscriptionInfo,
  micTrack,
  hideStreamButtons = false,
  allowEscapeToExitFullscreen,
  className,
  videoShader,
  onVideoShaderChange,
}: StreamViewProps): JSX.Element {
  const [showHints, setShowHints] = useState(true);
  const [showSessionClock, setShowSessionClock] = useState(false);
  const [antiAfkToggleAck, setAntiAfkToggleAck] = useState<"on" | "off" | null>(null);
  const [isPointerLocked, setIsPointerLocked] = useState(false);
  const [pointerLockHintVisible, setPointerLockHintVisible] = useState(false);
  const pointerLockHintTimerRef = useRef<number | null>(null);
  const nativeRendererActive = useStreamDiagnosticsSelector(
    diagnosticsStore,
    (stats) => stats.nativeRendererActive,
  );
  const nativeStackedRenderer = useStreamDiagnosticsSelector(
    diagnosticsStore,
    (stats) => stats.nativeStackedRenderer,
  );
  const localVideoRef = useRef<HTMLVideoElement | null>(null);
  const localAudioRef = useRef<HTMLAudioElement | null>(null);
  const shaderPipelineRef = useRef<VideoShaderPipeline | null>(null);
  const streamHasVideo = useStreamDiagnosticsSelector(
    diagnosticsStore,
    (stats) => hasVisibleStreamVideo(stats),
  );
  const [videoElementHasFrame, setVideoElementHasFrame] = useState(false);

  useEffect(() => {
    if (isConnecting) {
      setVideoElementHasFrame(false);
      return undefined;
    }

    const video = localVideoRef.current;
    if (!video) {
      return undefined;
    }

    const syncVideoFrame = (): void => {
      setVideoElementHasFrame(video.videoWidth > 0 && video.videoHeight > 0);
    };

    syncVideoFrame();
    video.addEventListener("loadeddata", syncVideoFrame);
    video.addEventListener("playing", syncVideoFrame);
    video.addEventListener("resize", syncVideoFrame);

    return () => {
      video.removeEventListener("loadeddata", syncVideoFrame);
      video.removeEventListener("playing", syncVideoFrame);
      video.removeEventListener("resize", syncVideoFrame);
    };
  }, [isConnecting]);

  const streamVideoReady = streamHasVideo || videoElementHasFrame;
  const [sessionReadySplashVisible, setSessionReadySplashVisible] = useState(false);
  const sessionReadySplashShownRef = useRef(false);
  // Stacked mode keeps the video in a native window behind the transparent
  // shell, so DOM overlays (stats HUD etc.) must stay visible above it.
  const showStatsHud = statsMode !== "off" && (!nativeRendererActive || nativeStackedRenderer) && !isConnecting;

  useEffect(() => {
    if (isConnecting) {
      sessionReadySplashShownRef.current = false;
      setSessionReadySplashVisible(false);
      return;
    }
    if (
      nativeRendererActive
      || !streamVideoReady
      || !streamRevealComplete
      || sessionReadySplashShownRef.current
    ) {
      return;
    }
    sessionReadySplashShownRef.current = true;
    setSessionReadySplashVisible(true);
  }, [isConnecting, nativeRendererActive, streamRevealComplete, streamVideoReady]);

  const handleSessionReadySplashFinished = useCallback(() => {
    setSessionReadySplashVisible(false);
  }, []);

  const handleFullscreenToggle = useCallback(() => {
    onToggleFullscreen();
  }, [onToggleFullscreen]);

  const handlePointerLockToggle = useCallback(() => {
    if (isPointerLocked) {
      if (onReleasePointerLock) {
        onReleasePointerLock();
        return;
      }
      document.exitPointerLock();
      return;
    }
    if (onRequestPointerLock) {
      onRequestPointerLock();
    }
  }, [isPointerLocked, onReleasePointerLock, onRequestPointerLock]);

  useEffect(() => {
    const timer = setTimeout(() => setShowHints(false), 5000);
    return () => clearTimeout(timer);
  }, []);

  useEffect(() => {
    if (!sessionCounterEnabled) {
      setShowSessionClock(false);
      return;
    }

    if (isConnecting) {
      setShowSessionClock(false);
      return;
    }

    const intervalMinutes = Math.max(0, Math.floor(sessionClockShowEveryMinutes || 0));
    const durationSeconds = Math.max(1, Math.floor(sessionClockShowDurationSeconds || 1));
    const intervalMs = intervalMinutes * 60 * 1000;
    const durationMs = durationSeconds * 1000;

    let hideTimer: number | undefined;
    let periodicTimer: number | undefined;

    const showFor = (durationMs: number): void => {
      setShowSessionClock(true);
      if (hideTimer !== undefined) {
        window.clearTimeout(hideTimer);
      }
      hideTimer = window.setTimeout(() => {
        setShowSessionClock(false);
      }, durationMs);
    };

    // Show session clock at stream start.
    showFor(durationMs);

    if (intervalMs > 0) {
      periodicTimer = window.setInterval(() => {
        showFor(durationMs);
      }, intervalMs);
    }

    return () => {
      if (hideTimer !== undefined) {
        window.clearTimeout(hideTimer);
      }
      if (periodicTimer !== undefined) {
        window.clearInterval(periodicTimer);
      }
    };
  }, [isConnecting, sessionClockShowDurationSeconds, sessionClockShowEveryMinutes, sessionCounterEnabled]);

  useEffect(() => {
    if (antiAfkAckNonce === 0 || isConnecting) {
      setAntiAfkToggleAck(null);
      return;
    }

    // Omit transient "on" message when persistent ANTI-AFK badge already shows it
    if (antiAfkEnabled && showAntiAfkIndicator) {
      setAntiAfkToggleAck(null);
      return;
    }

    setAntiAfkToggleAck(antiAfkEnabled ? "on" : "off");

    const hideTimer = window.setTimeout(() => {
      setAntiAfkToggleAck(null);
    }, ANTI_AFK_TOGGLE_ACK_MS);

    return (): void => {
      window.clearTimeout(hideTimer);
    };
  }, [antiAfkAckNonce, antiAfkEnabled, showAntiAfkIndicator, isConnecting]);

  const warningSeconds = formatWarningSeconds(streamWarning?.secondsLeft);
  const sessionTimeRemainingText = formatSessionTimeRemaining(sessionTimeRemainingSeconds);
  const showSessionTimeRemainingInStats =
    sessionTimeRemainingText !== null && showSessionTimeRemainingInStatsOverlay;
  const platformName = platformStore ? getStoreDisplayName(platformStore) : "";
  const PlatformIcon = platformStore ? getStoreIconComponent(platformStore) : null;
  const isMacClient = navigator.platform?.toLowerCase().includes("mac") || navigator.userAgent.includes("Macintosh");
  const sidebarToggleRaw = isMacClient ? "Meta+G" : "Ctrl+G";
  const sidebarToggleShortcutDisplay = formatShortcutForDisplay(sidebarToggleRaw, isMacClient);

  const screenshotGallery = useScreenshotGallery({
    videoRef: localVideoRef,
    gameTitle,
    // Native mode renders video in the native sink window; the renderer's
    // <video> element has no frames, so capture from the native video chain.
    nativeCaptureScreenshot: nativeRendererActive
      ? () => window.openNow.captureNativeScreenshot({ gameTitle })
      : undefined,
  });
  const streamRecorder = useStreamRecorder({
    videoRef: localVideoRef,
    audioRef: localAudioRef,
    gameTitle,
    micTrack: micTrack ?? null,
    nativeRecordingEnabled: nativeRendererActive,
    recordingBitrateMbps,
    recordingResolution,
    recordingFps,
  });
  const releasePointerLockForMenu = useCallback(() => {
    if (document.pointerLockElement) {
      if (onReleasePointerLock) {
        onReleasePointerLock();
      } else {
        document.exitPointerLock();
      }
    }
  }, [onReleasePointerLock]);
  const {
    showSideBar,
    setShowSideBar,
    activeSidebarTab,
    setActiveSidebarTab,
    sidebarRef,
  } = useStreamMenuNavigation({
    shortcuts,
    isMacClient,
    exitPromptOpen: exitPrompt.open,
    selectedScreenshotId: screenshotGallery.selectedScreenshotId,
    setSelectedScreenshotId: screenshotGallery.setSelectedScreenshotId,
    captureScreenshot: screenshotGallery.captureScreenshot,
    toggleRecording: streamRecorder.toggleRecording,
    onCancelExit,
    onConfirmExit,
    onBeforeOpen: releasePointerLockForMenu,
  });
  // Latest overlay/state values read by the native surface publisher. The
  // publish effect below mounts once (empty deps) so its observers/listeners
  // are never recreated when the quick menu / exit prompt / stats visibility
  // toggles — recreating them used to tear down and re-publish the surface,
  // which made the native side hide + wipe the stacked sink's cached rect
  // (flicker + surface rect churn on overlay open).
  const surfaceStateRef = useRef({
    showSideBar,
    exitOpen: exitPrompt.open,
    statsMode,
    showNativeStats,
  });
  const publishSurfaceRef = useRef<(() => void) | null>(null);
  const suppressVideoFocusOnSidebarCloseRef = useRef(false);

  // Video shader post-processing pipeline (embedded WebRTC path only; the
  // native streamer renders outside Chromium so shaders cannot apply there).
  useEffect(() => {
    const video = localVideoRef.current;
    if (!video) return;
    const effective = gstreamerEnabled || nativeRendererActive
      ? { ...videoShader, enabled: false }
      : videoShader;
    if (!shaderPipelineRef.current) {
      if (!effective.enabled) return;
      shaderPipelineRef.current = new VideoShaderPipeline(video, effective, {
        // Also covers runtime deactivation (e.g. WebGL context loss), which
        // StreamView would otherwise never learn about.
        onActiveChange: (active) => {
          const current = diagnosticsStore.getSnapshot();
          if (current.shaderActive !== active) {
            diagnosticsStore.set({ ...current, shaderActive: active });
          }
        },
      });
    } else {
      shaderPipelineRef.current.updateSettings(effective);
    }
    // Mirror the pipeline's real activation (a visible effect is being applied)
    // into diagnostics so the HUD can surface the WebGL post-processing load.
    const active = shaderPipelineRef.current.isActive();
    const snapshot = diagnosticsStore.getSnapshot();
    if (snapshot.shaderActive !== active) {
      diagnosticsStore.set({ ...snapshot, shaderActive: active });
    }
  }, [videoShader, gstreamerEnabled, nativeRendererActive]);

  useEffect(() => () => {
    shaderPipelineRef.current?.dispose();
    shaderPipelineRef.current = null;
    const snapshot = diagnosticsStore.getSnapshot();
    if (snapshot.shaderActive) {
      diagnosticsStore.set({ ...snapshot, shaderActive: false });
    }
  }, []);

  const setVideoRef = useCallback((element: HTMLVideoElement | null) => {
    localVideoRef.current = element;
    if (typeof videoRef === "function") {
      videoRef(element);
    } else if (videoRef && "current" in videoRef) {
      (videoRef as React.MutableRefObject<HTMLVideoElement | null>).current = element;
    }
  }, [videoRef]);

  const setAudioRef = useCallback((element: HTMLAudioElement | null) => {
    localAudioRef.current = element;
    if (typeof audioRef === "function") {
      audioRef(element);
    } else if (audioRef && "current" in audioRef) {
      (audioRef as React.MutableRefObject<HTMLAudioElement | null>).current = element;
    }
  }, [audioRef]);

  useEffect(() => {
    const updateSurface = window.openNow?.updateNativeRenderSurface;
    if (typeof updateSurface !== "function") {
      return undefined;
    }

    let frame = 0;
    const publish = (): void => {
      const element = localVideoRef.current;
      const dpr = window.devicePixelRatio || 1;
      if (!element || document.visibilityState === "hidden") {
        updateSurface({ rect: null, visible: false, deviceScaleFactor: dpr });
        return;
      }

      const rect = element.getBoundingClientRect();
      const width = Math.round(rect.width * dpr);
      const height = Math.round(rect.height * dpr);
      // getBoundingClientRect() is VIEWPORT-relative (CSS px), but the native
      // stacked sink is placed in SCREEN coordinates (physical px). The
      // window's screenX/screenY (CSS px) converts it — without this, a
      // non-origin window (windowed mode, second monitor) puts the video
      // sink at (0,0) of the screen and crops the far corner = the
      // "slightly zoomed / cropped" native display report.
      const screenLeft = rect.left + (window.screenX || 0);
      const screenTop = rect.top + (window.screenY || 0);
      const { showSideBar, exitOpen, statsMode, showNativeStats } = surfaceStateRef.current;
      // In stacked mode the native video window lives behind the transparent
      // shell for the whole session, so it must stay visible while overlays
      // (sidebar, exit prompt) are open — they float above it. Only a hidden
      // document (minimized window) suppresses it, handled by the branch above.
      const stacked = diagnosticsStore.getSnapshot().nativeStackedRenderer;
      const visible = width >= 2 && height >= 2 && (stacked || (!showSideBar && !exitOpen));
      // The built-in GStreamer stats overlay (dwritetextoverlay) is a native
      // rendering, so only the dedicated "Show Native Streamer Stats" setting
      // controls it — the DOM HUD (statsMode) is drawn by React above the
      // video and must not force the native overlay on/off.
      updateSurface({
        deviceScaleFactor: dpr,
        visible,
        showStats: showNativeStats,
        rect: visible
          ? {
              x: Math.round(screenLeft * dpr),
              y: Math.round(screenTop * dpr),
              width,
              height,
            }
          : null,
      });
    };

    const schedule = (): void => {
      if (frame !== 0) {
        return;
      }
      frame = window.requestAnimationFrame(() => {
        frame = 0;
        publish();
      });
    };

    const observer = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(schedule);
    if (observer && localVideoRef.current) {
      observer.observe(localVideoRef.current);
    }

    window.addEventListener("resize", schedule);
    window.addEventListener("fullscreenchange", schedule);
    document.addEventListener("visibilitychange", schedule);
    window.visualViewport?.addEventListener("resize", schedule);
    window.visualViewport?.addEventListener("scroll", schedule);
    publishSurfaceRef.current = schedule;
    schedule();

    return () => {
      publishSurfaceRef.current = null;
      if (frame !== 0) {
        window.cancelAnimationFrame(frame);
      }
      observer?.disconnect();
      window.removeEventListener("resize", schedule);
      window.removeEventListener("fullscreenchange", schedule);
      document.removeEventListener("visibilitychange", schedule);
      window.visualViewport?.removeEventListener("resize", schedule);
      window.visualViewport?.removeEventListener("scroll", schedule);
      // Real unmount (session end): release the surface. Overlay/state changes
      // never reach here anymore — the trigger effect below re-publishes in
      // place, so the sink window is never hidden/re-created on menu open.
      updateSurface({
        rect: null,
        visible: false,
        deviceScaleFactor: window.devicePixelRatio || 1,
        showStats: false,
      });
    };
  }, []);

  // Overlay/state changed: refresh the refs the mount-once publish effect reads
  // and re-publish in place. No teardown, no null-surface publish — the native
  // side keeps the stacked sink visible and its cached rect intact, so the
  // dedup skips the SetWindowPos entirely. isStreaming is included so the
  // surface is re-published when video actually starts: the very first publish
  // can run before the native sink window exists (the launch flash where the
  // video sits at GStreamer's default position until some later event), and
  // re-publishing here makes the native side position the sink immediately.
  useEffect(() => {
    surfaceStateRef.current = {
      showSideBar,
      exitOpen: exitPrompt.open,
      statsMode,
      showNativeStats,
    };
    publishSurfaceRef.current?.();
  }, [exitPrompt.open, isStreaming, showNativeStats, showSideBar, statsMode]);

  useEffect(() => {
    const handlePointerLockChange = () => {
      setIsPointerLocked(
        document.pointerLockElement === localVideoRef.current || nativeInputCaptureActive,
      );
    };
    handlePointerLockChange();
    document.addEventListener("pointerlockchange", handlePointerLockChange);
    return () => document.removeEventListener("pointerlockchange", handlePointerLockChange);
  }, [nativeInputCaptureActive]);

  useEffect(() => {
    // Show a transient HUD hint when pointer lock is acquired
    if (isPointerLocked) {
      setPointerLockHintVisible(true);
      if (pointerLockHintTimerRef.current) {
        window.clearTimeout(pointerLockHintTimerRef.current);
      }
      pointerLockHintTimerRef.current = window.setTimeout(() => {
        pointerLockHintTimerRef.current = null;
        setPointerLockHintVisible(false);
      }, 3000);
    } else {
      if (pointerLockHintTimerRef.current) {
        window.clearTimeout(pointerLockHintTimerRef.current);
        pointerLockHintTimerRef.current = null;
      }
      setPointerLockHintVisible(false);
    }
    return () => {
      if (pointerLockHintTimerRef.current) {
        window.clearTimeout(pointerLockHintTimerRef.current);
        pointerLockHintTimerRef.current = null;
      }
    };
  }, [isPointerLocked]);

  useEffect(() => {
    onNativeInputPaused?.(showSideBar);
    return () => {
      if (showSideBar) {
        onNativeInputPaused?.(false);
      }
    };
  }, [onNativeInputPaused, showSideBar]);

  // When the Exit Stream prompt is open, treat it like the sidebar: release the
  // pointer lock and mark input blocked so mouse clicks/movement go to the popup
  // (and the OS cursor), not through to the game behind it. Reuses the same
  // body dataset flag the auto-lock + input-block checks already honor.
  useEffect(() => {
    if (!exitPrompt.open) {
      return;
    }
    try {
      document.body.dataset.sidebarOpen = "1";
    } catch {}
    onNativeInputPaused?.(true);
    if (onReleasePointerLock) {
      void onReleasePointerLock();
    } else if (document.pointerLockElement) {
      document.exitPointerLock();
    }
    return () => {
      onNativeInputPaused?.(false);
      try {
        delete (document.body.dataset as DOMStringMap).sidebarOpen;
      } catch {}
    };
  }, [exitPrompt.open, onNativeInputPaused, onReleasePointerLock]);

  useEffect(() => {
    if (showSideBar) {
      // Mark sidebar open so input auto-lock code can avoid re-requesting.
      try {
        document.body.dataset.sidebarOpen = "1";
      } catch {}

      if (onReleasePointerLock) {
        void onReleasePointerLock();
      } else {
        document.exitPointerLock();
      }
      void screenshotGallery.refreshScreenshots();
      void streamRecorder.refreshRecordings();
      return () => {
        try {
          delete (document.body.dataset as DOMStringMap).sidebarOpen;
        } catch {}
      };
    }
    if (suppressVideoFocusOnSidebarCloseRef.current) {
      suppressVideoFocusOnSidebarCloseRef.current = false;
      return undefined;
    }
    // Sidebar just closed — restore focus to the video so clicks register
    // immediately. Without this, focus stays on the last sidebar element and
    // mousedown's preventDefault() blocks the browser from re-focusing on click.
    const timer = window.setTimeout(() => {
      if (localVideoRef.current && document.activeElement !== localVideoRef.current) {
        localVideoRef.current.focus({ preventScroll: true });
      }
    }, 50);
    try {
      delete (document.body.dataset as DOMStringMap).sidebarOpen;
    } catch {}
    return () => clearTimeout(timer);
  }, [screenshotGallery.refreshScreenshots, showSideBar, streamRecorder.refreshRecordings]);

  const handleSidebarExitSession = useCallback(() => {
    suppressVideoFocusOnSidebarCloseRef.current = true;
    setShowSideBar(false);
    onEndSession();
  }, [onEndSession, setShowSideBar]);

  useEffect(() => {
    const blurStreamFocusTarget = (): void => {
      const active = document.activeElement;
      if (active instanceof HTMLElement && active.closest(".sv")) {
        active.blur();
      }
    };

    const hideFocusRingOnAccessKey = (event: KeyboardEvent): void => {
      if (event.key === "Alt" && !event.repeat) {
        blurStreamFocusTarget();
      }
    };

    const restoreStreamVideoFocus = (event: PointerEvent): void => {
      if (showSideBar || isConnecting || exitPrompt.open) {
        return;
      }
      const target = event.target as HTMLElement | null;
      if (target?.closest(".sv-sidebar, .sv-exit, .sv-shot-modal, button, a, input, textarea, select")) {
        return;
      }
      const video = localVideoRef.current;
      if (video && document.activeElement !== video) {
        video.focus({ preventScroll: true });
      }
    };

    window.addEventListener("blur", blurStreamFocusTarget);
    window.addEventListener("keydown", hideFocusRingOnAccessKey, true);
    window.addEventListener("pointerdown", restoreStreamVideoFocus, true);
    return () => {
      window.removeEventListener("blur", blurStreamFocusTarget);
      window.removeEventListener("keydown", hideFocusRingOnAccessKey, true);
      window.removeEventListener("pointerdown", restoreStreamVideoFocus, true);
    };
  }, [exitPrompt.open, isConnecting, showSideBar]);

  const nativeInternalHole =
    (nativeRendererActive || gstreamerEnabled) && !nativeExternalRenderer;

  return (
    <div
      className={["sv", streamVideoReady ? "sv--video-ready" : "sv--video-pending", nativeInternalHole ? "sv--native-hole" : "", className].filter(Boolean).join(" ")}
      // While the native streamer owns RawInput mouse capture, the OS cursor is
      // hidden/confined by capture; hide it over the shell too (falls back to
      // the addon's ShowCursor when that path is unavailable). Overlays release
      // capture (input-capture-changed: false) so their cursor returns.
      style={nativeInputCaptureActive ? { cursor: "none" } : undefined}
    >
      {nativeInternalHole ? (
        <video
          ref={setVideoRef}
          autoPlay
          playsInline
          muted
          tabIndex={-1}
          className="sv-video sv-video--native-hole"
          onClick={() => {
            if (localVideoRef.current && document.activeElement !== localVideoRef.current) {
              localVideoRef.current.focus({ preventScroll: true });
            }
          }}
        />
      ) : (
        <video
          ref={setVideoRef}
          autoPlay
          playsInline
          muted
          tabIndex={-1}
          className="sv-video"
          onClick={() => {
            if (localVideoRef.current && document.activeElement !== localVideoRef.current) {
              localVideoRef.current.focus({ preventScroll: true });
            }
          }}
        />
      )}
      <audio ref={setAudioRef} autoPlay playsInline />
      <VideoFocusOnReady
        diagnosticsStore={diagnosticsStore}
        isConnecting={isConnecting}
        videoRef={localVideoRef}
      />

      {pointerLockHintVisible && (
        <div className="sv-pointerlock-hint" role="status" aria-live="polite">
          <div>Press {shortcuts.toggleFullscreen} to exit fullscreen & release mouse</div>
          <div className="sv-pointerlock-hint-sub">
            {allowEscapeToExitFullscreen
              ? "Press Escape will also exit fullscreen per your settings."
              : "Escape goes to the game while pointer-locked; hold Escape ~1.5s to exit fullscreen."}
          </div>
        </div>
      )}

      <StreamQuickMenu
        open={showSideBar}
        onClose={() => setShowSideBar(false)}
        sidebarRef={sidebarRef}
        activeTab={activeSidebarTab}
        setActiveTab={setActiveSidebarTab}
        onEndSession={handleSidebarExitSession}
        gameTitle={gameTitle}
        platformName={platformName}
        PlatformIcon={PlatformIcon}
        subscriptionInfo={subscriptionInfo}
        sessionStartedAtMs={sessionStartedAtMs}
        isStreaming={isStreaming}
        sessionTimeRemainingText={sessionTimeRemainingText}
        isFullscreen={isFullscreen}
        isPointerLocked={isPointerLocked}
        onToggleFullscreen={handleFullscreenToggle}
        onTogglePointerLock={handlePointerLockToggle}
        onToggleMicrophone={onToggleMicrophone}
        showSessionTimeRemainingInStatsOverlay={showSessionTimeRemainingInStatsOverlay}
        onShowSessionTimeRemainingInStatsOverlayChange={onShowSessionTimeRemainingInStatsOverlayChange}
        statsPosition={statsPosition}
        onStatsPositionChange={onStatsPositionChange}
        sidebarToggleShortcutDisplay={sidebarToggleShortcutDisplay}
        controllerSidebarShortcutDisplay={CONTROLLER_SIDEBAR_SHORTCUT_DISPLAY}
        mouseSensitivity={mouseSensitivity}
        onMouseSensitivityChange={onMouseSensitivityChange}
        mouseAcceleration={mouseAcceleration}
        onMouseAccelerationChange={onMouseAccelerationChange}
        maxBitrateMbps={maxBitrateMbps}
        onMaxBitrateMbpsChange={onMaxBitrateMbpsChange}
        gstreamerEnabled={gstreamerEnabled}
        videoShader={videoShader}
        onVideoShaderChange={onVideoShaderChange}
        microphoneMode={microphoneMode}
        onMicrophoneModeChange={onMicrophoneModeChange}
        diagnosticsStore={diagnosticsStore}
        micTrack={micTrack ?? null}
        shortcuts={shortcuts}
        isMacClient={isMacClient}
        onScreenshotShortcutChange={onScreenshotShortcutChange}
        onRecordingShortcutChange={onRecordingShortcutChange}
        screenshotGallery={screenshotGallery}
        streamRecorder={streamRecorder}
        recordingBitrateMbps={recordingBitrateMbps}
        recordingResolution={recordingResolution}
        recordingFps={recordingFps}
        onRecordingResolutionChange={onRecordingResolutionChange}
        onRecordingFpsChange={onRecordingFpsChange}
        onRecordingBitrateMbpsChange={onRecordingBitrateMbpsChange}
      />

      {/* Gradient background when no video */}
      <StreamEmptyState diagnosticsStore={diagnosticsStore} />
      <StreamWaitingForVideo diagnosticsStore={diagnosticsStore} isConnecting={isConnecting} />

      {/* Connecting overlay */}
      {isConnecting && (
        <div className="sv-connect">
          <div className="sv-connect-inner">
            <MotionSpinner className="sv-connect-spin" size={44} label="Connecting to stream" />
            <p className="sv-connect-title">Connecting to {gameTitle}</p>
            {PlatformIcon && (
              <div className="sv-connect-platform" title={platformName}>
                <span className="sv-connect-platform-icon">
                  <PlatformIcon />
                </span>
                <span>{platformName}</span>
              </div>
            )}
            <p className="sv-connect-sub">Setting up stream...</p>
          </div>
        </div>
      )}

      {sessionCounterEnabled && !isConnecting && (
        <div
          className={`sv-session-clock${showSessionClock ? " is-visible" : ""}`}
          title="Current gaming session elapsed time"
          aria-hidden={!showSessionClock}
        >
          <SessionElapsedIndicator startedAtMs={sessionStartedAtMs} active={isStreaming} />
        </div>
      )}

      {streamWarning && !isConnecting && !exitPrompt.open && (
        <div
          className={`sv-time-warning sv-time-warning--${streamWarning.tone}`}
          title="Session time warning"
        >
          <AlertTriangle size={14} />
          <span>
            {streamWarning.message}
            {warningSeconds ? ` · ${warningSeconds} left` : ""}
          </span>
        </div>
      )}

      {antiAfkToggleAck && !isConnecting && (
        <div className={`sv-afk-ack sv-afk-ack--${antiAfkToggleAck}`} role="status" aria-live="polite">
          <span className="sv-afk-ack-dot" aria-hidden />
          <span>{antiAfkToggleAck === "on" ? "Anti-AFK on" : "Anti-AFK off"}</span>
        </div>
      )}

      <SessionStartedSplash
        visible={sessionReadySplashVisible && !isConnecting}
        gameTitle={gameTitle}
        onFinished={handleSessionReadySplashFinished}
      />

      <AnimatePresence>
        {showStatsHud && (
          <StreamStatsHud
            key="stream-stats-hud"
            diagnosticsStore={diagnosticsStore}
            mode={statsMode === "full" ? "full" : "compact"}
            position={statsPosition}
            gstreamerEnabled={gstreamerEnabled}
            serverRegion={serverRegion}
            userSelectedRegionName={userSelectedRegionName}
            sessionTimeRemainingText={showSessionTimeRemainingInStats ? sessionTimeRemainingText : null}
            hintsVisible={showHints}
          />
        )}
      </AnimatePresence>

      {/* Microphone toggle button */}
      <MicrophoneIndicator
        diagnosticsStore={diagnosticsStore}
        showAntiAfkIndicator={antiAfkEnabled && showAntiAfkIndicator}
        hideStreamButtons={hideStreamButtons}
        isConnecting={isConnecting}
        onToggleMicrophone={onToggleMicrophone}
      />

      {/* Anti-AFK indicator */}
      <AntiAfkIndicator
        diagnosticsStore={diagnosticsStore}
        antiAfkEnabled={antiAfkEnabled}
        showAntiAfkIndicator={showAntiAfkIndicator}
        isConnecting={isConnecting}
      />

      {/* Recording indicator (top-left, stacked below other badges) */}
      <RecordingIndicator
        diagnosticsStore={diagnosticsStore}
        showAntiAfkIndicator={antiAfkEnabled && showAntiAfkIndicator}
        hideStreamButtons={hideStreamButtons}
        isConnecting={isConnecting}
        isRecording={streamRecorder.isRecording}
        onToggleMicrophone={onToggleMicrophone}
        recordingDurationMs={streamRecorder.recordingDurationMs}
      />

      {/* Processing indicator: the brief window after STOP while the native
          streamer remuxes the recording offline into the final MP4. */}
      {streamRecorder.isProcessing && !streamRecorder.isRecording && (
        <ProcessingIndicator
          hideStreamButtons={hideStreamButtons}
          isConnecting={isConnecting}
        />
      )}

      {exitPrompt.open && !isConnecting && typeof document !== "undefined" && createPortal(
        <div className="sv-exit" role="dialog" aria-modal="true" aria-label="Exit stream confirmation">
          <button
            type="button"
            className="sv-exit-backdrop"
            onClick={onCancelExit}
            aria-label="Cancel exit"
          />
          <div className="sv-exit-card">
            <div className="sv-exit-kicker">Session Control</div>
            <h3 className="sv-exit-title">Exit Stream?</h3>
            <p className="sv-exit-text">
              Do you really want to exit <strong>{exitPrompt.gameTitle}</strong>?
            </p>
            <p className="sv-exit-subtext">Your current cloud gaming session will be closed.</p>
            <div className="sv-exit-actions">
              <button type="button" className="sv-exit-btn sv-exit-btn-cancel" onClick={onCancelExit}>
                Keep Playing
              </button>
              <button type="button" className="sv-exit-btn sv-exit-btn-confirm" onClick={onConfirmExit}>
                Exit Stream
              </button>
            </div>
            <div className="sv-exit-hint">
              <span><kbd>Enter</kbd> confirm · <kbd>Esc</kbd> cancel</span>
              <span><kbd>A</kbd> select · <kbd>B</kbd> cancel</span>
            </div>
          </div>
        </div>,
        document.body,
      )}

      {/* Fullscreen toggle */}
      {!hideStreamButtons && (
        <button
          className="sv-fs"
          onClick={handleFullscreenToggle}
          title={isFullscreen ? "Exit fullscreen" : "Enter fullscreen"}
          aria-label={isFullscreen ? "Exit fullscreen" : "Enter fullscreen"}
        >
          {isFullscreen ? <Minimize size={18} /> : <Maximize size={18} />}
        </button>
      )}

      {/* End session button */}
      {!hideStreamButtons && (
        <button
          className="sv-end"
          onClick={onEndSession}
          title="End session"
          aria-label="End session"
        >
          <LogOut size={18} />
        </button>
      )}

      {/* Keyboard hints */}
      {showHints && !isConnecting && (
        <div className="sv-hints">
          <div className="sv-hint"><kbd>{shortcuts.toggleStats}</kbd><span>Stats</span></div>
          <div className="sv-hint"><kbd>{shortcuts.togglePointerLock}</kbd><span>Mouse lock</span></div>
          <div className="sv-hint"><kbd>{shortcuts.toggleFullscreen}</kbd><span>Full screen</span></div>
          <div className="sv-hint"><kbd>{shortcuts.stopStream}</kbd><span>Stop</span></div>
          <div className="sv-hint"><kbd>{CONTROLLER_SIDEBAR_SHORTCUT_DISPLAY}</kbd><span>Controller menu</span></div>
          {shortcuts.toggleMicrophone && <div className="sv-hint"><kbd>{shortcuts.toggleMicrophone}</kbd><span>Mic</span></div>}
        </div>
      )}

      {/* Game title (bottom-center, fades) */}
      <StreamTitleBar
        diagnosticsStore={diagnosticsStore}
        gameTitle={gameTitle}
        platformName={platformName}
        PlatformIcon={PlatformIcon}
        showHints={showHints}
      />
    </div>
  );
}
