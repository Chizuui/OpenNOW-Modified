import { useEffect } from "react";
import type { MainToRendererSignalingEvent, Settings } from "@shared/gfn";

import { dispatchStreamShortcutAction } from "../../streamShortcutActions";
import {
  ICE_DISCONNECTED_RECOVERY_GRACE_MS,
  RECOVERABLE_STREAM_STATUSES,
  SIGNALING_REMOTE_ICE_GRACE_MS,
  isExpectedNativeSessionClose,
  readStreamClipboardText,
  sendStreamClipboardPaste,
} from "../../lib/streamSessionHelpers";
import { streamStatusToLoadingStage } from "../../lib/sessionState";
import { mergeNativeStreamStats } from "../../lib/streamDiagnostics";
import { decideSignalingDisconnect } from "../../lib/streamRecoveryDecisions";
import { warningMessage, warningTone } from "../../lib/sessionWarnings";
import { resolveStreamProfileCodec, type CodecTestResult, type NativeCodecAvailability } from "../../lib/codecDiagnostics";
import { GfnWebRtcClient } from "../../platforms/gfn/webrtcClient";
import { dispatchNativeDataChannelMessage } from "../../lib/nativeDataChannelRegistry";
import {
  GFN_CONTROL_CHANNEL_LABEL,
  installClipboardControlChannelHandler,
} from "../../platforms/gfn/controlChannel";
import { DEFAULT_CLIPBOARD_MAX_BYTES } from "../../platforms/gfn/clipboardProtocol";
import type { StreamDiagnosticsStore } from "../../utils/streamDiagnosticsStore";
import type { StreamRuntimeState } from "./useStreamRuntimeState";

type TranslateFunction = typeof import("../../i18n").t;
type ResetLaunchRuntime = (options?: {
  keepLaunchError?: boolean;
  keepStreamingContext?: boolean;
}) => void;

export interface SignalingEventOptions {
  runtime: StreamRuntimeState;
  attemptSessionRecovery: (reason: string) => Promise<boolean>;
  diagnosticsStore: StreamDiagnosticsStore;
  handleExpectedNativeSessionClose: (reason: string) => void;
  /**
   * Native AV1 → fallback codec auto-downgrade: the native streamer produced
   * zero decoded frames with the negotiated codec during startup. The renderer
   * marks the session as explicitly shut down and relaunches the same game
   * with `toCodec`.
   */
  handleNativeCodecDowngrade: (fromCodec: string, toCodec: string) => void | Promise<void>;
  markDiscordStreamStarted: () => void;
  refreshNavbarActiveSession: () => Promise<void>;
  resetLaunchRuntime: ResetLaunchRuntime;
  scheduleStableRecoveryReset: (sessionId: string) => void;
  /** Codec probe results for hardware-aware auto resolution (codecDiagnostics). */
  codecResults: CodecTestResult[] | null;
  /** Native streamer decoder capabilities (codecDiagnostics), when in native mode. */
  nativeAvailability: NativeCodecAvailability | null;
  settings: Settings;
  t: TranslateFunction;
}

