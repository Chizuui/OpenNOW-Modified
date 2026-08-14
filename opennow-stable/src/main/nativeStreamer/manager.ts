import { app } from "electron";
import { randomUUID } from "node:crypto";
import { tmpdir } from "node:os";
import { spawn, type ChildProcessWithoutNullStreams } from "node:child_process";

import {
  answerHasVideoCodec,
  createUnsupportedNativeStreamerStatus,
  extractNegotiatedVideoCodec,
  isNativeStreamerSupportedPlatform,
  NATIVE_STREAMER_WINDOWS_ONLY_MESSAGE,
  type IceCandidatePayload,
  type KeyframeRequest,
  type MainToRendererSignalingEvent,
  type NativeStreamerBackendPreference,
  type NativeStreamerFeatureMode,
  type NativeVideoBackendPreference,
  type NativeStreamerStatus,
  type NativeGstreamerRuntimeStatus,
  type NativeRenderSurface,
  type NativeStreamerSessionContext,
  type SendAnswerRequest,
} from "@shared/gfn";
import {
  NATIVE_STREAMER_PROTOCOL_VERSION,
  type NativeStreamerCapabilities,
  type NativeStreamerCommand,
  type NativeStreamerEvent,
  type NativeStreamerInputPacket,
  type NativeStreamerMessage,
  type NativeStreamerResponse,
} from "@shared/nativeStreamer";
import type { NativeStreamerShortcutBindings } from "@shared/gfn";
import {
  createNativeStreamerDetectionFailureStatus,
  createNativeStreamerStatus,
  formatError,
} from "./capabilities";
import { appendRecordingChunk } from "../media/recordings";
import { resolveNativeStreamerExecutableCandidates } from "./executableDiscovery";
import {
  isNativeStreamerEvent,
  isNativeStreamerResponse,
  type NativeStreamerCommandInput,
} from "./protocol";
import {
  parseNetworkHealthBitrateMbps,
  stepNativeBitratePushVerification,
  type NativeBitratePushVerification,
} from "./networkHealth";
import { createNativeStreamerRuntimeEnvironment } from "./runtime";
import { NativeSurfaceUpdateQueue } from "./surfaceUpdateQueue";

interface NativeStreamerCallbacks {
  sendAnswer(payload: SendAnswerRequest): Promise<void>;
  sendIceCandidate(candidate: IceCandidatePayload): Promise<void>;
  requestKeyframe(payload: KeyframeRequest): Promise<void>;
  emit(event: MainToRendererSignalingEvent): void;
}

interface NativeStreamerManagerOptions extends NativeStreamerCallbacks {
  mainDir: string;
  getBackendPreference(): NativeStreamerBackendPreference;
  getVideoBackendPreference(): NativeVideoBackendPreference;
  getExecutablePathOverride(): string;
  getCloudGsyncMode(): NativeStreamerFeatureMode;
  getD3dFullscreenMode(): NativeStreamerFeatureMode;
  getExternalRendererEnabled(): boolean;
  getStackedRendererEnabled(): boolean;
}

interface PendingRequest {
  resolve(message: NativeStreamerResponse): void;
  reject(error: Error): void;
  timeout: NodeJS.Timeout;
}

const HELLO_TIMEOUT_MS = 10000;
// PCs with multiple GPUs (Intel iGPU + NVIDIA dGPU + virtual display adapters
// for streaming software like Parsec/GameViewer) can take 60–90 s to finish
// gst::init() plugin scanner because Vulkan/D3D/D3D12 sinks enumerate drivers.
const BUNDLED_GSTREAMER_HELLO_TIMEOUT_MS = process.platform === "win32" ? 90000 : 30000;
const CONTROL_TIMEOUT_MS = 8000;
const SESSION_START_TIMEOUT_MS = process.platform === "win32" ? 90000 : 45000;
const SURFACE_UPDATE_TIMEOUT_MS = 15000;
const OFFER_TIMEOUT_MS = 20000;
const STOP_TIMEOUT_MS = 1200;
const SCREENSHOT_TIMEOUT_MS = 5000;
// Native stop(finalize=true) budget: queue drain (4s) + EOS flush (4s) +
// muxer-direct EOS failsafe (2s) = 10s, plus the OFFLINE pass-through remux
// (45s cap: decode + FULL→LIMITED + H.264 re-encode of a tens-of-seconds
// 1080p60 clip through the software x264 fallback on a weak CPU), so the
// Electron side must not give up first. A dead recording audio branch used
// to leave the muxer waiting forever and the old 5s/20s timeouts fired
// ("stop-recording timed out") mid-remux — errors still resolve fast via
// the recording-stop-failed event, this is only a ceiling.
const RECORDING_STOP_TIMEOUT_MS = 60000;
const DATA_CHANNEL_SEND_TIMEOUT_MS = 3000;
const MAX_INPUT_STDIN_BUFFER_BYTES = 64 * 1024;
const MIN_NATIVE_BITRATE_KBPS = 5_000;
const MAX_NATIVE_BITRATE_KBPS = 150_000;
/**
 * Cooldown between automatic network-triggered profile downgrades. The poor
 * verdict oscillates under jitter, and every downgrade restarts the session
 * (a ~10-20 s disruption) — the GFN pre-stream test equivalent must not fire
 * repeatedly. A session restarts at the lower profile, so one step per
 * cooldown is the safe cadence.
 */
const NETWORK_DOWNGRADE_COOLDOWN_MS = 60_000;

function normalizeBitrateKbps(value: number): number {
  if (!Number.isFinite(value)) {
    return MIN_NATIVE_BITRATE_KBPS;
  }

  return Math.min(
    MAX_NATIVE_BITRATE_KBPS,
    Math.max(MIN_NATIVE_BITRATE_KBPS, Math.round(value)),
  );
}

