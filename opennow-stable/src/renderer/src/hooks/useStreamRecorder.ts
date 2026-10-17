import { useCallback, useEffect, useRef, useState } from "react";
import type { RefObject } from "react";
import type { RecordingEntry } from "@shared/gfn";
import {
  clampRecordingBitrate,
  computeRecordingFrameShortfall,
  fitThumbnailSize,
  resolveRecordCap,
  resolveRecordingStreamFps,
  selectRecordingStrategy,
  type RecordingStrategy,
} from "../components/stream/streamRuntimeHelpers";
import {
  inspectEncodedCapture,
  inspectEncodedCodecOnly,
  type EncodedCaptureInfo,
} from "../recording/encodedCapture";
import type { RecordCanvasWorkerOutboundMessage } from "../recording/recordCanvasWorker";
import type { EncodedWorkerOutboundMessage } from "../recording/recordEncodedWorker";
import {
  isWebCodecsRecordingSupported,
  startWebCodecsTrackRecorder,
  type WebCodecsRecordingResult,
  type WebCodecsTrackRecorder,
} from "../recording/webCodecsTrackRecorder";
import { getActiveWebRtcPeerConnection } from "../platforms/gfn/webrtcClient";

// Web recording uses one of the strategies from selectRecordingStrategy:
// "encoded-transform" captures the stream's encoded bitstream pre-decode
// (zero re-encode, cap not applied); "webcodecs-track" ingests the live track
// through a MediaStreamTrackProcessor and hardware-encodes it — a recorder
// that cannot keep up drops frames from the FILE instead of backpressuring
// the stream; "raw-track" hands MediaRecorder the raw WebRTC track directly
// (legacy fallback when the WebCodecs surface is missing); "canvas-downscale"
// bounds the encode cost to the user's recording cap
// (recordingResolution/recordingFps settings).
// Resolver that extracts the video track a MediaStreamTrackGenerator exposes
// for MediaRecorder, regardless of which API shape the runtime ships.
type GeneratorTrackResolver = (generator: MediaStreamTrackGenerator) => MediaStreamTrack | null;

/**
 * Outcome of an Instant Replay save, mapped by the caller to a user-facing
 * notice. "disabled" = IR is off, "busy" = a recording/save is in flight,
 * "unavailable" = no armed ring (or still filling), "failed" = save error.
 */
export type InstantReplaySaveResult = "saved" | "disabled" | "busy" | "unavailable" | "failed";

// Module-level cache: the generator API cannot appear or disappear within a
// session, so probe once and reuse. `undefined` = not probed yet, `null` =
// unusable (recording goes straight to the captureStream fallback).
let cachedGeneratorTrackResolver: GeneratorTrackResolver | null | undefined;

/**
 * Probes whether a usable MediaStreamTrackGenerator exists and returns a
 * resolver that extracts its video track. Runs once per app session, BEFORE
 * any worker or generator is created, so an unsupported runtime never
 * half-starts the worker path (no worker spawn, no rVFC chain, no generator
 * that has to be torn down mid-start).
 *
 * The API shape changed in Chromium ~M100: the legacy `readable` accessor was
 * removed and the generator object itself became the track (the class now
 * inherits MediaStreamTrack). Both shapes are accepted here; anything else —
 * constructor missing/throws, no writable stream, no usable track — resolves
 * to null.
 */
/**
 * Client-side mirror of the main-process recording listing cap: newest 500
 * entries are kept in renderer state. Retention on disk is governed by the
 * gallery disk budget, not this cap.
 */
const RECORDING_LIST_CAP = 500;

const getGeneratorTrackResolver = (): GeneratorTrackResolver | null => {
  if (cachedGeneratorTrackResolver !== undefined) {
    return cachedGeneratorTrackResolver;
  }
  if (typeof MediaStreamTrackGenerator === "undefined") {
    cachedGeneratorTrackResolver = null;
    return null;
  }
  let probe: MediaStreamTrackGenerator;
  try {
    probe = new MediaStreamTrackGenerator({ kind: "video" });
  } catch {
    cachedGeneratorTrackResolver = null;
    return null;
  }
  try {
    if (!probe.writable) {
      return null;
    }
    // Legacy Chromium (Chrome 94-99 era): the track lives on `readable`.
    const legacyTrack = probe.readable;
    if (legacyTrack && typeof legacyTrack.stop === "function") {
      cachedGeneratorTrackResolver = (generator) => generator.readable ?? null;
      return cachedGeneratorTrackResolver;
    }
    // Modern Chromium (~M100+): the generator object itself IS the track.
    if (typeof probe.stop === "function") {
      cachedGeneratorTrackResolver = (generator) => generator as unknown as MediaStreamTrack;
      return cachedGeneratorTrackResolver;
    }
  } finally {
    // Stop the throwaway probe track (no-op where the legacy shape lacks
    // `stop` — never let cleanup throw and void the resolver above).
    try {
      probe.stop();
    } catch {
      /* noop */
    }
  }
  cachedGeneratorTrackResolver = null;
  return null;
};

interface UseStreamRecorderOptions {
  videoRef: RefObject<HTMLVideoElement | null>;
  audioRef: RefObject<HTMLAudioElement | null>;
  gameTitle: string;
  micTrack: MediaStreamTrack | null;
  recordingBitrateMbps: number | null;
  recordingResolution: string;
  recordingFps: number;
  /**
   * The session's negotiated stream frame rate (e.g. 60/120), or `null` when
   * unknown. Used for the recording-strategy gate instead of the remote
   * track's live `getSettings().frameRate`, which can spike above the stream
   * rate during decode catch-up bursts and wrongly bulk the recording into
   * the bounded canvas re-encode path (see resolveRecordingStreamFps).
   */
  streamFps: number | null;
  /**
   * Native streamer mode: the renderer's <video> element has no frames, so
   * recording happens in the native pipeline (H.264 fragmented MP4) and its
   * chunks are streamed to the main process instead of MediaRecorder.
   */
  nativeRecordingEnabled: boolean;
  /**
   * Mix the live microphone into encoded-bitstream recordings. The mic is
   * captured as raw PCM and mixed into the recording's audio track inside the
   * worker — only the audio is re-encoded; the video bitstream stays
   * untouched. Requires an active mic (micTrack live).
   */
  mixMic: boolean;
  /**
   * Called when a recording requested mic mix but no live mic was available
   * — lets the caller auto-reset the setting so a recording is never
   * silently captured without the user's voice.
   */
  onMixMicUnavailable?: () => void;
  /**
   * Instant Replay (official capture model): while enabled, the encoded
   * worker is armed from session start and keeps a keyframe-aligned ring of
   * the last `instantReplaySeconds` seconds, prepended to every recording
   * and saved by the Instant Replay hotkey. Disabled disarms the ring
   * entirely (capture starts at record, like the legacy recorder).
   */
  instantReplayEnabled: boolean;
  /** Instant Replay buffer length (seconds) — 5..300 from the shared setting. */
  instantReplaySeconds: number;
  /**
   * True while a stream session is live. The DVR worker arms on the session
   * start (once per peer connection) so the ring fills before the first
   * record; re-arms when a new session begins.
   */
  streamActive: boolean;
  /**
   * True once the session's <video> element carries frames (dimensions known
   * — the DVR arm needs them resolvable). Flips true after the session is
   * up, so the arm effect re-runs exactly when the stream becomes ready.
   */
  streamVideoReady: boolean;
}

