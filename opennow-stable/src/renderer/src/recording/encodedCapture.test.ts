import test from "node:test";
import assert from "node:assert/strict";

import {
  annexBToLengthPrefixed,
  buildAv1CFromObuStream,
  buildAvcCFromNalus,
  buildHvcCFromNalus,
  buildOpusHead,
  containerForCodec,
  encodedCodecFromMime,
  extractNalUnits,
  mimeTypeForEncodedCapture,
  mixGameAudioWithMic,
  parseAv1SequenceHeader,
  patchAv1CInMp4,
  rtpTimestampToMicroseconds,
  unwrapRtpTimestamp,
  MicPcmFifo,
  MicSync,
} from "./encodedCapture";

/** Build an Annex-B H.264-style NAL payload with a 4-byte start code. */
function annexB(key: Uint8Array, ...nalPayloads: Uint8Array[]): Uint8Array {
  const parts: Uint8Array[] = [];
  for (const payload of nalPayloads) {
    parts.push(new Uint8Array([0, 0, 0, 1]), payload);
  }
  void key;
  const total = parts.reduce((sum, part) => sum + part.length, 0);
  const out = new Uint8Array(total);
  let offset = 0;
  for (const part of parts) {
    out.set(part, offset);
    offset += part.length;
  }
  return out;
}

/** Minimal fake SPS payload for H.264 (header + profile/level bytes). */
function h264Sps(profile = 100, constraint = 0, level = 40): Uint8Array {
  return new Uint8Array([0x67, profile, constraint, level, 0xff, 0xe0, 0x1f]);
}

test("Annex-B NAL splitting handles 3/4-byte start codes and trailing NAL", () => {
  const sps = h264Sps();
  const pps = new Uint8Array([0x68, 0xce, 0x3c, 0x80]);
  const slice = new Uint8Array([0x65, 0x88, 0x84, 0x01]);
  // Mix 4-byte and 3-byte start codes; the LAST NAL must not be truncated.
  const stream = new Uint8Array([
    ...new Uint8Array([0, 0, 0, 1]),
    ...sps,
    ...new Uint8Array([0, 0, 1]),
    ...pps,
    ...new Uint8Array([0, 0, 0, 1]),
    ...slice,
  ]);
  const nals = extractNalUnits(stream);
  assert.equal(nals.length, 3);
  assert.deepEqual([...nals[0].payload], [...sps]);
  assert.deepEqual([...nals[1].payload], [...pps]);
  assert.deepEqual([...nals[2].payload], [...slice]); // trailing NAL intact
});

test("Annex-B → length-prefixed conversion prepends 4-byte sizes", () => {
  const nals = extractNalUnits(
    annexB(new Uint8Array(0), new Uint8Array([0x65, 0x01, 0x02]), new Uint8Array([0x41, 0x03])),
  );
  const out = annexBToLengthPrefixed(nals);
  assert.equal(out.length, 4 + 3 + 4 + 2);
  assert.deepEqual([...out.subarray(0, 4)], [0, 0, 0, 3]);
  assert.deepEqual([...out.subarray(4, 7)], [0x65, 0x01, 0x02]);
  assert.deepEqual([...out.subarray(7, 11)], [0, 0, 0, 2]);
  assert.deepEqual([...out.subarray(11, 13)], [0x41, 0x03]);
});

test("avcC is built from keyframe SPS/PPS (profile/level copied from SPS)", () => {
  const sps = h264Sps(100, 0x00, 40); // High profile, level 4.0
  const pps = new Uint8Array([0x68, 0xce, 0x3c, 0x80]);
  const config = buildAvcCFromNalus(extractNalUnits(annexB(new Uint8Array(0), sps, pps)));
  assert.ok(config, "config should be built");
  assert.equal(config![0], 1); // configurationVersion
  assert.equal(config![1], 100); // profile_idc
  assert.equal(config![3], 40); // level_idc
  assert.equal(config![4], 0xff); // lengthSizeMinusOne = 3
  assert.equal(config![5], 0xe1); // one SPS
  const spsLen = (config![6] << 8) | config![7];
  assert.equal(spsLen, sps.length);
  assert.deepEqual([...config!.subarray(8, 8 + spsLen)], [...sps]);
  const ppsStart = 8 + spsLen + 3; // numOfPPS(1) + length(2)
  assert.deepEqual([...config!.subarray(ppsStart)], [...pps]);
  // No SPS/PPS → no config.
  assert.equal(buildAvcCFromNalus(extractNalUnits(annexB(new Uint8Array(0), new Uint8Array([0x65, 0x01])))), null);
});

