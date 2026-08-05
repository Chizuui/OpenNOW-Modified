import { normalizeJitterBufferMode, type JitterBufferMode } from "@shared/gfn";

export interface DecoderPressureSample {
  framesReceived: number;
  framesDecoded: number;
  framesDropped: number;
  decodeTimeMs: number;
  decodeFps: number;
  prevSample: {
    framesReceived: number;
    framesDecoded: number;
    framesDropped: number;
  } | null;
}

export interface DecoderPressureSignal {
  active: boolean;
  reason: string;
  backlogFrames: number;
  dropRatePercent: number;
}

export type DecoderRecoveryAction =
  | "none"
  | "sender_keyframe"
  | "control_channel_keyframe"
  | "signaling_keyframe";

export interface DecoderPressureState {
  active: boolean;
  recoveryAttempts: number;
  recoveryAction: DecoderRecoveryAction;
}

interface DecoderPressureControllerDependencies {
  log: (message: string) => void;
  getPeerConnection: () => RTCPeerConnection | null;
  getControlChannel: () => RTCDataChannel | null;
  requestSignalingKeyframe: (request: {
    reason: string;
    backlogFrames: number;
    attempt: number;
  }) => Promise<unknown>;
  onStateChange: (state: DecoderPressureState) => void;
  now?: () => number;
}

const PRESSURE_VIDEO_JITTER_TARGET_MS = 30;
const PRESSURE_AUDIO_JITTER_TARGET_MS = 32;
// User-selectable normal-playback jitter buffer floors (video/audio ms).
// libwebrtc's fully-adaptive buffer shrinks toward a few ms whenever the link
// looks clean, leaving zero headroom for the next jitter spike — which is
// exactly the "occasional lag with healthy ping" on long international links
// (e.g. India → Indonesia). An explicit floor delays frames instead of dropping
// them, and it grows with the measured RTT so NACK retransmissions (one full
// RTT) still fit inside.
//   low      → minimal buffering, lowest latency, jitter-sensitive
//   balanced → ~2 frames of headroom, good default
//   smooth   → large headroom that absorbs jitter spikes at the cost of latency
export const JITTER_BUFFER_PRESETS: Record<JitterBufferMode, {
  video: number;
  audio: number;
}> = {
  low: { video: 20, audio: 35 },
  balanced: { video: 35, audio: 50 },
  smooth: { video: 70, audio: 100 },
};

const MAX_VIDEO_JITTER_TARGET_MS = 100;
const VIDEO_JITTER_FLOOR_FROM_RTT_FACTOR = 0.5;
// Only re-apply receiver tuning when the floor moves by at least this much, so
// oscillating RTT (43↔45ms) does not churn the jitter buffer target every poll.
const JITTER_FLOOR_DEADBAND_MS = 5;
const PRESSURE_CONSECUTIVE_POLLS = 3;
const STABLE_CONSECUTIVE_POLLS = 6;
const RECOVERY_COOLDOWN_MS = 1500;
const KEYFRAME_COOLDOWN_MS = 1200;
export const DECODER_MIN_RECOVERY_BITRATE_KBPS = 4000;

export function classifyDecoderPressureSample(
  params: DecoderPressureSample,
): DecoderPressureSignal {
  const backlogFrames = Math.max(0, params.framesReceived - params.framesDecoded);
  const dropRatePercent = params.framesReceived > 0
    ? (params.framesDropped / params.framesReceived) * 100
    : 0;
  const severeStall = params.framesReceived > 120 && params.framesDecoded === 0;
  const backlogHigh = backlogFrames >= 45;
  const dropRateHigh = dropRatePercent >= 6;

  let dropBurst = false;
  if (params.prevSample) {
    const decodedDelta = params.framesDecoded - params.prevSample.framesDecoded;
    const droppedDelta = params.framesDropped - params.prevSample.framesDropped;
    dropBurst = droppedDelta >= 8 && decodedDelta <= 4;
  }

  let decodeSaturated = false;
  if (params.decodeFps > 0 && params.decodeTimeMs > 0) {
    const frameBudgetMs = 1000 / params.decodeFps;
    decodeSaturated = params.decodeTimeMs >= frameBudgetMs * 0.82;
  }

  if (severeStall) {
    return {
      active: true,
      reason: "severe_stall",
      backlogFrames,
      dropRatePercent,
    };
  }

  // A sudden burst of dropped frames (while the decoder otherwise keeps up) is
  // the signature of a jitter spike overrunning the buffer. The picture stays
  // frozen until the next natural keyframe, which reads as "sometimes lags".
  // Flag it so recovery requests a keyframe immediately instead of waiting.
  if (dropBurst) {
    return {
      active: true,
      reason: "drop_burst",
      backlogFrames,
      dropRatePercent,
    };
  }

  const active = backlogHigh && (dropRateHigh || decodeSaturated);
  return {
    active,
    reason: active ? "backlog_and_drop" : "stable",
    backlogFrames,
    dropRatePercent,
  };
}