export class NativeStreamerManager {
  private child: ChildProcessWithoutNullStreams | null = null;
  private startupPromise: Promise<void> | null = null;
  private stdoutBuffer = "";
  private stderrTail: string[] = [];
  private gstreamerRuntime: NativeGstreamerRuntimeStatus | null = null;
  private pending = new Map<string, PendingRequest>();
  private capabilities: NativeStreamerCapabilities | null = null;
  private activeSessionId: string | null = null;
  /** Mid-session NVST bitrate cap updates pushed to the server during the current session. */
  private bitratePushesThisSession = 0;
  /** Of those, how many were followed by the measured [NetworkHealth] bitrate rising by at least the threshold within the verify window. */
  private bitratePushesVerifiedThisSession = 0;
  /** Pending proof that a push took effect; fed by [NetworkHealth] log lines until verified or the window expires. */
  private bitratePushObservation: NativeBitratePushVerification | null = null;
  /** Cooldown so a slider drag does not spam the server with mid-session NVST updates (WebRTC parity). */
  private lastBitratePushAtMs = 0;
  private static readonly BITRATE_PUSH_COOLDOWN_MS = 1_000;
  /** Wall-clock of the last automatic network-triggered profile downgrade (0 = none yet this process). */
  private lastNetworkDowngradeAtMs = 0;
  /** Guards the automatic profile downgrade so it fires at most once per session (the relaunch restarts at the lower profile). */
  private networkDowngradeFiredThisSession = false;
  private inputBackpressureWarned = false;
  private answerInFlight = false;
  /** Captured by `handleEvent` for the in-flight `take-screenshot` request. */
  private pendingScreenshot: { pngBase64: string; width: number; height: number } | null = null;
  /** recordingId of the active native recording; chunks are appended to it. */
  private activeNativeRecordingId: string | null = null;
  /** Resolver for the in-flight `stop-recording` finalization wait. */
  private pendingRecordingFinishedResolve:
    | ((result: { thumbnailBase64?: string; droppedFrames: number }) => void)
    | null = null;
  private pendingRecordingFinishedReject: ((error: Error) => void) | null = null;
  /** Serialize native chunk writes; stdout events can arrive faster than disk writes. */
  private recordingChunkQueue: Promise<void> = Promise.resolve();
  private queuedLocalIce: IceCandidatePayload[] = [];
  private queuedRemoteIceSessionId: string | null = null;
  private queuedRemoteIce: IceCandidatePayload[] = [];
  private readonly surfaceUpdates: NativeSurfaceUpdateQueue;

  constructor(private readonly options: NativeStreamerManagerOptions) {
    this.surfaceUpdates = new NativeSurfaceUpdateQueue(
      (surface) => this.request({ type: "surface", surface }, SURFACE_UPDATE_TIMEOUT_MS).then(() => undefined),
      (error) => console.warn("[NativeStreamer] Failed to update native render surface:", error),
    );
  }

  isRunning(): boolean {
    return this.child !== null;
  }

  hasActiveSession(): boolean {
    return this.activeSessionId !== null;
  }

  async prepareForSession(context: NativeStreamerSessionContext): Promise<void> {
    if (this.activeSessionId && this.activeSessionId !== context.session.sessionId) {
      await this.stop("new native streamer session");
    }
    // A fresh session starts at the negotiated profile; re-arm the automatic
    // network downgrade so the new (already lower) profile can downgrade
    // again if the network is still too weak.
    this.networkDowngradeFiredThisSession = false;
    this.prepareRemoteIceQueue(context.session.sessionId);

    await this.ensureProcess();

    if (this.activeSessionId === context.session.sessionId) {
      return;
    }

    if (context.settings.enableCloudGsync) {
      console.log(
        "[NativeStreamer] Cloud G-Sync/VRR mode resolved for this session; preserving unthrottled low-latency present behavior.",
      );
    }

    await this.request({
      type: "start",
      context,
    }, SESSION_START_TIMEOUT_MS);
    this.activeSessionId = context.session.sessionId;
    await this.flushQueuedRemoteIce(context.session.sessionId);
  }

  async handleOffer(sdp: string, context: NativeStreamerSessionContext): Promise<void> {
    const negotiatedProfile = context.session.negotiatedStreamProfile;
    console.log(
      "[NativeStreamer] Session context:",
      JSON.stringify({
        sessionId: context.session.sessionId,
        requestedResolution: context.settings.resolution,
        requestedFps: context.settings.fps,
        requestedCodec: context.settings.codec,
        negotiatedResolution: negotiatedProfile?.resolution,
        negotiatedFps: negotiatedProfile?.fps,
        negotiatedCodec: negotiatedProfile?.codec ?? context.settings.codec,
        requestedStreamingFeatures: context.session.requestedStreamingFeatures,
        finalizedStreamingFeatures: context.session.finalizedStreamingFeatures,
      }),
    );

    await this.prepareForSession(context);

    if (!this.capabilities?.supportsOfferAnswer) {
      console.warn(
        `[NativeStreamer] Backend "${this.capabilities?.backend ?? "unknown"}" reports offer/answer is not ready; forwarding offer for validation/fallback.`,
      );
    }

    this.answerInFlight = true;
    this.queuedLocalIce = [];

    try {
      const response = await this.request({
        type: "offer",
        sdp,
        context,
      }, OFFER_TIMEOUT_MS);

      if (response.type !== "answer") {
        throw new Error(`Native streamer returned ${response.type} instead of answer.`);
      }

      // Validate the native answer actually negotiated a video codec. A broken
      // answer that dropped the video m-line (port 0 / missing from the BUNDLE
      // group) would hang the session on "Waiting for game video..."; throwing
      // here makes SignalingCoordinator fall back to the web streamer for this
      // session instead.
      if (!answerHasVideoCodec(response.answer.sdp)) {
        throw new Error(
          "Native streamer answer has no video codec (video m-line rejected).",
        );
      }
      const negotiatedCodec = extractNegotiatedVideoCodec(response.answer.sdp);
      console.log(`[NativeStreamer] Answer negotiated video codec: ${negotiatedCodec}`);

      await this.options.sendAnswer(response.answer);
      this.answerInFlight = false;
      await this.flushQueuedLocalIce();
    } catch (error) {
      this.answerInFlight = false;
      this.queuedLocalIce = [];
      throw error;
    }

    this.options.emit({
      type: "log",
      message: "Native streamer accepted the WebRTC offer; waiting for decoded media.",
    });
  }

