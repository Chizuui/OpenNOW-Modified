/**
 * Muxer worker for the WebCodecs recording path (the stream-protecting
 * recorder).
 *
 * The renderer taps the live remote tracks with MediaStreamTrackProcessor,
 * re-encodes them with hardware WebCodecs encoders (AVC Annex-B video + AAC
 * audio) and posts every encoded chunk here. This worker feeds the chunks into
 * mp4-muxer (fragmented MP4) and streams the file bytes back out through a
 * PositionalWriter in exact write order, so the recording file grows as the
 * recording runs and nothing is retained in memory.
 *
 * Input contract: video chunks are Annex-B H.264 exactly as produced by
 * `VideoEncoder` configured with `avc: { format: "annexb" }`; the avcC record
 * is taken from the encoder metadata when the runtime provides one and
 * otherwise built from the first key chunk's parameter sets. Audio chunks are
 * raw AAC access units with the AudioSpecificConfig in the first chunk's
 * decoderConfig (optional — mp4-muxer falls back to a generated record).
 * Timestamps are rebased to the track's first chunk by the caller and carry
 * the encoder's µs timeline.
 */

import { Muxer as Mp4Muxer, StreamTarget as Mp4StreamTarget } from "mp4-muxer";

import { annexBToLengthPrefixed, buildAvcCFromNalus, extractNalUnits } from "./encodedCapture";
import { PositionalWriter } from "./positionalWriter";

export interface WebCodecsRecorderInit {
  width: number;
  height: number;
  fps: number;
  hasAudio: boolean;
  audioChannels: number;
  audioSampleRate: number;
}

export type WebCodecsRecorderWorkerInbound =
  | ({ type: "init" } & WebCodecsRecorderInit)
  | {
      type: "video";
      data: ArrayBuffer;
      timestampUs: number;
      durationUs: number;
      isKey: boolean;
      /** avcC record from the encoder metadata; built from the first key chunk when null. */
      avcC: ArrayBuffer | null;
      /** Encoder codec string for the muxer's decoderConfig (e.g. "avc1.640028"). */
      codec: string | null;
    }
  | {
      type: "audio";
      data: ArrayBuffer;
      timestampUs: number;
      durationUs: number;
      /** AudioSpecificConfig from the encoder metadata (optional for AAC). */
      description: ArrayBuffer | null;
    }
  | { type: "finalize" }
  | { type: "abort" };

export type WebCodecsRecorderWorkerOutbound =
  | { type: "chunk"; data: ArrayBuffer }
  | { type: "ready" }
  | { type: "finalized"; videoFrames: number; durationMs: number }
  | { type: "error"; message: string };

// Minimal worker-global typing: the DOM lib's `self` (Window) is a lie inside
// a dedicated worker, and pulling lib.webworker into this DOM project would
// collide with the DOM declarations (same pattern as recordEncodedWorker.ts).
declare const self: {
  onmessage: ((event: MessageEvent<WebCodecsRecorderWorkerInbound>) => void) | null;
  postMessage(message: WebCodecsRecorderWorkerOutbound, transfer?: Transferable[]): void;
};

interface MuxerHandle {
  video(
    data: Uint8Array,
    isKey: boolean,
    timestampUs: number,
    durationUs: number,
    meta?: EncodedVideoChunkMetadata,
  ): void;
  audio(
    data: Uint8Array,
    timestampUs: number,
    durationUs: number,
    meta?: EncodedAudioChunkMetadata,
  ): void;
  finalize(): void;
}

let init: WebCodecsRecorderInit | null = null;
let handle: MuxerHandle | null = null;
let writer: PositionalWriter | null = null;
let videoConfigReady = false;
let audioConfigReady = false;
let videoFrames = 0;
let lastVideoTsUs: number | null = null;
let lastAudioTsUs: number | null = null;

function postError(message: string): void {
  self.postMessage({ type: "error", message });
}

function postChunk(data: Uint8Array): void {
  const buffer: ArrayBuffer =
    data.byteOffset === 0 && data.byteLength === data.buffer.byteLength
      ? (data.buffer as ArrayBuffer)
      : (data.slice().buffer as ArrayBuffer);
  self.postMessage({ type: "chunk", data: buffer }, [buffer]);
}

function reset(): void {
  init = null;
  handle = null;
  writer = null;
  videoConfigReady = false;
  audioConfigReady = false;
  videoFrames = 0;
  lastVideoTsUs = null;
  lastAudioTsUs = null;
}

