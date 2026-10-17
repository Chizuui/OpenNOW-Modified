/**
 * Stream-protecting WebCodecs recorder for web (non-native) mode.
 *
 * Root cause this exists for: MediaRecorder bound to the raw remote track
 * backpressures the WebRTC receive pipeline whenever its encoder cannot hold
 * the stream's rate — the decoder stalls (decTime climbs, decode fps drops,
 * frames fall) and the STREAM itself stutters for as long as the recording
 * runs (field-verified on a 1080p60 H.265 stream: decode 60 → 53 fps, decTime
 * 0.7 → 75 ms, dropped 0 → 86, recovery the instant recording stopped).
 *
 * MediaStreamTrackProcessor decouples the consumer: frames arrive through a
 * bounded queue with live-edge semantics — a slow consumer drops frames and
 * never backpressures the track (probe-verified: with the consumer stalled for
 * 5 s the video element kept presenting 60 fps and the reader resumed at the
 * live edge). This recorder reads the live track through MSTP, re-encodes with
 * the hardware H.264 encoder, and drops frames on encoder backpressure instead
 * of letting a queue grow. The recording (the FILE) degrades when the machine
 * cannot keep up; the stream never does.
 *
 * The encoded chunks stream into recordWebCodecsWorker (fragmented MP4) and
 * out through `onChunk` in exact file order, so the recording is written
 * incrementally and nothing is retained in memory beyond the active encoders.
 */

import type {
  WebCodecsRecorderInit,
  WebCodecsRecorderWorkerInbound,
  WebCodecsRecorderWorkerOutbound,
} from "./recordWebCodecsWorker";

/** H.264 profiles tried in order: High 4.0, Main 4.0, Baseline 3.0. */
const VIDEO_CODEC_CANDIDATES = ["avc1.640028", "avc1.4D0028", "avc1.42E01E"] as const;
/** AAC-LC in MP4 — universally playable. */
const AUDIO_CODEC = "mp4a.40.2";
const DEFAULT_AUDIO_BITRATE = 128_000;
/** Queued encodes above this skip frames instead of growing encoder latency. */
const VIDEO_ENCODE_QUEUE_LIMIT = 3;
/**
 * Audio is never worth dropping for latency — the file is the only consumer,
 * and delaying audio writes costs nothing (timestamps are preserved). The
 * limit only bounds memory: 240 AAC frames ≈ 5 s ≈ 2 MB. The probe caught the
 * AAC encoder absorbing its start-up burst slower than the MSTP reader
 * delivered it; the old limit of 8 dropped ~0.45 s of the opening audio.
 */
const AUDIO_ENCODE_QUEUE_LIMIT = 240;
/** Keyframe cadence for seekability (drops happen pre-encode, so GOPs stay clean). */
const KEYFRAME_INTERVAL_MS = 2_000;
const STATS_LOG_INTERVAL_MS = 5_000;
/** AAC frame duration: 1024 samples at 48 kHz. */
const AAC_FRAME_DURATION_US = 21_333;

/** Convert a WebCodecs `description` buffer source to a transferable ArrayBuffer. */
function toArrayBuffer(source: AllowSharedBufferSource | undefined | null): ArrayBuffer | null {
  if (!source) return null;
  if (ArrayBuffer.isView(source)) {
    return source.buffer.slice(source.byteOffset, source.byteOffset + source.byteLength) as ArrayBuffer;
  }
  if (source instanceof ArrayBuffer) return source;
  // SharedArrayBuffer (never produced by Chromium's encoders, but part of the
  // WebCodecs type): copy into a plain, transferable buffer.
  const copy = new Uint8Array(source.byteLength);
  copy.set(new Uint8Array(source));
  return copy.buffer;
}

export function isWebCodecsRecordingSupported(): boolean {
  return (
    typeof MediaStreamTrackProcessor !== "undefined" &&
    typeof VideoEncoder !== "undefined" &&
    typeof AudioEncoder !== "undefined" &&
    typeof VideoFrame !== "undefined"
  );
}