export class DecoderPressureController {
  private pressureActive = false;
  private pressureConsecutivePolls = 0;
  private stableConsecutivePolls = 0;
  private recoveryAttemptCount = 0;
  private lastRecoveryAtMs = 0;
  private lastKeyframeRequestAtMs = 0;
  private negotiatedMaxBitrateKbps = 0;
  private recoveryAction: DecoderRecoveryAction = "none";
  private lowLatencyTargetsActive = false;
  private readonly receiverLatencyTargets: Record<"video" | "audio", number | null> = {
    video: null,
    audio: null,
  };
  private jitterBufferMode: JitterBufferMode = "balanced";
  /** Base floors from the active preset; the RTT-adaptive floor grows on top. */
  private presetJitterTargets: Record<"video" | "audio", number> = {
    ...JITTER_BUFFER_PRESETS.balanced,
  };
  private baseJitterTargets: Record<"video" | "audio", number> = {
    ...JITTER_BUFFER_PRESETS.balanced,
  };
  private activeReceivers: Array<{
    receiver: RTCRtpReceiver;
    kind: "audio" | "video";
  }> = [];

  constructor(private readonly dependencies: DecoderPressureControllerDependencies) {}

  get targetBitrateKbps(): number {
    return this.negotiatedMaxBitrateKbps;
  }

  initializeBitrate(maxBitrateKbps: number): void {
    this.negotiatedMaxBitrateKbps = Math.max(
      DECODER_MIN_RECOVERY_BITRATE_KBPS,
      Math.floor(maxBitrateKbps),
    );
  }

  classifySample(sample: DecoderPressureSample): DecoderPressureSignal {
    return classifyDecoderPressureSample(sample);
  }

  configureReceiver(receiver: RTCRtpReceiver, kind: string): void {
    if (kind !== "video" && kind !== "audio") {
      return;
    }
    if (!this.activeReceivers.some((entry) => entry.receiver === receiver)) {
      this.activeReceivers.push({ receiver, kind });
    }

    try {
      const targetMs = this.effectiveTargetMs(kind);
      const rawReceiver = receiver as unknown as Record<string, unknown>;
      if ("jitterBufferTarget" in receiver) {
        rawReceiver.jitterBufferTarget = targetMs;
        this.dependencies.log(
          `${kind} receiver: jitterBufferTarget ${targetMs}ms`,
        );
      }
      if ("playoutDelayHint" in receiver) {
        const playoutDelaySeconds = targetMs / 1000;
        rawReceiver.playoutDelayHint = playoutDelaySeconds;
        this.dependencies.log(
          `${kind} receiver: playoutDelayHint ${playoutDelaySeconds}s`,
        );
      }
      if (kind === "video" && "contentHint" in receiver.track) {
        receiver.track.contentHint = "motion";
      }
    } catch (error) {
      this.dependencies.log(
        `Warning: could not apply ${kind} low-latency receiver tuning: ${String(error)}`,
      );
    }
  }

  /**
   * Switch the normal-playback jitter buffer preset. Re-applies the receiver
   * targets immediately so the change takes effect mid-stream, and resets the
   * RTT-adaptive floor back to the new preset's base (it grows again on the
   * next stats poll). No-op when the mode is unchanged.
   */
  setJitterBufferMode(mode: JitterBufferMode): void {
    const normalized = normalizeJitterBufferMode(mode);
    if (normalized === this.jitterBufferMode) {
      return;
    }
    this.jitterBufferMode = normalized;
    this.presetJitterTargets = { ...JITTER_BUFFER_PRESETS[normalized] };
    this.baseJitterTargets = { ...this.presetJitterTargets };
    this.dependencies.log(
      `Jitter buffer preset: ${normalized} (video=${this.presetJitterTargets.video}ms audio=${this.presetJitterTargets.audio}ms)`,
    );
    for (const { receiver, kind } of this.activeReceivers) {
      this.configureReceiver(receiver, kind);
    }
  }

