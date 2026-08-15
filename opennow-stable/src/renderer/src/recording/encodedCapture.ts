/**
 * Encoded capture helpers for the web recorder's receiver-side encoded
 * transform (GFN parity: record the stream's encoded bitstream pre-decode and
 * mux it, so recording does ZERO re-encode and never competes with the
 * decoder). All functions here are pure (no DOM/WebRTC access) except
 * `inspectEncodedCapture`, which runs on the main thread only; the worker
 * imports only the pure builders.
 */

/** mp4-muxer / webm-muxer codec names + the container each one maps to. */
export type EncodedCaptureCodec = "avc" | "hevc" | "av1" | "vp8" | "vp9";
export type EncodedCaptureContainer = "mp4" | "webm";

/** Every GFN video codec uses the 90 kHz RTP clock; Opus uses 48 kHz. */
export const VIDEO_RTP_CLOCK = 90_000;
export const AUDIO_RTP_CLOCK = 48_000;

export interface EncodedCaptureInfo {
  codec: EncodedCaptureCodec;
  container: EncodedCaptureContainer;
  /** mimeType to hand `beginRecording` (drives the saved file extension). */
  mimeType: string;
  width: number;
  height: number;
  fps: number;
  hasAudio: boolean;
  audioChannels: number;
  audioSampleRate: number;
}

/**
 * Map an RTCRtpCodecParameters mimeType ("video/H264", "video/AV1", …) to the
 * muxer codec name, or `null` when the codec cannot be captured/muxed by the
 * encoded path (caller then falls back to the classic recorder).
 */
export function encodedCodecFromMime(mimeType: string): EncodedCaptureCodec | null {
  const m = mimeType.toLowerCase();
  if (m.includes("h264") || m.includes("avc")) return "avc";
  if (m.includes("h265") || m.includes("hevc")) return "hevc";
  if (m.includes("av1")) return "av1";
  if (m.includes("vp9")) return "vp9";
  if (m.includes("vp8")) return "vp8";
  return null;
}

export function containerForCodec(codec: EncodedCaptureCodec): EncodedCaptureContainer {
  // AVC/HEVC/AV1 live in MP4 (Annex-B → length-prefixed samples + avcC/hvcC
  // for AVC/HEVC; the AV1 OBU stream passes through with the av1C box patched
  // in place — see patchAv1CInMp4). VP8/VP9 are WebM-native (mp4-muxer's vp9
  // path additionally needs a color-space config we cannot provide).
  return codec === "avc" || codec === "hevc" || codec === "av1" ? "mp4" : "webm";
}

export function mimeTypeForEncodedCapture(codec: EncodedCaptureCodec): string {
  return containerForCodec(codec) === "mp4" ? "video/mp4" : "video/webm";
}

/**
 * Decide whether the encoded recorder can carry the audio track, from the
 * receiver's negotiated codec list. Returns the opus codec params when audio
 * can be muxed, else null.
 *
 * Two GFN realities are handled:
 * - the game-audio m-line negotiates RED (redundant encoding) in front of
 *   opus (`a=rtpmap:63 red/48000/2`, fmtp `111/111`), so codecs[0] is RED,
 *   not opus — the opus payload is found by scanning the FULL list;
 * - when RED is negotiated, the receiver-side encoded transform delivers the
 *   RED-WRAPPED packets (RED unwrapping happens inside the engine's audio
 *   decoder, after the transform) — muxing those bytes as opus would corrupt
 *   the track, so the audio track is skipped rather than recorded as garbage.
 *   The recording stays playable (video-only).
 */
export function encodedAudioParamsForCodecs(
  codecs: Array<{ mimeType?: string | null; channels?: number }>,
): { codec: { mimeType?: string | null; channels?: number }; channels: number } | null {
  const opus = codecs.find((codec) => (codec.mimeType ?? "").toLowerCase().includes("opus"));
  const hasRed = codecs.some((codec) => (codec.mimeType ?? "").toLowerCase().includes("red"));
  if (!opus || hasRed) return null;
  return {
    codec: opus,
    channels: Math.max(1, Math.min(2, opus.channels ?? 2)),
  };
}