export interface WebCodecsRecordingOptions {
  videoTrack: MediaStreamTrack;
  /** Composed (game + optional mic) audio track; null records video only. */
  audioTrack: MediaStreamTrack | null;
  width: number;
  height: number;
  fps: number | null;
  /** Bits per second; undefined leaves the encoder default. */
  videoBitrate: number | undefined;
  audioBitrate?: number;
  /** Receives in-order MP4 file bytes as they are muxed. */
  onChunk: (chunk: ArrayBuffer) => void;
  /** Human-readable diagnostics for the exported log. */
  onDiagnostic?: (message: string) => void;
  /** Mid-recording failure (encoder error): abort the recording. */
  onFatal?: (error: Error) => void;
  /** Worker factory (kept explicit so the caller owns the bundler URL). */
  workerFactory: () => Worker;
}

export interface WebCodecsRecordingResult {
  videoFrames: number;
  /** Frames the pump skipped on encoder backpressure (excludes MSTP-side drops). */
  droppedFrames: number;
  /** Media duration from the last muxed video frame. */
  durationMs: number;
  /** Encoded frames per wall-clock second over the recording. */
  averageFps: number;
}

export interface WebCodecsTrackRecorder {
  /** Flush the encoders, finalize the file and resolve with the stats. */
  stop(): Promise<WebCodecsRecordingResult>;
  /** Drop the recording without finalizing (teardown / fatal failure). */
  abort(): void;
}

