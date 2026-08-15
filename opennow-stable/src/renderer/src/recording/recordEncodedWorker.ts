/**
 * Receiver-side encoded transform worker (GFN parity recorder).
 *
 * Hosts the RTCRtpScriptTransforms attached to the video/audio receivers of
 * the live RTCPeerConnection. Every encoded frame is forwarded unchanged to
 * the decoder (pass-through — the stream is untouched) while a copy of its
 * bytes + RTP timestamp is fed into a muxer (mp4-muxer for AVC/HEVC/AV1,
 * webm-muxer for VP8/VP9) running in this worker. Zero decode, zero
 * re-encode: recording can never compete with the decoder, and the user's
 * recording cap does not apply — the capture is whatever the stream is.
 *
 * Codec-configuration records (avcC/hvcC/av1C/OpusHead) are built from the
 * parameter sets / sequence header carried in-band by the first keyframe, so
 * no WebCodecs pipeline is involved.
 */

import { Muxer as Mp4Muxer, StreamTarget as Mp4StreamTarget } from "mp4-muxer";
import { Muxer as WebmMuxer, StreamTarget as WebmStreamTarget } from "webm-muxer";

import {
  annexBToLengthPrefixed,
  buildAv1CFromObuStream,
  buildAvcCFromNalus,
  buildHvcCFromNalus,
  buildOpusHead,
  extractNalUnits,
  mixGameAudioWithMic,
  patchAv1CInMp4,
  rtpTimestampToMicroseconds,
  unwrapRtpTimestamp,
  AUDIO_RTP_CLOCK,
  DEFAULT_MIC_MIX_GAIN,
  MicPcmFifo,
  MicSync,
  VIDEO_RTP_CLOCK,
  type EncodedCaptureCodec,
  type EncodedCaptureContainer,
} from "./encodedCapture";

export type EncodedWorkerOutboundMessage =
  | { type: "ready" }
  | { type: "chunk"; data: ArrayBuffer }
  | { type: "done" }
  | { type: "error"; message: string };

interface EncodedCaptureInit {
  type: "init";
  codec: EncodedCaptureCodec;
  container: EncodedCaptureContainer;
  width: number;
  height: number;
  fps: number;
  hasAudio: boolean;
  audioChannels: number;
  audioSampleRate: number;
  /**
   * Mix the live microphone (raw PCM, 48 kHz mono) into the recording's audio
   * track. The game opus is decoded, mixed with the mic, and re-encoded — the
   * video bitstream stays untouched. Requires WebCodecs audio in the worker.
   */
  mixMic: boolean;
}

interface EncodedCaptureStop {
  type: "stop";
}

/** Raw mic PCM captured on the main thread (48 kHz, mono, transferable). */
interface EncodedCaptureMicPcm {
  type: "mic-pcm";
  samples: Float32Array;
  /**
   * performance.now() at capture — a real clock, independent of the
   * AudioContext (whose currentTime is tied to its own sample counter). The
   * worker measures the mic's actual sample rate from these tags and consumes
   * by RTP frame duration, so the voice cannot drift from the game audio.
   */
  capturedAtMs: number;
}

/** Message delivered to the worker for each RTCRtpScriptTransform attached. */
interface RtcTransformEventData {
  type: "rtc-transform-event";
  transformer: {
    options?: { kind?: "video" | "audio" };
    readable: ReadableStream<RTCEncodedVideoFrame | RTCEncodedAudioFrame>;
    writable: WritableStream<RTCEncodedVideoFrame | RTCEncodedAudioFrame>;
  };
}

type WorkerInbound = EncodedCaptureInit | EncodedCaptureStop | EncodedCaptureMicPcm | RtcTransformEventData;

// Minimal worker-global typing: the DOM lib's `self` (Window) is a lie inside
// a dedicated worker, and pulling lib.webworker into this DOM project would
// collide with the DOM declarations (same pattern as recordCanvasWorker.ts).
declare const self: {
  onmessage: ((event: MessageEvent<WorkerInbound>) => void) | null;
  postMessage(message: EncodedWorkerOutboundMessage, transfer?: Transferable[]): void;
};