test("hvcC is built from keyframe VPS/SPS/PPS with profile_tier_level from SPS", () => {
  // HEVC SPS byte 0: sps_video_parameter_set_id(4) + sps_max_sub_layers_minus1(3) + temporal_id_nesting(1).
  // Byte 1: general_profile_space(2) + tier(1) + profile_idc(5).
  // Bytes 2-5: compatibility flags; 6-11: constraint; byte 12: level.
  const vps = new Uint8Array([0x40, 0x01, 0x0c, 0x01, 0xff, 0xff, 0x01, 0x60, 0x00, 0x00, 0x03, 0x00, 0x90, 0x00, 0x00, 0x03, 0x00, 0x00, 0x03, 0x00, 0x78, 0x99, 0x98, 0x09]);
  const sps = new Uint8Array([
    0x42, // vps id 1 (0b0001), max_sub_layers 0, temporal_id_nesting 0
    0x01, // profile_space 0, tier 0, profile_idc 1 (Main)
    0x60, 0x00, 0x00, // compatibility flags (3 bytes of the 4)
    0x00, // 4th compat byte
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // constraint flags (48 bits)
    0x5a, // general_level_idc = 90 (5.0 × 30)
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  ]);
  const pps = new Uint8Array([0x44, 0x01, 0xc1, 0x72, 0xb4, 0x62, 0x40]);
  const config = buildHvcCFromNalus(extractNalUnits(annexB(new Uint8Array(0), vps, sps, pps)));
  assert.ok(config, "config should be built");
  assert.equal(config![0], 1);
  assert.equal(config![1], 0x01); // profile_space 0, tier 0, profile_idc 1
  assert.deepEqual([...config!.subarray(2, 6)], [0x60, 0x00, 0x00, 0x00]); // compat
  assert.equal(config![12], 0x5a); // level
  assert.equal(config![22], 3); // numOfArrays = VPS + SPS + PPS
  // Missing any of VPS/SPS/PPS → no config.
  assert.equal(buildHvcCFromNalus(extractNalUnits(annexB(new Uint8Array(0), sps, pps))), null);
});

test("OpusHead identification header matches the 19-byte layout", () => {
  const head = buildOpusHead(2);
  assert.equal(head.length, 19);
  assert.equal(String.fromCharCode(...head.subarray(0, 8)), "OpusHead");
  assert.equal(head[8], 1); // version
  assert.equal(head[9], 2); // channels
  assert.equal(head[10] | (head[11] << 8), 3840); // pre-skip
  assert.equal(head[12] | (head[13] << 8) | (head[14] << 16) | (head[15] << 24), 48000); // sample rate
});

test("RTP timestamp conversion and wraparound unwrapping", () => {
  // 90 kHz: 90000 ticks = 1 s → 1e6 µs.
  assert.equal(rtpTimestampToMicroseconds(90_000, 90_000), 1_000_000);
  assert.equal(rtpTimestampToMicroseconds(48_000, 48_000), 1_000_000);
  // 32-bit wraparound keeps monotonicity.
  assert.equal(unwrapRtpTimestamp(0xffff_fff0, null), 0xffff_fff0);
  assert.equal(unwrapRtpTimestamp(100, 0xffff_fff0), 0x1_0000_0064);
  // A small backwards jump without a wraparound epoch is left alone (the
  // worker's monotonic clamp absorbs it; recording continues).
  assert.equal(unwrapRtpTimestamp(500, 1000), 500);
});

test("codec → container mapping routes everything except VP8/VP9 to MP4", () => {
  assert.equal(encodedCodecFromMime("video/H264"), "avc");
  assert.equal(encodedCodecFromMime("video/H265"), "hevc");
  assert.equal(encodedCodecFromMime("video/AV1"), "av1");
  assert.equal(encodedCodecFromMime("video/VP9"), "vp9");
  assert.equal(encodedCodecFromMime("video/VP8"), "vp8");
  assert.equal(encodedCodecFromMime("audio/opus"), null);
  assert.equal(containerForCodec("avc"), "mp4");
  assert.equal(containerForCodec("hevc"), "mp4");
  assert.equal(containerForCodec("av1"), "mp4");
  assert.equal(containerForCodec("vp9"), "webm");
  assert.equal(containerForCodec("vp8"), "webm");
  assert.equal(mimeTypeForEncodedCapture("avc"), "video/mp4");
  assert.equal(mimeTypeForEncodedCapture("av1"), "video/mp4");
  assert.equal(mimeTypeForEncodedCapture("vp9"), "video/webm");
});

