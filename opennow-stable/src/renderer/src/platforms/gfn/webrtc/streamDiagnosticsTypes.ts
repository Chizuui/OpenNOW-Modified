import type { NativeQueueMode } from "@shared/gfn";
import type { MicState } from "../microphoneManager";

export interface StreamDiagnostics {
  // Connection state
  connectionState: RTCPeerConnectionState | "closed";
  inputReady: boolean;
  nativeRendererActive: boolean;
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
  renderFps: number;
  gameFps?: number;

  // Network stats
  packetsLost: number;
  packetsReceived: number;
  packetLossPercent: number;
  jitterMs: number;
  rttMs: number;

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