export function useStreamRecorder({
  videoRef,
  audioRef,
  gameTitle,
  micTrack,
  recordingBitrateMbps,
  recordingResolution,
  recordingFps,
  streamFps,
  nativeRecordingEnabled,
  mixMic,
  onMixMicUnavailable,
  instantReplayEnabled,
  instantReplaySeconds,
  streamActive,
  streamVideoReady,
}: UseStreamRecorderOptions) {
  // Effective DVR ring length for the encoded worker: the Instant Replay
  // setting gates the ring (off = capture starts at record, like the legacy
  // recorder).
  const dvrSeconds = instantReplayEnabled ? instantReplaySeconds : 0;
  const [isRecording, setIsRecording] = useState(false);
  const [isProcessing, setIsProcessing] = useState(false);
  const [recordings, setRecordings] = useState<RecordingEntry[]>([]);
  const [recordingDurationMs, setRecordingDurationMs] = useState(0);
  const [recordingError, setRecordingError] = useState<string | null>(null);
  // Set after a recording that dropped frames (native: queue drops;
  // web: draw loop could not sustain the target rate), cleared on the next
  // start — so a choppy recording is explained, not a mystery.
  const [recordingDropNotice, setRecordingDropNotice] = useState<string | null>(null);
  const [usedMimeType, setUsedMimeType] = useState<string | null>(null);
  const [usedStrategy, setUsedStrategy] = useState<RecordingStrategy | null>(null);
  // True while an Instant Replay save is muxing/receiving chunks: guards the
  // save hotkey against re-entry (the ref also guards the worker route) and
  // feeds the save toast's timing.
  const [isSavingInstantReplay, setIsSavingInstantReplay] = useState(false);
  const mediaRecorderRef = useRef<MediaRecorder | null>(null);
  // Stream-protecting WebCodecs recorder (MSTP ingest + hardware encoders):
  // the primary web path for within-cap streams. Non-null while recording.
  const webCodecsRecorderRef = useRef<WebCodecsTrackRecorder | null>(null);
  // The stream frame rate the WebCodecs recording was started against: the
  // stop-time shortfall notice compares the recorded frame count to it.
  const webCodecsExpectedFpsRef = useRef<number | null>(null);
  const recordingIdRef = useRef<string | null>(null);
  const recordingStartTimeRef = useRef(0);
  const recordingTimerRef = useRef<number | undefined>(undefined);
  const recordingFrameCallbackRef = useRef<number | undefined>(undefined);
  const recordingVideoTrackRef = useRef<MediaStreamTrack | null>(null);
  const thumbnailDataUrlRef = useRef<string | null>(null);
  const recCarouselRef = useRef<HTMLDivElement | null>(null);
  const audioCtxRef = useRef<AudioContext | null>(null);
  const recordWorkerRef = useRef<Worker | null>(null);
  // Fallback canvas used only when the worker/generator path can't start
  // (see toggleRecording): holds the downscaled frame for thumbnails.
  const recordFallbackCanvasRef = useRef<HTMLCanvasElement | null>(null);
  const videoGeneratorRef = useRef<MediaStreamTrackGenerator | null>(null);
  // Writer on the generator's WritableStream (WritableStream itself has no
  // write(); TS 6.0's DOM lib models the real spec, so we must go through
  // getWriter()). Kept so cleanup can close the stream.
  const videoGeneratorWriterRef = useRef<WritableStreamDefaultWriter<VideoFrame> | null>(null);
  // True while a frame is in flight (worker draw + generator write). The draw
  // loop skips frames while this is set, which both bounds the pipeline and
  // feeds the honest shortfall notice at stop.
  const writePendingRef = useRef(false);
  // Serialized queue of recording chunk sends (read + IPC + disk write). Each
  // chunk awaits the previous one, so at most one send is in flight — bounded
  // memory on slow disks, and onstop can drain the queue before
  // finishRecording (no more "Unknown recording id" race with the final
  // chunk). The chain never rejects (each link catches), so awaiting it is
  // always safe.
  const chunkSendChainRef = useRef<Promise<void>>(Promise.resolve());
  // Receiver-side encoded capture (GFN parity: the stream's encoded bitstream
  // is captured pre-decode and muxed in a worker — zero re-encode, so the
  // recording never competes with the decoder). Non-null while active.
  const encodedWorkerRef = useRef<Worker | null>(null);
  const encodedReceiversRef = useRef<{ video: RTCRtpReceiver; audio: RTCRtpReceiver | null } | null>(null);
  const encodedReadyRef = useRef<{ resolve: () => void; reject: (error: unknown) => void } | null>(null);
  const encodedDoneRef = useRef<{ resolve: () => void; reject: (error: unknown) => void } | null>(null);
  // Recording id of an Instant Replay save in flight: worker chunks/done flow
  // to this id while set (a manual recording cannot co-exist with a save —
  // see saveInstantReplay's guards) and the promise settles on done/error.
  // The native save path uses it as a guard sentinel only: the native IPC
  // promise settles that operation itself (resolve/reject are no-ops there).
  const instantReplaySaveRef = useRef<{
    recordingId: string;
    resolve: (durationMs: number) => void;
    reject: (error: unknown) => void;
  } | null>(null);
  // The peer connection the encoded worker was armed against (session id for
  // the DVR ring): a new session gets a fresh worker instead of reusing a
  // stale one.
  const encodedArmedPeerConnRef = useRef<RTCPeerConnection | null>(null);
  // Live mic PCM capture for the encoded recording's mic mix (GFN parity).
  // The mic track itself is owned by the app (clientRef.getMicTrack()) — we
  // only tap it, never stop it.
  const micCaptureRef = useRef<{
    context: AudioContext;
    source: MediaStreamAudioSourceNode;
    processor: ScriptProcessorNode;
  } | null>(null);

  // Stops the recording's video feed: the generator track (which ends the
  // MediaRecorder's video), the writer (which closes the frame input), and
  // the downscale worker. The worker is terminated outright — it owns no
  // state worth flushing once recording has ended.
  const stopRecordingVideoTrack = (): void => {
    const track = recordingVideoTrackRef.current;
    if (track && track.readyState === "live") {
      track.stop();
    }
    recordingVideoTrackRef.current = null;
    const writer = videoGeneratorWriterRef.current;
    if (writer) {
      videoGeneratorWriterRef.current = null;
      writer.close().catch(() => undefined);
    }
    const generator = videoGeneratorRef.current;
    if (generator) {
      videoGeneratorRef.current = null;
    }
    const worker = recordWorkerRef.current;
    if (worker) {
      recordWorkerRef.current = null;
      worker.terminate();
    }
  };
  // Downscale a frame source to the small JPEG thumbnail shown on the
  // recording entry. The canvas path takes the thumbnail from its worker; the
  // raw-track MediaRecorder path and the WebCodecs recorder take it from the
  // live video element (cheap there — no draw loop is running).
  const thumbnailFromSource = useCallback(
    (source: CanvasImageSource, sourceWidth: number, sourceHeight: number): void => {
      const { width, height } = fitThumbnailSize(sourceWidth, sourceHeight);
      const canvas = document.createElement("canvas");
      canvas.width = width;
      canvas.height = height;
      const context = canvas.getContext("2d");
      if (context) {
        context.drawImage(source, 0, 0, width, height);
        thumbnailDataUrlRef.current = canvas.toDataURL("image/jpeg", 0.72);
      }
    },
    [],
  );
  const recordingApiAvailable =
    typeof window.openNow?.beginRecording === "function" &&
    typeof window.openNow?.sendRecordingChunk === "function" &&
    typeof window.openNow?.finishRecording === "function" &&
    typeof window.openNow?.abortRecording === "function" &&
    typeof window.openNow?.listRecordings === "function" &&
    typeof window.openNow?.deleteRecording === "function" &&
    typeof window.openNow?.startNativeRecording === "function" &&
    typeof window.openNow?.stopNativeRecording === "function" &&
    typeof window.openNow?.abortNativeRecording === "function";

  const refreshRecordings = useCallback(async () => {
    setRecordingError(null);
    if (!recordingApiAvailable) return;
    try {
      const items = await window.openNow.listRecordings();
      setRecordings(items);
    } catch (error) {
      console.error("[StreamView] Failed to load recordings:", error);
      setRecordingError("Unable to load recordings.");
    }
  }, [recordingApiAvailable]);

  const deleteRecording = useCallback(async (filePath: string) => {
    setRecordingError(null);
    if (!recordingApiAvailable) return;
    try {
      await window.openNow.deleteRecording({ filePath });
      setRecordings((prev) => prev.filter((recording) => recording.filePath !== filePath));
    } catch (error) {
      console.error("[StreamView] Failed to delete recording:", error);
      setRecordingError("Unable to delete recording.");
    }
  }, [recordingApiAvailable]);

  const scrollRecordings = useCallback((direction: "left" | "right") => {
    const strip = recCarouselRef.current;
    if (!strip) return;
    strip.scrollBy({ left: direction === "left" ? -200 : 200, behavior: "smooth" });
  }, []);

  const stopEncodedMicCapture = useCallback((): void => {
    const capture = micCaptureRef.current;
    micCaptureRef.current = null;
    if (!capture) return;
    try {
      capture.processor.onaudioprocess = null;
      capture.processor.disconnect();
      capture.source.disconnect();
      void capture.context.close();
    } catch (error) {
      console.warn("[StreamView] Failed to stop mic capture:", error);
    }
  }, []);

  // Shared worker plumbing for both worker creation sites (early ARM and
  // on-demand start). Chunk/done routing prefers an in-flight Instant Replay
  // save (it owns its own recording id and cannot co-exist with a manual
  // recording — see saveInstantReplay's guards), otherwise the manual
  // recording id owns the chunks.
  const handleEncodedWorkerMessage = useCallback(
    (event: MessageEvent<EncodedWorkerOutboundMessage>): void => {
      const message = event.data;
      if (message.type === "chunk") {
        const id = instantReplaySaveRef.current?.recordingId ?? recordingIdRef.current;
        if (!id) return;
        chunkSendChainRef.current = chunkSendChainRef.current
          .then(() => window.openNow.sendRecordingChunk({ recordingId: id, chunk: message.data }))
          .catch((error: unknown) => {
            console.error("[StreamView] Failed to send encoded recording chunk:", error);
          });
      } else if (message.type === "ready") {
        encodedReadyRef.current?.resolve();
        encodedReadyRef.current = null;
      } else if (message.type === "done") {
        if (instantReplaySaveRef.current) {
          // The snapshot's exact ring window (worker-measured): the save
          // resolves with it instead of the manual recording's done signal.
          instantReplaySaveRef.current.resolve(message.durationMs ?? 0);
          instantReplaySaveRef.current = null;
        } else {
          encodedDoneRef.current?.resolve();
          encodedDoneRef.current = null;
        }
      } else if (message.type === "diag") {
        // One-shot worker diagnostics (first frame bytes/format, config
        // build result) — surface them so a header-only recording pins
        // the failing stage straight from the exported log.
        console.info(`[StreamView] ${message.message}`);
      } else if (message.type === "error") {
        const error = new Error(message.message);
        encodedReadyRef.current?.reject(error);
        encodedReadyRef.current = null;
        encodedDoneRef.current?.reject(error);
        encodedDoneRef.current = null;
        instantReplaySaveRef.current?.reject(error);
        instantReplaySaveRef.current = null;
        console.error("[StreamView] Encoded recording worker error:", message.message);
      }
    },
    [],
  );
  const handleEncodedWorkerError = useCallback((event: ErrorEvent): void => {
    const error = new Error(event.message || "Recording worker crashed");
    encodedReadyRef.current?.reject(error);
    encodedReadyRef.current = null;
    encodedDoneRef.current?.reject(error);
    encodedDoneRef.current = null;
    instantReplaySaveRef.current?.reject(error);
    instantReplaySaveRef.current = null;
    console.error("[StreamView] Encoded recording worker crashed:", event);
  }, []);

  // Taps the app's live mic track and streams mono 48 kHz PCM to the encoded
  // worker. Returns false when no usable mic is available (recording proceeds
  // without the mic, surfaced via a warning).
  const startEncodedMicCapture = useCallback(
    (worker: Worker): boolean => {
      const track = micTrack;
      if (!track || track.readyState !== "live") {
        console.warn("[StreamView] Mic mix requested but no live mic available — recording without mic.");
        // Auto-reset the toggle: a recording that silently omits the user's
        // voice should not leave "Mix Mic" looking active.
        onMixMicUnavailable?.();
        return false;
      }
      try {
        const context = new AudioContext({ sampleRate: 48_000 });
        const source = context.createMediaStreamSource(new MediaStream([track]));
        // ScriptProcessorNode: deprecated but universal in Chromium and needs
        // no worklet module to bundle. One mono channel in → one mono out;
        // the worker re-encodes the mixed track itself.
        const processor = context.createScriptProcessor(4096, 1, 1);
        processor.onaudioprocess = (event) => {
          const input = event.inputBuffer.getChannelData(0);
          const samples = new Float32Array(input.length);
          samples.set(input);
          // Real-clock capture time: the worker measures the mic's actual
          // sample rate from these tags and consumes by RTP frame duration,
          // so the voice cannot drift from the game audio.
          worker.postMessage(
            { type: "mic-pcm", samples, capturedAtMs: performance.now() },
            [samples.buffer],
          );
        };
        // Keep the graph pulled without routing the mic to the speakers.
        const mute = context.createGain();
        mute.gain.value = 0;
        source.connect(processor);
        processor.connect(mute);
        mute.connect(context.destination);
        micCaptureRef.current = { context, source, processor };
        console.info("[StreamView] Mic mix active — recording will mix the microphone into the audio track.");
        return true;
      } catch (error) {
        console.warn("[StreamView] Mic capture failed — recording without mic mix.", error);
        micCaptureRef.current = null;
        return false;
      }
    },
    [micTrack, onMixMicUnavailable],
  );

  const startEncodedRecording = useCallback(
    async (info: EncodedCaptureInfo, mimeType: string): Promise<void> => {
      setRecordingError(null);
      setRecordingDropNotice(null);
      const pc = getActiveWebRtcPeerConnection();
      if (!pc) {
        setRecordingError("Could not start recording.");
        return;
      }
      const videoReceiver = pc.getReceivers().find((receiver) => receiver.track?.kind === "video") ?? null;
      const audioReceiver = pc.getReceivers().find((receiver) => receiver.track?.kind === "audio") ?? null;
      if (!videoReceiver) {
        setRecordingError("Could not start recording.");
        return;
      }
      let recordingId: string;
      try {
        const result = await window.openNow.beginRecording({ mimeType });
        recordingId = result.recordingId;
      } catch (error) {
        console.error("[StreamView] Failed to begin encoded recording:", error);
        const detail = error instanceof Error && error.message ? error.message : "";
        setRecordingError(detail ? `Could not start recording: ${detail}` : "Could not start recording.");
        return;
      }
      recordingIdRef.current = recordingId;
      thumbnailDataUrlRef.current = null;
      recordFallbackCanvasRef.current = null;
      recordingStartTimeRef.current = Date.now();
      setRecordingDurationMs(0);
      // The DVR worker is persistent for the session: created (and armed,
      // transforms attached, ring filling) at stream start when the DVR
      // pre-roll setting is on, or on demand here when it is off / the arm
      // failed. Either way this recording just re-inits (fresh dimensions /
      // mixMic) and starts the muxer on the EXISTING worker.
      const worker = encodedWorkerRef.current ?? new Worker(
        new URL("../recording/recordEncodedWorker.ts", import.meta.url),
        { type: "module" },
      );
      if (!encodedWorkerRef.current) {
        encodedWorkerRef.current = worker;
        worker.onmessage = handleEncodedWorkerMessage;
        worker.onerror = handleEncodedWorkerError;
      }
      try {
        // Init (the worker acks 'ready'); then start. When the worker was
        // armed early (DVR), init refreshes params only and start slices the
        // ring — the transforms are already attached.
        worker.postMessage({
          type: "init",
          codec: info.codec,
          container: info.container,
          width: info.width,
          height: info.height,
          fps: info.fps,
          hasAudio: info.hasAudio,
          audioChannels: info.audioChannels,
          audioSampleRate: info.audioSampleRate,
          mixMic,
          dvrSeconds,
        });
        await new Promise<void>((resolve, reject) => {
          encodedReadyRef.current = { resolve, reject };
        });
        if (!encodedArmedPeerConnRef.current || encodedArmedPeerConnRef.current !== pc) {
          // Not armed for this session yet (DVR off, or the arm failed):
          // attach the transforms now. The ring starts filling here, so this
          // recording has no pre-roll, but the next one will.
          videoReceiver.transform = new RTCRtpScriptTransform(worker, { kind: "video" });
          if (audioReceiver) {
            audioReceiver.transform = new RTCRtpScriptTransform(worker, { kind: "audio" });
          }
          encodedArmedPeerConnRef.current = pc;
          encodedReceiversRef.current = { video: videoReceiver, audio: audioReceiver };
        }
        worker.postMessage({ type: "start" });
        if (mixMic) {
          startEncodedMicCapture(worker);
        }
      } catch (error) {
        console.warn("[StreamView] Encoded capture unavailable, aborting:", error);
        stopEncodedMicCapture();
        // Reset unconditionally: a pre-armed worker that failed this start is
        // not trustworthy for the next recording either, and the armed ref
        // must not outlive it (the next start would skip re-attaching).
        videoReceiver.transform = null;
        if (audioReceiver) audioReceiver.transform = null;
        worker.terminate();
        encodedWorkerRef.current = null;
        encodedReceiversRef.current = null;
        encodedArmedPeerConnRef.current = null;
        encodedReadyRef.current = null;
        encodedDoneRef.current = null;
        recordingIdRef.current = null;
        window.openNow.abortRecording({ recordingId }).catch(() => undefined);
        setRecordingError("Could not start recording.");
        return;
      }
      setUsedMimeType(mimeType);
      setIsRecording(true);
      setIsProcessing(false);
      recordingTimerRef.current = window.setInterval(() => {
        setRecordingDurationMs(Date.now() - recordingStartTimeRef.current);
      }, 500);
      console.info(
        `[StreamView] Encoded capture started: ${info.codec} ${info.width}x${info.height}@${info.fps} → ${mimeType} (zero re-encode, DVR pre-roll ${dvrSeconds}s${mixMic ? ", mic mix" : ""})`,
      );
    },
    [handleEncodedWorkerError, handleEncodedWorkerMessage, mixMic, dvrSeconds, startEncodedMicCapture, stopEncodedMicCapture],
  );

  const stopEncodedRecording = useCallback(async (): Promise<void> => {
    setIsRecording(false);
    setIsProcessing(true);
    window.clearInterval(recordingTimerRef.current);
    recordingTimerRef.current = undefined;
    const id = recordingIdRef.current;
    recordingIdRef.current = null;
    const worker = encodedWorkerRef.current;
    if (!worker) {
      setIsProcessing(false);
      return;
    }
    // Stop the mic tap before the worker finalizes so its mic-chain flush
    // cannot stall waiting on PCM that will never arrive. The worker itself
    // stays armed (transforms attached, rings buffering) for the next record.
    stopEncodedMicCapture();
    const donePromise = new Promise<void>((resolve, reject) => {
      encodedDoneRef.current = { resolve, reject };
    });
    // A wedged worker must not leave the UI stuck in the PROCESSING pill.
    const doneWithTimeout = Promise.race([
      donePromise,
      new Promise<never>((_, reject) =>
        window.setTimeout(() => reject(new Error("Encoded recording finalize timed out")), 10_000),
      ),
    ]);
    worker.postMessage({ type: "stop" });
    let thumbnailDataUrl: string | undefined;
    const currentVideo = videoRef.current;
    if (currentVideo && currentVideo.videoWidth > 0 && currentVideo.videoHeight > 0) {
      const { width: thumbWidth, height: thumbHeight } = fitThumbnailSize(
        currentVideo.videoWidth,
        currentVideo.videoHeight,
      );
      const canvas = document.createElement("canvas");
      canvas.width = thumbWidth;
      canvas.height = thumbHeight;
      const context = canvas.getContext("2d");
      if (context) {
        context.drawImage(currentVideo, 0, 0, thumbWidth, thumbHeight);
        thumbnailDataUrl = canvas.toDataURL("image/jpeg", 0.72);
      }
    }
    try {
      await doneWithTimeout;
      // Chunks arrive strictly before 'done' (worker message ordering), so
      // draining the send chain here covers every byte of the file.
      await chunkSendChainRef.current;
      if (id) {
        const durationMs = Date.now() - recordingStartTimeRef.current;
        const entry = await window.openNow.finishRecording({
          recordingId: id,
          durationMs,
          gameTitle,
          thumbnailDataUrl,
        });
        setRecordings((prev) => [entry, ...prev].slice(0, RECORDING_LIST_CAP));
      }
    } catch (error) {
      console.error("[StreamView] Failed to finalize encoded recording:", error);
      setRecordingError("Recording could not be saved.");
      if (id) {
        window.openNow.abortRecording({ recordingId: id }).catch(() => undefined);
      }
    } finally {
      encodedDoneRef.current = null;
      setIsProcessing(false);
    }
  }, [gameTitle]);

  // Abort the in-flight encoded recording (start failure / session teardown):
  // tell the worker to drop the recording state and stay armed, close the
  // mic tap, and abort the main-process file. The worker lives on so a later
  // recording still gets its pre-roll.
  const abortEncodedRecording = useCallback(async (): Promise<void> => {
    setIsRecording(false);
    setIsProcessing(false);
    window.clearInterval(recordingTimerRef.current);
    recordingTimerRef.current = undefined;
    const id = recordingIdRef.current;
    recordingIdRef.current = null;
    const worker = encodedWorkerRef.current;
    stopEncodedMicCapture();
    if (worker) {
      worker.postMessage({ type: "stop", abort: true });
    }
    if (id) {
      window.openNow.abortRecording({ recordingId: id }).catch(() => undefined);
    }
  }, [stopEncodedMicCapture]);

  // Instant Replay save (official client's Ctrl+0): mux the armed DVR ring's
  // window into a standalone clip through the same recording pipeline as a
  // manual recording (zero re-encode — the worker remuxes the buffered
  // bitstream). Never runs while a manual recording or another save is in
  // flight; the worker stays armed afterwards so the ring keeps filling.
  const saveInstantReplay = useCallback(async (): Promise<InstantReplaySaveResult> => {
    if (!instantReplayEnabled || !recordingApiAvailable) return "disabled";
    if (isProcessing || recordingIdRef.current || instantReplaySaveRef.current) return "busy";

    if (nativeRecordingEnabled) {
      // Native mode has no WebRTC MediaStream to slice: the streamer's own
      // DVR ring (armed at session start from `instantReplaySeconds`) is
      // snapshotted natively and remuxed offline — the same zero-re-encode
      // model, entirely inside the streamer process. The clip flows through
      // the normal recording API (begin → native save → finish).
      const nativeSave = window.openNow?.saveNativeInstantReplay;
      if (typeof nativeSave !== "function") return "unavailable";
      let recordingId: string;
      try {
        const result = await window.openNow.beginRecording({ mimeType: "video/mp4" });
        recordingId = result.recordingId;
      } catch (error) {
        console.error("[StreamView] Instant Replay could not open a recording:", error);
        return "failed";
      }
      setIsSavingInstantReplay(true);
      // The save owns the recording id for its whole duration: toggleRecording
      // and a second save check this ref, so a manual start cannot interleave
      // with the native remux (same contract as the web save's worker ref).
      instantReplaySaveRef.current = {
        recordingId,
        resolve: () => undefined,
        reject: () => undefined,
      };
      try {
        const result = await nativeSave(recordingId);
        const entry = await window.openNow.finishRecording({
          recordingId,
          durationMs: Math.max(1_000, Math.round(result.durationMs ?? 0)),
          gameTitle,
          thumbnailDataUrl: result.thumbnailBase64
            ? `data:image/jpeg;base64,${result.thumbnailBase64}`
            : undefined,
        });
        setRecordings((prev) => [entry, ...prev].slice(0, RECORDING_LIST_CAP));
        console.info(
          `[StreamView] Instant Replay saved (native): ${Math.round(result.durationMs ?? 0)}ms ring window`,
        );
        return "saved";
      } catch (error) {
        console.error("[StreamView] Instant Replay save failed (native):", error);
        window.openNow.abortRecording({ recordingId }).catch(() => undefined);
        // Readiness rejections (young ring) are not failures: the player
        // just has to wait a moment before the next attempt.
        if (error instanceof Error && /still filling|not ready yet/.test(error.message)) {
          return "unavailable";
        }
        return "failed";
      } finally {
        if (instantReplaySaveRef.current?.recordingId === recordingId) {
          instantReplaySaveRef.current = null;
        }
        setIsSavingInstantReplay(false);
      }
    }

    const video = videoRef.current;
    const stream = (video?.srcObject ?? null) as MediaStream | null;
    const pc = getActiveWebRtcPeerConnection();
    const worker = encodedWorkerRef.current;
    // The armed worker + attached transforms ARE the ring: without them there
    // is nothing to save (IR just enabled, arm failed, or a HEVC session).
    if (!video || !stream || !pc || !worker || encodedArmedPeerConnRef.current !== pc) {
      return "unavailable";
    }
    const info = inspectEncodedCapture(pc, video, stream);
    if (!info) return "unavailable";

    let recordingId: string;
    try {
      const result = await window.openNow.beginRecording({ mimeType: info.mimeType });
      recordingId = result.recordingId;
    } catch (error) {
      console.error("[StreamView] Instant Replay could not open a recording:", error);
      return "failed";
    }

    // Thumbnail from the current frame — same recipe as the manual stop.
    let thumbnailDataUrl: string | undefined;
    if (video.videoWidth > 0 && video.videoHeight > 0) {
      const { width: thumbWidth, height: thumbHeight } = fitThumbnailSize(
        video.videoWidth,
        video.videoHeight,
      );
      const canvas = document.createElement("canvas");
      canvas.width = thumbWidth;
      canvas.height = thumbHeight;
      const context = canvas.getContext("2d");
      if (context) {
        context.drawImage(video, 0, 0, thumbWidth, thumbHeight);
        thumbnailDataUrl = canvas.toDataURL("image/jpeg", 0.72);
      }
    }

    setIsSavingInstantReplay(true);
    // Set the chunk route BEFORE posting init: a setup failure rejects this
    // promise through the shared worker error handler.
    const donePromise = new Promise<number>((resolve, reject) => {
      instantReplaySaveRef.current = { recordingId, resolve, reject };
    });
    try {
      // Re-init so the muxer gets the live dimensions/codec params (the
      // arm-time init carried none); the mic chain stays out of the snapshot
      // — the ring carries the game audio track only.
      worker.postMessage({
        type: "init",
        codec: info.codec,
        container: info.container,
        width: info.width,
        height: info.height,
        fps: info.fps,
        hasAudio: info.hasAudio,
        audioChannels: info.audioChannels,
        audioSampleRate: info.audioSampleRate,
        mixMic: false,
        dvrSeconds,
      });
      worker.postMessage({ type: "save" });
      const durationMs = await Promise.race([
        donePromise,
        new Promise<never>((_, reject) =>
          window.setTimeout(() => reject(new Error("Instant Replay save timed out")), 10_000),
        ),
      ]);
      // Chunks arrive strictly before 'done'; drain the send chain before
      // finishing so every byte of the clip is on disk.
      await chunkSendChainRef.current;
      const entry = await window.openNow.finishRecording({
        recordingId,
        durationMs: Math.max(1_000, Math.round(durationMs)),
        gameTitle,
        thumbnailDataUrl,
      });
      setRecordings((prev) => [entry, ...prev].slice(0, RECORDING_LIST_CAP));
      console.info(`[StreamView] Instant Replay saved: ${Math.round(durationMs)}ms ring window`);
      return "saved";
    } catch (error) {
      console.error("[StreamView] Instant Replay save failed:", error);
      window.openNow.abortRecording({ recordingId }).catch(() => undefined);
      // Readiness rejections (young ring, no keyframe yet) are not failures:
      // the player just has to wait a moment before the next attempt.
      if (error instanceof Error && /still filling|not ready yet/.test(error.message)) {
        return "unavailable";
      }
      return "failed";
    } finally {
      instantReplaySaveRef.current = null;
      setIsSavingInstantReplay(false);
    }
  }, [
    dvrSeconds,
    gameTitle,
    instantReplayEnabled,
    isProcessing,
    nativeRecordingEnabled,
    recordingApiAvailable,
    videoRef,
  ]);

  // Arm the DVR recorder at session start (GFN parity): spawn the encoded
  // worker, attach the transforms and let its ring buffer the last N seconds
  // of keyframe-aligned video from the moment the stream is up — long before
  // the user presses record — so every recording carries a pre-roll. Runs
  // once per peer connection; the worker then lives for the session.
  const armEncodedRecorder = useCallback((): void => {
    const pc = getActiveWebRtcPeerConnection();
    if (!pc || encodedArmedPeerConnRef.current === pc || !dvrSeconds) return;
    const codecOnly = inspectEncodedCodecOnly(pc);
    if (!codecOnly) return; // unsupported codec / no transform support — the
    // on-demand path falls back to the MediaRecorder recorder at record time.
    const videoReceiver = pc.getReceivers().find((receiver) => receiver.track?.kind === "video") ?? null;
    const audioReceiver = pc.getReceivers().find((receiver) => receiver.track?.kind === "audio") ?? null;
    if (!videoReceiver) return;
    // Don't re-arm while a recording is in flight (startEncodedRecording
    // already owns the worker then).
    if (recordingIdRef.current) return;
    let worker = encodedWorkerRef.current;
    if (worker) {
      // Reuse the worker for a new session: cancel any stale recording state.
      worker.postMessage({ type: "stop", abort: true });
      stopEncodedMicCapture();
    } else {
      worker = new Worker(
        new URL("../recording/recordEncodedWorker.ts", import.meta.url),
        { type: "module" },
      );
      encodedWorkerRef.current = worker;
      worker.onmessage = handleEncodedWorkerMessage;
      worker.onerror = handleEncodedWorkerError;
    }
    // Codec-only init (dimensions may not be resolvable yet — the muxer is
    // created per-recording with the full info at `start`, so this is enough
    // to arm the ring). The worker acks 'ready'; only then do we attach the
    // transforms so no frame can arrive before the rings exist.
    worker.postMessage({
      type: "init",
      codec: codecOnly.codec,
      container: codecOnly.container,
      width: 0,
      height: 0,
      fps: 60,
      hasAudio: codecOnly.hasAudio,
      audioChannels: codecOnly.audioChannels,
      audioSampleRate: codecOnly.audioSampleRate,
      mixMic,
      dvrSeconds,
    });
    const ready = new Promise<void>((resolve, reject) => {
      encodedReadyRef.current = { resolve, reject };
    });
    void ready
      .then(() => {
        videoReceiver.transform = new RTCRtpScriptTransform(worker as Worker, { kind: "video" });
        if (audioReceiver) {
          audioReceiver.transform = new RTCRtpScriptTransform(worker as Worker, { kind: "audio" });
        }
        encodedArmedPeerConnRef.current = pc;
        encodedReceiversRef.current = { video: videoReceiver, audio: audioReceiver };
        console.info(
          `[StreamView] Encoded DVR armed: ${codecOnly.codec} pre-roll ${dvrSeconds}s (ring fills from session start)`,
        );
      })
      .catch((error: unknown) => {
        // Arm failed (e.g. worker crash): fall back to on-demand recording.
        console.warn("[StreamView] Encoded DVR arm failed — recording will start on demand:", error);
        worker?.terminate();
        encodedWorkerRef.current = null;
        encodedReadyRef.current = null;
      });
  }, [handleEncodedWorkerError, handleEncodedWorkerMessage, mixMic, dvrSeconds, stopEncodedMicCapture]);

  // Arm once the session's stream is up (video element has frames and the
  // peer connection exists). Re-arms when a NEW session starts (streamActive
  // flips true again); the guard inside armEncodedRecorder makes it a no-op
  // while the same peer connection is already armed.
  useEffect(() => {
    if (!dvrSeconds || !streamActive || !streamVideoReady) return;
    const pc = getActiveWebRtcPeerConnection();
    if (!pc) return;
    armEncodedRecorder();
  }, [armEncodedRecorder, dvrSeconds, streamActive, streamVideoReady]);

  /**
   * Stop the active WebCodecs recording: flush the encoders, finalize the
   * muxer (both ends of the FIFO drain into a complete fragmented MP4), drain
   * the chunk queue and finish the recording — the same end-to-end contract
   * as the MediaRecorder onstop flow. `abort: true` (session teardown or a
   * mid-recording encoder failure) drops the file instead.
   */
  const stopWebCodecsRecording = useCallback(
    async (stopOptions?: { abort?: boolean }): Promise<void> => {
      const recorder = webCodecsRecorderRef.current;
      if (!recorder) return;
      webCodecsRecorderRef.current = null;
      window.clearInterval(recordingTimerRef.current);
      recordingTimerRef.current = undefined;
      setIsRecording(false);
      const id = recordingIdRef.current;
      recordingIdRef.current = null;

      if (stopOptions?.abort) {
        recorder.abort();
        audioCtxRef.current?.close().catch(() => undefined);
        audioCtxRef.current = null;
        webCodecsExpectedFpsRef.current = null;
        if (id) {
          window.openNow.abortRecording({ recordingId: id }).catch(() => undefined);
        }
        setRecordingError("Recording encountered an error.");
        return;
      }

      let result: WebCodecsRecordingResult;
      try {
        result = await recorder.stop();
      } catch (error) {
        console.error("[StreamView] Failed to finalize WebCodecs recording:", error);
        setRecordingError("Recording could not be saved.");
        if (id) {
          window.openNow.abortRecording({ recordingId: id }).catch(() => undefined);
        }
        return;
      } finally {
        audioCtxRef.current?.close().catch(() => undefined);
        audioCtxRef.current = null;
      }

      const expectedFps = webCodecsExpectedFpsRef.current;
      webCodecsExpectedFpsRef.current = null;
      const durationMs = Date.now() - recordingStartTimeRef.current;
      // Frames missing against the stream's own rate: the honest "device
      // could not keep up" signal for this path. The recorder drops frames
      // from the FILE only — encoder-queue skips are counted live, MSTP's
      // live-edge drops are not, so the shortfall is the truthful number.
      if (expectedFps !== null) {
        const lostFrames = computeRecordingFrameShortfall(result.videoFrames, durationMs, expectedFps);
        if (lostFrames > 0) {
          setRecordingDropNotice(
            `Recording missed ${lostFrames} frame${lostFrames === 1 ? "" : "s"} — the device could not keep up.`,
          );
        }
      }
      if (!id) return;
      // Worker chunks are already queued in the send chain (message order);
      // drain it before finishing so every byte of the file is on disk.
      await chunkSendChainRef.current;
      try {
        const entry = await window.openNow.finishRecording({
          recordingId: id,
          durationMs,
          gameTitle,
          thumbnailDataUrl: thumbnailDataUrlRef.current ?? undefined,
        });
        setRecordings((prev) => [entry, ...prev].slice(0, RECORDING_LIST_CAP));
        thumbnailDataUrlRef.current = null;
      } catch (error) {
        console.error("[StreamView] Failed to finish recording:", error);
        setRecordingError("Recording could not be saved.");
      }
    },
    [gameTitle],
  );

  /**
   * WebCodecs recorder start for within-cap streams (web mode). Taps the live
   * remote video track and the composed audio track with
   * MediaStreamTrackProcessor, re-encodes with hardware WebCodecs encoders
   * and streams fragmented MP4 straight into the recording file. The MSTP
   * ingest is what protects the stream: a recorder that cannot keep up drops
   * frames from the file and never backpressures the WebRTC receive pipeline
   * (MediaRecorder on the raw track did exactly that — the field stutter).
   *
   * Returns false (with nothing left behind) when the path cannot start; the
   * caller then falls back to the MediaRecorder raw-track flow.
   */
  const startWebCodecsRecording = useCallback(
    async (input: {
      videoTrack: MediaStreamTrack;
      audioTrack: MediaStreamTrack | null;
    }): Promise<boolean> => {
      const expectedFps = resolveRecordingStreamFps(
        streamFps,
        input.videoTrack.getSettings().frameRate,
      );
      const currentVideo = videoRef.current;
      const width = currentVideo?.videoWidth ?? 0;
      const height = currentVideo?.videoHeight ?? 0;

      let recordingId: string;
      try {
        const result = await window.openNow.beginRecording({ mimeType: "video/mp4" });
        recordingId = result.recordingId;
      } catch (error) {
        console.error("[StreamView] Failed to begin WebCodecs recording:", error);
        return false;
      }

      let recorder: WebCodecsTrackRecorder;
      try {
        recorder = await startWebCodecsTrackRecorder({
          videoTrack: input.videoTrack,
          audioTrack: input.audioTrack,
          width,
          height,
          fps: expectedFps,
          videoBitrate: clampRecordingBitrate(recordingBitrateMbps, "webcodecs-track"),
          onChunk: (chunk) => {
            chunkSendChainRef.current = chunkSendChainRef.current
              .then(() => window.openNow.sendRecordingChunk({ recordingId, chunk }))
              .catch((error: unknown) => {
                console.error("[StreamView] Failed to send recording chunk:", error);
              });
          },
          onDiagnostic: (message) => console.info(`[StreamView] ${message}`),
          onFatal: (error) => {
            console.error("[StreamView] WebCodecs recorder failed mid-recording:", error);
            void stopWebCodecsRecording({ abort: true });
          },
          workerFactory: () =>
            new Worker(new URL("../recording/recordWebCodecsWorker.ts", import.meta.url), {
              type: "module",
            }),
        });
      } catch (error) {
        console.warn(
          "[StreamView] WebCodecs recorder could not start — falling back to MediaRecorder:",
          error,
        );
        window.openNow.abortRecording({ recordingId }).catch(() => undefined);
        return false;
      }

      webCodecsRecorderRef.current = recorder;
      webCodecsExpectedFpsRef.current = expectedFps;
      recordingIdRef.current = recordingId;
      thumbnailDataUrlRef.current = null;
      recordingStartTimeRef.current = Date.now();
      setRecordingDurationMs(0);
      setRecordingDropNotice(null);
      setIsRecording(true);
      recordingTimerRef.current = window.setInterval(() => {
        setRecordingDurationMs(Date.now() - recordingStartTimeRef.current);
      }, 500);
      if (currentVideo && width > 0 && height > 0) {
        thumbnailFromSource(currentVideo, width, height);
      }
      return true;
    },
    [recordingBitrateMbps, stopWebCodecsRecording, streamFps, thumbnailFromSource, videoRef],
  );

  const toggleRecording = useCallback(async () => {
    setRecordingError(null);

    // An Instant Replay save owns the worker + its own recording id for a few
    // hundred milliseconds; a start interleaving with it would corrupt both.
    if (instantReplaySaveRef.current) return;

    if (isRecording) {
      if (nativeRecordingEnabled) {
        // Native: finalize the encoder (flush remaining chunks + OFFLINE remux
        // into the final MP4 — takes a few seconds for longer clips), then save
        // via the same recording API. isRecording flips first so a second click
        // during finalization cannot re-enter; the PROCESSING pill covers the
        // remux window so the UI shows feedback instead of appearing frozen.
        setIsRecording(false);
        setIsProcessing(true);
        window.clearInterval(recordingTimerRef.current);
        recordingTimerRef.current = undefined;
        const id = recordingIdRef.current;
        recordingIdRef.current = null;
        let thumbnailBase64: string | undefined;
        try {
          const result = await window.openNow.stopNativeRecording();
          thumbnailBase64 = result.thumbnailBase64;
          if (result.droppedFrames > 0) {
            setRecordingDropNotice(
              `Recording lost ${result.droppedFrames} frame${result.droppedFrames === 1 ? "" : "s"} (the device could not keep up).`,
            );
          }
        } catch (error) {
          console.error("[StreamView] Failed to finalize native recording:", error);
          setRecordingError("Recording could not be saved.");
          if (id) {
            window.openNow.abortRecording({ recordingId: id }).catch(() => undefined);
          }
          setIsProcessing(false);
          return;
        }
        if (!id) {
          setIsProcessing(false);
          return;
        }
        const durationMs = Date.now() - recordingStartTimeRef.current;
        try {
          const entry = await window.openNow.finishRecording({
            recordingId: id,
            durationMs,
            gameTitle,
            // Thumbnail from the native streamer's first encoded frame (JPEG).
            thumbnailDataUrl: thumbnailBase64
              ? `data:image/jpeg;base64,${thumbnailBase64}`
              : undefined,
          });
          setRecordings((prev) => [entry, ...prev].slice(0, RECORDING_LIST_CAP));
        } catch (error) {
          console.error("[StreamView] Failed to finish native recording:", error);
          setRecordingError("Recording could not be saved.");
        }
        setIsProcessing(false);
        return;
      }
      if (encodedWorkerRef.current) {
        void stopEncodedRecording();
        return;
      }
      if (webCodecsRecorderRef.current) {
        void stopWebCodecsRecording();
        return;
      }
      mediaRecorderRef.current?.stop();
      return;
    }

    if (!recordingApiAvailable) {
      setRecordingError("Recording API unavailable. Restart OpenNOW to enable recording.");
      return;
    }

    if (nativeRecordingEnabled) {
      // No <video> frames in native mode — the native streamer encodes the
      // grabbed frames itself and streams the chunks back.
      let recordingId: string;
      try {
        const result = await window.openNow.beginRecording({ mimeType: "video/mp4" });
        recordingId = result.recordingId;
        await window.openNow.startNativeRecording(recordingId);
      } catch (error) {
        console.error("[StreamView] Failed to start native recording:", error);
        // Surface the native streamer's reason (e.g. the disk-space guard
        // refusing a start) instead of hiding it behind a generic message.
        const detail = error instanceof Error && error.message ? error.message : "";
        setRecordingError(detail ? `Could not start recording: ${detail}` : "Could not start recording.");
        return;
      }
      recordingIdRef.current = recordingId;
      recordingStartTimeRef.current = Date.now();
      setRecordingDurationMs(0);
      setRecordingDropNotice(null);
      setIsRecording(true);
      setIsProcessing(false);
      recordingTimerRef.current = window.setInterval(() => {
        setRecordingDurationMs(Date.now() - recordingStartTimeRef.current);
      }, 500);
      return;
    }

    const video = videoRef.current;
    if (!video || !video.srcObject) {
      setRecordingError("Stream is not ready for recording yet.");
      return;
    }

    const stream = video.srcObject as MediaStream;
    const recordCap = resolveRecordCap(recordingResolution, recordingFps);
    const streamVideoTrack = stream.getVideoTracks()[0] ?? null;
    // Receiver-side encoded capture (bitstream pre-decode) wins whenever it is
    // available: zero re-encode, so the recording cap does not apply.
    const encodedCapture = inspectEncodedCapture(getActiveWebRtcPeerConnection(), video, stream);
    if (encodedCapture === null) {
      // Make the HEVC bypass visible from the exported log: Chromium never
      // delivers receiver-transform frames for H.265 (runtime-verified), so
      // the recorder silently falls back to the MediaRecorder path here.
      const pc = getActiveWebRtcPeerConnection();
      const videoMime =
        pc?.getReceivers().find((receiver) => receiver.track?.kind === "video")?.getParameters()
          .codecs[0]?.mimeType ?? "";
      const videoMimeLower = videoMime.toLowerCase();
      if (videoMimeLower.includes("hevc") || videoMimeLower.includes("h265")) {
        console.info(
          "[StreamView] HEVC negotiated — Chromium does not deliver receiver encoded transforms for H.265; the recording re-encodes the frames instead.",
        );
      }
    }
    const { strategy, mimeType, reason } = await selectRecordingStrategy(
      (candidate) => MediaRecorder.isTypeSupported(candidate),
      {
        width: video.videoWidth,
        height: video.videoHeight,
        fps: resolveRecordingStreamFps(streamFps, streamVideoTrack?.getSettings().frameRate),
      },
      recordCap,
      encodedCapture !== null,
      isWebCodecsRecordingSupported(),
    );
    const effectiveMimeType =
      strategy === "encoded-transform" && encodedCapture ? encodedCapture.mimeType : mimeType;
    console.info(`[StreamView] Recording strategy: ${strategy} — ${reason}`);
    setUsedMimeType(effectiveMimeType);
    setUsedStrategy(strategy);
    if (strategy === "encoded-transform" && encodedCapture) {
      await startEncodedRecording(encodedCapture, effectiveMimeType);
      return;
    }

    // The whole web start path below is guarded: a throw anywhere in the
    // audio/canvas/MediaRecorder setup used to surface as an unhandled
    // promise rejection ("Uncaught (in promise) …") with no UI feedback and
    // leaked worker/generator/AudioContext — the field "clicking record does
    // nothing". Every failure now cleans up and shows a visible error.
    const audioCtx = new AudioContext();
    audioCtxRef.current = audioCtx;
    const audioDest = audioCtx.createMediaStreamDestination();

    const audioElement = audioRef.current;
    const gameAudioStream = audioElement?.srcObject instanceof MediaStream ? audioElement.srcObject : null;
    if (gameAudioStream && gameAudioStream.getAudioTracks().length > 0) {
      audioCtx.createMediaStreamSource(gameAudioStream).connect(audioDest);
    }

    if (micTrack && micTrack.readyState === "live") {
      const micStream = new MediaStream([micTrack]);
      audioCtx.createMediaStreamSource(micStream).connect(audioDest);
    }

    if (strategy === "webcodecs-track" && streamVideoTrack) {
      const started = await startWebCodecsRecording({
        videoTrack: streamVideoTrack,
        audioTrack: audioDest.stream.getAudioTracks()[0] ?? null,
      });
      if (started) return;
      // The WebCodecs recorder could not start (unsupported configuration or
      // a worker failure at init) and has already aborted its file. Fall
      // through to MediaRecorder on the raw track — the same within-cap
      // reasoning that selected WebCodecs, through Chromium's own encoder.
      // rawTrackMode below keeps the canvas setup out of this fallback.
      setUsedStrategy("raw-track");
    }

    // Raw-track path (also the WebCodecs fallback when its start failed): no
    // canvas, no draw loop — MediaRecorder encodes every decoded frame
    // directly. Canvas-downscale path (taken when the stream exceeds the cap
    // or no AVC container is available): draw at most one frame per
    // record-period into a capped canvas so a full-res software re-encode
    // can't starve the main thread that also runs the WebRTC decoder. The cap
    // follows the user's recordingResolution/recordingFps settings.
    const rawTrackMode = strategy === "raw-track" || strategy === "webcodecs-track";
    // Cap resolved once above — the strategy gate and the canvas draw loop
    // below share it, so the raw-track path can never outrun the user's
    // recordingResolution/recordingFps settings.
    const recordFps = recordCap.fps;
    // Canvas path only: actual draw count vs the target rate is compared at
    // stop (computeRecordingFrameShortfall) so the user is told when the
    // recording was choppier than its nominal fps. The raw-track path has no
    // draw loop, so there is nothing to count.
    let drawnFrames = 0;
    let recordVideoTrack: MediaStreamTrack | null = null;
    try {
    if (!rawTrackMode && video.videoWidth > 0 && video.videoHeight > 0) {
      const scale = Math.min(
        1,
        recordCap.width / video.videoWidth,
        recordCap.height / video.videoHeight,
      );
      const capWidth = Math.max(1, Math.round(video.videoWidth * scale));
      const capHeight = Math.max(1, Math.round(video.videoHeight * scale));
      // Worker path (preferred): downscale off the renderer main thread. The
      // worker draws each frame into an OffscreenCanvas and returns a
      // GPU-backed ImageBitmap; we feed it to a MediaStreamTrackGenerator
      // whose track MediaRecorder consumes. The only main-thread work left is
      // the frame handoff (VideoFrame in, ImageBitmap out) — no synchronous
      // drawImage, no canvas rasterize timer.
      try {
        // Early capability probe — see getGeneratorTrackResolver. Runs BEFORE
        // the worker, generator, or rVFC chain exist: when this runtime
        // cannot produce a usable generator track (Chromium removed the
        // legacy `readable` accessor; the generator object itself is now the
        // track), throw immediately so the catch below tears down nothing and
        // recording falls through to captureStream with zero half-started
        // state — no worker spawn, no frame loop to cancel.
        const generatorTrackResolver = getGeneratorTrackResolver();
        if (!generatorTrackResolver) {
          throw new Error("MediaStreamTrackGenerator unavailable in this runtime.");
        }
        const recordWorker = new Worker(
          new URL("../recording/recordCanvasWorker.ts", import.meta.url),
          { type: "module" },
        );
        recordWorkerRef.current = recordWorker;
        const generator = new MediaStreamTrackGenerator({ kind: "video" });
        videoGeneratorRef.current = generator;
        videoGeneratorWriterRef.current = generator.writable.getWriter();
        recordVideoTrack = generatorTrackResolver(generator);
        if (!recordVideoTrack) {
          // e.g. a runtime where the generator's track is unavailable — the
          // contentHint-on-undefined field failure class. Fall through to the
          // captureStream fallback below instead of crashing the start.
          throw new Error("MediaStreamTrackGenerator produced no video track.");
        }
        recordVideoTrack.contentHint = "detail";
        recordingVideoTrackRef.current = recordVideoTrack;
      recordWorker.onmessage = (
        event: MessageEvent<RecordCanvasWorkerOutboundMessage>,
      ): void => {
        const message = event.data;
        if (message.type === "bitmap") {
          // Wrap the returned bitmap in a frame carrying the pacing timestamp
          // the draw loop assigned, then hand it to the encoder.
          const videoFrame = new VideoFrame(message.bitmap, {
            timestamp: message.timestamp,
          });
          message.bitmap.close();
          const writer = videoGeneratorWriterRef.current;
          if (!writer) {
            // Cleanup already ran (recording stopped) — drop the frame.
            videoFrame.close();
            writePendingRef.current = false;
            return;
          }
          const write = writer.write(videoFrame);
          write
            .then(() => {
              writePendingRef.current = false;
            })
            .catch(() => {
              writePendingRef.current = false;
            });
        } else if (message.type === "thumb") {
          thumbnailDataUrlRef.current = message.dataUrl;
        }
      };
      const { width: thumbWidth, height: thumbHeight } = fitThumbnailSize(capWidth, capHeight);
      recordWorker.postMessage({
        type: "init",
        width: capWidth,
        height: capHeight,
        thumbWidth,
        thumbHeight,
      });
      // Hand one frame to the worker per record-period, at most — draw only
      // when a NEW video frame is presented and the record-period elapsed. If
      // the previous frame is still in flight (worker draw or generator write
      // pending), skip: the missed frames surface in the shortfall notice at
      // stop instead of backing up the pipeline.
      let lastDrawMs = 0;
      let frameTimestampUs = 0;
      const drawLatestFrame = (now: number): void => {
        recordingFrameCallbackRef.current = video.requestVideoFrameCallback(drawLatestFrame);
        if (video.videoWidth === 0 || video.readyState < 2) {
          return;
        }
        if (now - lastDrawMs < 1000 / recordFps) {
          return;
        }
        lastDrawMs = now;
        if (writePendingRef.current) {
          return;
        }
        writePendingRef.current = true;
        drawnFrames += 1;
        const timestamp = frameTimestampUs;
        frameTimestampUs += Math.round(1_000_000 / recordFps);
        let videoFrame: VideoFrame;
        try {
          videoFrame = new VideoFrame(video);
        } catch (error) {
          console.error("[StreamView] Failed to grab recording frame:", error);
          writePendingRef.current = false;
          return;
        }
        recordWorker.postMessage({ type: "frame", frame: videoFrame, timestamp }, [videoFrame]);
      };
        recordingFrameCallbackRef.current = video.requestVideoFrameCallback(drawLatestFrame);
      } catch (workerError) {
        // Tear down whatever the worker path partially created before it
        // threw, then fall back to the universal captureStream path below.
        // The worker draw loop must be un-registered explicitly: it keeps
        // re-queuing requestVideoFrameCallback and posting frames to the now-
        // terminated worker forever (inflating the drop accounting and
        // leaking an rVFC chain) if left running.
        console.warn(
          "[StreamView] Worker recording path unavailable — falling back to canvas.captureStream:",
          workerError,
        );
        const frameVideo = videoRef.current;
        if (recordingFrameCallbackRef.current !== undefined && frameVideo) {
          frameVideo.cancelVideoFrameCallback?.(recordingFrameCallbackRef.current);
        }
        recordingFrameCallbackRef.current = undefined;
        stopRecordingVideoTrack();
        recordVideoTrack = null;
      }
      if (!recordVideoTrack) {
        // Fallback: classic main-thread canvas + captureStream (universal,
        // no MediaStreamTrackGenerator needed). Draw the first frame
        // synchronously so the track exists immediately — a blank canvas can
        // hand back a stream with no video track (the field "Cannot set
        // properties of undefined (setting 'contentHint')" failure).
        const cap = document.createElement("canvas");
        cap.width = capWidth;
        cap.height = capHeight;
        const capCtx = cap.getContext("2d", { alpha: false });
        if (capCtx) {
          capCtx.drawImage(video, 0, 0, capWidth, capHeight);
        }
        // captureStream must run at a NON-ZERO rate: with 0, the track only
        // emits on canvas invalidation, which this Chromium never delivers
        // for a detached canvas — the MediaRecorder saw a single frame and
        // the file came back as a frozen black frame. With a real rate the
        // track samples the canvas on its own rasterize timer, so every
        // drawn frame is captured and the timeline stays alive.
        const capStream = cap.captureStream(recordFps);
        recordVideoTrack = capStream.getVideoTracks()[0] ?? null;
        if (!recordVideoTrack) {
          throw new Error("Canvas captureStream produced no video track.");
        }
        recordVideoTrack.contentHint = "detail";
        recordingVideoTrackRef.current = recordVideoTrack;
        recordFallbackCanvasRef.current = cap;
        // Main-thread draw loop, bounded to the recording frame rate (the
        // field-proven pre-worker loop): draw at most one frame per
        // record-period into the capped canvas.
        let lastDrawMs = 0;
        const drawFallbackFrame = (now: number): void => {
          recordingFrameCallbackRef.current = video.requestVideoFrameCallback(drawFallbackFrame);
          if (video.videoWidth === 0 || video.readyState < 2) {
            return;
          }
          if (now - lastDrawMs < 1000 / recordFps) {
            return;
          }
          lastDrawMs = now;
          drawnFrames += 1;
          if (capCtx) {
            capCtx.drawImage(video, 0, 0, capWidth, capHeight);
          }
        };
        recordingFrameCallbackRef.current = video.requestVideoFrameCallback(drawFallbackFrame);
      }
    }
    } catch (error) {
      console.error("[StreamView] Failed to set up web recording source:", error);
      const frameVideo = videoRef.current;
      if (recordingFrameCallbackRef.current !== undefined && frameVideo) {
        frameVideo.cancelVideoFrameCallback?.(recordingFrameCallbackRef.current);
      }
      recordingFrameCallbackRef.current = undefined;
      stopRecordingVideoTrack();
      audioCtxRef.current?.close().catch(() => undefined);
      audioCtxRef.current = null;
      setRecordingError("Could not start recording.");
      return;
    }
    const videoTracksForRecord =
      !rawTrackMode && recordVideoTrack ? [recordVideoTrack] : stream.getVideoTracks();

    const composed = new MediaStream([
      ...videoTracksForRecord,
      ...audioDest.stream.getAudioTracks(),
    ]);

    let recordingId: string;
    try {
      const result = await window.openNow.beginRecording({ mimeType });
      recordingId = result.recordingId;
    } catch (error) {
      console.error("[StreamView] Failed to begin recording:", error);
      const frameVideo = videoRef.current;
      if (recordingFrameCallbackRef.current !== undefined && frameVideo) {
        frameVideo.cancelVideoFrameCallback?.(recordingFrameCallbackRef.current);
      }
      recordingFrameCallbackRef.current = undefined;
      stopRecordingVideoTrack();
      audioCtx.close().catch(() => undefined);
      audioCtxRef.current = null;
      setRecordingError("Could not start recording.");
      return;
    }      recordingIdRef.current = recordingId;
    thumbnailDataUrlRef.current = null;
    recordFallbackCanvasRef.current = null;
    recordingStartTimeRef.current = Date.now();
    setRecordingDurationMs(0);
    setRecordingDropNotice(null);
    setIsRecording(true);

    recordingTimerRef.current = window.setInterval(() => {
      setRecordingDurationMs(Date.now() - recordingStartTimeRef.current);
    }, 500);

    let isFirstChunk = true;
    const recorderOptions: MediaRecorderOptions = { mimeType };
    const recordBitrate = clampRecordingBitrate(recordingBitrateMbps, strategy);
    if (recordBitrate !== undefined) {
      // Canvas path caps at 12 Mbps — 720p30 never benefits from more, and the
      // cap avoids burning encode budget on a bitrate the picture can't use (a
      // major source of stream FPS drops during recording). The raw-track path
      // records at stream resolution, so the user's explicit bitrate is honored
      // up to the slider's ceiling instead. Auto mode (null) is left untouched
      // in both: Chromium's conservative resolution-based default is already
      // below the cap, so overriding it would only add encode load.
      recorderOptions.videoBitsPerSecond = recordBitrate;
    }
    try {
    const recorder = new MediaRecorder(composed, recorderOptions);
    console.info(`[StreamView] MediaRecorder active — mimeType "${recorder.mimeType}"`);

    recorder.ondataavailable = (event: BlobEvent) => {
      if (!event.data || event.data.size === 0) return;

      if (isFirstChunk) {
        isFirstChunk = false;
        // Canvas path: the worker already posted the thumbnail after its
        // first drawn frame. The captureStream fallback has no worker — take
        // the thumbnail from its (already downscaled) canvas. Raw-track mode
        // has neither — fall back to the presented video frame (cheap there,
        // no draw loop is running).
        if (rawTrackMode) {
          const currentVideo = videoRef.current;
          if (currentVideo && currentVideo.videoWidth > 0 && currentVideo.videoHeight > 0) {
            thumbnailFromSource(currentVideo, currentVideo.videoWidth, currentVideo.videoHeight);
          }
        } else if (recordFallbackCanvasRef.current) {
          const fallbackCanvas = recordFallbackCanvasRef.current;
          if (fallbackCanvas.width > 0 && fallbackCanvas.height > 0) {
            thumbnailFromSource(fallbackCanvas, fallbackCanvas.width, fallbackCanvas.height);
          }
        }
      }

      // Serialize the send: append this chunk (read + IPC + disk write) to
      // the send chain. This runs synchronously inside dataavailable, which
      // always fires before onstop — so by the time onstop drains the chain,
      // every chunk including the final one is already queued.
      const recordingId = recordingIdRef.current;
      if (!recordingId) return;
      chunkSendChainRef.current = chunkSendChainRef.current
        .then(() => event.data.arrayBuffer())
        .then((buffer) => window.openNow.sendRecordingChunk({ recordingId, chunk: buffer }))
        .catch((error: unknown) => {
          console.error("[StreamView] Failed to send recording chunk:", error);
        });
    };

    recorder.onstop = () => {
      window.clearInterval(recordingTimerRef.current);
      recordingTimerRef.current = undefined;
      const frameVideo = videoRef.current;
      if (recordingFrameCallbackRef.current !== undefined && frameVideo) {
        frameVideo.cancelVideoFrameCallback?.(recordingFrameCallbackRef.current);
      }
      recordingFrameCallbackRef.current = undefined;
      stopRecordingVideoTrack();
      audioCtxRef.current?.close().catch(() => undefined);
      audioCtxRef.current = null;
      const id = recordingIdRef.current;
      recordingIdRef.current = null;
      setIsRecording(false);

      if (!id) return;

      const durationMs = Date.now() - recordingStartTimeRef.current;
      // Frame-deficit accounting only applies to the canvas path — the
      // raw-track path has no draw loop to fall behind.
      if (strategy === "canvas-downscale") {
        const lostFrames = computeRecordingFrameShortfall(drawnFrames, durationMs, recordFps);
        if (lostFrames > 0) {
          setRecordingDropNotice(
            `Recording missed ${lostFrames} frame${lostFrames === 1 ? "" : "s"} — the device could not keep up.`,
          );
        }
      }
      // Drain the chunk queue first: finishRecording moves the temp file, so
      // any chunk still in flight (read or IPC write) would hit "Unknown
      // recording id" on the main process and truncate the file. The chain
      // always resolves (each link catches), so a failed send can't hang this
      // — only a genuinely wedged disk would.
      void chunkSendChainRef.current
        .then(() =>
          window.openNow.finishRecording({
            recordingId: id,
            durationMs,
            gameTitle,
            thumbnailDataUrl: thumbnailDataUrlRef.current ?? undefined,
          }),
        )
        .then((entry) => {
          setRecordings((prev) => [entry, ...prev].slice(0, RECORDING_LIST_CAP));
          thumbnailDataUrlRef.current = null;
        })
        .catch((error: unknown) => {
          console.error("[StreamView] Failed to finish recording:", error);
          setRecordingError("Recording could not be saved.");
        });
    };

    recorder.onerror = () => {
      window.clearInterval(recordingTimerRef.current);
      recordingTimerRef.current = undefined;
      const frameVideo = videoRef.current;
      if (recordingFrameCallbackRef.current !== undefined && frameVideo) {
        frameVideo.cancelVideoFrameCallback?.(recordingFrameCallbackRef.current);
      }
      recordingFrameCallbackRef.current = undefined;
      stopRecordingVideoTrack();
      audioCtxRef.current?.close().catch(() => undefined);
      audioCtxRef.current = null;
      const id = recordingIdRef.current;
      recordingIdRef.current = null;
      setIsRecording(false);
      thumbnailDataUrlRef.current = null;
      if (id) {
        window.openNow.abortRecording({ recordingId: id }).catch(() => undefined);
      }
      setRecordingError("Recording encountered an error.");
    };

    mediaRecorderRef.current = recorder;
    recorder.start(5000);
    } catch (error) {
      console.error("[StreamView] Failed to start web recording:", error);
      const frameVideo = videoRef.current;
      if (recordingFrameCallbackRef.current !== undefined && frameVideo) {
        frameVideo.cancelVideoFrameCallback?.(recordingFrameCallbackRef.current);
      }
      recordingFrameCallbackRef.current = undefined;
      stopRecordingVideoTrack();
      audioCtxRef.current?.close().catch(() => undefined);
      audioCtxRef.current = null;
      setRecordingError("Could not start recording.");
      return;
    }
  }, [
    abortEncodedRecording,
    audioRef,
    gameTitle,
    isRecording,
    micTrack,
    mixMic,
    nativeRecordingEnabled,
    recordingApiAvailable,
    recordingBitrateMbps,
    recordingFps,
    recordingResolution,
    startEncodedMicCapture,
    startEncodedRecording,
    startWebCodecsRecording,
    stopEncodedMicCapture,
    stopEncodedRecording,
    stopWebCodecsRecording,
    streamActive,
    videoRef,
  ]);

  useEffect(() => {
    return () => {
      window.clearInterval(recordingTimerRef.current);
      const frameVideo = videoRef.current;
      if (recordingFrameCallbackRef.current !== undefined && frameVideo) {
        frameVideo.cancelVideoFrameCallback?.(recordingFrameCallbackRef.current);
      }
      recordingFrameCallbackRef.current = undefined;
      stopRecordingVideoTrack();
      stopEncodedMicCapture();
      const encodedWorker = encodedWorkerRef.current;
      if (encodedWorker) {
        encodedWorkerRef.current = null;
        const receivers = encodedReceiversRef.current;
        encodedReceiversRef.current = null;
        encodedArmedPeerConnRef.current = null;
        if (receivers) {
          receivers.video.transform = null;
          if (receivers.audio) receivers.audio.transform = null;
        }
        encodedWorker.terminate();
      }
      const webCodecsRecorder = webCodecsRecorderRef.current;
      if (webCodecsRecorder) {
        webCodecsRecorderRef.current = null;
        webCodecsRecorder.abort();
      }
      webCodecsExpectedFpsRef.current = null;
      const recorder = mediaRecorderRef.current;
      const id = recordingIdRef.current;
      if (recorder && recorder.state !== "inactive") {
        recorder.stop();
      }
      if (id) {
        if (nativeRecordingEnabled) {
          window.openNow.abortNativeRecording().catch(() => undefined);
        }
        window.openNow.abortRecording({ recordingId: id }).catch(() => undefined);
        recordingIdRef.current = null;
      }
      // An Instant Replay save in flight at teardown: settle its promise so
      // the awaiting save exits, then drop the half-written file.
      const instantReplayId = instantReplaySaveRef.current?.recordingId;
      if (instantReplayId) {
        instantReplaySaveRef.current?.reject(new Error("stream session ended"));
        instantReplaySaveRef.current = null;
        window.openNow.abortRecording({ recordingId: instantReplayId }).catch(() => undefined);
      }
      audioCtxRef.current?.close().catch(() => undefined);
      audioCtxRef.current = null;
    };
  }, [nativeRecordingEnabled, stopEncodedMicCapture]);

  return {
    isRecording,
    isProcessing,
    isSavingInstantReplay,
    saveInstantReplay,
    recordings,
    recordingDurationMs,
    recordingError,
    recordingDropNotice,
    usedMimeType,
    usedStrategy,
    recCarouselRef,
    recordingApiAvailable,
    refreshRecordings,
    deleteRecording,
    scrollRecordings,
    toggleRecording,
  };
}