  /**
   * Grow the normal-playback jitter buffer floor with the measured RTT so a
   * NACK retransmission (which takes one full RTT) still lands inside the
   * buffer instead of dropping the frame and stuttering. The preset's base
   * floor is the lower bound; the adaptive floor only ever grows from there.
   * Called once per stats poll; no-op when the floor did not change.
   */
  updateJitterFloorFromRtt(rttMs: number): void {
    if (!Number.isFinite(rttMs) || rttMs <= 0) {
      return;
    }
    const videoFloor = Math.round(
      Math.min(
        MAX_VIDEO_JITTER_TARGET_MS,
        Math.max(this.presetJitterTargets.video, rttMs * VIDEO_JITTER_FLOOR_FROM_RTT_FACTOR),
      ),
    );
    const audioFloor = Math.max(this.presetJitterTargets.audio, videoFloor + 15);
    if (
      Math.abs(videoFloor - this.baseJitterTargets.video) < JITTER_FLOOR_DEADBAND_MS
      && Math.abs(audioFloor - this.baseJitterTargets.audio) < JITTER_FLOOR_DEADBAND_MS
    ) {
      return;
    }
    this.baseJitterTargets.video = videoFloor;
    this.baseJitterTargets.audio = audioFloor;
    this.dependencies.log(
      `Jitter buffer floor adapted to link RTT ${rttMs}ms: video=${videoFloor}ms audio=${audioFloor}ms`,
    );
    for (const { receiver, kind } of this.activeReceivers) {
      this.configureReceiver(receiver, kind);
    }
  }

  reset(): void {
    this.pressureActive = false;
    this.pressureConsecutivePolls = 0;
    this.stableConsecutivePolls = 0;
    this.recoveryAttemptCount = 0;
    this.lastRecoveryAtMs = 0;
    this.lastKeyframeRequestAtMs = 0;
    this.negotiatedMaxBitrateKbps = 0;
    this.recoveryAction = "none";
    this.lowLatencyTargetsActive = false;
    this.receiverLatencyTargets.video = null;
    this.receiverLatencyTargets.audio = null;
    this.presetJitterTargets = { ...JITTER_BUFFER_PRESETS[this.jitterBufferMode] };
    this.baseJitterTargets = { ...this.presetJitterTargets };
    this.activeReceivers = [];
    this.emitState();
  }

  async recover(signal: DecoderPressureSignal): Promise<void> {
    if (!signal.active) {
      this.pressureConsecutivePolls = 0;
      this.stableConsecutivePolls++;
      if (this.stableConsecutivePolls >= STABLE_CONSECUTIVE_POLLS) {
        this.recoveryAttemptCount = 0;
        this.recoveryAction = "none";
        this.setPressureMode(false);
        this.emitState();
      }
      return;
    }

    this.stableConsecutivePolls = 0;
    this.pressureConsecutivePolls++;

    // A drop burst is a single-sample event — waiting PRESSURE_CONSECUTIVE_POLLS
    // (≈3s) would leave the picture frozen for seconds. React immediately; the
    // keyframe cooldown still prevents spamming. severe_stall keeps the debounce
    // (it already implies ~2s of received-but-undecoded frames).
    const urgent = signal.reason === "drop_burst";
    if (!urgent && this.pressureConsecutivePolls < PRESSURE_CONSECUTIVE_POLLS) {
      return;
    }

    this.setPressureMode(true, signal.reason === "severe_stall");
    const now = this.dependencies.now?.() ?? performance.now();
    if (now - this.lastRecoveryAtMs < RECOVERY_COOLDOWN_MS) {
      return;
    }

    // Backlog/drop pressure without a hard decode stall is usually a transient
    // decode spike or a short network burst. Interrupting the stream with
    // keyframes (and the old bitrate step-downs via local-SDP rewrites) just
    // amplified the lag it was meant to fix — matching the "unexplained lag
    // with healthy ping" reports. Only a hard decode stall (frames received,
    // zero decoded) or an actual drop burst (picture visibly froze) justifies
    // a keyframe.
    if (signal.reason !== "severe_stall" && signal.reason !== "drop_burst") {
      return;
    }

    const keyframeRequested = await this.requestKeyframe(
      signal.backlogFrames,
      signal.reason,
    );
    if (keyframeRequested) {
      this.recoveryAttemptCount++;
      this.lastRecoveryAtMs = now;
      this.emitState();
    }
  }