/**
 * Convert an RTP timestamp (codec clock ticks) to microseconds for the
 * muxer. Handles the unsigned 32-bit wraparound of the RTP clock via
 * `unwrapRtpTimestamp`.
 */
export function rtpTimestampToMicroseconds(rtpTimestamp: number, clockRate: number): number {
  return Math.round((rtpTimestamp / clockRate) * 1_000_000);
}

/**
 * Unwrap a 32-bit RTP timestamp against the previous one, so a wraparound
 * (or a mid-stream reset) keeps the timeline monotonic instead of jumping
 * backwards by ~2^32.
 */
export function unwrapRtpTimestamp(timestamp: number, last: number | null): number {
  if (last === null) return timestamp;
  if (timestamp < last) {
    const wrapped = timestamp + 0x1_0000_0000;
    // Only treat it as a wraparound when the jump is smaller than half the
    // 32-bit range; anything larger is a genuine reset we cannot reconcile.
    if (wrapped - last < 0x8000_0000) return wrapped;
  }
  return timestamp;
}

/** One Annex-B NAL unit payload (first byte = NAL header). */
export interface NalUnit {
  payload: Uint8Array;
}

/**
 * Split an Annex-B bitstream (3- or 4-byte start codes) into NAL payloads.
 * NAL payloads keep emulation-prevention bytes intact, exactly as decoders
 * and codec-configuration records expect them.
 */
export function extractNalUnits(data: Uint8Array): NalUnit[] {
  const nals: NalUnit[] = [];
  let i = 0;
  while (i + 2 < data.length) {
    let start = -1;
    if (data[i] === 0 && data[i + 1] === 0) {
      if (data[i + 2] === 1) {
        start = i + 3;
      } else if (i + 3 < data.length && data[i + 2] === 0 && data[i + 3] === 1) {
        start = i + 4;
      }
    }
    if (start === -1) {
      i += 1;
      continue;
    }
    let end = start;
    let foundNext = false;
    while (end + 2 < data.length) {
      if (data[end] === 0 && data[end + 1] === 0) {
        if (data[end + 2] === 1) {
          foundNext = true;
          break;
        }
        if (end + 3 < data.length && data[end + 2] === 0 && data[end + 3] === 1) {
          foundNext = true;
          break;
        }
      }
      end += 1;
    }
    if (!foundNext) end = data.length;
    if (end > start) nals.push({ payload: data.slice(start, end) });
    i = end;
  }
  return nals;
}

/**
 * Convert Annex-B NAL units to the length-prefixed sample format MP4
 * requires (each NAL preceded by its size — 4 bytes, matching
 * lengthSizeMinusOne = 3 in avcC/hvcC).
 */
export function annexBToLengthPrefixed(nals: NalUnit[], lengthSize = 4): Uint8Array {
  let total = 0;
  for (const nal of nals) total += lengthSize + nal.payload.length;
  const out = new Uint8Array(total);
  let offset = 0;
  for (const nal of nals) {
    const length = nal.payload.length;
    for (let b = lengthSize - 1; b >= 0; b -= 1) out[offset++] = (length >>> (8 * b)) & 0xff;
    out.set(nal.payload, offset);
    offset += length;
  }
  return out;
}

/**
 * Build an AVCDecoderConfigurationRecord (avcC) from the SPS/PPS NALs of an
 * H.264 keyframe. Returns null when the keyframe lacks the parameter sets.
 */
export function buildAvcCFromNalus(nalus: NalUnit[]): Uint8Array | null {
  const sps = nalus.find((nal) => (nal.payload[0] & 0x1f) === 7);
  const pps = nalus.find((nal) => (nal.payload[0] & 0x1f) === 8);
  if (!sps || !pps || sps.payload.length < 4) return null;
  const spsBytes = sps.payload;
  const ppsBytes = pps.payload;
  const out = new Uint8Array(11 + spsBytes.length + ppsBytes.length);
  out[0] = 1; // configurationVersion
  out[1] = spsBytes[1]; // profile_idc
  out[2] = spsBytes[2]; // constraint_set flags + reserved
  out[3] = spsBytes[3]; // level_idc
  out[4] = 0xff; // reserved(6) + lengthSizeMinusOne(2) = 3
  out[5] = 0xe1; // reserved(3) + numOfSequenceParameterSets(5) = 1
  out[6] = (spsBytes.length >> 8) & 0xff;
  out[7] = spsBytes.length & 0xff;
  out.set(spsBytes, 8);
  let offset = 8 + spsBytes.length;
  out[offset++] = 1; // numOfPictureParameterSets
  out[offset++] = (ppsBytes.length >> 8) & 0xff;
  out[offset++] = ppsBytes.length & 0xff;
  out.set(ppsBytes, offset);
  return out;
}

