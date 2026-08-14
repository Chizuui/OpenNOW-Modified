import { useCallback, useEffect, useRef, useState } from "react";
import type { RefObject } from "react";
import type { RecordingEntry } from "@shared/gfn";
import {
  clampRecordingBitrate,
  computeRecordingFrameShortfall,
  fitThumbnailSize,
  selectRecordingStrategy,
} from "../components/stream/streamRuntimeHelpers";
import type { RecordCanvasWorkerOutboundMessage } from "../recording/recordCanvasWorker";

// Web recording uses one of two strategies (see selectRecordingStrategy):
// "raw-track" hands MediaRecorder the incoming WebRTC track directly (hardware
// AVC encoder off the decode pipeline — the GFN-native model, ~zero main-thread
// cost), or "canvas-downscale" bounds the encode cost when only software codecs
// exist (720p30 by default; cap resolution/FPS are user-selectable via
// recordingResolution / recordingFps settings).
const DEFAULT_RECORD_CAP_WIDTH = 1280;
const DEFAULT_RECORD_CAP_HEIGHT = 720;
const DEFAULT_RECORD_CAP_FPS = 30;

const RECORD_CAP_BY_RESOLUTION: Record<string, { width: number; height: number }> = {
  "1440p": { width: 2560, height: 1440 },
  "1080p": { width: 1920, height: 1080 },
  "720p": { width: 1280, height: 720 },
};

// Resolver that extracts the video track a MediaStreamTrackGenerator exposes
// for MediaRecorder, regardless of which API shape the runtime ships.
type GeneratorTrackResolver = (generator: MediaStreamTrackGenerator) => MediaStreamTrack | null;

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
   * Native streamer mode: the renderer's <video> element has no frames, so
   * recording happens in the native pipeline (H.264 fragmented MP4) and its
   * chunks are streamed to the main process instead of MediaRecorder.
   */
  nativeRecordingEnabled: boolean;
}

export function useStreamRecorder({
  videoRef,
  audioRef,
  gameTitle,
  micTrack,
  recordingBitrateMbps,
  recordingResolution,
  recordingFps,
  nativeRecordingEnabled,
}: UseStreamRecorderOptions) {
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
  const mediaRecorderRef = useRef<MediaRecorder | null>(null);
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

  const deleteRecording = useCallback(async (id: string) => {
    setRecordingError(null);
    if (!recordingApiAvailable) return;
    try {
      await window.openNow.deleteRecording({ id });
      setRecordings((prev) => prev.filter((recording) => recording.id !== id));
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

  const toggleRecording = useCallback(async () => {
    setRecordingError(null);

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
          setRecordings((prev) => [entry, ...prev].slice(0, 20));
        } catch (error) {
          console.error("[StreamView] Failed to finish native recording:", error);
          setRecordingError("Recording could not be saved.");
        }
        setIsProcessing(false);
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
        setRecordingError("Could not start recording.");
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
    const { strategy, mimeType } = await selectRecordingStrategy(
      (candidate) => MediaRecorder.isTypeSupported(candidate),
    );
    setUsedMimeType(mimeType);

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

    // Raw-track path: no canvas, no draw loop — MediaRecorder encodes every
    // decoded frame directly. Canvas-downscale path (fallback for software-only
    // platforms): draw at most one frame per record-period into a capped canvas
    // so a full-res software re-encode can't starve the main thread that also
    // runs the WebRTC decoder. The cap follows the user's
    // recordingResolution/recordingFps settings.
    const rawTrackMode = strategy === "raw-track";
    const recordCap = RECORD_CAP_BY_RESOLUTION[recordingResolution]
      ?? { width: DEFAULT_RECORD_CAP_WIDTH, height: DEFAULT_RECORD_CAP_HEIGHT };
    const recordFps = Number.isFinite(recordingFps) && recordingFps > 0
      ? Math.min(60, Math.round(recordingFps))
      : DEFAULT_RECORD_CAP_FPS;
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

    // Downscale a frame source to the small JPEG thumbnail. Used only in
    // raw-track mode — the canvas path gets its thumbnail from the worker —
    // and the encode itself is at thumb size.
    const thumbnailFromSource = (
      source: CanvasImageSource,
      sourceWidth: number,
      sourceHeight: number,
    ): void => {
      const { width, height } = fitThumbnailSize(sourceWidth, sourceHeight);
      const canvas = document.createElement("canvas");
      canvas.width = width;
      canvas.height = height;
      const context = canvas.getContext("2d");
      if (context) {
        context.drawImage(source, 0, 0, width, height);
        thumbnailDataUrlRef.current = canvas.toDataURL("image/jpeg", 0.72);
      }
    };

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
          setRecordings((prev) => [entry, ...prev].slice(0, 20));
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
    audioRef,
    gameTitle,
    isRecording,
    micTrack,
    nativeRecordingEnabled,
    recordingApiAvailable,
    recordingBitrateMbps,
    recordingFps,
    recordingResolution,
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
      audioCtxRef.current?.close().catch(() => undefined);
      audioCtxRef.current = null;
    };
  }, [nativeRecordingEnabled]);

  return {
    isRecording,
    isProcessing,
    recordings,
    recordingDurationMs,
    recordingError,
    recordingDropNotice,
    usedMimeType,
    recCarouselRef,
    recordingApiAvailable,
    refreshRecordings,
    deleteRecording,
    scrollRecordings,
    toggleRecording,
  };
}