/** MSB-first bit writer for crafting an AV1 sequence header in tests. */
class TestBitWriter {
  private bytes: number[] = [];
  private bitIndex = 0;
  private current = 0;

  write(value: number, count: number): this {
    for (let i = count - 1; i >= 0; i -= 1) {
      this.current = (this.current << 1) | ((value >> i) & 1);
      this.bitIndex += 1;
      if (this.bitIndex === 8) {
        this.bytes.push(this.current);
        this.current = 0;
        this.bitIndex = 0;
      }
    }
    return this;
  }

  toBytes(): Uint8Array {
    if (this.bitIndex > 0) this.bytes.push(this.current << (8 - this.bitIndex));
    return new Uint8Array(this.bytes);
  }
}

/** Wrap a sequence-header payload in a low-overhead OBU (type 1, size field). */
function wrapAv1SequenceHeader(payload: Uint8Array): Uint8Array {
  const header = 0x0c; // obu_type 1, has_size_field 1
  const out = new Uint8Array(2 + payload.length);
  out[0] = header;
  out[1] = payload.length; // LEB128, fits one byte for our payloads
  out.set(payload, 2);
  return out;
}

function buildAv1SeqHeader(): Uint8Array {
  const w = new TestBitWriter();
  w.write(0, 3); // seq_profile
  w.write(0, 1); // still_picture
  w.write(0, 1); // reduced_still_picture_header
  w.write(0, 1); // timing_info_present_flag
  w.write(0, 1); // decoder_model_info_present_flag
  w.write(0, 1); // initial_display_delay_present_flag
  w.write(0, 5); // operating_points_cnt_minus_1
  w.write(0, 12); // operating_point_idc[0]
  w.write(4, 5); // seq_level_idx[0] = 4 (level 4.0)
  w.write(7, 4); // frame_width_bits_minus_1
  w.write(7, 4); // frame_height_bits_minus_1
  w.write(1919, 8); // max_frame_width_minus_1
  w.write(1079, 8); // max_frame_height_minus_1
  w.write(0, 1); // frame_id_numbers_present_flag
  w.write(1, 1); // use_128x128_superblock
  w.write(0, 1); // enable_filter_intra
  w.write(0, 1); // enable_intra_edge_filter
  w.write(0, 1); // enable_interintra_compound
  w.write(0, 1); // enable_masked_compound
  w.write(0, 1); // enable_warped_motion
  w.write(0, 1); // enable_dual_filter
  w.write(1, 1); // enable_order_hint
  w.write(1, 1); // enable_jnt_comp
  w.write(1, 1); // enable_ref_frame_mvs
  w.write(0, 1); // seq_choose_screen_content_tools
  w.write(0, 1); // seq_force_screen_content_tools
  w.write(0, 1); // enable_superres
  w.write(1, 1); // enable_cdef
  w.write(1, 1); // enable_restoration
  w.write(0, 1); // color_config: high_bitdepth
  w.write(0, 1); // monochrome
  w.write(0, 1); // color_description_present_flag
  w.write(1, 1); // subsampling_x (4:2:0)
  w.write(1, 1); // subsampling_y
  w.write(0, 2); // chroma_sample_position
  w.write(0, 1); // color_range
  return w.toBytes();
}

test("AV1 sequence header parses to the expected av1C record", () => {
  const payload = buildAv1SeqHeader();
  const obuStream = wrapAv1SequenceHeader(payload);
  const parsed = parseAv1SequenceHeader(payload);
  assert.ok(parsed);
  assert.equal(parsed!.profile, 0);
  assert.equal(parsed!.level, 4);
  assert.equal(parsed!.tier, 0);
  assert.equal(parsed!.subsamplingX, true);
  assert.equal(parsed!.subsamplingY, true);
  const config = buildAv1CFromObuStream(obuStream);
  assert.ok(config);
  assert.equal(config!.length, 4);
  assert.equal(config![0], 0x81); // marker + version
  assert.equal(config![1], (0 << 5) | 4); // profile 0, level 4
  assert.equal(config![2], 0x0c); // 4:2:0 8-bit, no monochrome/tier
  assert.equal(config![3], 0);
  // Malformed / truncated payloads → null, never a throw.
  assert.equal(parseAv1SequenceHeader(new Uint8Array([0xff])), null);
  assert.equal(buildAv1CFromObuStream(new Uint8Array([0x08])), null);
});