/**
 * Build a HEVCDecoderConfigurationRecord (hvcC) from the VPS/SPS/PPS NALs of
 * an H.265 keyframe. The SPS bytes map directly onto the record's
 * profile_tier_level fields (general_profile_space/tier/profile_idc in
 * SPS byte 1, compatibility flags bytes 2-5, constraint flags 6-11, level in
 * byte 12 — the HEVC SPS syntax). Chroma format / bit depth are not parsed
 * from the SPS bitstream; GFN HEVC streams are 8-bit 4:2:0, which is the
 * default the record carries.
 */
export function buildHvcCFromNalus(nalus: NalUnit[]): Uint8Array | null {
  const vps = nalus.find((nal) => ((nal.payload[0] >> 1) & 0x3f) === 32);
  const sps = nalus.find((nal) => ((nal.payload[0] >> 1) & 0x3f) === 33);
  const pps = nalus.find((nal) => ((nal.payload[0] >> 1) & 0x3f) === 34);
  if (!vps || !sps || !pps || sps.payload.length < 13) return null;
  const s = sps.payload;
  const generalProfileSpace = (s[1] >> 6) & 0x03;
  const generalTierFlag = (s[1] >> 5) & 0x01;
  const generalProfileIdc = s[1] & 0x1f;
  const maxSubLayers = Math.min(7, (s[0] >> 3) & 0x07);
  const temporalIdNested = (s[0] >> 2) & 0x01;
  const arrays: { type: number; data: Uint8Array }[] = [
    { type: 32, data: vps.payload },
    { type: 33, data: sps.payload },
    { type: 34, data: pps.payload },
  ];
  let size = 23;
  for (const array of arrays) size += 3 + 2 + array.data.length;
  const out = new Uint8Array(size);
  let offset = 0;
  out[offset++] = 1; // configurationVersion
  out[offset++] = (generalProfileSpace << 6) | (generalTierFlag << 5) | generalProfileIdc;
  out.set(s.subarray(2, 6), offset); // general_profile_compatibility_flags
  offset += 4;
  out.set(s.subarray(6, 12), offset); // general_constraint_indicator_flags (48 bits)
  offset += 6;
  out[offset++] = s[12]; // general_level_idc
  out[offset++] = 0xf0; // reserved(4) + min_spatial_segmentation_idc(12) hi nibble
  out[offset++] = 0x00; // …lo byte
  out[offset++] = 0xfc; // reserved(6) + parallelismType(2) = 0
  out[offset++] = 0xfc | 0x01; // reserved(6) + chroma_format_idc = 1 (4:2:0)
  out[offset++] = 0xf8 | 0x00; // reserved(5) + bit_depth_luma_minus8 = 0
  out[offset++] = 0xf8 | 0x00; // reserved(5) + bit_depth_chroma_minus8 = 0
  out[offset++] = 0x00; // avgFrameRate hi
  out[offset++] = 0x00; // avgFrameRate lo
  out[offset++] =
    ((maxSubLayers & 0x07) << 3) | ((temporalIdNested & 0x01) << 2) | 0x03; // constantFrameRate=0, numTemporalLayers, temporalIdNested, lengthSizeMinusOne=3
  out[offset++] = arrays.length; // numOfArrays
  for (const array of arrays) {
    out[offset++] = 0x80 | (array.type & 0x3f); // array_completeness=1, reserved=0, NAL_unit_type
    out[offset++] = 0x00; // numNalus hi
    out[offset++] = 0x01; // numNalus lo
    out[offset++] = (array.data.length >> 8) & 0xff;
    out[offset++] = array.data.length & 0xff;
    out.set(array.data, offset);
    offset += array.data.length;
  }
  return out;
}

