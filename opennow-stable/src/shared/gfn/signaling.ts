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
  | {
      /**
       * The native streamer's negotiated video codec produced zero decoded
       * frames during startup (every decoder candidate exhausted). The
       * renderer restarts the game session with `toCodec` (GFN ladder: AV1 →
       * H265) so the session keeps running instead of showing a black screen.
       */
      type: "native-codec-downgrade-request";
      fromCodec: string;
      toCodec: string;
    }
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
   * Age (ms) of the `networkRttMs` sample — time since the last stats_channel
   * frame carrying a valid RTT arrived. The stats channel cadence is
   * irregular (bursty), so the renderer uses this to expire a server RTT
   * that stopped refreshing instead of holding it as the "current" ping.
   */
  networkRttAgeMs?: number;
  /**
   * Locally computed RTCP round-trip time (ms) from Receiver Reports the
   * server sends about our outgoing RTP (rtpsession LSR/DLSR). Absent for a
   * receiver-only pipeline — the native streamer sends no RTP today, so this
   * stays undefined until outgoing RTP exists. The native streamer only
   * reports the value while it is fresh (see `localRtcpRttAgeMs`):
   * rtpsession's `have-rb` flag sticks once set, so without expiry the local
   * value would override the server RTT forever.
   */
  localRtcpRttMs?: number;
  /**
   * Age (ms) of the `localRtcpRttMs` sample — time since the last Receiver
   * Report from the server changed the measurement. Lets the renderer expire
   * a local RTCP value whose RR stream stopped, and prefer the freshest
   * source when both the local and server RTT are present.
   */
  localRtcpRttAgeMs?: number;
  /**
   * Locally computed interarrival jitter (ms) of the incoming video stream,
   * from rtpsession's RFC 3550 `jitter`/`avg-jitter` source-stats fields
   * converted from RTP timestamp units via the source clock rate. Unlike
   * `localRtcpRttMs` this does not need outgoing RTP — it works in
   * receiver-only mode. Absent while the stream is stalled (no RTP within
   * the liveness window) so the HUD decays a frozen value.
   */
  localJitterMs?: number;
  /**
   * Target depth of the native adaptive pre-decode jitter buffer, in
   * milliseconds of buffered video (compressed-frame count × frame
   * interval) — the delay the streamer intentionally holds the stream at
   * before decoding so retransmissions and jitter bursts land inside the
   * buffer. The native analogue of the WebRTC `jitterBufferDelayMs` HUD
   * metric. Absent while the stream is stalled (RTP liveness gate) so the
   * HUD decays a frozen depth.
   */
  preDecodeJitterBufferMs?: number;
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
  /**
   * Duplicate-frame detector: decoded frames seen vs unique (differed from
   * the previous frame by PTS or strided content checksum). GFN re-encodes a
   * frame twice when the game renders slower than the negotiated stream rate,
   * so unique < seen shows how much of the delivered stream is real motion.
   * Equal when pixels were not readable (zero-copy GPU memory → same-PTS
   * check only).
   */
  duplicateFramesSeen?: number;
  duplicateFramesUnique?: number;
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