test("MicPcmFifo pushes, pulls oldest-first, and drains cleanly", () => {
  const fifo = new MicPcmFifo();
  assert.equal(fifo.available, 0);
  fifo.push(new Float32Array([1, 2, 3]));
  fifo.push(new Float32Array([4]));
  assert.equal(fifo.available, 4);
  // Oldest-first pull.
  assert.deepEqual([...fifo.pull(2)], [1, 2]);
  assert.equal(fifo.available, 2);
  assert.deepEqual([...fifo.pull(5)], [3, 4]); // underrun → returns what it has
  assert.equal(fifo.available, 0);
  // Buffer is reusable after a full drain.
  fifo.push(new Float32Array([9]));
  assert.deepEqual([...fifo.pull(1)], [9]);
  assert.equal(fifo.available, 0);
  // Empty pushes are no-ops.
  fifo.push(new Float32Array(0));
  assert.equal(fifo.available, 0);
});

test("MicSync measures the real mic rate from capture tags", () => {
  const sync = new MicSync();
  assert.equal(sync.measuredRatePerUs, 48_000 / 1_000_000); // nominal until measured
  // Mic actually runs at 48_010 Hz: 4096-sample chunks arrive every
  // 4096/48010 s = 85.315 ms of real time.
  const chunkLen = 4096;
  const chunkMs = (chunkLen / 48_010) * 1000;
  for (let i = 1; i <= 5; i += 1) {
    sync.push(chunkLen, i * chunkMs);
  }
  assert.ok(Math.abs(sync.measuredRatePerUs - 48_010 / 1_000_000) < 1e-7);
  // A garbage tag (huge time gap) is clamped to the sane window.
  sync.push(chunkLen, 1e9);
  assert.ok(sync.measuredRatePerUs >= 45_600 / 1_000_000);
  assert.ok(sync.measuredRatePerUs <= 50_400 / 1_000_000);
});

test("MicSync anchors on the first frame and drops pre-recording mic", () => {
  const sync = new MicSync();
  // Mic captured since real t=0; by the anchor moment (game tsUs = 1 s) the
  // worker has received 45056 samples (11 × 4096).
  const chunkLen = 4096;
  const chunkMs = (chunkLen / 48_000) * 1000;
  for (let i = 1; i <= 11; i += 1) sync.push(chunkLen, i * chunkMs);
  assert.equal(sync.pushed, 45_056);
  // Anchor frame: consumes nothing (its mic window is captured later).
  assert.equal(sync.samplesForFrame(1_000_000), 0);
  // Next frame, 20 ms later: consume one frame duration × measured rate.
  assert.equal(sync.samplesForFrame(1_020_000), 960);
});

test("MicSync consumes by RTP duration × measured rate — no drift with an off-nominal mic clock", () => {
  const sync = new MicSync();
  // Mic hardware runs at 48_010 Hz (0.02% fast — a realistic crystal error).
  const micRate = 48_010;
  const chunkLen = 4096;
  const chunkMs = (chunkLen / micRate) * 1000;
  const gameStartUs = 1_000_000;
  const frameUs = 20_000; // opus 20 ms packets
  const frames = 500; // 10 s of game audio
  // Simulate: feed mic chunks up to a real-clock moment, then process one
  // game frame (mic arrives at the real moment of its game position).
  let chunk = 0;
  let pushed = 0;
  let givenToMixer = 0;
  const feedMicThrough = (realMs: number): void => {
    while ((chunk + 1) * chunkMs <= realMs) {
      chunk += 1;
      sync.push(chunkLen, chunk * chunkMs);
      pushed += chunkLen;
    }
  };
  feedMicThrough(gameStartUs / 1000); // mic up to the anchor moment
  for (let i = 0; i < frames; i += 1) {
    const tsUs = gameStartUs + i * frameUs;
    feedMicThrough((gameStartUs + i * frameUs) / 1000);
    const desired = sync.samplesForFrame(tsUs);
    const take = Math.min(desired, Math.max(0, pushed - givenToMixer));
    givenToMixer += take;
    sync.commitPulled(take);
  }
  // The mixer received exactly as many mic samples as the mic produced
  // during the recorded game window — the timelines never drifted.
  const elapsedUs = (frames - 1) * frameUs;
  const expected = (elapsedUs / 1_000_000) * micRate;
  // The mixer received exactly as many mic samples as the mic produced during
  // the recorded game window — the timelines never drifted (bounded ±1 rounding).
  assert.ok(Math.abs(givenToMixer - expected) < 2, `consumed ${givenToMixer}, expected ~${expected}`);
  // And the FIFO never starved: every frame's requested pull was satisfiable.
  assert.equal(givenToMixer, Math.round(expected));
});