/** Common surface over both muxers (they differ in argument shape). */
interface MuxerHandle {
  video(data: Uint8Array, type: "key" | "delta", timestampUs: number, meta?: EncodedVideoChunkMetadata): void;
  audio(data: Uint8Array, type: "key" | "delta", timestampUs: number, meta?: EncodedAudioChunkMetadata): void;
  finalize(): void;
}

/**
 * Merges the muxer's (data, position) writes into a contiguous byte stream.
 * Both muxers emit merged, ascending sections (overlapping rewrites — moof
 * patches, CodecPrivate placeholders — are collapsed inside each flush), so
 * the only non-contiguity is the occasional gap, buffered here until it
 * fills. flushAll() drains whatever remains after finalize().
 */
class PositionalWriter {
  private readonly chunks = new Map<number, Uint8Array>();
  private nextEmit = 0;

  constructor(private readonly onData: (data: Uint8Array) => void) {}

  write(data: Uint8Array, position: number): void {
    if (position < this.nextEmit) {
      // Backward write into already-emitted bytes. Should not happen after the
      // muxer's own merge; dropping keeps the append-only file consistent.
      return;
    }
    this.chunks.set(position, data);
    this.flush();
  }

  flushAll(): void {
    this.flush();
  }

  private flush(): void {
    let data = this.chunks.get(this.nextEmit);
    while (data) {
      this.chunks.delete(this.nextEmit);
      this.nextEmit += data.byteLength;
      this.onData(data);
      data = this.chunks.get(this.nextEmit);
    }
  }
}

let config: EncodedCaptureInit | null = null;
let handle: MuxerHandle | null = null;
let writer: PositionalWriter | null = null;
/** Real av1C record, built from the first AV1 keyframe; mp4-muxer writes a
 * zeroed placeholder for AV1, so the emitted moov must be patched once. */
let av1C: Uint8Array | null = null;
let av1CPatched = false;
let videoConfigReady = false;
let audioConfigReady = false;
let lastVideoRtp: number | null = null;
let lastAudioRtp: number | null = null;
let lastVideoTsUs: number | null = null;
let lastAudioTsUs: number | null = null;
let finalized = false;
const pumpTasks: Promise<void>[] = [];
// Mic mixing (GFN parity: mic is mixed into the audio track, video untouched).
let micMixActive = false;
let micFifo: MicPcmFifo | null = null;
// Real-clock sync between the mic PCM and the game RTP timeline (see
// MicSync): consumes mic samples by RTP frame duration × the MEASURED mic
// rate, not by the game frame's sample count — the two clocks drift.
let micSync: MicSync | null = null;
let gameDecoder: AudioDecoder | null = null;
let mixEncoder: AudioEncoder | null = null;

function post(message: EncodedWorkerOutboundMessage): void {
  self.postMessage(message);
}

function postError(message: string): void {
  post({ type: "error", message });
}

function postChunk(data: Uint8Array): void {
  const buffer: ArrayBuffer =
    data.byteOffset === 0 && data.byteLength === data.buffer.byteLength
      ? (data.buffer as ArrayBuffer)
      : (data.slice().buffer as ArrayBuffer);
  self.postMessage({ type: "chunk", data: buffer }, [buffer]);
}

/**
 * Wire the game-opus decode → mic mix → opus re-encode chain used when
 * `mixMic` is on. Falls back to the passthrough audio path (no mic) when
 * WebCodecs audio is unavailable in this runtime.
 */
