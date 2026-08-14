import type {
  IceCandidatePayload,
  NativeStreamerBackend,
  NativeStreamStats,
  NativeRenderSurface,
  NativeStreamerShortcutAction,
  NativeStreamerSessionContext,
  NativeVideoTransition,
  NativeVideoBackendCapability,
  SendAnswerRequest,
} from "./gfn";

export const NATIVE_STREAMER_PROTOCOL_VERSION = 4;

export type { NativeStreamerBackend };

export interface NativeStreamerCapabilities {
  protocolVersion: number;
  backend: NativeStreamerBackend;
  requestedBackend?: string;
  fallbackReason?: string;
  supportsOfferAnswer: boolean;
  supportsRemoteIce: boolean;
  supportsLocalIce: boolean;
  supportsInput: boolean;
  videoBackends?: NativeVideoBackendCapability[];
}

export interface NativeStreamerInputPacket {
  payloadBase64: string;
  partiallyReliable?: boolean;
}

export type NativeStreamerCommand =
  | {
      id: string;
      type: "hello";
      protocolVersion: number;
    }
  | {
      id: string;
      type: "start";
      context: NativeStreamerSessionContext;
    }
  | {
      id: string;
      type: "offer";
      sdp: string;
      context: NativeStreamerSessionContext;
    }
  | {
      id: string;
      type: "remote-ice";
      candidate: IceCandidatePayload;
    }
  | {
      id: string;
      type: "input";
      input: NativeStreamerInputPacket;
    }
  | {
      id: string;
      type: "input-paused";
      paused: boolean;
    }
  | {
      id: string;
      type: "surface";
      surface: NativeRenderSurface;
    }
  | {
      id: string;
      type: "bitrate";
      maxBitrateKbps: number;
    }
  | {
      id: string;
      type: "pacing";
      /** `auto` | `stream` | `vrr` | `off` | a fixed fps like `144`. */
      pacingMode: string;
    }
  | {
      id: string;
      type: "stop";
      reason?: string;
    }
  | {
      id: string;
      type: "update-shortcuts";
      shortcuts: import("./gfn").NativeStreamerShortcutBindings;
    }
  | {
      id: string;
      type: "microphone";
      microphoneEnabled: boolean;
    }
  | {
      id: string;
      type: "take-screenshot";
    }
  | {
      id: string;
      type: "start-recording";
    }
  | {
      id: string;
      type: "stop-recording";
      finalize: boolean;
    }
  | {
      id: string;
      type: "send-data-channel-message";
      label: string;
      payloadBase64: string;
    };

export type NativeStreamerResponse =
  | {
      id: string;
      type: "ready";
      capabilities: NativeStreamerCapabilities;
    }
  | {
      id: string;
      type: "ok";
    }
  | {
      id: string;
      type: "answer";
      answer: SendAnswerRequest;
    }
  | {
      id: string;
      type: "error";
      code?: string;
      message: string;
    };

export type NativeStreamerEvent =
  | {
      type: "log";
      level: "debug" | "info" | "warn" | "error";
      message: string;
    }
  | {
      type: "status";
      status: "starting" | "ready" | "streaming" | "stopped";
      message?: string;
    }
  | {
      type: "local-ice";
      candidate: IceCandidatePayload;
    }
  | {
      type: "input-ready";
      protocolVersion: number;
    }
  | {
      type: "shortcut";
      action: NativeStreamerShortcutAction;
    }
  | {
      type: "clipboard-paste";
    }
  | {
      /** Server-initiated message on a remote data channel (e.g. GFN `control_channel`). */
      type: "data-channel-message";
      label: string;
      payloadBase64: string;
    }
  | {
      type: "input-capture-changed";
      captured: boolean;
    }
  | {
      type: "video-stall";
      stallMs: number;
      encodedKbps?: number;
      decodedFps: number;
      sinkFps: number;
      encodedAgeMs?: number;
      decodedAgeMs?: number;
      sinkAgeMs?: number;
      likelyStage?: string;
      sinkRendered?: number;
      sinkDropped?: number;
      memoryMode?: string;
      zeroCopy?: boolean;
      requestedFps?: number;
      capsFramerate?: string;
      queueMode?: string;
      partialFlushCount?: number;
      completeFlushCount?: number;
      lastTransitionType?: string;
      lastTransitionAtMs?: number;
      requestedStreamingFeaturesSummary?: string;
      finalizedStreamingFeaturesSummary?: string;
      zeroCopyD3D11: boolean;
      zeroCopyD3D12: boolean;
      recoveryAttempt: number;
    }
  | {
      /** Keyframe request to forward to the RTCP/PLI signaling path. Never a GStreamer CustomUpstream event (that kills the transport). */
      type: "video-keyframe-request";
      reason: string;
      attempt?: number;
    }
  | {
      type: "video-transition";
      transition: NativeVideoTransition;
    }
  | {
      type: "stats";
      stats: NativeStreamStats;
    }
  | {
      type: "screenshot";
      screenshot: {
        pngBase64: string;
        width: number;
        height: number;
      };
    }
  | {
      type: "recording-chunk";
      chunkBase64: string;
    }
  | {
      type: "recording-finished";
      /** Base64 JPEG of the first encoded recording frame (gallery thumbnail). */
      thumbnailBase64?: string;
      /**
       * Recording frames the branch dropped because the encoder/queue could
       * not keep up (leaky queue drops) — surfaced so a choppy recording is
       * explained. 0 when the encoder kept up.
       */
      droppedFrames: number;
    }
  | {
      /**
       * The negotiated video codec produced zero decoded frames during
       * startup (every decoder candidate exhausted). The manager forwards
       * this to the renderer so it can restart the session with `toCodec`
       * instead of leaving the user on a black screen.
       */
      type: "codec-downgrade-request";
      fromCodec: string;
      toCodec: string;
    }
  | {
      /**
       * Runtime network health verdict (the native analogue of GFN's
       * pre-stream "stream test"). Emitted when the verdict or a recovery
       * recommendation changes; the manager already acted on the keyframe
       * suggestion, the renderer surfaces it and may trigger the profile
       * downgrade.
       */
      type: "network-assessment";
      assessment: NativeNetworkAssessment;
    }
  | {
      type: "error";
      code?: string;
      message: string;
    };

export interface NativeNetworkAssessment {
  verdict: "stable" | "degraded" | "poor";
  jitterMs?: number;
  rttMs?: number;
  /** Packet loss percent (0-100), smoothed EMA from the stats channel. */
  lossPercent?: number;
  recommendLowerFps: boolean;
  recommendLowerResolution: boolean;
  suggestKeyframe: boolean;
}

export type NativeStreamerMessage = NativeStreamerResponse | NativeStreamerEvent;
