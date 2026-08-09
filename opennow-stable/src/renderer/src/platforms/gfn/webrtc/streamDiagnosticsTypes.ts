import type { NativeQueueMode } from "@shared/gfn";
import type { MicState } from "../microphoneManager";

export interface StreamDiagnostics {
  // Connection state
  connectionState: RTCPeerConnectionState | "closed";
  inputReady: boolean;
  nativeRendererActive: boolean;
  /** GFN-style stacked renderer: video in a native window behind the transparent shell, DOM overlays still shown */
  nativeStackedRenderer: boolean;
  connectedGamepads: number;

  // Video stats
  resolution: string;
  /** Codec actually negotiated for the live stream (from inbound-rtp stats in web mode; native stats otherwise). */
  codec: string;
  /** Codec requested in settings at session start; differs from `codec` when the client fell back to a supported codec. */
  requestedCodec: string;
  hardwareAcceleration: string;
  colorCodec: string;
  isHdr: boolean;
  bitrateKbps: number;
  targetBitrateKbps: number;
  decodeFps: number;
  /** Frames received from the network per second (≈ server-sent rate), computed from per-interval deltas. */
  receiveFps: number;
  renderFps: number;
  gameFps?: number;

  // Network stats
  packetsLost: number;
  packetsReceived: number;
  packetLossPercent: number;
  jitterMs: number;
  rttMs: number;
  /**
   * Transport of the active ICE candidate pair: "udp" | "tcp" | "unknown".
   * TCP means Chromium fell back because UDP is unreachable (e.g. an ISP
   * blocking/throttling UDP) — a common cause of a low hard bitrate ceiling.
   */
  transportType: "udp" | "tcp" | "unknown";
  /** Local ICE candidate type of the active pair (host/srflx/prflx/relay). */
  localCandidateType: string;

  // Frame counters
  framesReceived: number;
  framesDecoded: number;
  framesDropped: number;

  // Timing
  decodeTimeMs: number;
  renderTimeMs: number;
  jitterBufferDelayMs: number;

  // Input channel pressure
  inputQueueBufferedBytes: number;
  inputQueuePeakBufferedBytes: number;
  partiallyReliableInputQueueBufferedBytes: number;
  partiallyReliableInputQueuePeakBufferedBytes: number;
  inputQueueDropCount: number;
  inputQueueMaxSchedulingDelayMs: number;
  partiallyReliableInputOpen: boolean;
  mouseMoveTransport: "reliable" | "partially_reliable";
  mouseFlushIntervalMs: number;
  mousePacketsPerSecond: number;
  mouseResidualMagnitude: number;
  mouseAdaptiveFlushActive: boolean;
  /**
   * Effective mouse input path this session: sink-native (native streamer
   * captures RawInput on the stacked sink window, in-process), addon (native
   * raw-mouse addon → IPC → stdin), pointer-lock (DOM fallback), or none.
   */
  mousePath: "sink-native" | "addon" | "pointer-lock" | "internal" | "external" | "none";
  /** Renderer-side event→send hop latency (EMA) for the addon / pointer-lock paths. */
  mouseHopLatencyMs?: number;
  /** Active capture path reported by the native streamer (stats event). */
  nativeInputPath?: string;
  /** Native-measured in-process mouse delta latency (EMA, µs) for sink-native capture. */
  nativeMouseDeltaLatencyUs?: number;
  /**
   * Server-reported session bitrate (kbps) from the stats_channel counter
   * (confidence-gated in the native streamer — only present once the counter
   * is verified to be cumulative bytes).
   */
  nativeServerBitrateKbps?: number;
  /**
   * Server-reported packet loss (percent, 0..100) from the stats_channel,
   * kept separate from the receiver-computed `packetLossPercent` so the
   * native HUD banner can key off the server's own (cleaner) measurement —
   * same value the native merge also mirrors into `packetLossPercent` for the
   * loss row display.
   */
  nativePacketLossPercent?: number;

  lagReason: StreamLagReason;
  lagReasonDetail: string;

  // System info
  gpuType: string;
  serverRegion: string;
  /** Raw CloudMatch zone code (e.g. "NP-TYO-01"), used for a friendly location label. */
  serverZone: string;
  /** Pre-resolved friendly location label (e.g. "India (BOM)"), computed once at session start before serverRegion degrades to an IP. */
  serverLocationLabel?: string;

  // Decoder recovery status
  decoderPressureActive: boolean;
  decoderRecoveryAttempts: number;
  decoderRecoveryAction: string;
  nativeRequestedFps?: number;
  nativeCapsFramerate?: string;
  nativeQueueMode?: NativeQueueMode;
  nativeFramesPendingToPresent?: number;
  nativePartialFlushCount?: number;
  nativeCompleteFlushCount?: number;
  nativeTransitionSummary?: string;
  nativeRequestedStreamingFeaturesSummary?: string;
  nativeFinalizedStreamingFeaturesSummary?: string;

  // Client-side post-processing (video shader pipeline)
  /**
   * True while the WebGL2 post-processing pipeline is actively applying a
   * visible effect to stream frames. Owned by StreamView (which creates the
   * pipeline), NOT by the WebRTC client — callers that replace the snapshot
   * wholesale (e.g. onStats in useSignalingEvents) must preserve this field.
   */
  shaderActive: boolean;

  // Microphone state
  micState: MicState;
  micEnabled: boolean;
}

export type StreamLagReason =
  | "unknown"
  | "stable"
  | "network"
  | "decoder"
  | "input_backpressure"
  | "render";

export interface StreamTimeWarning {
  code: 1 | 2 | 3;
  secondsLeft?: number;
}