function setupMicMix(init: EncodedCaptureInit): void {
  if (
    typeof AudioDecoder === "undefined" ||
    typeof AudioEncoder === "undefined" ||
    typeof AudioData === "undefined"
  ) {
    console.warn("[recordEncodedWorker] WebCodecs audio unavailable — recording without mic mix.");
    return;
  }
  const opusHead = buildOpusHead(init.audioChannels);
  const decoder = new AudioDecoder({
    output: (data: AudioData) => {
      // Decoded game audio → interleaved stereo f32 PCM (both 48 kHz).
      const frames = data.numberOfFrames;
      const timestampUs = data.timestamp;
      const game = new Float32Array(frames * 2);
      data.copyTo(game, { planeIndex: 0, format: "f32" });
      data.close();
      // Consume mic samples by the RTP frame duration × the measured mic
      // rate (anchored to the first frame) instead of `frames` — the game
      // RTP clock and the mic's hardware clock drift, so a per-sample
      // assumption slowly walks the voice away from the game. Underruns
      // (mic momentarily behind) fold into the next frame via commitPulled.
      const desired = micSync?.samplesForFrame(timestampUs) ?? 0;
      const mic =
        desired > 0 ? (micFifo?.pull(desired) ?? new Float32Array(0)) : new Float32Array(0);
      if (mic.length > 0) micSync?.commitPulled(mic.length);
      const mixed = mixGameAudioWithMic(game, mic, DEFAULT_MIC_MIX_GAIN);
      mixEncoder?.encode(
        new AudioData({
          format: "f32",
          sampleRate: 48_000,
          numberOfFrames: frames,
          numberOfChannels: 2,
          timestamp: timestampUs,
          // AudioData.data is a BufferSource; pass the Float32Array's backing
          // buffer directly (freshly allocated, exactly frames×2×4 bytes).
          data: mixed.buffer as ArrayBuffer,
        }),
      );
    },
    error: (error: Error) => {
      console.error("[recordEncodedWorker] Game audio decode failed:", error);
      // Fall back to passthrough so the recording survives a decode hiccup.
      teardownMicMix();
    },
  });
  const encoder = new AudioEncoder({
    output: (chunk: EncodedAudioChunk) => {
      if (!config || !handle) return;
      const bytes = new Uint8Array(chunk.byteLength);
      chunk.copyTo(bytes);
      const timestampUs = chunk.timestamp;
      if (!audioConfigReady) {
        audioConfigReady = true;
        handle.audio(bytes, "key", timestampUs, {
          decoderConfig: {
            codec: "opus",
            description: buildOpusHead(config.audioChannels),
            numberOfChannels: config.audioChannels,
            sampleRate: config.audioSampleRate,
          },
        });
      } else {
        handle.audio(bytes, "key", timestampUs);
      }
    },
    error: (error: Error) => {
      console.error("[recordEncodedWorker] Mic mix encode failed:", error);
      teardownMicMix();
    },
  });
  try {
    decoder.configure({
      codec: "opus",
      sampleRate: 48_000,
      numberOfChannels: 2,
      description: opusHead,
    });
    encoder.configure({
      codec: "opus",
      sampleRate: 48_000,
      numberOfChannels: 2,
      bitrate: 128_000,
    });
  } catch (error) {
    console.warn("[recordEncodedWorker] Opus decode/encode unavailable — recording without mic mix.", error);
    decoder.close();
    encoder.close();
    return;
  }
  gameDecoder = decoder;
  mixEncoder = encoder;
  micFifo = new MicPcmFifo();
  micSync = new MicSync();
  micMixActive = true;
}

function teardownMicMix(): void {
  micMixActive = false;
  micFifo = null;
  micSync = null;
  if (gameDecoder) {
    gameDecoder.close();
    gameDecoder = null;
  }
  if (mixEncoder) {
    mixEncoder.close();
    mixEncoder = null;
  }
}

function setup(init: EncodedCaptureInit): void {
  try {
    config = init;
    if (init.mixMic) setupMicMix(init);
    writer = new PositionalWriter(postChunk);
    const onData = (data: Uint8Array, position: number): void => {
      // mp4-muxer hardcodes a zeroed av1C box for AV1 (it ignores
      // decoderConfig.description); patch the emitted moov with the real
      // record built from the first keyframe. The moov is the first section
      // the muxer emits, so the first match is the box, never sample data.
      if (!av1CPatched && config?.codec === "av1" && av1C && patchAv1CInMp4(data, av1C)) {
        av1CPatched = true;
      }
      writer?.write(data, position);
    };
    if (init.container === "mp4") {
      const muxer = new Mp4Muxer({
        target: new Mp4StreamTarget({ onData }),
        video: {
          codec: init.codec === "avc" ? "avc" : init.codec === "hevc" ? "hevc" : "av1",
          width: init.width,
          height: init.height,
          frameRate: init.fps,
        },
        ...(init.hasAudio
          ? { audio: { codec: "opus", numberOfChannels: init.audioChannels, sampleRate: init.audioSampleRate } }
          : {}),
        fastStart: "fragmented",
        minFragmentDuration: 1,
        firstTimestampBehavior: "offset",
      });
      handle = {
        video(data, type, timestampUs, meta) {
          // mp4-muxer refines durations from the next sample, so an estimate is fine.
          muxer.addVideoChunkRaw(data, type, timestampUs, Math.round(1e6 / Math.max(1, init.fps)), meta);
        },
        audio(data, type, timestampUs, meta) {
          muxer.addAudioChunkRaw(data, type, timestampUs, 20_000, meta);
        },
        finalize: () => muxer.finalize(),
      };
    } else {
      const muxer = new WebmMuxer({
        target: new WebmStreamTarget({ onData }),
        video: {
          codec: init.codec === "av1" ? "V_AV1" : init.codec === "vp9" ? "V_VP9" : "V_VP8",
          width: init.width,
          height: init.height,
          frameRate: init.fps,
        },
        ...(init.hasAudio
          ? { audio: { codec: "A_OPUS", numberOfChannels: init.audioChannels, sampleRate: init.audioSampleRate } }
          : {}),
        firstTimestampBehavior: "offset",
      });
      handle = {
        video(data, type, timestampUs, meta) {
          muxer.addVideoChunkRaw(data, type, timestampUs, meta);
        },
        audio(data, type, timestampUs, meta) {
          muxer.addAudioChunkRaw(data, type, timestampUs, meta);
        },
        finalize: () => muxer.finalize(),
      };
    }
    post({ type: "ready" });
  } catch (error) {
    postError(`Encoded capture init failed: ${String(error)}`);
  }
}