/**
 * Opus identification header (OpusHead, 19 bytes) — the codec-configuration
 * record both mp4-muxer (dOps) and webm-muxer (CodecPrivate) consume.
 * Pre-skip 3840 samples at 48 kHz, channel mapping family 0 (one stream).
 */
export function buildOpusHead(channels: number): Uint8Array {
  const out = new Uint8Array(19);
  out.set([0x4f, 0x70, 0x75, 0x73, 0x48, 0x65, 0x61, 0x64], 0); // "OpusHead"
  out[8] = 1; // version
  out[9] = channels;
  out[10] = 0x00;
  out[11] = 0x0f; // pre-skip (little-endian) = 3840
  out[12] = 0x80;
  out[13] = 0xbb;
  out[14] = 0x00;
  out[15] = 0x00; // input sample rate (LE) = 48000
  out[16] = 0x00;
  out[17] = 0x00; // output gain
  out[18] = 0x00; // channel mapping family
  return out;
}

// --- AV1 sequence-header parsing (AV1 spec §5.5.1) → av1C record -----------

/** Minimal MSB-first bit reader over an OBU payload. */
class Av1BitReader {
  private byteIndex = 0;
  private bitIndex = 0;

  constructor(private readonly data: Uint8Array) {}

  readBits(count: number): number {
    let value = 0;
    for (let i = 0; i < count; i += 1) {
      if (this.byteIndex >= this.data.length) throw new Error("av1: bitstream too short");
      const bit = (this.data[this.byteIndex] >> (7 - this.bitIndex)) & 1;
      value = (value << 1) | bit;
      this.bitIndex += 1;
      if (this.bitIndex === 8) {
        this.bitIndex = 0;
        this.byteIndex += 1;
      }
    }
    return value;
  }
}

export interface Av1SequenceHeader {
  profile: number;
  level: number;
  tier: number;
  highBitdepth: boolean;
  twelveBit: boolean;
  monochrome: boolean;
  subsamplingX: boolean;
  subsamplingY: boolean;
  chromaSamplePosition: number;
}

/**
 * Extract the AV1 sequence-header OBU (type 1) from an OBU stream (the
 * low-overhead format WebRTC's AV1 depacketizer produces) and parse the
 * fields the av1C record needs.
 */
export function extractAv1SequenceHeader(data: Uint8Array): Av1SequenceHeader | null {
  let offset = 0;
  while (offset < data.length) {
    const header = data[offset];
    const obuType = (header >> 3) & 0x0f;
    const extensionFlag = (header >> 4) & 0x01;
    const hasSizeField = (header >> 2) & 0x01;
    let payloadStart = offset + 1 + (extensionFlag ? 1 : 0);
    if (!hasSizeField) {
      return obuType === 1 ? parseAv1SequenceHeader(data.slice(payloadStart)) : null;
    }
    // LEB128 size
    let size = 0;
    let shift = 0;
    let p = payloadStart;
    let finished = false;
    for (let i = 0; i < 8; i += 1) {
      if (p >= data.length) return null;
      const byte = data[p++];
      size |= (byte & 0x7f) << shift;
      if ((byte & 0x80) === 0) {
        finished = true;
        break;
      }
      shift += 7;
    }
    if (!finished || p + size > data.length) return null;
    if (obuType === 1) return parseAv1SequenceHeader(data.slice(p, p + size));
    offset = p + size;
  }
  return null;
}