  async probeStatus(): Promise<NativeStreamerStatus> {
    if (!isNativeStreamerSupportedPlatform(process.platform)) {
      return createUnsupportedNativeStreamerStatus();
    }

    // Reuse cached capabilities when the child process is still alive; spawn + hello only on first probe
    // or after a restart.
    if (this.capabilities && this.child && this.gstreamerRuntime) {
      return createNativeStreamerStatus(
        this.capabilities,
        this.gstreamerRuntime,
        this.options.getVideoBackendPreference(),
        process.platform,
      );
    }

    try {
      await this.ensureProcess();
      return createNativeStreamerStatus(
        this.capabilities,
        this.gstreamerRuntime,
        this.options.getVideoBackendPreference(),
        process.platform,
      );
    } catch (error) {
      return createNativeStreamerDetectionFailureStatus(
        error,
        this.gstreamerRuntime,
        process.platform,
      );
    }
  }

  async addRemoteIce(candidate: IceCandidatePayload, context: NativeStreamerSessionContext): Promise<void> {
    const sessionId = context.session.sessionId;
    if (!this.child || this.activeSessionId !== sessionId) {
      this.queueRemoteIce(sessionId, candidate);
      return;
    }

    await this.sendRemoteIce(candidate);
  }

  drainQueuedRemoteIce(sessionId: string): IceCandidatePayload[] {
    if (this.queuedRemoteIceSessionId !== sessionId) {
      return [];
    }

    const queued = this.queuedRemoteIce;
    this.queuedRemoteIceSessionId = null;
    this.queuedRemoteIce = [];
    return queued;
  }

  sendInput(input: NativeStreamerInputPacket): void {
    const child = this.child;
    if (
      !child
      || child.killed
      || !child.stdin.writable
      || !this.activeSessionId
      || !this.capabilities?.supportsInput
    ) {
      return;
    }

    if (child.stdin.writableLength > MAX_INPUT_STDIN_BUFFER_BYTES) {
      if (!this.inputBackpressureWarned) {
        this.inputBackpressureWarned = true;
        console.warn("[NativeStreamer] Dropping native input while streamer stdin is backpressured.");
      }
      return;
    }

    const payload = {
      id: randomUUID(),
      type: "input",
      input,
    } satisfies NativeStreamerCommand;

    const flushed = child.stdin.write(`${JSON.stringify(payload)}\n`, "utf8", (error) => {
      if (error && !this.inputBackpressureWarned) {
        this.inputBackpressureWarned = true;
        console.warn("[NativeStreamer] Failed to write native input:", error);
      }
    });

    if (!flushed && !this.inputBackpressureWarned) {
      this.inputBackpressureWarned = true;
      console.warn("[NativeStreamer] Native input writer reported backpressure; input will be dropped until it drains.");
      child.stdin.once("drain", () => {
        this.inputBackpressureWarned = false;
      });
    } else if (flushed) {
      this.inputBackpressureWarned = false;
    }
  }

  updateSurface(surface: NativeRenderSurface): void {
    this.surfaceUpdates.update(surface);
  }

  /**
   * Apply a runtime present-limiter pacing mode (the native analogue of GFN's
   * NVST p-f pacing framework control): `auto` | `stream` | `vrr` | `off` |
   * a fixed fps. The native streamer applies it to the live present limiter
   * immediately without rebuilding the pipeline. No-op when no session is
   * running (the mode is per-session and resets with the pipeline).
   */
  setPacingMode(pacingMode: string): void {
    if (!this.child || this.activeSessionId === null) {
      console.log(
        `[NativeStreamer] Ignoring pacing mode "${pacingMode}": no active session.`,
      );
      return;
    }
    this.request({ type: "pacing", pacingMode }, CONTROL_TIMEOUT_MS).catch((error) => {
      console.warn(`[NativeStreamer] Failed to set pacing mode "${pacingMode}":`, error);
    });
  }

  updateBitrateLimit(maxBitrateKbps: number): void {
    if (!this.child || !this.activeSessionId) {
      return;
    }
    // Cooldown so a slider drag does not spam the server with identical
    // mid-session NVST updates (WebRTC parity).
    const now = Date.now();
    if (now - this.lastBitratePushAtMs < NativeStreamerManager.BITRATE_PUSH_COOLDOWN_MS) {
      return;
    }
    this.lastBitratePushAtMs = now;

    void this.request({
      type: "bitrate",
      maxBitrateKbps: normalizeBitrateKbps(maxBitrateKbps),
    }, CONTROL_TIMEOUT_MS)
      .then(async (response) => {
        // Mid-session re-offer: the streamer rebuilt the answer + nvstSdp
        // with the new cap. Push it to the server over the same signaling
        // channel as the session-start answer; the server reads
        // `vqos.bw.maximumBitrateKbps` from nvstSdp in answer messages.
        if (response.type !== "answer") {
          return;
        }
        await this.options.sendAnswer(response.answer);
        this.bitratePushesThisSession += 1;
        // Arm the proof: the first [NetworkHealth] bitrate sample after this
        // push becomes the baseline; a later sample at least the threshold
        // above it within the verify window means the server honored the push.
        this.bitratePushObservation = {
          pushedAtMs: Date.now(),
          baselineMbps: null,
          pushNumber: this.bitratePushesThisSession,
        };
        this.options.emit({
          type: "log",
          message: `[Bitrate] Pushed mid-session native bitrate cap update #${this.bitratePushesThisSession} to the GFN server (${normalizeBitrateKbps(maxBitrateKbps)} Kbps); watching [NetworkHealth] bitrate for the server honoring it.`,
        });
      })
      .catch((error) => {
        console.warn("[NativeStreamer] Failed to update native bitrate limit:", error);
      });
  }

  /**
   * Per-session observability for mid-session bitrate pushes: logs how many
   * NVST cap updates were pushed to the server this session and how many were
   * followed by the measured [NetworkHealth] bitrate rising (server honored
   * the push), then resets the counters. The native streamer has no
   * `availableIncomingBitrate`-style BWE estimate (that is a browser getStats
   * metric), so the received RTP bitrate in `[NetworkHealth]` is the only
   * proxy for whether the server honored a push.
   */
  private logBitratePushSessionSummary(): void {
    if (this.bitratePushesThisSession > 0) {
      console.log(
        `[Bitrate] Mid-session native bitrate cap pushes this session: ${this.bitratePushesThisSession} sent, ${this.bitratePushesVerifiedThisSession} followed by a measured bitrate rise (server honored the cap); if the rest stayed flat, the cap applies on the next offer/reconnect.`,
      );
    }
    this.bitratePushesThisSession = 0;
    this.bitratePushesVerifiedThisSession = 0;
    this.bitratePushObservation = null;
  }