function captureVideo(frame: RTCEncodedVideoFrame): void {
  if (!config || !handle || finalized) return;
  const metadata = frame.getMetadata();
  const rtp = metadata.rtpTimestamp ?? metadata.timestamp;
  if (rtp === undefined) return;
  lastVideoRtp = unwrapRtpTimestamp(rtp, lastVideoRtp);
  const timestampUs = rtpTimestampToMicroseconds(lastVideoRtp, VIDEO_RTP_CLOCK);
  // Keep the muxer happy: timestamps must be strictly increasing. Guard
  // against resets that unwrapRtpTimestamp could not reconcile.
  const tsUs = lastVideoTsUs === null ? timestampUs : Math.max(timestampUs, lastVideoTsUs + 1);
  lastVideoTsUs = tsUs;
  const raw = new Uint8Array(frame.data.byteLength);
  raw.set(new Uint8Array(frame.data));
  const type: "key" | "delta" = frame.type === "key" ? "key" : "delta";
  if (!videoConfigReady) {
    // Codec config (avcC/hvcC/av1C) lives in the first keyframe's parameter
    // sets / sequence header; skip until it arrives.
    if (type !== "key") return;
    let description: Uint8Array | null = null;
    let codecString = "";
    if (config.codec === "avc" || config.codec === "hevc") {
      const nalus = extractNalUnits(raw);
      description =
        config.codec === "avc" ? buildAvcCFromNalus(nalus) : buildHvcCFromNalus(nalus);
      codecString = config.codec === "avc" ? "avc1.42001f" : "hvc1.1.6.L93.B0";
    } else if (config.codec === "av1") {
      description = buildAv1CFromObuStream(raw);
      codecString = "av01.0.04M.08";
    }
    if (!description) return;
    videoConfigReady = true;
    if (config.codec === "av1") {
      // Remember the record for the emitted-stream patch; AV1 samples are
      // passed through unchanged (low-overhead OBU stream, already the
      // format MP4 stores).
      av1C = description;
    }
    handle.video(convertSample(raw), type, tsUs, {
      decoderConfig: { codec: codecString, description },
    });
    return;
  }
  handle.video(convertSample(raw), type, tsUs);
}

/**
 * Annex-B → length-prefixed for AVC/HEVC in MP4; AV1 (whose samples are the
 * low-overhead OBU stream as-is) and VP8/VP9 (WebM) pass through untouched.
 */
function convertSample(raw: Uint8Array): Uint8Array {
  if (!config || config.container !== "mp4" || config.codec === "av1") return raw;
  const nalus = extractNalUnits(raw);
  const parameterSetTypes =
    config.codec === "avc" ? [7, 8] : [32, 33, 34]; // SPS/PPS or VPS/SPS/PPS
  const filtered = nalus.filter((nal) => {
    const type = nal.payload[0] & 0x1f;
    return !parameterSetTypes.includes(type);
  });
  return annexBToLengthPrefixed(filtered);
}