function setup(next: WebCodecsRecorderInit): void {
  try {
    reset();
    init = next;
    writer = new PositionalWriter(postChunk);
    const muxer = new Mp4Muxer({
      target: new Mp4StreamTarget({
        onData: (data, position) => writer?.write(data, position),
      }),
      video: {
        codec: "avc",
        width: next.width,
        height: next.height,
        // Deliberately NO frameRate: the recorder drops frames by design, and
        // mp4-muxer's frameRate rounds every timestamp onto the fps grid —
        // frames closer than one grid step collapse into duplicate DTS
        // (runtime-verified: the canvas-mirror probe at ~36 fps produced
        // non-monotonic DTS warnings on the 60-grid). Without it the muxer
        // keeps raw timestamps and derives each sample's stts duration from
        // the timestamp delta — exact for VFR-including-drops content.
      },
      ...(next.hasAudio
        ? {
            audio: {
              codec: "aac" as const,
              numberOfChannels: next.audioChannels,
              sampleRate: next.audioSampleRate,
            },
          }
        : {}),
      fastStart: "fragmented",
      minFragmentDuration: 1,
      firstTimestampBehavior: "offset",
    });
    handle = {
      video(data, isKey, timestampUs, durationUs, meta) {
        muxer.addVideoChunkRaw(
          data,
          isKey ? "key" : "delta",
          timestampUs,
          durationUs,
          meta,
        );
      },
      audio(data, timestampUs, durationUs, meta) {
        muxer.addAudioChunkRaw(data, "key", timestampUs, durationUs, meta);
      },
      finalize() {
        muxer.finalize();
      },
    };
    self.postMessage({ type: "ready" });
  } catch (error) {
    postError(`WebCodecs recorder init failed: ${String(error)}`);
  }
}

/**
 * Convert an Annex-B encoder chunk to MP4 sample form: length-prefixed, with
 * the parameter sets stripped (they live in the avcC record, exactly like the
 * receiver-transform path's convertSample).
 */
function avcSample(raw: Uint8Array): Uint8Array {
  const nals = extractNalUnits(raw);
  const filtered = nals.filter((nal) => {
    const type = nal.payload[0] & 0x1f;
    return type !== 7 && type !== 8; // SPS/PPS
  });
  return annexBToLengthPrefixed(filtered);
}

function captureVideo(message: Extract<WebCodecsRecorderWorkerInbound, { type: "video" }>): void {
  if (!handle) return;
  // The muxer needs increasing timestamps; the caller already rebased to the
  // track start, the guard absorbs any residual non-monotonic wobble. The
  // step is 200 µs — above one tick of the muxer's 57600 timescale (~17 µs)
  // so two clamped timestamps can never collapse onto the same tick.
  const tsUs =
    lastVideoTsUs === null
      ? Math.max(0, message.timestampUs)
      : Math.max(message.timestampUs, lastVideoTsUs + 200);
  lastVideoTsUs = tsUs;
  const raw = new Uint8Array(message.data);
  if (!videoConfigReady) {
    // The codec record seeds from the encoder metadata when provided, and
    // otherwise from the first key chunk's in-band parameter sets. Nothing
    // muxable exists before it.
    const description = message.avcC
      ? new Uint8Array(message.avcC)
      : buildAvcCFromNalus(extractNalUnits(raw));
    if (!description) return;
    videoConfigReady = true;
    handle.video(avcSample(raw), true, tsUs, message.durationUs, {
      decoderConfig: { codec: message.codec ?? "avc1.640028", description },
    });
  } else {
    handle.video(avcSample(raw), message.isKey, tsUs, message.durationUs);
  }
  videoFrames += 1;
}

function captureAudio(message: Extract<WebCodecsRecorderWorkerInbound, { type: "audio" }>): void {
  if (!handle || !init) return;
  // Same 200 µs tick-safe guard as the video path (muxer timescale 57600).
  const tsUs =
    lastAudioTsUs === null
      ? Math.max(0, message.timestampUs)
      : Math.max(message.timestampUs, lastAudioTsUs + 200);
  lastAudioTsUs = tsUs;
  const bytes = new Uint8Array(message.data);
  if (!audioConfigReady) {
    audioConfigReady = true;
    handle.audio(bytes, tsUs, message.durationUs, {
      decoderConfig: {
        codec: "mp4a.40.2",
        numberOfChannels: init.audioChannels,
        sampleRate: init.audioSampleRate,
        ...(message.description ? { description: new Uint8Array(message.description) } : {}),
      },
    });
  } else {
    handle.audio(bytes, tsUs, message.durationUs);
  }
}

function finalize(): void {
  if (!handle) {
    postError("WebCodecs recorder finalize without an active muxer.");
    return;
  }
  if (videoFrames === 0) {
    // Nothing reached the muxer (no key chunk?): finalizing would emit an
    // empty/header-only file. Drop it and report — the renderer aborts the
    // file instead of saving a broken clip.
    reset();
    postError("Recording captured no video frames — nothing was saved.");
    return;
  }
  try {
    handle.finalize();
    writer?.flushAll();
    self.postMessage({
      type: "finalized",
      videoFrames,
      durationMs: Math.round((lastVideoTsUs ?? 0) / 1000),
    });
  } catch (error) {
    postError(`WebCodecs recorder finalize failed: ${String(error)}`);
  } finally {
    reset();
  }
}

self.onmessage = (event: MessageEvent<WebCodecsRecorderWorkerInbound>): void => {
  const message = event.data;
  try {
    if (message.type === "init") {
      setup(message);
    } else if (message.type === "video") {
      captureVideo(message);
    } else if (message.type === "audio") {
      captureAudio(message);
    } else if (message.type === "finalize") {
      finalize();
    } else if (message.type === "abort") {
      reset();
    }
  } catch (error) {
    postError(`WebCodecs recorder failed: ${String(error)}`);
  }
};
