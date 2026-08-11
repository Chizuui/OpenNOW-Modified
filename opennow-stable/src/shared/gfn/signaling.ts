import type { NativeQueueMode } from "./stream";
import type { SessionInfo, StreamSettings } from "./session";

export interface SignalingConnectRequest {
  sessionId: string;
  signalingServer: string;
  signalingUrl?: string;
  nativeStreamer?: NativeStreamerSessionContext;
}

export interface IceCandidatePayload {
  candidate: string;
  sdpMid?: string | null;
  sdpMLineIndex?: number | null;
  usernameFragment?: string | null;
}

export interface SendAnswerRequest {
  sdp: string;
  nvstSdp?: string;
}

export type NativeStreamerShortcutAction =
  | "toggleStats"
  | "togglePointerLock"
  | "toggleFullscreen"
  | "stopStream"
  | "toggleAntiAfk"
  | "toggleMicrophone"
  | "screenshot"
  | "toggleRecording";

export interface NativeStreamerShortcutBindings {
  toggleStats: string;
  togglePointerLock: string;
  toggleFullscreen: string;
  stopStream: string;
  toggleAntiAfk: string;
  toggleMicrophone: string;
  screenshot: string;
  toggleRecording: string;
}

export interface NativeStreamerSessionContext {
  session: SessionInfo;
  settings: StreamSettings;
  shortcuts: NativeStreamerShortcutBindings;
  nvstVideo?: NvstVideoSession;
}

export interface NvstVideoSession {
  clientUdpPort: number;
  videoPeerIp: string;
  videoPeerPort: number;
  srtpAesKeyHex: string;
  srtpKeyId: number;
  pingPayload?: string;
  codec?: string;
}

export function buildNativeStreamerSessionContext(
  session: SessionInfo,
  settings: StreamSettings,
  shortcuts: NativeStreamerShortcutBindings,
  nvstVideo?: NvstVideoSession,
): NativeStreamerSessionContext {
  const negotiatedStreamProfile = session.negotiatedStreamProfile
    ? {
      ...session.negotiatedStreamProfile,
      codec: session.negotiatedStreamProfile.codec ?? settings.codec,
    }
    : { codec: settings.codec };

  return {
    session: {
      ...session,
      negotiatedStreamProfile,
    },
    settings: {
      ...settings,
      enableCloudGsync:
        session.negotiatedStreamProfile?.enableCloudGsync ?? settings.enableCloudGsync,
      // The renderer resolves microphoneMode → microphoneEnabled ("always-on"
      // and "push-to-talk" both capture); the native streamer owns mute via
      // its volume element.
      microphoneEnabled: settings.microphoneEnabled ?? false,
    },
    shortcuts,
    ...(nvstVideo ? { nvstVideo } : {}),
  };
}

export interface NativeVideoTransition {
  transitionType: string;
  source: string;
  atMs: number;
  oldCaps?: string;
  newCaps?: string;
  oldFramerate?: string;
  newFramerate?: string;
  oldMemoryMode?: string;
  newMemoryMode?: string;
  renderGapMs?: number;
  requestedFps?: number;
  capsFramerate?: string;
  highFpsRisk?: boolean;
  queueMode?: NativeQueueMode;
  summary?: string;
}

export interface NativeInputPacket {
  payload: ArrayBuffer | Uint8Array | number[];
  partiallyReliable?: boolean;
}

export interface NativeRenderSurfaceRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface NativeRenderSurfaceUpdate {
  rect: NativeRenderSurfaceRect | null;
  visible: boolean;
  deviceScaleFactor: number;
  showStats?: boolean;
}

export interface NativeRenderSurface extends NativeRenderSurfaceUpdate {
  windowHandle?: string;
}

export interface KeyframeRequest {
  reason: string;
  backlogFrames: number;
  attempt: number;
}