  private emitState(): void {
    this.dependencies.onStateChange({
      active: this.pressureActive,
      recoveryAttempts: this.recoveryAttemptCount,
      recoveryAction: this.recoveryAction,
    });
  }

  private effectiveTargetMs(kind: "video" | "audio"): number {
    return this.receiverLatencyTargets[kind] ?? this.baseJitterTargets[kind];
  }

  private setPressureMode(active: boolean, useLowLatencyTargets = false): void {
    const targetsActive = active && useLowLatencyTargets;
    if (
      this.pressureActive === active
      && this.lowLatencyTargetsActive === targetsActive
    ) {
      return;
    }
    this.pressureActive = active;
    this.lowLatencyTargetsActive = targetsActive;
    // Only a hard decode stall pins an explicit low-latency jitter target.
    // Backlog/drop pressure without a stall keeps the normal jitter floor so
    // transient jitter is absorbed instead of turning into frame drops.
    this.receiverLatencyTargets.video = targetsActive
      ? PRESSURE_VIDEO_JITTER_TARGET_MS
      : null;
    this.receiverLatencyTargets.audio = targetsActive
      ? PRESSURE_AUDIO_JITTER_TARGET_MS
      : null;
    this.dependencies.log(
      `Decoder pressure mode ${active ? "enabled" : "cleared"}; receiver targets video=${this.effectiveTargetMs("video")}ms audio=${this.effectiveTargetMs("audio")}ms`,
    );
    for (const { receiver, kind } of this.activeReceivers) {
      this.configureReceiver(receiver, kind);
    }
    this.emitState();
  }

  private async requestKeyframe(
    backlogFrames: number,
    reason: string,
  ): Promise<boolean> {
    const now = this.dependencies.now?.() ?? performance.now();
    if (now - this.lastKeyframeRequestAtMs < KEYFRAME_COOLDOWN_MS) {
      return false;
    }

    let requested = false;
    const pc = this.dependencies.getPeerConnection();
    if (pc) {
      for (const sender of pc.getSenders()) {
        if (sender.track?.kind !== "video") {
          continue;
        }
        const senderWithKeyframe = sender as RTCRtpSender & {
          requestKeyFrame?: () => Promise<void>;
        };
        if (typeof senderWithKeyframe.requestKeyFrame !== "function") {
          continue;
        }
        try {
          await senderWithKeyframe.requestKeyFrame();
          requested = true;
        } catch (error) {
          this.dependencies.log(
            `requestKeyFrame failed on sender (non-fatal): ${String(error)}`,
          );
        }
      }
    }

    const attempt = this.recoveryAttemptCount + 1;
    const controlChannel = this.dependencies.getControlChannel();
    if (!requested && controlChannel?.readyState === "open") {
      try {
        controlChannel.send(JSON.stringify({
          type: "request_keyframe",
          reason,
          backlogFrames,
          attempt,
        }));
        requested = true;
        this.recoveryAction = "control_channel_keyframe";
      } catch (error) {
        this.dependencies.log(
          `control_channel keyframe request failed (non-fatal): ${String(error)}`,
        );
      }
    }

    if (!requested) {
      try {
        await this.dependencies.requestSignalingKeyframe({
          reason,
          backlogFrames,
          attempt,
        });
        requested = true;
        this.recoveryAction = "signaling_keyframe";
      } catch (error) {
        this.dependencies.log(
          `signaling keyframe request failed (non-fatal): ${String(error)}`,
        );
      }
    }

    if (!requested) {
      return false;
    }
    this.lastKeyframeRequestAtMs = now;
    if (this.recoveryAction === "none") {
      this.recoveryAction = "sender_keyframe";
    }
    this.dependencies.log(
      `Decoder recovery: keyframe requested (reason=${reason}, backlog=${backlogFrames}, attempt=${attempt})`,
    );
    return true;
  }

}