  /**
   * Feeds a streamer `[NetworkHealth]` log line into the pending mid-session
   * bitrate push verification (if any): the first usable bitrate sample
   * becomes the baseline, a later sample at least the threshold above it
   * within the window is logged as VERIFIED (server honored the push), and an
   * expired window without movement is logged as unchanged (the cap then only
   * applies on the next offer/reconnect). Pure decision logic lives in
   * `networkHealth.ts`.
   */
  private handleNetworkHealthLog(text: string): void {
    const verification = this.bitratePushObservation;
    if (!verification || !text.includes("[NetworkHealth]")) {
      return;
    }
    const event = stepNativeBitratePushVerification(
      verification,
      parseNetworkHealthBitrateMbps(text),
      Date.now(),
    );
    if (!event) {
      return;
    }
    if (event.kind === "baseline") {
      verification.baselineMbps = event.baselineMbps;
      return;
    }
    this.bitratePushObservation = null;
    if (event.kind === "verified") {
      this.bitratePushesVerifiedThisSession += 1;
      const text = `[Bitrate] Mid-session native cap push #${event.pushNumber} VERIFIED: measured bitrate moved ${event.baselineMbps} → ${event.currentMbps} Mbps in ${event.elapsedMs}ms after the push. Verified ${this.bitratePushesVerifiedThisSession}/${this.bitratePushesThisSession} pushes this session.`;
      console.log(text);
      this.options.emit({ type: "log", message: text });
    } else {
      const text = `[Bitrate] Mid-session native cap push #${event.pushNumber}: measured bitrate unchanged (${event.baselineMbps} Mbps) after ${event.elapsedMs}ms — the server likely applies the new cap on the next offer/reconnect.`;
      console.log(text);
      this.options.emit({ type: "log", message: text });
    }
  }

  setInputPaused(paused: boolean): void {
    if (!this.child || !this.activeSessionId) {
      return;
    }

    void this.request({
      type: "input-paused",
      paused,
    }, CONTROL_TIMEOUT_MS).catch((error) => {
      console.warn("[NativeStreamer] Failed to update native input pause state:", error);
    });
  }

  updateShortcuts(shortcuts: NativeStreamerShortcutBindings): void {
    if (!this.child || !this.activeSessionId) {
      return;
    }

    void this.request({
      type: "update-shortcuts",
      shortcuts,
    }, CONTROL_TIMEOUT_MS).catch((error) => {
      console.warn("[NativeStreamer] Failed to update native shortcut bindings:", error);
    });
  }

  setMicrophoneEnabled(enabled: boolean): void {
    if (!this.child || !this.activeSessionId) {
      return;
    }

    void this.request({
      type: "microphone",
      microphoneEnabled: enabled,
    }, CONTROL_TIMEOUT_MS).catch((error) => {
      console.warn("[NativeStreamer] Failed to update native microphone state:", error);
    });
  }

  /**
   * Ask the native streamer to grab the last presented video frame as a PNG.
   * The native process writes a `screenshot` event just before the `ok`
   * response, so by the time `request()` resolves, `pendingScreenshot` holds
   * the captured frame. Single-flight is guaranteed by the UI's in-flight
   * guard, so a plain latest-wins slot is sufficient.
   */
  /**
   * Start a native recording. Chunks flow back as `recording-chunk` events
   * and are appended to the recording file (in order) by `handleEvent`.
   */
  async startNativeRecording(recordingId: string): Promise<void> {
    if (!this.child || !this.activeSessionId) {
      throw new Error("Native streamer is not running.");
    }
    this.activeNativeRecordingId = recordingId;
    this.recordingChunkQueue = Promise.resolve();
    try {
      await this.request({ type: "start-recording" }, CONTROL_TIMEOUT_MS);
    } catch (error) {
      this.activeNativeRecordingId = null;
      throw error;
    }
  }

  /**
   * Send a message on a remote WebRTC data channel (e.g. GFN's
   * `control_channel` — clipboard responses). The channel must have been
   * created by the server and registered by the native streamer.
   */
  async sendDataChannelMessage(label: string, payloadBase64: string): Promise<void> {
    if (!this.child || !this.activeSessionId) {
      throw new Error("Native streamer is not running.");
    }
    await this.request(
      { type: "send-data-channel-message", label, payloadBase64 },
      DATA_CHANNEL_SEND_TIMEOUT_MS,
    );
  }

  /**
   * Finalize the native recording: flush the encoder/muxer with EOS, wait for
   * the `recording-finished` event (which arrives strictly after every chunk
   * has been emitted and appended), then resolve with the thumbnail (if the
   * streamer captured one) and the dropped-frame count.
   */
  async stopNativeRecording(): Promise<{ thumbnailBase64?: string; droppedFrames: number }> {
    if (!this.child || !this.activeSessionId) {
      this.activeNativeRecordingId = null;
      throw new Error("Native streamer is not running.");
    }

    const finished = new Promise<{ thumbnailBase64?: string; droppedFrames: number }>(
      (resolve, reject) => {
        this.pendingRecordingFinishedResolve = resolve;
        this.pendingRecordingFinishedReject = reject;
      },
    );
    const clearPending = () => {
      this.pendingRecordingFinishedResolve = null;
      this.pendingRecordingFinishedReject = null;
    };
    try {
      await this.request({ type: "stop-recording", finalize: true }, RECORDING_STOP_TIMEOUT_MS);
    } catch (error) {
      clearPending();
      this.activeNativeRecordingId = null;
      throw error;
    }
    try {
      const result = await Promise.race([
        finished,
        new Promise<never>((_, reject) => {
          const timeout = setTimeout(() => {
            clearPending();
            reject(new Error("Native recording did not finalize in time."));
          }, RECORDING_STOP_TIMEOUT_MS);
          timeout.unref?.();
        }),
      ]);
      return result;
    } finally {
      this.activeNativeRecordingId = null;
    }
  }