export function parseAv1SequenceHeader(payload: Uint8Array): Av1SequenceHeader | null {
  try {
    const r = new Av1BitReader(payload);
    const profile = r.readBits(3);
    r.readBits(1); // still_picture
    const reduced = r.readBits(1);
    let level = 0;
    let tier = 0;
    if (!reduced) {
      if (r.readBits(1) === 1) {
        // timing_info()
        r.readBits(32); // num_units_in_display_tick
        r.readBits(32); // time_scale
        if (r.readBits(1) === 1) r.readBits(32); // equal_picture_interval → num_ticks_per_picture_minus_1
      }
      const decoderModelPresent = r.readBits(1);
      let bufferRemovalTimeLength = 0;
      let numUnitsInDecodingTick = 0;
      if (decoderModelPresent) {
        // decoder_model_info()
        r.readBits(5); // buffer_delay_length_minus_1
        numUnitsInDecodingTick = r.readBits(32);
        bufferRemovalTimeLength = r.readBits(5) + 1;
        r.readBits(5); // frame_presentation_time_length_minus_1
      }
      const initialDisplayDelayPresent = r.readBits(1);
      const operatingPoints = r.readBits(5); // operating_points_cnt_minus_1
      for (let i = 0; i <= operatingPoints; i += 1) {
        r.readBits(12); // operating_point_idc
        const seqLevel = r.readBits(5);
        if (i === 0) level = seqLevel;
        if (seqLevel > 7) {
          const seqTier = r.readBits(1);
          if (i === 0) tier = seqTier;
        }
        if (decoderModelPresent && r.readBits(1) === 1) {
          // decoder_model(): buffer_removal_time for each decode unit. The
          // count is NumUnitsInDecodingTick; clamp against pathological values
          // so a malformed stream cannot hang the parse.
          const count = Math.min(numUnitsInDecodingTick, 1 << 20);
          r.readBits(count * bufferRemovalTimeLength);
        }
        if (initialDisplayDelayPresent && r.readBits(1) === 1) r.readBits(4);
      }
      const frameWidthBits = r.readBits(4) + 1;
      const frameHeightBits = r.readBits(4) + 1;
      r.readBits(frameWidthBits); // max_frame_width_minus_1
      r.readBits(frameHeightBits); // max_frame_height_minus_1
      if (r.readBits(1) === 1) {
        r.readBits(4); // delta_frame_id_length_minus_2
        r.readBits(3); // additional_frame_id_length_minus_1
      }
      r.readBits(1); // use_128x128_superblock
      r.readBits(1); // enable_filter_intra
      r.readBits(1); // enable_intra_edge_filter
      r.readBits(1); // enable_interintra_compound
      r.readBits(1); // enable_masked_compound
      r.readBits(1); // enable_warped_motion
      r.readBits(1); // enable_dual_filter
      const enableOrderHint = r.readBits(1);
      if (enableOrderHint) {
        r.readBits(1); // enable_jnt_comp
        r.readBits(1); // enable_ref_frame_mvs
      }
      if (r.readBits(1) === 0) {
        // seq_choose_screen_content_tools = 0
        const forceScreenContent = r.readBits(1);
        if (forceScreenContent && r.readBits(1) === 0) r.readBits(1); // seq_choose_integer_mv → seq_force_integer_mv
      }
      r.readBits(1); // enable_superres
      r.readBits(1); // enable_cdef
      r.readBits(1); // enable_restoration
      // color_config()
      const highBitdepth = r.readBits(1) === 1;
      let twelveBit = false;
      if (profile === 2 && highBitdepth) twelveBit = r.readBits(1) === 1;
      const monochrome = r.readBits(1) === 1;
      if (r.readBits(1) === 1) {
        r.readBits(8); // color_primaries
        r.readBits(8); // transfer_characteristics
        r.readBits(8); // matrix_coefficients
      }
      let subsamplingX = false;
      let subsamplingY = false;
      let chromaSamplePosition = 0;
      if (monochrome) {
        subsamplingX = true;
        subsamplingY = true;
        r.readBits(1); // color_range
      } else if (profile === 0) {
        subsamplingX = r.readBits(1) === 1;
        subsamplingY = r.readBits(1) === 1;
        chromaSamplePosition = r.readBits(2);
        r.readBits(1); // color_range
      } else if (profile === 1) {
        r.readBits(1); // color_range
      } else {
        // profile 2
        const bitDepth = twelveBit ? 12 : highBitdepth ? 10 : 8;
        if (bitDepth === 12) {
          subsamplingX = r.readBits(1) === 1;
          subsamplingY = r.readBits(1) === 1;
          chromaSamplePosition = r.readBits(2);
          r.readBits(1); // color_range
        } else {
          subsamplingX = true;
          subsamplingY = false;
          r.readBits(1); // color_range
        }
      }
      return {
        profile,
        level,
        tier,
        highBitdepth,
        twelveBit,
        monochrome,
        subsamplingX,
        subsamplingY,
        chromaSamplePosition,
      };
    }
    // reduced_still_picture_header: the header ends right after level/tier.
    level = r.readBits(5);
    if (level > 7) tier = r.readBits(1);
    return {
      profile,
      level,
      tier,
      highBitdepth: false,
      twelveBit: false,
      monochrome: false,
      subsamplingX: true,
      subsamplingY: true,
      chromaSamplePosition: 0,
    };
  } catch {
    return null;
  }
}