function captureAudio(frame: RTCEncodedAudioFrame): void {
  if (!config || !handle || !config.hasAudio || finalized) return;
  const metadata = frame.getMetadata();
  // Audio metadata has only rtpTimestamp in the DOM lib; some runtimes also
  // expose a legacy `timestamp` alias — accept both.
  const rtp = metadata.rtpTimestamp ?? (metadata as { timestamp?: number }).timestamp;
  if (rtp === undefined) return;
  lastAudioRtp = unwrapRtpTimestamp(rtp, lastAudioRtp);
  const timestampUs = rtpTimestampToMicroseconds(lastAudioRtp, AUDIO_RTP_CLOCK);
  const tsUs = lastAudioTsUs === null ? timestampUs : Math.max(timestampUs, lastAudioTsUs + 1);
  lastAudioTsUs = tsUs;
  const raw = new Uint8Array(frame.data.byteLength);
  raw.set(new Uint8Array(frame.data));
  if (micMixActive && gameDecoder) {
    // Mic mixing: decode the game opus, mix with the mic PCM, re-encode.
    // Audio chunks are treated as key (every opus packet is independently
    // decodable; the transform's frames carry no key/delta distinction).
    gameDecoder.decode(
      new EncodedAudioChunk({
        type: "key",
        timestamp: tsUs,
        data: raw,
      }),
    );
    return;
  }
  if (!audioConfigReady) {
    audioConfigReady = true;
    handle.audio(raw, "key", tsUs, {
      decoderConfig: {
        codec: "opus",
        description: buildOpusHead(config.audioChannels),
        numberOfChannels: config.audioChannels,
        sampleRate: config.audioSampleRate,
      },
    });
    return;
  }
  handle.audio(raw, "key", tsUs);
}

async function pump(
  transformer: RtcTransformEventData["transformer"],
  kind: "video" | "audio",
): Promise<void> {
  try {
    const reader = transformer.readable.getReader();
    const writerStream = transformer.writable.getWriter();
    // eslint-disable-next-line no-constant-condition
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      if (kind === "video") {
        captureVideo(value as RTCEncodedVideoFrame);
      } else {
        captureAudio(value as RTCEncodedAudioFrame);
      }
      // Forward the ORIGINAL frame (not the copy) to the decoder. Must not
      // be closed — the WebRTC pipeline owns it after this point.
      await writerStream.write(value);
    }
  } catch (error) {
    postError(`Encoded transform pipeline failed: ${String(error)}`);
  }
}

async function finalize(): Promise<void> {
  if (finalized) return;
  finalized = true;
  try {
    // Drain any frames still in flight after the transforms were detached,
    // with a timeout so a readable that never closes cannot hang the stop.
    await Promise.race([
      Promise.allSettled(pumpTasks),
      new Promise((resolve) => setTimeout(resolve, 2000)),
    ]);
    if (micMixActive && gameDecoder && mixEncoder) {
      // Flush decode → mix → encode so the tail of the recording is not cut
      // off; the encoder output feeds the muxer before finalize. Sequential:
      // decoder.flush() resolves once every pending frame has been decoded
      // (and pushed into the encoder); only then can encoder.flush() drain
      // the re-encoded tail.
      try {
        await gameDecoder.flush();
        await mixEncoder.flush();
      } catch (error) {
        console.warn("[recordEncodedWorker] Mic mix flush failed:", error);
      }
    }
    handle?.finalize();
    writer?.flushAll();
    teardownMicMix();
    post({ type: "done" });
  } catch (error) {
    postError(`Encoded capture finalize failed: ${String(error)}`);
  }
}

self.onmessage = (event: MessageEvent<WorkerInbound>): void => {
  const data = event.data;
  if (data.type === "init") {
    setup(data);
  } else if (data.type === "mic-pcm") {
    // Raw mic PCM (48 kHz mono) + the real-clock capture time. The sync
    // measures the mic's actual rate from the tags; the FIFO holds the
    // samples for the next decoded game-audio frame's pull.
    micSync?.push(data.samples.length, data.capturedAtMs);
    micFifo?.push(data.samples);
  } else if (data.type === "rtc-transform-event") {
    const kind = data.transformer.options?.kind ?? "video";
    pumpTasks.push(pump(data.transformer, kind));
  } else if (data.type === "stop") {
    void finalize();
  }
};