  /** Abort the native recording without finalizing (keeps the branch usable). */
  async abortNativeRecording(): Promise<void> {
    this.activeNativeRecordingId = null;
    if (!this.child || !this.activeSessionId) {
      return;
    }
    try {
      await this.request({ type: "stop-recording", finalize: false }, CONTROL_TIMEOUT_MS);
    } catch (error) {
      console.warn("[NativeStreamer] Failed to abort native recording:", error);
    }
  }

  async captureScreenshot(): Promise<{ pngBase64: string; width: number; height: number }> {
    if (!this.child || !this.activeSessionId) {
      throw new Error("Native streamer is not running.");
    }

    this.pendingScreenshot = null;
    await this.request({ type: "take-screenshot" }, SCREENSHOT_TIMEOUT_MS);
    const screenshot = this.pendingScreenshot;
    this.pendingScreenshot = null;
    if (!screenshot) {
      throw new Error("Native streamer did not produce a screenshot frame.");
    }
    return screenshot;
  }

  async stop(reason = "stopped"): Promise<void> {
    const child = this.child;
    this.logBitratePushSessionSummary();
    this.activeSessionId = null;
    this.capabilities = null;
    this.surfaceUpdates.markNotReady();
    this.clearQueuedRemoteIce();

    if (!child) {
      return;
    }

    try {
      await this.request({ type: "stop", reason }, STOP_TIMEOUT_MS);
    } catch (error) {
      console.warn("[NativeStreamer] Stop request failed:", error);
    } finally {
      this.terminateProcess();
    }
  }

  dispose(reason = "disposed"): void {
    this.logBitratePushSessionSummary();
    this.activeSessionId = null;
    this.capabilities = null;
    this.surfaceUpdates.markNotReady();
    this.clearQueuedRemoteIce();
    this.rejectPending(new Error(`Native streamer ${reason}.`));
    this.terminateProcess();
  }

  private async ensureProcess(): Promise<void> {
    if (!isNativeStreamerSupportedPlatform(process.platform)) {
      throw new Error(NATIVE_STREAMER_WINDOWS_ONLY_MESSAGE);
    }

    if (this.child && this.capabilities) {
      return;
    }

    if (this.startupPromise) {
      await this.startupPromise;
      return;
    }

    if (this.child && !this.capabilities) {
      console.warn("[NativeStreamer] Restarting native streamer after an incomplete startup handshake.");
      this.rejectPending(new Error("Native streamer startup handshake did not complete."));
      this.terminateProcess();
      this.stdoutBuffer = "";
      this.stderrTail = [];
    }

    const startupPromise = (async () => {
      const backendPreference = this.options.getBackendPreference();
      let lastError: Error | null = null;

      for (const executablePath of resolveNativeStreamerExecutableCandidates({
        platform: process.platform,
        arch: process.arch,
        resourcesPath: process.resourcesPath,
        appPath: app.getAppPath(),
        mainDir: this.options.mainDir,
        isPackaged: app.isPackaged,
        envExecutablePath: process.env.OPENNOW_NATIVE_STREAMER,
        getConfiguredPath: () => this.options.getExecutablePathOverride(),
        cacheContext: {
          appVersion: app.getVersion(),
          isPackaged: app.isPackaged,
          platform: process.platform,
          resourcesPath: process.resourcesPath,
          tempDirectory: tmpdir(),
          userDataPath: app.getPath("userData"),
        },
      })) {
        try {
          await this.startProcess(executablePath, backendPreference);
          return;
        } catch (error) {
          lastError = error instanceof Error ? error : new Error(String(error));
          console.warn(
            `[NativeStreamer] Failed to initialize ${executablePath}: ${formatError(lastError)}`,
          );
          this.rejectPending(lastError);
          this.terminateProcess();
          this.stdoutBuffer = "";
          this.stderrTail = [];
          this.capabilities = null;
        }
      }

      throw lastError ?? new Error("Native streamer could not be initialized from any candidate path.");
    })();

    this.startupPromise = startupPromise;
    try {
      await startupPromise;
    } finally {
      if (this.startupPromise === startupPromise) {
        this.startupPromise = null;
      }
    }
  }

  private async startProcess(
    executablePath: string,
    backendPreference: NativeStreamerBackendPreference,
  ): Promise<void> {
    console.log("[NativeStreamer] Starting:", executablePath);
    console.log("[NativeStreamer] Backend preference:", backendPreference);
    const videoBackendPreference = this.options.getVideoBackendPreference();
    console.log("[NativeStreamer] Video backend preference:", videoBackendPreference);

    const { env: childEnv, runtimeStatus } = createNativeStreamerRuntimeEnvironment({
      executablePath,
      baseEnv: process.env,
      platform: process.platform,
      arch: process.arch,
      userDataPath: app.getPath("userData"),
      protocolVersion: NATIVE_STREAMER_PROTOCOL_VERSION,
      backendPreference,
      videoBackendPreference,
      externalRendererEnabled: process.platform === "win32"
        ? this.options.getExternalRendererEnabled()
        : false,
      stackedRendererEnabled: process.platform === "win32"
        ? this.options.getStackedRendererEnabled()
        : false,
      cloudGsyncMode: this.options.getCloudGsyncMode(),
      d3dFullscreenMode: this.options.getD3dFullscreenMode(),
    });
    this.gstreamerRuntime = runtimeStatus;
    if (runtimeStatus.bundled) {
      console.log("[NativeStreamer] Using bundled GStreamer runtime:", runtimeStatus.path);
    } else {
      console.log("[NativeStreamer]", runtimeStatus.message);
    }

    const child = spawn(executablePath, [], {
      stdio: "pipe",
      // The default native path lets the GStreamer video sink create its own
      // render window. Hiding the child process also hides that sink window on
      // Windows, which leaves the Electron input placeholder black.
      windowsHide: false,
      env: childEnv,
    });

    this.child = child;
    this.stdoutBuffer = "";
    this.stderrTail = [];

    child.stdout.setEncoding("utf8");
    child.stdout.on("data", (chunk: string) => this.handleStdout(chunk));
    child.stderr.setEncoding("utf8");
    child.stderr.on("data", (chunk: string) => {
      for (const line of chunk.split(/\r?\n/)) {
        if (line.trim()) {
          this.appendStderr(line);
          console.warn(`[NativeStreamer] ${line}`);
        }
      }
    });

    child.once("error", (error) => {
      this.options.emit({ type: "error", message: `Native streamer failed to start: ${formatError(error)}` });
      this.handleProcessExit(`spawn error: ${formatError(error)}`);
    });

    child.once("exit", (code, signal) => {
      const reason = signal ? `signal ${signal}` : `exit code ${code ?? "unknown"}`;
      this.handleProcessExit(reason);
    });

    const helloTimeoutMs = runtimeStatus.bundled ? BUNDLED_GSTREAMER_HELLO_TIMEOUT_MS : HELLO_TIMEOUT_MS;
    const response = await this.request({
      type: "hello",
      protocolVersion: NATIVE_STREAMER_PROTOCOL_VERSION,
    }, helloTimeoutMs);

    if (response.type !== "ready") {
      throw new Error(`Native streamer returned ${response.type} instead of ready.`);
    }

    this.capabilities = response.capabilities;
    console.log("[NativeStreamer] Capabilities:", response.capabilities);
    if (response.capabilities.protocolVersion !== NATIVE_STREAMER_PROTOCOL_VERSION) {
      throw new Error(
        `Native streamer reported protocolVersion=${response.capabilities.protocolVersion}, expected ${NATIVE_STREAMER_PROTOCOL_VERSION}.`,
      );
    }
    this.assertBackendPreference(response.capabilities, backendPreference);
    await this.surfaceUpdates.markReady();
  }

