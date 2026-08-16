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
  detectNalFormat,
  extractNalUnits,
  hasAv1SequenceHeader,
  hasH264ParameterSets,
  hasH265KeyframeNal,
  hasH265ParameterSets,
  h264NalType,
  mixGameAudioWithMic,
  patchAv1CInMp4,
  rtpTimestampToMicroseconds,
  unwrapRtpTimestamp,
  AUDIO_RTP_CLOCK,
  DEFAULT_MIC_MIX_GAIN,
  DvrAudioRing,
  DvrVideoRing,
  dvrPreRollUsForSeconds,
  DVR_RING_MAX_BYTES,
  MicPcmFifo,
  MicSync,
  VIDEO_RTP_CLOCK,
  type EncodedCaptureCodec,
  type EncodedCaptureContainer,
  type NalUnit,
} from "./encodedCapture";

export type EncodedWorkerOutboundMessage =
  | { type: "ready" }
  | { type: "chunk"; data: ArrayBuffer }
  | { type: "done" }
  | { type: "diag"; message: string }
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
  /**
   * GFN-style DVR pre-roll: seconds of keyframe-aligned video (and audio)
   * kept in the ring while the worker is armed, prepended to the recording
   * when `start` arrives. 0 disables the ring (capture starts at `start`).
   */
  dvrSeconds: number;
}

/** Begin a recording from the armed state: slice the DVR ring and mux live. */
interface EncodedCaptureStart {
  type: "start";
}