/**
 * Build the 4-byte AV1CodecConfigurationRecord from the sequence-header OBU
 * inside a keyframe's OBU stream. This is the av1C box content for AV1-in-MP4
 * (sample entry 'av01'); mp4-muxer writes a zeroed placeholder that the worker
 * patches in place via `patchAv1CInMp4`.
 */
export function buildAv1CFromObuStream(data: Uint8Array): Uint8Array | null {
  const header = extractAv1SequenceHeader(data);
  if (!header) return null;
  const out = new Uint8Array(4);
  out[0] = 0x81; // marker(1) = 1, version(1) = 1
  out[1] = ((header.profile & 0x07) << 5) | (header.level & 0x1f);
  out[2] =
    ((header.tier & 0x01) << 7) |
    ((header.highBitdepth ? 1 : 0) << 6) |
    ((header.twelveBit ? 1 : 0) << 5) |
    ((header.monochrome ? 1 : 0) << 4) |
    ((header.subsamplingX ? 1 : 0) << 3) |
    ((header.subsamplingY ? 1 : 0) << 2) |
    (header.chromaSamplePosition & 0x03);
  out[3] = 0; // reserved(3) + initial_presentation_delay_present(1) + delay(4)
  return out;
}

/**
 * Replace the content of the av1C box inside an MP4 byte stream with the real
 * AV1CodecConfigurationRecord. mp4-muxer writes a zeroed 4-byte placeholder
 * for AV1 (marker/version only); the record is built from the first keyframe
 * long before the moov (which carries the box) is flushed, so this patches the
 * emitted bytes in place. The moov is the first section the muxer emits, so
 * scanning stops at the first match — the AV1 sample data (which could
 * theoretically contain the 'av1C' bytes by coincidence) is never touched.
 * Returns true when a box was patched.
 */
export function patchAv1CInMp4(data: Uint8Array, av1C: Uint8Array): boolean {
  if (av1C.length < 4) return false;
  for (let i = 0; i + 7 < data.length; i += 1) {
    if (data[i] === 0x61 && data[i + 1] === 0x76 && data[i + 2] === 0x31 && data[i + 3] === 0x43) {
      data[i + 4] = av1C[0];
      data[i + 5] = av1C[1];
      data[i + 6] = av1C[2];
      data[i + 7] = av1C[3];
      return true;
    }
  }
  return false;
}

// --- Mic mixing (encoded recordings) ----------------------------------------

/** Default mic gain applied while mixing the mic into the game audio track. */
export const DEFAULT_MIC_MIX_GAIN = 0.6;

/**
 * Sample FIFO for the live mic PCM captured on the main thread. The worker
 * pulls one chunk per decoded game-audio frame; underruns pad with silence,
 * overruns (mic ahead of the game clock) drop the oldest samples so the
 * mixing stays real-time aligned and the buffer stays bounded.
 */
export class MicPcmFifo {
  private samples: Float32Array = new Float32Array(0);
  private head = 0;
  private len = 0;

  /** Total samples currently buffered (logical live count). */
  get available(): number {
    return this.len;
  }

  push(chunk: Float32Array): void {
    if (chunk.length === 0) return;
    if (this.head > 0 && this.len === 0) {
      // Everything consumed — reset instead of compacting an empty tail.
      this.samples = new Float32Array(0);
      this.head = 0;
    }
    const writeAt = this.head + this.len; // logical end of buffered samples
    const needed = writeAt + chunk.length;
    if (this.samples.length < needed) {
      // Grow (doubling) and compact: only the live window is kept.
      const grown = new Float32Array(Math.max(needed, this.samples.length * 2));
      grown.set(this.samples.subarray(this.head, writeAt));
      this.samples = grown;
      this.head = 0;
    }
    this.samples.set(chunk, this.head + this.len);
    this.len += chunk.length;
  }