test("mixGameAudioWithMic adds the mono mic to both channels and clamps", () => {
  // Game: two stereo frames. Mic: one mono sample (second frame → silence).
  const game = new Float32Array([0.2, -0.2, 0.5, 0.5]);
  const mic = new Float32Array([0.5]);
  const out = mixGameAudioWithMic(game, mic, 1.0);
  assert.equal(out.length, 4);
  assert.ok(Math.abs(out[0] - 0.7) < 1e-6); // 0.2 + 0.5
  assert.ok(Math.abs(out[1] - 0.3) < 1e-6); // -0.2 + 0.5
  assert.ok(Math.abs(out[2] - 0.5) < 1e-6); // no mic → game unchanged
  assert.ok(Math.abs(out[3] - 0.5) < 1e-6);
  // Clamp: 0.8 + 0.5 = 1.3 → 1.0.
  const loud = mixGameAudioWithMic(new Float32Array([0.8, 0, 0, 0]), new Float32Array([0.5]), 1.0);
  assert.equal(loud[0], 1.0);
  // Default gain is applied (0.6): 0.5 × 0.6 = 0.3 → 0.2 + 0.3.
  const defaulted = mixGameAudioWithMic(new Float32Array([0.2, 0, 0, 0]), new Float32Array([0.5]));
  assert.ok(Math.abs(defaulted[0] - 0.5) < 1e-6);
});

test("patchAv1CInMp4 replaces the av1C box content of an emitted MP4 stream", () => {
  const av1C = buildAv1CFromObuStream(wrapAv1SequenceHeader(buildAv1SeqHeader()))!;
  assert.ok(av1C);
  assert.equal(av1C.length, 4);
  // Mimic mp4-muxer's emitted output: ftyp + moov carrying a zeroed av1C box
  // (size 12 = 4 size + 4 type + 4 content) + a tiny mdat whose sample bytes
  // coincidentally contain 'av1C' — the patch must hit the box, not the data.
  const ftyp = new Uint8Array([0, 0, 0, 0x18, 0x66, 0x74, 0x79, 0x70]);
  const av1CBox = new Uint8Array([0, 0, 0, 12, 0x61, 0x76, 0x31, 0x43, 0x81, 0, 0, 0]);
  const mdatSample = new Uint8Array([0, 0, 0, 0x10, 0x6d, 0x64, 0x61, 0x74, 0x61, 0x76, 0x31, 0x43, 0x00, 0x00, 0x00, 0x00]);
  const stream = new Uint8Array([...ftyp, ...av1CBox, ...mdatSample]);
  assert.equal(patchAv1CInMp4(stream, av1C), true);
  // The box content (after the 4-byte box size + 4-byte type) is the record.
  const boxStart = ftyp.length + 8;
  assert.deepEqual([...stream.subarray(boxStart, boxStart + 4)], [...av1C]);
  // The coincidental 'av1C' inside mdat sample data must NOT be touched.
  const mdatStart = ftyp.length + 12;
  assert.equal(stream[mdatStart + 8], 0x61); // 'a' still there
  assert.equal(stream[mdatStart + 11], 0x43); // 'C' still there
  // Second pass is a no-op match (already patched); a stream without a box
  // returns false.
  assert.equal(patchAv1CInMp4(stream, av1C), true);
  assert.equal(patchAv1CInMp4(new Uint8Array([0, 0, 0, 0x18, 0x66, 0x74, 0x79, 0x70]), av1C), false);
});