  private assertBackendPreference(
    capabilities: NativeStreamerCapabilities,
    backendPreference: NativeStreamerBackendPreference,
  ): void {
    if (backendPreference === "auto" || capabilities.backend === backendPreference) {
      return;
    }

    const reason = capabilities.fallbackReason ? ` ${capabilities.fallbackReason}` : "";
    throw new Error(
      `Native streamer backend "${backendPreference}" is unavailable; process selected "${capabilities.backend}".${reason}`,
    );
  }

  private request(input: NativeStreamerCommandInput, timeoutMs: number): Promise<NativeStreamerResponse> {
    const child = this.child;
    if (!child || child.killed || !child.stdin.writable) {
      return Promise.reject(new Error("Native streamer process is not running."));
    }

    const id = randomUUID();
    const payload = { ...input, id } as NativeStreamerCommand;

    return new Promise<NativeStreamerResponse>((resolveRequest, rejectRequest) => {
      const timeout = setTimeout(() => {
        this.pending.delete(id);
        rejectRequest(new Error(`Native streamer request "${input.type}" timed out after ${timeoutMs}ms.${this.formatStderrTail()}`));
      }, timeoutMs);
      timeout.unref?.();

      this.pending.set(id, {
        resolve: (message) => {
          clearTimeout(timeout);
          resolveRequest(message);
        },
        reject: (error) => {
          clearTimeout(timeout);
          rejectRequest(error);
        },
        timeout,
      });

      child.stdin.write(`${JSON.stringify(payload)}\n`, "utf8", (error) => {
        if (!error) {
          return;
        }
        const pending = this.pending.get(id);
        if (pending) {
          this.pending.delete(id);
          pending.reject(error);
        }
      });
    });
  }

  private handleStdout(chunk: string): void {
    this.stdoutBuffer += chunk;
    const lines = this.stdoutBuffer.split(/\r?\n/);
    this.stdoutBuffer = lines.pop() ?? "";

    for (const line of lines) {
      const trimmed = line.trim();
      if (!trimmed) {
        continue;
      }
      this.handleLine(trimmed);
    }
  }

  private handleLine(line: string): void {
    let message: NativeStreamerMessage;
    try {
      message = JSON.parse(line) as NativeStreamerMessage;
    } catch {
      console.log(`[NativeStreamer] ${line}`);
      return;
    }

    if (isNativeStreamerResponse(message)) {
      this.handleResponse(message);
      return;
    }

    if (isNativeStreamerEvent(message)) {
      this.handleEvent(message);
    }
  }

  private handleResponse(message: NativeStreamerResponse): void {
    const pending = this.pending.get(message.id);
    if (!pending) {
      console.warn("[NativeStreamer] Ignoring response for unknown request:", message.id);
      return;
    }

    this.pending.delete(message.id);
    if (message.type === "error") {
      pending.reject(new Error(message.code ? `${message.code}: ${message.message}` : message.message));
      return;
    }

    pending.resolve(message);
  }