  /**
   * Pull at most `count` samples (oldest first). Returns fewer when the
   * buffer underruns; the caller pads with silence to keep the timeline
   * continuous.
   */
  pull(count: number): Float32Array {
    const take = Math.min(count, this.len);
    const out = new Float32Array(take);
    out.set(this.samples.subarray(this.head, this.head + take));
    this.head += take;
    this.len -= take;
    if (this.len === 0 && this.head > 0) {
      // Fully drained — free the buffer.
      this.samples = new Float32Array(0);
      this.head = 0;
    }
    return out;
  }
}

/**
 * Real-clock sync between the mic PCM and the game audio timeline.
 *
 * The mic chunks are captured on the main thread and tagged with
 * `performance.now()` (a real clock — NOT `AudioContext.currentTime`, which
 * is tied to the context's own sample counter and therefore always "perfectly
 * in sync" with its sample count). The game timeline is the RTP-derived µs of
 * the encoded audio frames. Naively consuming one mic sample per game sample
 * assumes both clocks tick at exactly the same rate; real devices drift (a
 * mic's hardware clock is typically ±0.1% and the server's RTP clock is its
 * own oscillator), so the FIFO gradually empties or piles up and the voice
 * walks away from the game.
 *
 * This class measures the mic's ACTUAL sample rate from the capture tags and
 * consumes mic samples proportional to the RTP frame duration (anchored to
 * the first decoded game frame) instead of the game frame's sample count.
 * Drift is gone by construction — the only residue is a bounded ±1-sample
 * phase jitter from rounding. Pure and deterministic; the worker owns one
 * instance per recording.
 */
export class MicSync {
  /** Samples per microsecond — nominal 48 kHz, corrected by measurement. */
  private ratePerUs = 48_000 / 1_000_000;
  /** Cumulative mic samples ever fed via push(). */
  private totalPushed = 0;
  private lastTag: { realMs: number; total: number } | null = null;
  /** Game tsUs (RTP-derived µs) of the first decoded game frame. */
  private anchorGameUs: number | null = null;
  /** Mic sample index captured at the real moment of the anchor frame. */
  private anchorMicSample = 0;
  /** Mic samples handed to the mixer so far (anchor-relative). */
  private consumed = 0;

  /** Currently measured mic rate (samples per µs). */
  get measuredRatePerUs(): number {
    return this.ratePerUs;
  }

  /** Cumulative mic samples fed via push(). */
  get pushed(): number {
    return this.totalPushed;
  }

  /**
   * Feed one captured mic chunk: its sample count plus the real-clock time it
   * was captured (performance.now() on the main thread). The rate is measured
   * from the DELTA between consecutive tags — delivery jitter cannot skew it
   * because the tag is stamped at capture, not at postMessage. Clamped to a
   * sane ±5% window so a garbage tag cannot yank the timeline.
   */
  push(samples: number, capturedAtMs: number): void {
    this.totalPushed += samples;
    const prev = this.lastTag;
    if (prev) {
      const dMs = capturedAtMs - prev.realMs;
      const dSamples = this.totalPushed - prev.total;
      // Chunks arrive every ~85 ms (4096 samples / 48 kHz); only re-measure
      // over a meaningful window.
      if (dMs >= 50) {
        this.ratePerUs = clamp((dSamples / dMs) / 1000, 45_600 / 1_000_000, 50_400 / 1_000_000);
        this.lastTag = { realMs: capturedAtMs, total: this.totalPushed };
      }
    } else {
      this.lastTag = { realMs: capturedAtMs, total: this.totalPushed };
    }
  }