export type MainToRendererSignalingEvent =
  | { type: "connected" }
  | { type: "disconnected"; reason: string }
  | { type: "offer"; sdp: string }
  | { type: "remote-ice"; candidate: IceCandidatePayload }
  | { type: "native-shortcut"; action: NativeStreamerShortcutAction }
  | { type: "native-clipboard-paste" }
  | {
      type: "native-data-channel-message";
      label: string;
      payloadBase64: string;
    }
  | { type: "native-input-capture-changed"; captured: boolean }
  | { type: "native-stream-started"; message?: string }
  | { type: "native-stream-stopped"; reason?: string }
  | { type: "native-stream-stats"; stats: NativeStreamStats }
  | { type: "native-stream-transition"; transition: NativeVideoTransition }
  | { type: "native-input-ready"; protocolVersion: number }
  | { type: "error"; message: string }
  | { type: "log"; message: string };

export interface NativeStreamStats {
  codec: string;
  resolution: string;
  hardwareAcceleration: string;
  memoryMode?: string;
  zeroCopy?: boolean;
  requestedFps?: number;
  capsFramerate?: string;
  bitrateKbps: number;
  targetBitrateKbps: number;
  bitratePerformancePercent: number;
  decodedFps: number;
  renderFps: number;
  /** Server-reported game render FPS from the stats_channel (may exceed the stream FPS). */
  gameFps?: number;
  /** Server-reported network round-trip time (ms) from the stats_channel. */
  networkRttMs?: number;
  /**
   * Locally computed RTCP round-trip time (ms) from Receiver Reports the
   * server sends about our outgoing RTP (rtpsession LSR/DLSR). Absent for a
   * receiver-only pipeline — the native streamer sends no RTP today, so this
   * stays undefined until outgoing RTP exists.
   */
  localRtcpRttMs?: number;
  /** Server-reported packet loss (percent) from the stats_channel. */
  networkPacketLossPercent?: number;
  /**
   * Server-reported session bitrate (kbps) derived from the stats_channel
   * counter deltas — confidence-gated in the native streamer, so it only
   * appears once the counter is verified to be cumulative bytes.
   */
  networkBitrateKbps?: number;
  /**
   * Raw CloudMatch gpuType code (e.g. "2080d / T10") stamped by the main
   * process onto native-stream-stats so the renderer HUD can show the
   * server rig. The renderer maps it to the official rig name via
   * mapServerGpuType(). Absent in WebRTC (non-native) sessions, where the
   * client reads session.gpuType directly.
   */
  serverGpuType?: string;
  /**
   * Zone LB hostname / datacenter code (e.g. "npa-yes-kul-01.yes.geforcenow...")
   * stamped by the main process onto native-stream-stats so the renderer HUD
   * can show the region label. The renderer maps it to "Country (NP-CODE)"
   * via formatServerLocation(). Absent in WebRTC (non-native) sessions, where
   * the client resolves the label from session.serverLocation directly.
   */
  serverLocation?: string;
  /** Average decode→present pipeline latency in ms. */
  decodeTimeMs?: number;
  /** Active input capture path in the native streamer: sink-native / internal / external / bridge / none. */
  inputPath?: string;
  /** Measured in-process mouse delta latency (capture → data channel send) in µs. */
  mouseDeltaLatencyUs?: number;
  framesDecoded: number;
  framesRendered: number;
  framesPendingToPresent?: number;
  sinkRendered?: number;
  sinkDropped?: number;
  zeroCopyD3D11: boolean;
  zeroCopyD3D12: boolean;
  queueMode?: NativeQueueMode;
  queueDepthChanges?: number;
  presentPacingChanges?: number;
  partialFlushCount?: number;
  completeFlushCount?: number;
  lastTransitionType?: string;
  lastTransitionAtMs?: number;
  lastTransitionSummary?: string;
  requestedStreamingFeaturesSummary?: string;
  finalizedStreamingFeaturesSummary?: string;
}