  private handleEvent(message: NativeStreamerEvent): void {
    if (message.type === "log") {
      const text = `[NativeStreamer] ${message.message}`;
      if (message.level === "error") {
        console.error(text);
      } else if (message.level === "warn") {
        console.warn(text);
      } else {
        console.log(text);
      }
      this.options.emit({ type: "log", message: text });
      // Mid-session bitrate push proof: [NetworkHealth] lines carry the
      // measured receive bitrate, the proxy for the server honoring a push.
      this.handleNetworkHealthLog(message.message);
      return;
    }

    if (message.type === "local-ice") {
      if (this.answerInFlight) {
        this.queuedLocalIce.push(message.candidate);
        return;
      }

      this.forwardLocalIce(message.candidate);
      return;
    }

    if (message.type === "input-ready") {
      console.log(`[NativeStreamer] Input protocol ready: v${message.protocolVersion}`);
      this.options.emit({ type: "native-input-ready", protocolVersion: message.protocolVersion });
      return;
    }

    if (message.type === "shortcut") {
      this.options.emit({ type: "native-shortcut", action: message.action });
      return;
    }

    if (message.type === "clipboard-paste") {
      this.options.emit({ type: "native-clipboard-paste" });
      return;
    }

    if (message.type === "data-channel-message") {
      this.options.emit({
        type: "native-data-channel-message",
        label: message.label,
        payloadBase64: message.payloadBase64,
      });
      return;
    }

    if (message.type === "input-capture-changed") {
      this.options.emit({ type: "native-input-capture-changed", captured: message.captured });
      return;
    }

    if (message.type === "video-stall") {
      const formatAge = (value: number | undefined): string => value === undefined ? "n/a" : `${value}ms`;
      const stats = [
        `stall=${message.stallMs}ms`,
        `stage=${message.likelyStage ?? "unknown"}`,
        `encoded=${(message.encodedKbps ?? 0).toFixed(0)}kbps`,
        `decoded=${message.decodedFps.toFixed(1)}fps`,
        `sink=${message.sinkFps.toFixed(1)}fps`,
        `requestedFps=${message.requestedFps ?? "n/a"}`,
        `capsFramerate=${message.capsFramerate ?? "n/a"}`,
        `queueMode=${message.queueMode ?? "unknown"}`,
        `partialFlushes=${message.partialFlushCount ?? 0}`,
        `completeFlushes=${message.completeFlushCount ?? 0}`,
        `lastTransition=${message.lastTransitionType ?? "none"}`,
        `ages=encoded:${formatAge(message.encodedAgeMs)} decoded:${formatAge(message.decodedAgeMs)} sink:${formatAge(message.sinkAgeMs)}`,
        `rendered=${message.sinkRendered ?? "n/a"}`,
        `dropped=${message.sinkDropped ?? "n/a"}`,
        `memory=${message.memoryMode ?? "unknown"}`,
        `zeroCopy=${message.zeroCopy ?? "unknown"}`,
        `zeroCopyD3D11=${message.zeroCopyD3D11}`,
        `zeroCopyD3D12=${message.zeroCopyD3D12}`,
      ].join(" ");
      console.warn(`[NativeStreamer] Video stall recovery attempt ${message.recoveryAttempt}: ${stats}`);
      this.options.emit({
        type: "log",
        message: `[NativeStreamer] Video stall recovery attempt ${message.recoveryAttempt}: ${stats}`,
      });
      void this.options.requestKeyframe({
        reason: "native-video-stall",
        backlogFrames: 0,
        attempt: message.recoveryAttempt,
      }).catch((error) => {
        console.warn("[NativeStreamer] Failed to request video keyframe after stall:", error);
      });
      return;
    }

    if (message.type === "video-keyframe-request") {
      // Keyframe recovery over RTCP via signaling (data channel) only — the
      // native streamer must NEVER push a GstForceKeyUnit CustomUpstream event
      // into the WebRTC media pipeline (it propagates upstream into the UDP
      // receiver and kills the transport: the record-start stream death).
      void this.options
        .requestKeyframe({
          reason: message.reason,
          backlogFrames: 0,
          attempt: message.attempt ?? 0,
        })
        .catch((error) => {
          console.warn("[NativeStreamer] Failed to request video keyframe via signaling:", error);
        });
      return;
    }

    if (message.type === "video-transition") {
      const transition = message.transition;
      const summary = transition.summary ?? `${transition.transitionType} @ ${transition.atMs}ms`;
      console.warn(`[NativeStreamer] Video transition: ${summary}`);
      this.options.emit({
        type: "native-stream-transition",
        transition,
      });
      this.options.emit({
        type: "log",
        message: `[NativeStreamer] Video transition: ${summary}`,
      });
      return;
    }

    if (message.type === "stats") {
      this.options.emit({
        type: "native-stream-stats",
        stats: message.stats,
      });
      return;
    }

    if (message.type === "screenshot") {
      this.pendingScreenshot = message.screenshot;
      return;
    }

    if (message.type === "recording-chunk") {
      const recordingId = this.activeNativeRecordingId;
      if (recordingId) {
        const buffer = Buffer.from(message.chunkBase64, "base64");
        // Keep chunks strictly ordered and do not let recording-finished race
        // a still-pending disk write. This matters for non-fragmented qtmux:
        // the final MP4 can be one very large stdout event.
        this.recordingChunkQueue = this.recordingChunkQueue
          .catch(() => undefined)
          .then(() => appendRecordingChunk({
            recordingId,
            // Buffer's underlying ArrayBuffer spans exactly the decoded bytes.
            chunk: buffer.buffer.slice(
              buffer.byteOffset,
              buffer.byteOffset + buffer.byteLength,
            ) as ArrayBuffer,
          }))
          .catch((error) => {
            console.warn("[NativeStreamer] Failed to append native recording chunk:", error);
          });
      }
      return;
    }

    if (message.type === "recording-finished") {
      const resolve = this.pendingRecordingFinishedResolve;
      this.pendingRecordingFinishedResolve = null;
      this.pendingRecordingFinishedReject = null;
      void this.recordingChunkQueue.finally(() => {
        resolve?.({
          thumbnailBase64: message.thumbnailBase64,
          droppedFrames: message.droppedFrames ?? 0,
        });
      });
      return;
    }

    if (message.type === "status") {
      console.log(`[NativeStreamer] Status: ${message.status}${message.message ? ` (${message.message})` : ""}`);
      if (message.status === "streaming") {
        this.options.emit({ type: "native-stream-started", message: message.message });
      } else if (message.status === "stopped") {
        this.options.emit({ type: "native-stream-stopped", reason: message.message });
      }
      return;
    }

    if (message.type === "codec-downgrade-request") {
      // The negotiated codec produced zero decoded frames during startup
      // (every decoder candidate exhausted). Forward the request and end the
      // black session; the renderer relaunches the game session with the
      // fallback codec (GFN ladder: AV1 → H265). The subsequent
      // native-stream-stopped is ignored by the renderer because it marks the
      // shutdown as explicit before relaunching.
      console.warn(`[NativeStreamer] Codec downgrade requested: ${message.fromCodec} → ${message.toCodec} (zero decoded frames during startup).`);
      this.options.emit({
        type: "native-codec-downgrade-request",
        fromCodec: message.fromCodec,
        toCodec: message.toCodec,
      });
      void this.stop(`native codec downgrade (${message.fromCodec} → ${message.toCodec})`);
      return;
    }

    if (message.type === "network-assessment") {
      const { assessment } = message;
      console.log(
        `[NativeStreamer] Network assessment: verdict=${assessment.verdict} rtt=${assessment.rttMs ?? "n/a"}ms loss=${assessment.lossPercent ?? "n/a"}% jitter=${assessment.jitterMs ?? "n/a"}ms lowerFps=${assessment.recommendLowerFps} lowerRes=${assessment.recommendLowerResolution} keyframe=${assessment.suggestKeyframe}`,
      );
      // Surface the verdict to the renderer so the user sees why the session
      // is degrading (and why a downgrade restart may follow).
      this.options.emit({
        type: "native-network-assessment",
        assessment,
      });
      // Client half of LTR/PLI recovery: loss is climbing while the stream is
      // still alive — ask for a fresh keyframe NOW so recovery starts before
      // the picture visibly corrupts (cheap: RTCP PLI over signaling).
      if (assessment.suggestKeyframe) {
        void this.options
          .requestKeyframe({
            reason: "native-network-assessment",
            backlogFrames: 0,
            attempt: 0,
          })
          .catch((error) => {
            console.warn("[NativeStreamer] Failed to request keyframe after network assessment:", error);
          });
      }
      // Auto-downgrade: the negotiated profile is no longer sustainable
      // (poor verdict). Relaunch at a lower profile instead of letting the
      // session keep flickering. Guarded so it fires at most once per session
      // and with a cooldown — the relaunch itself restarts at the lower
      // profile, and repeated restarts would be worse than the degradation.
      const now = Date.now();
      const cooldownElapsed = now - this.lastNetworkDowngradeAtMs >= NETWORK_DOWNGRADE_COOLDOWN_MS;
      if (
        assessment.verdict === "poor"
        && assessment.recommendLowerFps
        && !this.networkDowngradeFiredThisSession
        && cooldownElapsed
      ) {
        this.networkDowngradeFiredThisSession = true;
        this.lastNetworkDowngradeAtMs = now;
        console.warn(
          "[NativeStreamer] Network assessment is poor; downgrading session profile and relaunching.",
        );
        this.options.emit({
          type: "native-network-downgrade-request",
          reason: `native network assessment poor (rtt=${assessment.rttMs ?? "n/a"}ms, loss=${assessment.lossPercent ?? "n/a"}%)`,
        });
        void this.stop(`native network downgrade (${assessment.verdict})`);
      }
      return;
    }

    if (message.type === "error") {
      // A native recording finalize that errors (e.g. the offline remux found
      // empty elementary streams) never emits `recording-finished` — reject the
      // pending finalize promise now instead of letting the renderer time out
      // ~20 s later. Anything else is surfaced as a plain streamer error.
      if (message.code === "recording-stop-failed" && this.pendingRecordingFinishedReject) {
        const reject = this.pendingRecordingFinishedReject;
        this.pendingRecordingFinishedResolve = null;
        this.pendingRecordingFinishedReject = null;
        reject(new Error(message.message));
      }
      this.options.emit({ type: "error", message: `Native streamer error: ${message.message}` });
    }
  }