export async function startWebCodecsTrackRecorder(
  options: WebCodecsRecordingOptions,
): Promise<WebCodecsTrackRecorder> {
  if (!isWebCodecsRecordingSupported()) {
    throw new Error("WebCodecs track recording is not supported in this runtime.");
  }
  const { videoTrack, audioTrack } = options;
  const width = Math.round(options.width);
  const height = Math.round(options.height);
  if (!(width > 0) || !(height > 0)) {
    throw new Error("Stream dimensions are not known yet.");
  }
  const streamFps =
    typeof options.fps === "number" && Number.isFinite(options.fps) && options.fps > 0
      ? options.fps
      : 60;

  // Encoder selection runs BEFORE any resource exists, so an unsupported
  // configuration fails without anything to tear down.
  let videoCodec: string | null = null;
  let lastSupportError = "";
  for (const candidate of VIDEO_CODEC_CANDIDATES) {
    try {
      const support = await VideoEncoder.isConfigSupported({
        codec: candidate,
        width,
        height,
        framerate: streamFps,
        hardwareAcceleration: "prefer-hardware",
        avc: { format: "annexb" },
        ...(options.videoBitrate ? { bitrate: options.videoBitrate } : {}),
      });
      if (support.supported) {
        videoCodec = candidate;
        break;
      }
    } catch (error) {
      lastSupportError = String(error);
    }
  }
  if (!videoCodec) {
    throw new Error(
      `No hardware H.264 encoder for ${width}x${height}@${streamFps}` +
        (lastSupportError ? ` (${lastSupportError})` : ""),
    );
  }

  const audioSettings = audioTrack ? audioTrack.getSettings() : null;
  const audioChannels =
    typeof audioSettings?.channelCount === "number" && audioSettings.channelCount > 0
      ? audioSettings.channelCount
      : 2;
  const audioSampleRate =
    typeof audioSettings?.sampleRate === "number" && audioSettings.sampleRate > 0
      ? audioSettings.sampleRate
      : 48_000;
  if (audioTrack) {
    let audioSupported = false;
    try {
      const support = await AudioEncoder.isConfigSupported({
        codec: AUDIO_CODEC,
        sampleRate: audioSampleRate,
        numberOfChannels: audioChannels,
        bitrate: options.audioBitrate ?? DEFAULT_AUDIO_BITRATE,
      });
      audioSupported = support.supported === true;
    } catch (error) {
      lastSupportError = String(error);
    }
    if (!audioSupported) {
      throw new Error(`AAC audio encoding is unavailable (${lastSupportError}).`);
    }
  }

  const worker = options.workerFactory();
  let videoEncoder: VideoEncoder | null = null;
  let audioEncoder: AudioEncoder | null = null;
  let videoReader: ReadableStreamDefaultReader<VideoFrame> | null = null;
  let audioReader: ReadableStreamDefaultReader<AudioData> | null = null;
  let videoLoop: Promise<void> | null = null;
  let audioLoop: Promise<void> | null = null;

  let stopping = false;
  let fatalError: Error | null = null;
  let readyResolve: (() => void) | null = null;
  let readyReject: ((error: unknown) => void) | null = null;
  let finalizedResolve: ((result: WebCodecsRecordingResult) => void) | null = null;
  let finalizedReject: ((error: unknown) => void) | null = null;
  let startedAtMs = 0;
  let encodedFrames = 0;
  let droppedFrames = 0;
  let droppedAudioFrames = 0;
  let lastStatsLogMs = 0;
  let lastStatsFrames = 0;
  let lastKeyframeMs = 0;
  let sawFirstEncode = false;
  let videoBaseUs: number | null = null;
  let audioBaseUs: number | null = null;

  const log = (message: string): void => {
    options.onDiagnostic?.(message);
  };

  const toError = (error: unknown): Error =>
    error instanceof Error ? error : new Error(String(error));

  const fail = (error: Error): void => {
    if (fatalError) return;
    fatalError = error;
    readyReject?.(error);
    readyResolve = null;
    readyReject = null;
    finalizedReject?.(error);
    finalizedResolve = null;
    finalizedReject = null;
    // A failure during startup is reported through the start rejection; only
    // a failure while recording needs the out-of-band fatal callback.
    if (startedAtMs > 0 && !stopping) {
      options.onFatal?.(error);
    }
  };

  const cleanupPartial = (): void => {
    stopping = true;
    void videoReader?.cancel().catch(() => undefined);
    void audioReader?.cancel().catch(() => undefined);
    try {
      videoEncoder?.close();
    } catch {
      // Already closed.
    }
    try {
      audioEncoder?.close();
    } catch {
      // Already closed.
    }
    worker.terminate();
  };

  worker.onmessage = (event: MessageEvent<WebCodecsRecorderWorkerOutbound>): void => {
    const message = event.data;
    if (message.type === "ready") {
      readyResolve?.();
      readyResolve = null;
      readyReject = null;
    } else if (message.type === "chunk") {
      options.onChunk(message.data);
    } else if (message.type === "finalized") {
      const result: WebCodecsRecordingResult = {
        videoFrames: message.videoFrames,
        droppedFrames,
        durationMs: message.durationMs,
        averageFps:
          startedAtMs > 0
            ? encodedFrames / Math.max(0.001, (performance.now() - startedAtMs) / 1000)
            : 0,
      };
      finalizedResolve?.(result);
      finalizedResolve = null;
      finalizedReject = null;
    } else if (message.type === "error") {
      fail(new Error(message.message));
    }
  };
  worker.onerror = (event: ErrorEvent): void => {
    fail(new Error(event.message || "Recording worker crashed"));
  };

  try {
    const readyPromise = new Promise<void>((resolve, reject) => {
      readyResolve = resolve;
      readyReject = reject;
    });
    const init: WebCodecsRecorderInit = {
      width,
      height,
      fps: streamFps,
      hasAudio: audioTrack !== null,
      audioChannels,
      audioSampleRate,
    };
    worker.postMessage({
      type: "init",
      ...init,
    } satisfies WebCodecsRecorderWorkerInbound);
    await readyPromise;

    videoEncoder = new VideoEncoder({
      output: (chunk, metadata) => {
        const bytes = new Uint8Array(chunk.byteLength);
        chunk.copyTo(bytes);
        if (videoBaseUs === null) videoBaseUs = chunk.timestamp;
        const avcC = toArrayBuffer(metadata?.decoderConfig?.description ?? null);
        const outbound: WebCodecsRecorderWorkerInbound = {
          type: "video",
          data: bytes.buffer as ArrayBuffer,
          timestampUs: Math.max(0, chunk.timestamp - videoBaseUs),
          durationUs: Math.round(1_000_000 / streamFps),
          isKey: chunk.type === "key",
          avcC,
          codec: metadata?.decoderConfig?.codec ?? videoCodec,
        };
        worker.postMessage(
          outbound,
          avcC
            ? [bytes.buffer as ArrayBuffer, avcC]
            : [bytes.buffer as ArrayBuffer],
        );
      },
      error: (error) => {
        fail(toError(error));
      },
    });
    audioEncoder = audioTrack
      ? new AudioEncoder({
          output: (chunk, metadata) => {
            const bytes = new Uint8Array(chunk.byteLength);
            chunk.copyTo(bytes);
            if (audioBaseUs === null) audioBaseUs = chunk.timestamp;
            const description = toArrayBuffer(metadata?.decoderConfig?.description ?? null);
            const outbound: WebCodecsRecorderWorkerInbound = {
              type: "audio",
              data: bytes.buffer as ArrayBuffer,
              timestampUs: Math.max(0, chunk.timestamp - audioBaseUs),
              durationUs: AAC_FRAME_DURATION_US,
              description,
            };
            worker.postMessage(
              outbound,
              description
                ? [bytes.buffer as ArrayBuffer, description]
                : [bytes.buffer as ArrayBuffer],
            );
          },
          error: (error) => {
            fail(toError(error));
          },
        })
      : null;

    videoEncoder.configure({
      codec: videoCodec,
      width,
      height,
      framerate: streamFps,
      hardwareAcceleration: "prefer-hardware",
      avc: { format: "annexb" },
      ...(options.videoBitrate ? { bitrate: options.videoBitrate } : {}),
    });
    audioEncoder?.configure({
      codec: AUDIO_CODEC,
      sampleRate: audioSampleRate,
      numberOfChannels: audioChannels,
      bitrate: options.audioBitrate ?? DEFAULT_AUDIO_BITRATE,
    });

    const videoProcessor = new MediaStreamTrackProcessor<VideoFrame>({ track: videoTrack });
    videoReader = videoProcessor.readable.getReader();
    if (audioTrack) {
      const audioProcessor = new MediaStreamTrackProcessor<AudioData>({ track: audioTrack });
      audioReader = audioProcessor.readable.getReader();
    }

    startedAtMs = performance.now();
    lastStatsLogMs = startedAtMs;

    videoLoop = (async (): Promise<void> => {
      const reader = videoReader;
      const encoder = videoEncoder;
      if (!reader || !encoder) return;
      try {
        while (!stopping && !fatalError) {
          const { done, value } = await reader.read();
          if (done) break;
          if (stopping || fatalError) {
            value.close();
            break;
          }
          if (encoder.encodeQueueSize >= VIDEO_ENCODE_QUEUE_LIMIT) {
            // The encoder is behind: drop the frame from the RECORDING (the
            // stream never learns about it) instead of growing latency.
            droppedFrames += 1;
            value.close();
            continue;
          }
          const now = performance.now();
          const wantKey = !sawFirstEncode || now - lastKeyframeMs >= KEYFRAME_INTERVAL_MS;
          if (wantKey) lastKeyframeMs = now;
          sawFirstEncode = true;
          try {
            encoder.encode(value, { keyFrame: wantKey });
            encodedFrames += 1;
          } catch (error) {
            droppedFrames += 1;
            fail(toError(error));
          }
          value.close();
          if (now - lastStatsLogMs >= STATS_LOG_INTERVAL_MS) {
            const seconds = (now - lastStatsLogMs) / 1000;
            const fps = (encodedFrames - lastStatsFrames) / Math.max(0.001, seconds);
            log(
              `webcodecs recorder: ${fps.toFixed(1)} fps encoded, ${droppedFrames} dropped` +
                (droppedAudioFrames > 0 ? `, ${droppedAudioFrames} audio dropped` : "") +
                `, encode queue ${encoder.encodeQueueSize}`,
            );
            lastStatsLogMs = now;
            lastStatsFrames = encodedFrames;
          }
        }
      } catch (error) {
        if (!stopping) fail(toError(error));
      }
    })();

    audioLoop = (async (): Promise<void> => {
      const reader = audioReader;
      if (!reader) return;
      try {
        while (!stopping && !fatalError) {
          const { done, value } = await reader.read();
          if (done) break;
          if (stopping || fatalError) {
            value.close();
            break;
          }
          if (audioEncoder && audioEncoder.encodeQueueSize < AUDIO_ENCODE_QUEUE_LIMIT) {
            try {
              audioEncoder.encode(value);
            } catch (error) {
              fail(toError(error));
            }
          } else {
            droppedAudioFrames += 1;
          }
          value.close();
        }
      } catch (error) {
        if (!stopping) fail(toError(error));
      }
    })();

    log(
      `webcodecs recorder: video ${videoCodec} ${width}x${height}@${streamFps}` +
        (options.videoBitrate ? ` ${(options.videoBitrate / 1e6).toFixed(1)} Mbps` : " (auto bitrate)") +
        ", audio " +
        (audioTrack
          ? `${AUDIO_CODEC} ${audioSampleRate} Hz x${audioChannels}`
          : "none (video only)"),
    );

    // An encoder/worker failure between the first pump iteration and this
    // point would otherwise be reported only through the out-of-band fatal
    // callback while start() still resolves with a dead recorder — the caller
    // must see the rejection so it can abort the file and fall back.
    if (fatalError) throw fatalError;

    return {
      stop: async (): Promise<WebCodecsRecordingResult> => {
        stopping = true;
        try {
          if (fatalError) throw fatalError;
          await Promise.allSettled([videoReader?.cancel(), audioReader?.cancel()]);
          await Promise.allSettled([videoLoop, audioLoop].filter((loop) => loop !== null));
          await Promise.allSettled([videoEncoder?.flush(), audioEncoder?.flush()]);
          if (fatalError) throw fatalError;
          const finalizePromise = new Promise<WebCodecsRecordingResult>((resolve, reject) => {
            finalizedResolve = resolve;
            finalizedReject = reject;
          });
          worker.postMessage({ type: "finalize" });
          const result = await Promise.race([
            finalizePromise,
            new Promise<never>((_, reject) =>
              window.setTimeout(() => reject(new Error("Recording finalize timed out")), 10_000),
            ),
          ]);
          log(
            `webcodecs recorder: stopped — ${result.videoFrames} frames (${result.averageFps.toFixed(1)} fps), ` +
              `${result.droppedFrames} dropped, ${(result.durationMs / 1000).toFixed(1)} s recorded`,
          );
          return result;
        } finally {
          finalizedResolve = null;
          finalizedReject = null;
          // Idempotent on the normal path (the readers were cancelled before
          // the flush); the fatal early-throw path relies on this to release
          // the MSTP readers.
          void videoReader?.cancel().catch(() => undefined);
          void audioReader?.cancel().catch(() => undefined);
          try {
            videoEncoder?.close();
          } catch {
            // Already closed.
          }
          try {
            audioEncoder?.close();
          } catch {
            // Already closed.
          }
          worker.terminate();
        }
      },
      abort: (): void => {
        cleanupPartial();
      },
    };
  } catch (error) {
    cleanupPartial();
    if (fatalError) throw fatalError;
    throw error;
  }
}