interface EncodedCaptureStop {
  type: "stop";
  /**
   * True when the recording is being aborted (no finalize, no chunks kept):
   * the worker clears the recording state and stays armed for the next one.
   */
  abort?: boolean;
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

/**
 * The transformer delivered to the worker for each RTCRtpScriptTransform
 * attached (video + audio receivers) via the `rtctransform` event.
 */
interface RtcTransformer {
  options?: { kind?: "video" | "audio" };
  readable: ReadableStream<RTCEncodedVideoFrame | RTCEncodedAudioFrame>;
  writable: WritableStream<RTCEncodedVideoFrame | RTCEncodedAudioFrame>;
}

type WorkerInbound = EncodedCaptureInit | EncodedCaptureStart | EncodedCaptureStop | EncodedCaptureMicPcm;

// Minimal worker-global typing: the DOM lib's `self` (Window) is a lie inside
// a dedicated worker, and pulling lib.webworker into this DOM project would
// collide with the DOM declarations (same pattern as recordCanvasWorker.ts).
declare const self: {
  onmessage: ((event: MessageEvent<WorkerInbound>) => void) | null;
  /**
   * `new RTCRtpScriptTransform(worker, options)` delivers the transformer via
   * an `rtctransform` EVENT at the worker global scope — it does NOT come
   * through postMessage/onmessage. Without this handler the attached
   * transforms never receive a single frame and the recording finalizes as a
   * header-only (~1 KB) file.
   */
  onrtctransform: ((event: { transformer: RtcTransformer }) => void) | null;
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
let lastAudioRtp: number | null = null;
let lastVideoTsUs: number | null = null;
let lastAudioTsUs: number | null = null;
let finalized = false;
// DVR ring (GFN-style pre-roll): the worker is armed from the moment the
// transforms attach (session start) and buffers the last `dvrPreRollUs` of
// keyframe-aligned video (plus a matching audio window) here. `start` slices
// the ring and muxes it ahead of the live frames; the ring keeps filling
// between recordings so every save carries a pre-roll, never starts mid-GOP
// and can never come out empty.
let dvrVideoRing: DvrVideoRing | null = null;
let dvrAudioRing: DvrAudioRing | null = null;
let dvrPreRollUs = 0;
// True between `start` and `stop`: live frames feed the muxer (and the ring
// keeps its pre-roll semantics — `start` already sliced the head).
let recordingActive = false;
// True between `start` and the first keyframe: the muxer cannot exist before
// the codec record is built, so `start` defers it until the first keyframe
// arrives and begins live capture then (pre-roll head already buffered).
let recordingPendingMuxer = false;
// µs added to every muxed audio timestamp while recording: the video pre-roll
// head lands earlier than the record point, so audio is shifted forward by
// the pre-roll duration to stay in sync (A/V never drift — both clocks are
// anchored to the RTP timeline; only the pre-roll video precedes the audio).
let audioShiftUs = 0;
// lastVideoTsUs at `start`: the DVR slice window (`nowUs` for sliceHead).
let recordingStartUs = 0;
// The codec configuration record (avcC/hvcC/av1C) + codec string, built once
// from the first keyframe and reused for every recording in this session.
let codecDescription: Uint8Array | null = null;
let codecString = "";
// One-shot diagnostics (bounded, so a session never floods the log): the
// first video/audio frame (format + first bytes) and config events. They
// pin down which stage a header-only recording fails at (no frame, no
// keyframe, or unparseable NALs) from the exported log alone.
let diagVideoSent = false;
let diagAudioSent = false;
let diagKeyNoParamsSent = false;
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

function postDiag(message: string): void {
  post({ type: "diag", message });
}

function hexOf(data: Uint8Array, max = 8): string {
  return Array.from(data.slice(0, max), (byte) => byte.toString(16).padStart(2, "0")).join(" ");
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
      // Shift by the pre-roll duration so the mixed track lands after the
      // pre-roll video (see audioShiftUs).
      const timestampUs = chunk.timestamp + audioShiftUs;
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
    // The worker is armed for the whole session (transforms stay attached).
    // The main thread re-posts init before each recording (fresh dimensions /
    // mixMic), so re-init must NOT reset the rings or the codec record — it
    // only refreshes the muxer params and the mic chain.
    const firstInit = config === null;
    config = init;
    if (firstInit) {
      dvrPreRollUs = dvrPreRollUsForSeconds(init.dvrSeconds);
      if (dvrPreRollUs > 0) {
        dvrVideoRing = new DvrVideoRing(DVR_RING_MAX_BYTES, dvrPreRollUs);
        if (init.hasAudio) dvrAudioRing = new DvrAudioRing(DVR_RING_MAX_BYTES / 8, dvrPreRollUs);
      }
    } else {
      // Re-init: cancel any in-flight recording state and reconcile the mic
      // chain with the (possibly changed) mixMic flag.
      handle = null;
      writer = null;
      recordingActive = false;
      recordingPendingMuxer = false;
      audioConfigReady = false;
      av1CPatched = false;
      audioShiftUs = 0;
      if (init.mixMic && !micMixActive) {
        setupMicMix(init);
      } else if (!init.mixMic && micMixActive) {
        teardownMicMix();
      }
    }
    if (init.mixMic && !micMixActive) {
      setupMicMix(init);
    }
    // The muxer is created per-recording by startRecording (it needs the
    // codec record from the first keyframe); the writer is created once and
    // reused across recordings.
    writer = new PositionalWriter(postChunk);
    post({ type: "ready" });
  } catch (error) {
    postError(`Encoded capture init failed: ${String(error)}`);
  }
}

/**
 * Create the muxer for a recording and feed it the DVR pre-roll head (video
 * slice + audio slice trimmed to the video head, timestamps shifted so the
 * audio lands after the pre-roll video). Called from `start` once the codec
 * record exists, and from `captureVideo` when the first keyframe arrives for
 * a recording that started before any keyframe.
 */
function beginMuxer(): void {
  const init = config;
  if (!init || handle || !videoConfigReady || !codecDescription) return;
  writer = new PositionalWriter(postChunk);
  const onData = (data: Uint8Array, position: number): void => {
    // mp4-muxer hardcodes a zeroed av1C box for AV1 (it ignores
    // decoderConfig.description); patch the emitted moov with the real
    // record built from the first keyframe. The moov is the first section
    // the muxer emits, so the first match is the box, never sample data.
    if (!av1CPatched && init.codec === "av1" && av1C && patchAv1CInMp4(data, av1C)) {
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
  // Feed the buffered pre-roll head. Video frames were pushed while armed
  // with their raw bytes; convert here (Annex-B → length-prefixed) exactly
  // once, then mux. The head is keyframe-aligned by construction.
  const head = dvrVideoRing?.sliceHead(recordingStartUs) ?? null;
  if (head) {
    for (const frame of head.frames) {
      // Ring frames carry no metadata — the decoderConfig for the first video
      // sample is supplied by the muxer's video config (codec record).
      handle.video(convertSample(frame.data), frame.type, frame.tsUs);
    }
  }
  if (init.hasAudio && !micMixActive) {
    // Trim the audio pre-roll to the video head so both tracks start at the
    // same wall-clock moment, then shift forward by the pre-roll duration so
    // the audio lands after the pre-roll video (the muxer's
    // firstTimestampBehavior offsets the timeline to the first chunk).
    const audioFrames = dvrAudioRing?.sliceFrom(head ? head.headUs : Number.POSITIVE_INFINITY) ?? [];
    for (const frame of audioFrames) {
      // Ring audio frames never carry metadata (opus needs none — the
      // OpusHead config is supplied by the muxer's audio config).
      handle.audio(frame.data, "key", frame.tsUs + audioShiftUs);
    }
  }
}

/** Start a recording from the armed state (DVR slice + live mux). */
function startRecording(): void {
  if (!config || recordingActive || finalized) return;
  recordingActive = true;
  recordingPendingMuxer = !videoConfigReady;
  recordingStartUs = lastVideoTsUs ?? 0;
  // The video head lands `preRollUs` before the record point; shift the audio
  // (pre-roll slice and live) forward by that duration to keep A/V in sync.
  audioShiftUs = dvrPreRollUs;
  // New muxer per recording (chunks for the previous one were already
  // emitted and the recording id closed by the main thread).
  handle = null;
  writer = null;
  av1CPatched = false;
  audioConfigReady = false;
  if (videoConfigReady) {
    beginMuxer();
    postDiag(
      `encoded recording: start (DVR pre-roll ${Math.round(audioShiftUs / 1000)}ms, ring=${dvrVideoRing?.length ?? 0} video frames)`,
    );
  } else {
    postDiag(
      "encoded recording: start before any keyframe — capturing from the next keyframe",
    );
  }
}

function captureVideo(frame: RTCEncodedVideoFrame): void {
  if (!config || finalized) return;
  // Video frame metadata carries NO rtpTimestamp — getMetadata() only returns
  // synchronizationSource/contributingSources/payloadType for video (the RTP
  // timestamp is audio-only). Reading it here returned undefined for every
  // frame and silently dropped the whole video track (header-only ~1 KB
  // recording). The frame's own `timestamp` (µs, derived from the RTP clock)
  // is the timeline; fall back to the RTP metadata when a runtime lacks it.
  const metadata = frame.getMetadata();
  let timestampUs = Number.isFinite(frame.timestamp) ? frame.timestamp : NaN;
  if (!Number.isFinite(timestampUs)) {
    const rtp = metadata.rtpTimestamp ?? (metadata as { timestamp?: number }).timestamp;
    if (rtp === undefined) return;
    timestampUs = rtpTimestampToMicroseconds(rtp, VIDEO_RTP_CLOCK);
  }
  // Keep the muxer happy: timestamps must be strictly increasing. Guard
  // against resets (NTP/clock jumps) that could walk the timeline backwards.
  const tsUs = lastVideoTsUs === null ? timestampUs : Math.max(timestampUs, lastVideoTsUs + 1);
  lastVideoTsUs = tsUs;
  const raw = new Uint8Array(frame.data.byteLength);
  raw.set(new Uint8Array(frame.data));
  const frameType: "key" | "delta" = frame.type === "key" ? "key" : "delta";
  // Keyframe detection is CONTENT-driven, not frame.type: Chromium has
  // shipped builds where the receiver-side frame.type is always "delta"
  // (the type is only reliably set on the sender path), which silently
  // dropped every keyframe and left the recording header-only. Parameter
  // sets (VPS/SPS/PPS, SPS/PPS) and the AV1 sequence header only ever
  // appear in keyframes, so their presence IS the keyframe test — and it is
  // exactly the content the config record is built from.
  let contentKey = false;
  let nals: NalUnit[] | null = null;
  let nalFormat = "";
  if (config.codec === "avc" || config.codec === "hevc") {
    nals = extractNalUnits(raw);
    nalFormat = detectNalFormat(raw);
    contentKey =
      config.codec === "avc"
        ? hasH264ParameterSets(nals) || nals.some((nal) => h264NalType(nal.payload) === 5)
        : hasH265ParameterSets(nals) || hasH265KeyframeNal(nals);
  } else if (config.codec === "av1") {
    contentKey = hasAv1SequenceHeader(raw);
  }
  const type: "key" | "delta" = frameType === "key" || contentKey ? "key" : "delta";
  if (!diagVideoSent) {
    diagVideoSent = true;
    postDiag(
      `encoded video: first frame len=${raw.length} frame.type=${frameType} contentKey=${contentKey}` +
        (config.codec === "avc" || config.codec === "hevc"
          ? ` nals=${nals?.length ?? 0} fmt=${nalFormat} first=[${hexOf(raw)}]`
          : ` seqHdr=${contentKey} first=[${hexOf(raw)}]`),
    );
  }
  // Build the codec record from the first keyframe exactly once per session
  // (while armed — long before any recording), then reuse it for every
  // recording's muxer.
  if (!videoConfigReady) {
    // Codec config (avcC/hvcC/av1C) lives in the first keyframe's parameter
    // sets / sequence header; skip until it arrives. Delta frames before it
    // carry no parameter sets and cannot seed the config record.
    if (!contentKey) {
      // A frame Chromium labelled key but which carried no parameter sets
      // (encoder skipped them mid-stream) is logged once so a recording that
      // stays header-only is diagnosable from the export.
      if (frameType === "key" && !diagKeyNoParamsSent) {
        diagKeyNoParamsSent = true;
        postDiag(
          `encoded video: key frame without parameter sets — waiting for the next keyframe (${config.codec})`,
        );
      }
      // Keep buffering while armed — the next keyframe (or a recording that
      // starts now) will slice whatever head exists.
      if (!recordingActive && dvrVideoRing) {
        dvrVideoRing.push({ data: raw, tsUs, type });
      }
      return;
    }
    let description: Uint8Array | null = null;
    if (config.codec === "avc" || config.codec === "hevc") {
      const nalus = nals ?? extractNalUnits(raw);
      description =
        config.codec === "avc" ? buildAvcCFromNalus(nalus) : buildHvcCFromNalus(nalus);
      codecString = config.codec === "avc" ? "avc1.42001f" : "hvc1.1.6.L93.B0";
    } else if (config.codec === "av1") {
      description = buildAv1CFromObuStream(raw);
      codecString = "av01.0.04M.08";
    }
    if (!description) return;
    videoConfigReady = true;
    codecDescription = description;
    postDiag(
      `encoded video: config ready codec=${config.codec} nals=${nals?.length ?? 0} fmt=${nalFormat} len=${raw.length}`,
    );
    if (config.codec === "av1") {
      // Remember the record for the emitted-stream patch; AV1 samples are
      // passed through unchanged (low-overhead OBU stream, already the
      // format MP4 stores).
      av1C = description;
    }
    if (recordingPendingMuxer) {
      // The recording started before any keyframe — the muxer could not exist
      // yet; create it now with the pre-roll head and start live capture.
      beginMuxer();
      recordingPendingMuxer = false;
    } else if (!recordingActive && dvrVideoRing) {
      dvrVideoRing.push({ data: raw, tsUs, type });
    }
    if (!recordingActive) return;
    handle?.video(convertSample(raw), type, tsUs, {
      decoderConfig: { codec: codecString, description },
    });
    return;
  }
  if (recordingPendingMuxer && handle) {
    // The muxer became available after the pending start (defensive — the
    // keyframe path above normally resolves it).
    recordingPendingMuxer = false;
  }
  if (recordingActive && handle) {
    handle.video(convertSample(raw), type, tsUs);
    return;
  }
  // Armed: buffer into the DVR ring for the next recording's pre-roll.
  if (dvrVideoRing) {
    dvrVideoRing.push({ data: raw, tsUs, type });
  }
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
  if (!config || !config.hasAudio || finalized) return;
  if (!diagAudioSent) {
    diagAudioSent = true;
    const bytes = new Uint8Array(frame.data.byteLength);
    bytes.set(new Uint8Array(frame.data));
    postDiag(`encoded audio: first frame len=${bytes.length} first=[${hexOf(bytes)}]`);
  }
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
  if (recordingActive && micMixActive && gameDecoder) {
    // Mic mixing (live, after the pre-roll head): decode the game opus, mix
    // with the mic PCM, re-encode. Audio chunks are treated as key (every
    // opus packet is independently decodable; the transform's frames carry
    // no key/delta distinction). The encoder output applies audioShiftUs so
    // the mixed track lands after the pre-roll video.
    gameDecoder.decode(
      new EncodedAudioChunk({
        type: "key",
        timestamp: tsUs,
        data: raw,
      }),
    );
    return;
  }
  if (recordingActive && handle) {
    // Live audio (no mic mix): mux after the pre-roll head, shifted by the
    // pre-roll duration so A/V stay in sync (the muxer's offset behavior
    // anchors the timeline to the first chunk, which is the pre-roll video).
    if (!audioConfigReady) {
      audioConfigReady = true;
      handle.audio(raw, "key", tsUs + audioShiftUs, {
        decoderConfig: {
          codec: "opus",
          description: buildOpusHead(config.audioChannels),
          numberOfChannels: config.audioChannels,
          sampleRate: config.audioSampleRate,
        },
      });
      return;
    }
    handle.audio(raw, "key", tsUs + audioShiftUs);
    return;
  }
  // Armed: buffer into the audio ring for the next recording's pre-roll.
  dvrAudioRing?.push({ data: raw, tsUs, type: "key" });
}

async function pump(
  transformer: RtcTransformer,
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

/**
 * Finalize the current recording. The worker stays ARMED afterwards (the
 * transforms remain attached, the rings keep buffering) so the next
 * recording can slice a fresh pre-roll without respawning the worker. The
 * main thread waits for `done` before starting the next recording, and
 * terminates the worker at session end.
 */
async function finalize(): Promise<void> {
  if (finalized) return;
  finalized = true;
  try {
    // The transforms stay attached, so the pump loops never exit on their
    // own — nothing to drain; the last frame the transform delivered was
    // already muxed synchronously by captureVideo/captureAudio. What remains
    // is the mic chain's re-encode tail.
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
    // Re-arm for the next recording: clear per-recording state (the codec
    // record and rings survive).
    handle = null;
    writer = null;
    recordingActive = false;
    recordingPendingMuxer = false;
    audioConfigReady = false;
    av1CPatched = false;
    audioShiftUs = 0;
    finalized = false;
    post({ type: "done" });
  } catch (error) {
    postError(`Encoded capture finalize failed: ${String(error)}`);
  }
}

/** Abort the current recording without finalizing: drop state, stay armed. */
function abortRecording(): void {
  handle = null;
  writer = null;
  recordingActive = false;
  recordingPendingMuxer = false;
  audioConfigReady = false;
  av1CPatched = false;
  audioShiftUs = 0;
  // Keep the rings (pre-roll continues uninterrupted for the next recording).
}

// The RTCRtpScriptTransform constructor fires this event at the worker for
// EACH transform attached (video + audio receivers), carrying the transformer
// with its readable/writable stream pair + the options passed at construction
// ({ kind: "video" | "audio" }). Diag once per delivered transformer so a
// header-only recording can distinguish "Chromium never invoked the transform"
// (event absent) from "event fired but no frame ever arrived" (event present,
// no first-frame diag).
function handleTransformer(transformer: RtcTransformer): void {
  const kind = transformer.options?.kind ?? "video";
  postDiag(
    `encoded transform delivered kind=${kind} options=${JSON.stringify(transformer.options ?? null)}`,
  );
  pumpTasks.push(pump(transformer, kind));
}

self.onrtctransform = (event: { transformer: RtcTransformer }): void => {
  handleTransformer(event.transformer);
};

self.onmessage = (event: MessageEvent<WorkerInbound>): void => {
  const data = event.data;
  // Legacy transformer delivery (older Chromium/Firefox): the transformer was
  // posted as a message payload (`event.data.transformer` / `.rtctransform`)
  // instead of the `rtctransform` event. Accept both so recordings still work
  // on runtimes that predate the event.
  const legacyTransformer = (data as { transformer?: unknown }).transformer ??
    (data as { rtctransform?: unknown }).rtctransform;
  if (legacyTransformer !== undefined && legacyTransformer !== null) {
    handleTransformer(legacyTransformer as RtcTransformer);
    return;
  }
  if (data.type === "init") {
    setup(data);
  } else if (data.type === "start") {
    startRecording();
  } else if (data.type === "mic-pcm") {
    // Raw mic PCM (48 kHz mono) + the real-clock capture time. The sync
    // measures the mic's actual rate from the tags; the FIFO holds the
    // samples for the next decoded game-audio frame's pull.
    micSync?.push(data.samples.length, data.capturedAtMs);
    micFifo?.push(data.samples);
  } else if (data.type === "stop") {
    if (data.abort) {
      abortRecording();
    } else {
      void finalize();
    }
  }
};
