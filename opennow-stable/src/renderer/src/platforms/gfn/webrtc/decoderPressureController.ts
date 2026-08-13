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
  /** Video element the stream renders into, used by the fast freeze watchdog. */
  getVideoElement?: () => HTMLVideoElement | null;
  /**
   * Optional: whether the decoder is still producing decoded frames right now
   * (e.g. framesDecoded advanced in the latest getStats poll). The fast
   * freeze watchdog only sees PRESENTATION gaps; a stuck present queue with a
   * live decoder is a compositor/display issue that a keyframe cannot fix
   * (it only adds decode load), so the watchdog skips the keyframe then.
   */
  isDecoderProgressing?: () => boolean;
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
// Port of the native streamer's pre-decode jitter depth logic (Rust):
//   - an RTT spike (raw sample far above the RTT EMA) means a jitter burst is
//     in flight RIGHT NOW, so the floor is pinned at MAX for this long to
//     absorb the burst and the following ones (spikes arrive in clusters);
//   - packet loss is the early indicator of jitter — it spikes before RTT
//     climbs — so ≥0.1% raises the floor to MID and ≥0.5% pins it at MAX.
const JITTER_BURST_HOLD_MS = 4000;
const JITTER_LOSS_FLOOR_MID_MS = 70;
const RTT_SPIKE_EMA_FACTOR = 1.5;
const RTT_SPIKE_MIN_DELTA_MS = 30;
// EMA blend for spike detection (75% history, 25% latest) — matches the native
// streamer so raw per-poll RTT bounces don't false-trigger the burst hold.
const RTT_EMA_HISTORY_WEIGHT = 3;
const PRESSURE_CONSECUTIVE_POLLS = 3;
const STABLE_CONSECUTIVE_POLLS = 6;
const RECOVERY_COOLDOWN_MS = 1500;
const KEYFRAME_COOLDOWN_MS = 1200;
export const DECODER_MIN_RECOVERY_BITRATE_KBPS = 4000;

// Fast freeze detection (requestVideoFrameCallback): if the video element
// presents no new frame within this window while supposedly playing, the
// decode pipeline is frozen — the same situation the getStats path calls
// severe_stall, but caught ~10x faster (the stats path needs a 1s poll plus
// PRESSURE_CONSECUTIVE_POLLS consecutive polls before acting). 600ms is ~36
// frames at 60fps: a delivery hiccup never pauses presentation that long, but
// a stalled decoder is flagged well within a second.
const FREEZE_DETECT_TIMEOUT_MS = 600;
// A single no-present window can be a transient decode hiccup (heavy keyframe
// decode on a slower machine, a momentary compositor stall) that recovers by
// itself. Escalating to a keyframe on the FIRST window turned every transient
// gap into a keyframe request — and the keyframe decode itself is heavy, so
// the watchdog re-armed and produced the "stutter every few seconds" loop
// reported on slower hardware. Require the freeze to persist across two
// consecutive windows (~1.2s) before interrupting the stream.
const FREEZE_CONSECUTIVE_STRIKES = 2;
// After a freeze-triggered keyframe, do not re-trigger for this long even if
// the picture is still frozen — the keyframe needs time to arrive and render
// before another request helps.
const FREEZE_RETRIGGER_COOLDOWN_MS = 2500;
// Sustained network loss high enough that NACK recovery is visibly failing:
// the reference frames are corrupted and the picture would stay broken until
// the next natural keyframe, so request one now (PLI) to resync. Threshold is
// in percent and requires LOSS_PLI_CONSECUTIVE_POLLS consecutive polls
// (~2s at a 1s poll) so a single lossy second does not interrupt the stream.
const LOSS_PLI_THRESHOLD_PERCENT = 2;
const LOSS_PLI_CONSECUTIVE_POLLS = 2;