  private handleProcessExit(reason: string): void {
    if (!this.child) {
      return;
    }

    const tail = this.formatStderrTail();
    const hadActiveSession = this.activeSessionId !== null;
    const stoppedReason = `process ended (${reason})`;
    console.warn(`[NativeStreamer] Process ended (${reason})${tail}`);
    this.child = null;
    this.stdoutBuffer = "";
    this.stderrTail = [];
    this.activeSessionId = null;
    this.capabilities = null;
    this.surfaceUpdates.markNotReady();
    this.clearQueuedRemoteIce();
    this.rejectPending(new Error(`Native streamer process ended (${reason}).${tail}`));

    if (hadActiveSession) {
      this.options.emit({ type: "native-stream-stopped", reason: stoppedReason });
      this.options.emit({ type: "error", message: `Native streamer ${stoppedReason}.${tail}` });
    }
  }

  private appendStderr(line: string): void {
    this.stderrTail.push(line);
    if (this.stderrTail.length > 12) this.stderrTail.shift();
  }

  private formatStderrTail(): string {
    return this.stderrTail.length > 0 ? ` Recent stderr: ${this.stderrTail.join(" | ")}` : "";
  }

  private rejectPending(error: Error): void {
    for (const [id, pending] of this.pending.entries()) {
      clearTimeout(pending.timeout);
      pending.reject(error);
      this.pending.delete(id);
    }
  }

  private async flushQueuedLocalIce(): Promise<void> {
    const queued = this.queuedLocalIce;
    this.queuedLocalIce = [];

    for (const candidate of queued) {
      await this.forwardLocalIce(candidate);
    }
  }

  private prepareRemoteIceQueue(sessionId: string): void {
    if (this.queuedRemoteIceSessionId !== null && this.queuedRemoteIceSessionId !== sessionId) {
      this.clearQueuedRemoteIce();
    }
    this.queuedRemoteIceSessionId = sessionId;
  }

  private queueRemoteIce(sessionId: string, candidate: IceCandidatePayload): void {
    this.prepareRemoteIceQueue(sessionId);
    this.queuedRemoteIce.push(candidate);
  }

  private clearQueuedRemoteIce(): void {
    this.queuedRemoteIceSessionId = null;
    this.queuedRemoteIce = [];
  }

  private async flushQueuedRemoteIce(sessionId: string): Promise<void> {
    const queued = this.drainQueuedRemoteIce(sessionId);
    for (const candidate of queued) {
      await this.sendRemoteIce(candidate);
    }
  }

  private async sendRemoteIce(candidate: IceCandidatePayload): Promise<void> {
    await this.request({
      type: "remote-ice",
      candidate,
    }, CONTROL_TIMEOUT_MS);
  }

  private async forwardLocalIce(candidate: IceCandidatePayload): Promise<void> {
    try {
      await this.options.sendIceCandidate(candidate);
    } catch (error) {
      console.warn("[NativeStreamer] Failed to forward local ICE candidate:", error);
    }
  }

  private terminateProcess(): void {
    this.surfaceUpdates.markNotReady();
    const child = this.child;
    if (!child) {
      return;
    }

    this.child = null;
    try {
      child.kill();
    } catch (error) {
      console.warn("[NativeStreamer] Failed to terminate process:", error);
    }
  }
}