export function useSignalingEvents({
  runtime,
  attemptSessionRecovery,
  diagnosticsStore,
  handleExpectedNativeSessionClose,
  handleNativeCodecDowngrade,
  markDiscordStreamStarted,
  refreshNavbarActiveSession,
  resetLaunchRuntime,
  scheduleStableRecoveryReset,
  codecResults,
  nativeAvailability,
  settings,
  t,
}: SignalingEventOptions): void {
  const {
    appUnloadingRef,
    audioRef,
    awaitingRecoveryRemoteIceRef,
    clientRef,
    hasConfirmedRemoteIceRef,
    handleStreamShortcutActionRef,
    iceDisconnectedRecoveryTimerRef,
    latestIceConnectionStateRef,
    launchInFlightRef,
    nativeInputBridgeReady,
    nativeInputProtocolVersionRef,
    nativeStreamingRef,
    pendingControlledDisconnectsRef,
    remoteIceGraceTimerRef,
    remoteIceRecoveryGenerationRef,
    remoteIceSeenForSessionRef,
    sessionRef,
    setLaunchError,
    setNativeInputBridgeReady,
    setNativeInputCaptureActive,
    setRemoteStreamWarning,
    setStreamStatus,
    signalingRecoveryRef,
    statsMode,
    streamMicLevel,
    streamStatusRef,
    streamVolume,
    videoRef,
  } = runtime;

  // Signaling events
  useEffect(() => {
    const ensureWebRtcClient = (): GfnWebRtcClient | null => {
      if (clientRef.current) {
        return clientRef.current;
      }
      if (!videoRef.current || !audioRef.current) {
        return null;
      }

      clientRef.current = new GfnWebRtcClient({
        videoElement: videoRef.current,
        audioElement: audioRef.current,
        autoFullScreen: settings.autoFullScreen,
        microphoneMode: settings.microphoneMode,
        microphoneDeviceId: settings.microphoneDeviceId || undefined,
        nativeCursorOverlay: settings.nativeCursorOverlay,
        mouseSensitivity: settings.mouseSensitivity,
        mouseAcceleration: settings.mouseAcceleration,
        keyboardLayout: settings.keyboardLayout,
        clipboardPaste: settings.clipboardPaste,
        statsHudVisible: statsMode !== "off",
        readClipboardText: readStreamClipboardText,
        onLog: (line: string) => console.log(`[WebRTC] ${line}`),
        onStats: (stats) => {
          // Keep client-side pipeline state (shaderActive) owned by StreamView
          // alive across the client's wholesale diagnostics snapshots.
          const snapshot = diagnosticsStore.getSnapshot();
          // In native mode the renderer client has no RTCPeerConnection, so
          // its snapshot carries no video/network data and would clobber the
          // native-streamer-reported fields (game FPS, server RTT/loss,
          // capture path, bitrate) whenever it re-emits (e.g. mouse-path
          // changes). Preserve those fields so the native HUD values and the
          // packet-loss banner stay stable.
          const preserved = snapshot.nativeRendererActive
            ? {
                gameFps: snapshot.gameFps,
                rttMs: snapshot.rttMs,
                packetLossPercent: snapshot.packetLossPercent,
                nativePacketLossPercent: snapshot.nativePacketLossPercent,
                bitrateKbps: snapshot.bitrateKbps,
                targetBitrateKbps: snapshot.targetBitrateKbps,
                decodeTimeMs: snapshot.decodeTimeMs,
                lagReason: snapshot.lagReason,
                lagReasonDetail: snapshot.lagReasonDetail,
                nativeInputPath: snapshot.nativeInputPath,
                nativeMouseDeltaLatencyUs: snapshot.nativeMouseDeltaLatencyUs,
                nativeServerBitrateKbps: snapshot.nativeServerBitrateKbps,
                nativeRequestedFps: snapshot.nativeRequestedFps,
                nativeCapsFramerate: snapshot.nativeCapsFramerate,
                nativeQueueMode: snapshot.nativeQueueMode,
                nativeFramesPendingToPresent: snapshot.nativeFramesPendingToPresent,
                nativePartialFlushCount: snapshot.nativePartialFlushCount,
                nativeCompleteFlushCount: snapshot.nativeCompleteFlushCount,
                nativeTransitionSummary: snapshot.nativeTransitionSummary,
                nativeRequestedStreamingFeaturesSummary: snapshot.nativeRequestedStreamingFeaturesSummary,
                nativeFinalizedStreamingFeaturesSummary: snapshot.nativeFinalizedStreamingFeaturesSummary,
              }
            : {};
          diagnosticsStore.set({ ...stats, ...preserved, shaderActive: snapshot.shaderActive });
        },
        onTimeWarning: (warning) => {
          setRemoteStreamWarning({
            code: warning.code,
            message: warningMessage(t, warning.code),
            tone: warningTone(warning.code),
            secondsLeft: warning.secondsLeft,
          });
        },
        onMicStateChange: (state) => {
          console.log(`[App] Mic state: ${state.state}${state.deviceLabel ? ` (${state.deviceLabel})` : ""}`);
        },
        onControllerMetaPress: () => {
          if (streamStatusRef.current === "streaming") {
            dispatchStreamShortcutAction("toggleSidebar");
          }
        },
        onIceConnectionStateChange: (iceState) => {
          latestIceConnectionStateRef.current = iceState;
          if (iceDisconnectedRecoveryTimerRef.current !== null) {
            window.clearTimeout(iceDisconnectedRecoveryTimerRef.current);
            iceDisconnectedRecoveryTimerRef.current = null;
          }
          if (appUnloadingRef.current) {
            return;
          }
          if (streamStatusRef.current !== "streaming") {
            return;
          }
          if (iceState === "failed") {
            console.warn("[Recovery] ICE failed; attempting targeted recovery");
            void attemptSessionRecovery("ICE failed").catch((error) => {
              console.error("[Recovery] ICE-failed recovery failed:", error);
            });
            return;
          }
          if (iceState === "disconnected") {
            iceDisconnectedRecoveryTimerRef.current = window.setTimeout(() => {
              iceDisconnectedRecoveryTimerRef.current = null;
              if (appUnloadingRef.current || streamStatusRef.current !== "streaming") {
                return;
              }
              if (latestIceConnectionStateRef.current !== "disconnected") {
                return;
              }
              console.warn("[Recovery] ICE remained disconnected; attempting targeted recovery");
              void attemptSessionRecovery("ICE disconnected timeout").catch((error) => {
                console.error("[Recovery] ICE-disconnected recovery failed:", error);
              });
            }, ICE_DISCONNECTED_RECOVERY_GRACE_MS);
          }
        },
      });
      clientRef.current.setOutputVolume(streamVolume);
      clientRef.current.setMicrophoneLevel(streamMicLevel);
      if (settings.microphoneMode !== "disabled") {
        void clientRef.current.startMicrophone();
      }
      return clientRef.current;
    };

    const activateNativeInputForCurrentSession = (protocolVersion?: number): void => {
      const activeSession = sessionRef.current;
      if (!activeSession) {
        console.warn("[App] Received native stream event but no active session in sessionRef!");
        return;
      }
      const client = ensureWebRtcClient();
      if (!client) {
        console.warn("[App] Native stream event received before media elements were ready");
        return;
      }

      const resolvedCodecProfile = resolveStreamProfileCodec(
        settings.codec,
        settings.colorQuality,
        codecResults,
        nativeAvailability,
      );

      nativeStreamingRef.current = true;
      pendingControlledDisconnectsRef.current = 0;
      const isWindowsHost = navigator.platform.toLowerCase().includes("win");
      // Stacked mode keeps the BrowserWindow as the focused, interactive shell
      // (the sink window is WS_EX_NOACTIVATE and can never take OS focus), so
      // input MUST flow through Electron's DOM capture → IPC bridge, exactly
      // like the internal child-surface mode.
      const electronInputBridge =
        settings.nativeStackedRenderer
        || /linux/i.test(`${navigator.platform} ${navigator.userAgent}`)
        || (!settings.nativeExternalRenderer && !isWindowsHost);
      client.activateNativeInput(
        protocolVersion,
        {
          codec: resolvedCodecProfile.codec,
          colorQuality: resolvedCodecProfile.colorQuality,
          resolution: settings.resolution,
          fps: settings.fps,
          maxBitrateKbps: settings.maxBitrateMbps * 1000,
          fallbackCodec: settings.fallbackCodec,
        },
        {
          // Windows internal: RawInput on the child HWND (Electron click-through is flaky).
          // Linux: always Electron → IPC (External floating renderer is unsupported).
          // macOS internal: Electron → IPC. External floating window: always OS capture.
          electronInputBridge,
        },
      );
      // Native mode owns the microphone: the native streamer captures WASAPI
      // and sends Opus over the negotiated mic m-line. The renderer WebRTC
      // client has no peer connection here, so its getUserMedia capture is
      // dead weight — stop it and mirror the native mic state in diagnostics
      // (the native streamer starts unmuted when the mode is enabled).
      if (settings.microphoneMode !== "disabled") {
        client.stopMicrophone();
        const snapshot = diagnosticsStore.getSnapshot();
        diagnosticsStore.set({
          ...snapshot,
          micState: "started",
          micEnabled: true,
        });
      }
      // The external native window exclusively owns Escape through RawInput.
      // Internal AND stacked mode leave Escape with Electron so it can prevent
      // Chromium's fullscreen exit and forward one synthetic tap to the native
      // streamer (the stacked sink never receives focus).
      window.openNow.notifyNativeInputModeChange(
        true,
        isWindowsHost && settings.nativeExternalRenderer && !settings.nativeStackedRenderer,
      );
      setLaunchError(null);
      setStreamStatus("streaming");
      markDiscordStreamStarted();
      scheduleStableRecoveryReset(activeSession.sessionId);
    };

    const unsubscribe = window.openNow.onSignalingEvent(async (event: MainToRendererSignalingEvent) => {
      console.log(`[App] Signaling event: ${event.type}`, event.type === "offer" ? `(SDP ${event.sdp.length} chars)` : "", event.type === "remote-ice" ? event.candidate : "");
      try {
        if (event.type === "offer") {
          pendingControlledDisconnectsRef.current = 0;
          const activeSession = sessionRef.current;
          if (!activeSession) {
            console.warn("[App] Received offer but no active session in sessionRef!");
            return;
          }
          const shouldEnforceRemoteIceGrace = awaitingRecoveryRemoteIceRef.current;
          remoteIceSeenForSessionRef.current = null;
          hasConfirmedRemoteIceRef.current = false;
          if (remoteIceGraceTimerRef.current !== null) {
            window.clearTimeout(remoteIceGraceTimerRef.current);
            remoteIceGraceTimerRef.current = null;
          }
          const expectedSessionId = activeSession.sessionId;
          const recoveryGenerationAtOffer = signalingRecoveryRef.current.generation;
          if (shouldEnforceRemoteIceGrace) {
            remoteIceGraceTimerRef.current = window.setTimeout(() => {
              remoteIceGraceTimerRef.current = null;
              if (sessionRef.current?.sessionId !== expectedSessionId) {
                return;
              }
              if (remoteIceSeenForSessionRef.current === expectedSessionId) {
                return;
              }
              if (remoteIceRecoveryGenerationRef.current === recoveryGenerationAtOffer) {
                return;
              }
              if (!RECOVERABLE_STREAM_STATUSES.includes(streamStatusRef.current)) {
                return;
              }
              awaitingRecoveryRemoteIceRef.current = false;
              remoteIceRecoveryGenerationRef.current = recoveryGenerationAtOffer;
              console.warn(
                `[Recovery] No remote ICE received within ${SIGNALING_REMOTE_ICE_GRACE_MS}ms after offer; forcing targeted recovery`,
              );
              void attemptSessionRecovery("No remote ICE received after offer").catch((error) => {
                console.error("[Recovery] ICE-timeout recovery failed:", error);
              });
            }, SIGNALING_REMOTE_ICE_GRACE_MS);
          }
          console.log("[App] Active session for offer:", JSON.stringify({
            sessionId: activeSession.sessionId,
            serverIp: activeSession.serverIp,
            signalingServer: activeSession.signalingServer,
            mediaConnectionInfo: activeSession.mediaConnectionInfo,
            iceServersCount: activeSession.iceServers?.length,
          }));

          const client = ensureWebRtcClient();

          if (client) {
            const offerCodecProfile = resolveStreamProfileCodec(
              settings.codec,
              settings.colorQuality,
              codecResults,
              nativeAvailability,
            );
            await client.handleOffer(event.sdp, activeSession, {
              codec: offerCodecProfile.codec,
              colorQuality: offerCodecProfile.colorQuality,
              resolution: settings.resolution,
              fps: settings.fps,
              maxBitrateKbps: settings.maxBitrateMbps * 1000,
              jitterBufferMode: settings.jitterBufferMode,
              fallbackCodec: settings.fallbackCodec,
              nativeTransitionDiagnostics: settings.nativeTransitionDiagnostics,
            });
            setLaunchError(null);
            setStreamStatus("streaming");
            markDiscordStreamStarted();
            scheduleStableRecoveryReset(activeSession.sessionId);
            console.log(
              "[Stream] Offer applied; use [WebRTC] logs for ICE/video dimensions. signalingServer=%s media=%s",
              activeSession.signalingServer,
              activeSession.mediaConnectionInfo
                ? `${activeSession.mediaConnectionInfo.ip}:${activeSession.mediaConnectionInfo.port}`
                : "n/a",
            );
          }
        } else if (event.type === "native-stream-started") {
          console.log("[App] Native streamer started:", event.message ?? "");
          activateNativeInputForCurrentSession(nativeInputProtocolVersionRef.current ?? undefined);
        } else if (event.type === "native-input-ready") {
          console.log("[App] Native input protocol ready:", event.protocolVersion);
          nativeInputProtocolVersionRef.current = event.protocolVersion;
          setNativeInputBridgeReady(true);
          clientRef.current?.setNativeInputProtocolVersion(event.protocolVersion);
          if (nativeStreamingRef.current || sessionRef.current) {
            activateNativeInputForCurrentSession(event.protocolVersion);
          }
        } else if (event.type === "native-shortcut") {
          handleStreamShortcutActionRef.current?.(event.action);
        } else if (event.type === "native-clipboard-paste") {
          if (settings.clipboardPaste && (!nativeStreamingRef.current || nativeInputBridgeReady)) {
            void sendStreamClipboardPaste(clientRef.current);
          }
        } else if (event.type === "native-data-channel-message") {
          // Generic relay: native streamer forwards every non-native remote
          // data channel verbatim; registered handlers (clipboard control
          // channel, etc.) decide what to do per label.
          dispatchNativeDataChannelMessage({
            label: event.label,
            payloadBase64: event.payloadBase64,
          });
        } else if (event.type === "native-input-capture-changed") {
          setNativeInputCaptureActive(event.captured);
          // When the native streamer captures input itself (stacked sink
          // window: mouse + keyboard), the renderer's addon/pointer-lock/DOM
          // sources stand down so the game never receives the same input twice.
          clientRef.current?.setNativeStreamerInputOwned(event.captured);
          // Treat OS RawInput capture like pointer lock so main-process Escape
          // interception keeps Chromium from exiting fullscreen on tap.
          try {
            window.openNow.notifyPointerLockChange(event.captured);
          } catch {
            /* best-effort */
          }
        } else if (event.type === "native-stream-stats") {
          diagnosticsStore.set({
            ...mergeNativeStreamStats(
              diagnosticsStore.getSnapshot(),
              event.stats,
            ),
            nativeStackedRenderer: settings.nativeStackedRenderer,
          });
        } else if (event.type === "native-stream-transition") {
          diagnosticsStore.set({
            ...diagnosticsStore.getSnapshot(),
            nativeRendererActive: true,
            nativeStackedRenderer: settings.nativeStackedRenderer,
            nativeTransitionSummary: event.transition.summary,
            nativeRequestedFps: event.transition.requestedFps,
            nativeCapsFramerate: event.transition.capsFramerate,
            nativeQueueMode: event.transition.queueMode,
            lagReasonDetail: event.transition.summary ?? "Native video transition detected",
          });
        } else if (event.type === "native-stream-stopped") {
          const reason = event.reason ?? "Native streamer stopped";
          console.warn("[App] Native streamer stopped:", reason);
          nativeStreamingRef.current = false;
          nativeInputProtocolVersionRef.current = null;
          setNativeInputBridgeReady(false);
          setNativeInputCaptureActive(false);
          window.openNow.notifyNativeInputModeChange(false, false);
          try {
            window.openNow.notifyPointerLockChange(false, true);
          } catch {
            /* best-effort */
          }
          clientRef.current?.dispose();
          clientRef.current = null;
          launchInFlightRef.current = false;

          if (appUnloadingRef.current) {
            console.log("[Recovery] Ignoring native streamer stop during app shutdown");
            return;
          }
          if (streamStatusRef.current === "streaming" && isExpectedNativeSessionClose(reason)) {
            handleExpectedNativeSessionClose(reason);
            return;
          }
          if (
            signalingRecoveryRef.current.explicitShutdown
            || !RECOVERABLE_STREAM_STATUSES.includes(streamStatusRef.current)
          ) {
            console.log("[Recovery] Ignoring native streamer stop after explicit shutdown or non-recoverable status");
            return;
          }

          const recovered = await attemptSessionRecovery(reason).catch((error) => {
            console.error("[Recovery] Native streamer recovery failed:", error);
            return false;
          });
          if (!recovered) {
            if (
              signalingRecoveryRef.current.explicitShutdown
              || !RECOVERABLE_STREAM_STATUSES.includes(streamStatusRef.current)
            ) {
              console.log("[Recovery] Ignoring native streamer stop after explicit shutdown or non-recoverable status");
              return;
            }
            setLaunchError({
              stage: streamStatusToLoadingStage(streamStatusRef.current),
              title: t("errors.nativeStreamerStoppedTitle"),
              description: t("errors.nativeStreamerStoppedDescription"),
            });
            resetLaunchRuntime({ keepLaunchError: true, keepStreamingContext: true });
            void refreshNavbarActiveSession();
            launchInFlightRef.current = false;
          }
        } else if (event.type === "native-codec-downgrade-request") {
          // The native streamer's negotiated codec produced zero decoded
          // frames during startup. The handler marks the session as
          // explicitly shut down FIRST (synchronously) so the streamer-stop
          // events the manager fires right after are ignored by recovery, then
          // relaunches the same game with the fallback codec in a fresh
          // session.
          await handleNativeCodecDowngrade(event.fromCodec, event.toCodec);
        } else if (event.type === "remote-ice") {
          remoteIceSeenForSessionRef.current = sessionRef.current?.sessionId ?? null;
          hasConfirmedRemoteIceRef.current = true;
          awaitingRecoveryRemoteIceRef.current = false;
          if (remoteIceGraceTimerRef.current !== null) {
            window.clearTimeout(remoteIceGraceTimerRef.current);
            remoteIceGraceTimerRef.current = null;
          }
          await clientRef.current?.addRemoteCandidate(event.candidate);
        } else if (event.type === "disconnected") {
          const iceState = latestIceConnectionStateRef.current;
          const decision = decideSignalingDisconnect({
            appUnloading: appUnloadingRef.current,
            streamStatus: streamStatusRef.current,
            reason: event.reason,
            hasConfirmedRemoteIce: hasConfirmedRemoteIceRef.current,
            iceState,
            pendingControlledDisconnects: pendingControlledDisconnectsRef.current,
          });
          if (decision === "ignore-app-unloading") {
            console.log("[Recovery] Ignoring signaling disconnect during app shutdown");
            return;
          }
          if (decision === "expected-session-close") {
            handleExpectedNativeSessionClose(event.reason);
            return;
          }
          if (decision === "ignore-active-ice") {
            console.log(`[Recovery] Ignoring signaling disconnect while ICE state is ${iceState}`);
            return;
          }
          if (decision === "fail-before-remote-ice") {
            console.warn("[Recovery] Skipping auto-recovery: disconnected before remote ICE handshake");
            clientRef.current?.dispose();
            clientRef.current = null;
            setLaunchError({
              stage: streamStatusToLoadingStage(streamStatusRef.current),
              title: t("errors.sessionConnectionLostTitle"),
              description: t("errors.resumeAttachFailedDescription"),
            });
            resetLaunchRuntime({ keepLaunchError: true, keepStreamingContext: true });
            void refreshNavbarActiveSession();
            launchInFlightRef.current = false;
            return;
          }
          if (remoteIceGraceTimerRef.current !== null) {
            window.clearTimeout(remoteIceGraceTimerRef.current);
            remoteIceGraceTimerRef.current = null;
          }
          remoteIceSeenForSessionRef.current = null;
          awaitingRecoveryRemoteIceRef.current = false;
          if (decision === "ignore-controlled-disconnect") {
            pendingControlledDisconnectsRef.current -= 1;
            console.log("[Recovery] Ignoring controlled signaling disconnect");
            return;
          }
          console.warn("Signaling disconnected:", event.reason);
          const recovered = await attemptSessionRecovery(event.reason).catch((error) => {
            console.error("[Recovery] Signaling recovery failed:", error);
            throw error;
          });
          if (!recovered) {
            if (
              signalingRecoveryRef.current.explicitShutdown
              || !RECOVERABLE_STREAM_STATUSES.includes(streamStatusRef.current)
            ) {
              console.log("[Recovery] Ignoring disconnect after explicit shutdown or non-recoverable status");
              return;
            }
            clientRef.current?.dispose();
            clientRef.current = null;
            setLaunchError({
              stage: streamStatusToLoadingStage(streamStatusRef.current),
              title: t("errors.sessionConnectionLostTitle"),
              description: t("errors.sessionConnectionLostDescription"),
            });
            resetLaunchRuntime({ keepLaunchError: true, keepStreamingContext: true });
            void refreshNavbarActiveSession();
            launchInFlightRef.current = false;
          }
        } else if (event.type === "error") {
          console.error("Signaling error:", event.message);
        }
      } catch (error) {
        if (appUnloadingRef.current) {
          console.log("[Recovery] Suppressing signaling handler errors during app shutdown");
          return;
        }
        if (
          signalingRecoveryRef.current.explicitShutdown
          || !RECOVERABLE_STREAM_STATUSES.includes(streamStatusRef.current)
        ) {
          console.log("[Recovery] Suppressing signaling error after explicit shutdown or non-recoverable status");
          return;
        }
        console.error("Signaling event error:", error);
        clientRef.current?.dispose();
        clientRef.current = null;
        const message = error instanceof Error ? error.message : t("errors.sessionResumeFailedDescription");
        setLaunchError({
          stage: streamStatusToLoadingStage(streamStatusRef.current),
          title: t("errors.sessionConnectionLostTitle"),
          description: message,
        });
        resetLaunchRuntime({ keepLaunchError: true, keepStreamingContext: true });
        void refreshNavbarActiveSession();
        launchInFlightRef.current = false;
      }
    });

    return () => unsubscribe();
  }, [attemptSessionRecovery, diagnosticsStore, handleExpectedNativeSessionClose, handleNativeCodecDowngrade, markDiscordStreamStarted, nativeInputBridgeReady, refreshNavbarActiveSession, resetLaunchRuntime, scheduleStableRecoveryReset, settings, streamMicLevel, streamVolume, t]);

  // Keep the WebRTC client's stats-polling cadence in sync with the stats HUD
  // when the user toggles it mid-session (1s while visible / while recovering,
  // 3s on a healthy hidden stream). The initial value is passed at client
  // construction; this covers runtime changes.
  useEffect(() => {
    clientRef.current?.setStatsHudVisible(statsMode !== "off");
  }, [clientRef, statsMode]);

  // Register the control-channel clipboard handler for the lifetime of this
  // session (native relay; web registers its own copy when the control channel
  // opens). Other server-initiated data channel features can register their own
  // handlers against the same registry.
  useEffect(
    () =>
      installClipboardControlChannelHandler({
        enabled: () => settings.clipboardPaste,
        maxBytes: DEFAULT_CLIPBOARD_MAX_BYTES,
        readClipboardText: readStreamClipboardText,
        sendReply: (payloadBase64) =>
          window.openNow.sendNativeDataChannelMessage(
            GFN_CONTROL_CHANNEL_LABEL,
            payloadBase64,
          ),
      }),
    [settings.clipboardPaste],
  );

}