// Auto low-latency jitter mode: on a clean link (RTT below the entry
// threshold and no packet loss for AUTO_LOW_CLEAN_POLLS consecutive polls)
// the base floor drops to the "low" preset — the balanced/smooth floors only
// add latency when there is jitter to absorb. Exits immediately on any
// degradation signal (packet loss, a burst hold, or RTT climbing past the
// exit threshold). Entry and exit RTT thresholds are separated (hysteresis)
// so a borderline link cannot flap the floors back and forth.
const AUTO_LOW_RTT_ENTER_MAX_MS = 40;
const AUTO_LOW_RTT_EXIT_MIN_MS = 50;
const AUTO_LOW_MAX_LOSS_PERCENT = 0.1;
const AUTO_LOW_CLEAN_POLLS = 6;

export interface VideoFreezeEligibilityParams {
  /** Timestamp (ms) of the last presented video frame; 0 = none seen yet. */
  lastFrameAtMs: number;
  /** Current time (ms), same time origin as the frame timestamps. */
  nowMs: number;
  /** Freeze threshold in ms. */
  timeoutMs: number;
  paused: boolean;
  hidden: boolean;
  /** HTMLMediaElement.readyState (>= HAVE_CURRENT_DATA means frames exist). */
  readyState: number;
}

/**
 * Whether the video pipeline qualifies as frozen: a frame was seen, the
 * element is playing (not paused / tab hidden), and no new frame was
 * presented within the timeout window. Pure so it's unit-testable.
 */
export function isVideoFreezeEligible(params: VideoFreezeEligibilityParams): boolean {
  return !params.paused
    && !params.hidden
    && params.readyState >= 2 // HTMLMediaElement.HAVE_CURRENT_DATA
    && params.lastFrameAtMs > 0
    && params.nowMs - params.lastFrameAtMs >= params.timeoutMs;
}

export interface FreezeWatchdogEscalationParams {
  /** Consecutive FREEZE_DETECT_TIMEOUT_MS windows with no presented frame. */
  consecutiveStrikes: number;
  /** Decoder still producing decoded frames (present gap, not a decode stall). */
  decoderProgressing: boolean;
  /** Retrigger cooldown since the last keyframe has elapsed. */
  retriggerCooldownElapsed: boolean;
}

/**
 * Whether the fast freeze watchdog should escalate to a keyframe: the freeze
 * must persist across two consecutive no-present windows (a single window is
 * a transient decode/compositor hiccup that self-recovers — escalating on the
 * first window turned every gap into a keyframe request and caused the
 * "stutter every few seconds" loop on slower machines), the decoder must NOT
 * still be progressing (a present gap with a live decoder is a compositor
 * issue a keyframe cannot fix), and the retrigger cooldown must have elapsed.
 * Pure so it's unit-testable.
 */
export function shouldEscalateFreezeWatchdog(params: FreezeWatchdogEscalationParams): boolean {
  return params.consecutiveStrikes >= FREEZE_CONSECUTIVE_STRIKES
    && !params.decoderProgressing
    && params.retriggerCooldownElapsed;
}

/**
 * Whether sustained network loss justifies a keyframe: the loss is above the
 * PLI threshold AND it has held for the required consecutive polls (so a
 * single lossy poll cannot interrupt the stream). Pure so it's testable.
 */