  /**
   * How many mic samples the mixer should consume for the game frame
   * presented at `gameTsUs` (RTP-derived µs). Anchors on the first frame:
   * mic samples captured before the game window started belong before the
   * recording, so they are dropped (the anchor remembers their count), and
   * the anchor frame itself consumes none — its mic window is only captured
   * over the following frame duration. Returns the count to pull (0 = none
   * yet); underruns are handled by the caller via commitPulled().
   */
  samplesForFrame(gameTsUs: number): number {
    if (this.anchorGameUs === null) {
      this.anchorGameUs = gameTsUs;
      this.anchorMicSample = this.totalPushed;
      this.consumed = this.totalPushed;
      return 0;
    }
    const target = this.anchorMicSample + (gameTsUs - this.anchorGameUs) * this.ratePerUs;
    const desired = Math.round(target - this.consumed);
    return desired > 0 ? desired : 0;
  }

  /**
   * Report how many samples were actually handed to the mixer (fewer than
   * requested on a FIFO underrun). The shortfall is folded into the next
   * frame's request, so the timeline re-syncs as soon as mic data arrives.
   */
  commitPulled(count: number): void {
    this.consumed += count;
  }
}

function clamp(value: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, value));
}

/**
 * Mix a mono mic chunk into interleaved stereo game audio (both 48 kHz). The
 * mic is duplicated to both channels at `gain`; samples are clamped so the
 * sum cannot exceed the PCM range. `game` must be interleaved stereo (length
 * 2 × frames); `mic` may be shorter (silence for the tail). Pure — no state.
 */
export function mixGameAudioWithMic(
  game: Float32Array,
  mic: Float32Array,
  gain = DEFAULT_MIC_MIX_GAIN,
): Float32Array {
  const frames = Math.floor(game.length / 2);
  const out = new Float32Array(frames * 2);
  for (let i = 0; i < frames; i += 1) {
    const micSample = i < mic.length ? mic[i] * gain : 0;
    const left = game[i * 2] + micSample;
    const right = game[i * 2 + 1] + micSample;
    out[i * 2] = Math.max(-1, Math.min(1, left));
    out[i * 2 + 1] = Math.max(-1, Math.min(1, right));
  }
  return out;
}

// --- Main-thread inspection -------------------------------------------------

/**
 * Inspect the live RTCPeerConnection for a capturable encoded stream. Returns
 * null when the runtime lacks receiver encoded transforms, no video receiver
 * exists yet, the negotiated video codec is not capturable, or the stream
 * dimensions are not known. When non-null, the recorder can attach
 * RTCRtpScriptTransforms to the video/audio receivers and record the
 * bitstream itself — the user's recording cap does NOT apply (capture is
 * zero-cost, so "jangan di cap": the recording is whatever the stream is).
 */
export function inspectEncodedCapture(
  pc: RTCPeerConnection | null,
  video: HTMLVideoElement,
  stream: MediaStream,
): EncodedCaptureInfo | null {
  if (!pc || typeof RTCRtpScriptTransform === "undefined") return null;
  const receivers = pc.getReceivers();
  const videoReceiver = receivers.find((receiver) => receiver.track?.kind === "video");
  if (!videoReceiver) return null;
  const videoMime = videoReceiver.getParameters().codecs[0]?.mimeType ?? "";
  const codec = encodedCodecFromMime(videoMime);
  if (!codec) return null;
  const videoTrackSettings = stream.getVideoTracks()[0]?.getSettings() ?? {};
  const width = video.videoWidth || videoTrackSettings.width || 0;
  const height = video.videoHeight || videoTrackSettings.height || 0;
  if (width <= 0 || height <= 0) return null;
  const fps =
    videoTrackSettings.frameRate && videoTrackSettings.frameRate > 0
      ? Math.max(1, Math.round(videoTrackSettings.frameRate))
      : 60;
  const audioReceiver = receivers.find((receiver) => receiver.track?.kind === "audio") ?? null;
  const audioParams = encodedAudioParamsForCodecs(
    (audioReceiver?.getParameters().codecs ?? []).map((codec) => ({
      mimeType: codec.mimeType,
      channels: codec.channels,
    })),
  );
  const hasAudio = audioReceiver !== null && audioParams !== null;
  return {
    codec,
    container: containerForCodec(codec),
    mimeType: mimeTypeForEncodedCapture(codec),
    width,
    height,
    fps,
    hasAudio,
    audioChannels: audioParams?.channels ?? 2,
    audioSampleRate: 48_000,
  };
}