export function shouldRequestLossKeyframe(
  packetLossPercent: number,
  consecutiveLossPolls: number,
): boolean {
  return packetLossPercent >= LOSS_PLI_THRESHOLD_PERCENT
    && consecutiveLossPolls >= LOSS_PLI_CONSECUTIVE_POLLS;
}

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
  /** EMA of the link RTT used for spike detection (75% history, 25% latest). */
  private rttEmaMs = 0;
  /** Timestamp until which a detected RTT spike pins the floor at MAX. */
  private burstHoldUntilMs = 0;
  /**
   * Whether the base jitter floor is currently dropped to the "low" preset
   * because the link has been clean (low RTT, no loss) for several polls.
   * The user's preset stays the fallback and the RTT/loss adaptive floor
   * still grows on top of the low base, so a degradation mid-session is
   * still absorbed instead of dropping frames.
   */
  private autoLowActive = false;
  /** Consecutive clean polls (low RTT + no loss) feeding auto-low activation. */
  private cleanPollStreak = 0;
  /** Fast-freeze watchdog (requestVideoFrameCallback) state. */
  private freezeMonitoring = false;
  private freezeWatchdogTimer: number | null = null;
  private freezeLastFrameAtMs = 0;
  private freezeTriggeredAtMs = 0;
  /**
   * Consecutive FREEZE_DETECT_TIMEOUT_MS windows without a presented frame.
   * A single window is a transient hiccup; only two consecutive windows
   * (with a stalled decoder) escalate to a keyframe.
   */
  private freezeConsecutiveStrikes = 0;
  /** Consecutive polls whose loss stayed above the PLI threshold. */
  private lossConsecutivePolls = 0;
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
   * next stats poll). Also clears any active auto-low state — the new preset
   * needs a fresh clean-link streak to drop back down. No-op when the mode is
   * unchanged.
   */
  setJitterBufferMode(mode: JitterBufferMode): void {
    const normalized = normalizeJitterBufferMode(mode);
    if (normalized === this.jitterBufferMode) {
      return;
    }
    this.jitterBufferMode = normalized;
    this.autoLowActive = false;
    this.cleanPollStreak = 0;
    this.presetJitterTargets = { ...JITTER_BUFFER_PRESETS[normalized] };
    this.baseJitterTargets = { ...this.presetJitterTargets };
    this.dependencies.log(
      `Jitter buffer preset: ${normalized} (video=${this.presetJitterTargets.video}ms audio=${this.presetJitterTargets.audio}ms)`,
    );
    for (const { receiver, kind } of this.activeReceivers) {
      this.configureReceiver(receiver, kind);
    }
  }

  /** Whether auto low-latency mode is currently active (link has been clean). */
  get isAutoLowActive(): boolean {
    return this.autoLowActive;
  }

  /**
   * Grow the normal-playback jitter buffer floor with the measured RTT so a
   * NACK retransmission (which takes one full RTT) still lands inside the
   * buffer instead of dropping the frame and stuttering. The preset's base
   * floor is the lower bound; the adaptive floor only ever grows from there.
   *
   * This is the WebRTC port of the native streamer's pre-decode jitter depth
   * logic: a raw RTT spike far above the RTT EMA pins the floor at MAX for
   * JITTER_BURST_HOLD_MS (the burst in flight is absorbed instead of the
   * picture blinking the previous frame), and packet loss raises the floor
   * early (≥0.1% → MID, ≥0.5% → MAX) because loss spikes before RTT climbs.
   * Called once per stats poll; no-op when the floor did not change.
   */
  updateJitterFloorFromRtt(rttMs: number, packetLossPercent?: number): void {
    if (!Number.isFinite(rttMs) || rttMs <= 0) {
      return;
    }

    // EMA (75% history, 25% latest) — the stats-channel RTT is a raw
    // per-sample value that can bounce between polls; the EMA gives the
    // spike-detection baseline and the band-switch hysteresis.
    const ema = this.rttEmaMs === 0
      ? Math.round(rttMs)
      : Math.round((this.rttEmaMs * RTT_EMA_HISTORY_WEIGHT + rttMs) / (RTT_EMA_HISTORY_WEIGHT + 1));
    this.rttEmaMs = ema;

    // Spike detection: a RAW sample far above the EMA means a jitter burst is
    // in flight RIGHT NOW. The EMA would need ~2-4 samples to climb, during
    // which the buffer starves and the picture freezes. Pin MAX and HOLD it
    // for JITTER_BURST_HOLD_MS so the burst in flight is absorbed and the
    // following bursts (spikes come in clusters) never leak through.
    const now = this.dependencies.now?.() ?? performance.now();
    const spike = rttMs > Math.floor(ema * RTT_SPIKE_EMA_FACTOR)
      && rttMs - ema >= RTT_SPIKE_MIN_DELTA_MS;
    if (spike) {
      this.burstHoldUntilMs = now + JITTER_BURST_HOLD_MS;
    }
    const burstHold = now < this.burstHoldUntilMs;

    // Packet-loss floor: loss is the early indicator of jitter — it spikes
    // before RTT climbs, so it must raise the floor immediately, not after
    // the EMA catches up. Percent → fraction to match the native thresholds.
    const lossFraction = Number.isFinite(packetLossPercent)
      ? Math.max(0, (packetLossPercent ?? 0) / 100)
      : 0;
    const lossPercent = lossFraction * 100;
    const lossFloor = lossFraction >= 0.005
      ? MAX_VIDEO_JITTER_TARGET_MS
      : lossFraction >= 0.001
        ? JITTER_LOSS_FLOOR_MID_MS
        : 0;

    // ── Auto low-latency mode ──
    // On a clean link (RTT below AUTO_LOW_RTT_ENTER_MAX_MS and no packet loss
    // for AUTO_LOW_CLEAN_POLLS consecutive polls) the base floor drops to the
    // "low" preset — the balanced/smooth floors only add latency when there
    // is jitter to absorb. While active, any degradation signal (packet loss,
    // a burst hold, or RTT climbing past AUTO_LOW_RTT_EXIT_MIN_MS) reverts
    // immediately to the user's preset; the adaptive floors below still guard
    // the transition. Never engages when the user already picked "low"
    // (nothing to gain).
    const clean = !burstHold
      && rttMs < AUTO_LOW_RTT_ENTER_MAX_MS
      && lossPercent < AUTO_LOW_MAX_LOSS_PERCENT;
    this.cleanPollStreak = clean ? this.cleanPollStreak + 1 : 0;
    let autoLowActive: boolean;
    if (this.autoLowActive) {
      autoLowActive = !(
        burstHold
        || lossPercent >= AUTO_LOW_MAX_LOSS_PERCENT
        || rttMs >= AUTO_LOW_RTT_EXIT_MIN_MS
      );
      if (!autoLowActive) {
        this.cleanPollStreak = 0;
      }
    } else {
      autoLowActive = this.jitterBufferMode !== "low" && this.cleanPollStreak >= AUTO_LOW_CLEAN_POLLS;
    }
    if (autoLowActive !== this.autoLowActive) {
      this.autoLowActive = autoLowActive;
      if (autoLowActive) {
        this.dependencies.log(
          `Auto jitter buffer: clean link (rtt=${rttMs}ms, loss=${lossPercent.toFixed(2)}%) — switching to low-latency floors (video=${JITTER_BUFFER_PRESETS.low.video}ms audio=${JITTER_BUFFER_PRESETS.low.audio}ms)`,
        );
      } else {
        this.dependencies.log(
          `Auto jitter buffer: link degraded (rtt=${rttMs}ms, loss=${lossPercent.toFixed(2)}%) — restored to "${this.jitterBufferMode}" floors (video=${this.presetJitterTargets.video}ms audio=${this.presetJitterTargets.audio}ms)`,
        );
      }
    }
    // The floor's preset base: "low" while auto-low is active, otherwise the
    // user's preset. The deadband below still applies the switch immediately
    // because the presets differ by well over JITTER_FLOOR_DEADBAND_MS.
    const preset = this.autoLowActive ? JITTER_BUFFER_PRESETS.low : this.presetJitterTargets;

    const rttFloor = Math.round(
      Math.min(
        MAX_VIDEO_JITTER_TARGET_MS,
        Math.max(preset.video, rttMs * VIDEO_JITTER_FLOOR_FROM_RTT_FACTOR),
      ),
    );
    const videoFloor = burstHold
      ? MAX_VIDEO_JITTER_TARGET_MS
      : Math.max(preset.video, rttFloor, lossFloor);
    const audioFloor = Math.max(preset.audio, videoFloor + 15);
    if (
      Math.abs(videoFloor - this.baseJitterTargets.video) < JITTER_FLOOR_DEADBAND_MS
      && Math.abs(audioFloor - this.baseJitterTargets.audio) < JITTER_FLOOR_DEADBAND_MS
    ) {
      return;
    }
    this.baseJitterTargets.video = videoFloor;
    this.baseJitterTargets.audio = audioFloor;
    const lossNote = lossFraction >= 0.001 || burstHold
      ? ` loss=${lossFraction.toFixed(3)}${burstHold ? " burstHold" : ""}`
      : "";
    this.dependencies.log(
      `Jitter buffer floor adapted to link RTT ${rttMs}ms: video=${videoFloor}ms audio=${audioFloor}ms${lossNote}`,
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
    this.rttEmaMs = 0;
    this.burstHoldUntilMs = 0;
    this.autoLowActive = false;
    this.cleanPollStreak = 0;
    this.activeReceivers = [];
    this.stopFreezeMonitoring();
    this.lossConsecutivePolls = 0;
    this.freezeTriggeredAtMs = 0;
    this.freezeConsecutiveStrikes = 0;
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
    // (it already implies ~2s of received-but-undecoded frames). video_freeze
    // (the fast watchdog) is likewise urgent — it already waited a full
    // FREEZE_DETECT_TIMEOUT_MS with no presented frame.
    const urgent = signal.reason === "drop_burst" || signal.reason === "video_freeze";
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
    // zero decoded), an actual drop burst (picture visibly froze), or the
    // fast freeze watchdog (no frame presented for FREEZE_DETECT_TIMEOUT_MS)
    // justifies a keyframe.
    if (
      signal.reason !== "severe_stall"
      && signal.reason !== "drop_burst"
      && signal.reason !== "video_freeze"
    ) {
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

  /**
   * Network-loss-triggered keyframe (PLI path): when the locally measured
   * packet loss stays above LOSS_PLI_THRESHOLD_PERCENT for consecutive polls,
   * NACK retransmission is visibly failing and the reference frames are
   * corrupted — the picture would stay broken until the next natural
   * keyframe. Request one immediately (shared keyframe cooldown) so the
   * stream resyncs. Single lossy polls are ignored so transient loss never
   * interrupts the stream.
   */
  reportPacketLoss(packetLossPercent: number): void {
    const loss = Number.isFinite(packetLossPercent) ? Math.max(0, packetLossPercent) : 0;
    this.lossConsecutivePolls = loss >= LOSS_PLI_THRESHOLD_PERCENT
      ? this.lossConsecutivePolls + 1
      : 0;
    if (!shouldRequestLossKeyframe(loss, this.lossConsecutivePolls)) {
      return;
    }
    void this.requestKeyframe(0, "network_loss").then((requested) => {
      if (requested) {
        this.recoveryAttemptCount += 1;
        this.emitState();
      }
    });
  }

  /**
   * Watch the video element for a frozen decode pipeline via
   * requestVideoFrameCallback: while frames are presented the callback fires
   * continuously and keeps pushing the watchdog back; if no new frame is
   * presented within FREEZE_DETECT_TIMEOUT_MS the watchdog fires and triggers
   * an immediate keyframe recovery — ~10x faster than the getStats-based
   * severe_stall path (1s poll + consecutive-poll debounce). This mirrors the
   * native streamer's fast watchdog on the renderer side. Call once when the
   * session starts; stopFreezeMonitoring() clears it on teardown.
   */
  startFreezeMonitoring(): void {
    const video = this.dependencies.getVideoElement?.();
    if (!video || this.freezeMonitoring) {
      return;
    }
    this.freezeMonitoring = true;
    this.freezeLastFrameAtMs = 0;
    this.freezeWatchdogTimer = null;
    this.freezeTriggeredAtMs = 0;
    this.armFreezeWatchdog(video);
  }

  stopFreezeMonitoring(): void {
    this.freezeMonitoring = false;
    if (this.freezeWatchdogTimer !== null) {
      window.clearTimeout(this.freezeWatchdogTimer);
      this.freezeWatchdogTimer = null;
    }
    this.freezeLastFrameAtMs = 0;
    this.freezeConsecutiveStrikes = 0;
  }

  private armFreezeWatchdog(video: HTMLVideoElement): void {
    if (!this.freezeMonitoring) {
      return;
    }
    if (this.freezeWatchdogTimer !== null) {
      window.clearTimeout(this.freezeWatchdogTimer);
    }
    this.freezeWatchdogTimer = window.setTimeout(() => {
      this.freezeWatchdogTimer = null;
      this.onFreezeWatchdog(video);
    }, FREEZE_DETECT_TIMEOUT_MS);
    if (typeof video.requestVideoFrameCallback === "function") {
      video.requestVideoFrameCallback((now) => this.onVideoFramePresented(video, now));
    }
  }

  private onVideoFramePresented(video: HTMLVideoElement, now: number): void {
    if (!this.freezeMonitoring) {
      return;
    }
    this.freezeLastFrameAtMs = now;
    // A presented frame breaks any freeze streak — the pipeline is alive.
    this.freezeConsecutiveStrikes = 0;
    this.armFreezeWatchdog(video);
  }

  private onFreezeWatchdog(video: HTMLVideoElement): void {
    if (!this.freezeMonitoring) {
      return;
    }
    const now = this.dependencies.now?.() ?? performance.now();
    const eligible = isVideoFreezeEligible({
      lastFrameAtMs: this.freezeLastFrameAtMs,
      nowMs: now,
      timeoutMs: FREEZE_DETECT_TIMEOUT_MS,
      paused: video.paused,
      hidden: typeof document !== "undefined" && document.hidden,
      readyState: video.readyState,
    });
    if (!eligible) {
      // Not playing/visible anymore (or a frame appeared) — reset the streak.
      this.freezeConsecutiveStrikes = 0;
      this.armFreezeWatchdog(video);
      return;
    }

    this.freezeConsecutiveStrikes += 1;
    const decoderProgressing = this.dependencies.isDecoderProgressing?.() ?? false;
    const retriggerElapsed = now - this.freezeTriggeredAtMs >= FREEZE_RETRIGGER_COOLDOWN_MS;
    if (shouldEscalateFreezeWatchdog({
      consecutiveStrikes: this.freezeConsecutiveStrikes,
      decoderProgressing,
      retriggerCooldownElapsed: retriggerElapsed,
    })) {
      this.freezeTriggeredAtMs = now;
      this.freezeConsecutiveStrikes = 0;
      this.dependencies.log(
        `Freeze watchdog: ${FREEZE_CONSECUTIVE_STRIKES} consecutive ${FREEZE_DETECT_TIMEOUT_MS}ms windows with no presented frame and a stalled decoder — requesting keyframe`,
      );
      void this.recover({
        active: true,
        reason: "video_freeze",
        backlogFrames: 0,
        dropRatePercent: 0,
      });
    } else if (this.freezeConsecutiveStrikes >= FREEZE_CONSECUTIVE_STRIKES && decoderProgressing) {
      // Present gap but the decoder is still decoding — a present/compositor
      // lag, not a decode stall. A keyframe cannot fix it and only adds
      // decode load, so hold without interrupting the stream.
      this.freezeConsecutiveStrikes = 0;
      this.dependencies.log(
        "Freeze watchdog: presentation gap with decoder still progressing (present/compositor lag) — holding without a keyframe",
      );
    } else {
      this.dependencies.log(
        `Freeze watchdog: ${this.freezeConsecutiveStrikes}/${FREEZE_CONSECUTIVE_STRIKES} no-present window${this.freezeConsecutiveStrikes === 1 ? " (transient, holding)" : ""}`,
      );
    }
    // Keep watching — the picture may recover before the next timeout.
    this.armFreezeWatchdog(video);
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
