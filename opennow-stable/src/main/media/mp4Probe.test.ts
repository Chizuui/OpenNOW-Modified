/// <reference types="node" />

import test from "node:test";
import assert from "node:assert/strict";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { parseMoovBox, probeMp4File } from "./mp4Probe";

function box(type: string, payload: Buffer): Buffer {
  const header = Buffer.alloc(8);
  header.writeUInt32BE(8 + payload.length, 0);
  header.write(type, 4, "latin1");
  return Buffer.concat([header, payload]);
}

function boxWithLargeSize(type: string, payload: Buffer): Buffer {
  const header = Buffer.alloc(16);
  header.writeUInt32BE(1, 0);
  header.write(type, 4, "latin1");
  header.writeBigUInt64BE(BigInt(16 + payload.length), 8);
  return Buffer.concat([header, payload]);
}

function mvhdV0(timescale: number, duration: number): Buffer {
  const payload = Buffer.alloc(100);
  payload.writeUInt8(0, 0);
  payload.writeUInt32BE(timescale, 12);
  payload.writeUInt32BE(duration, 16);
  return box("mvhd", payload);
}

function mvhdV1(timescale: number, duration: bigint): Buffer {
  const payload = Buffer.alloc(112);
  payload.writeUInt8(1, 0);
  payload.writeUInt32BE(timescale, 20);
  payload.writeBigUInt64BE(duration, 24);
  return box("mvhd", payload);
}

function hdlr(handlerType: string): Buffer {
  const payload = Buffer.alloc(24);
  payload.write(handlerType, 8, "latin1");
  return box("hdlr", payload);
}

function trakWith(handlerType: string): Buffer {
  return box("trak", box("mdia", hdlr(handlerType)));
}

const VIDEO_AND_AUDIO_MOOV = box(
  "moov",
  Buffer.concat([mvhdV0(1000, 12345), trakWith("vide"), trakWith("soun")]),
);

async function writeTempFile(name: string, content: Buffer): Promise<{
  dir: string;
  filePath: string;
}> {
  const dir = await mkdtemp(join(tmpdir(), "opennow-mp4-"));
  const filePath = join(dir, name);
  await writeFile(filePath, content);
  return { dir, filePath };
}

test("parseMoovBox reads duration and track handlers", () => {
  const parsed = parseMoovBox(VIDEO_AND_AUDIO_MOOV.subarray(8));
  assert.equal(parsed.durationMs, 12345);
  assert.deepEqual(parsed.handlerTypes, ["vide", "soun"]);
});

test("parseMoovBox supports version-1 mvhd (64-bit duration)", () => {
  const moov = box("moov", Buffer.concat([mvhdV1(1000, 7000n), trakWith("vide")]));
  const parsed = parseMoovBox(moov.subarray(8));
  assert.equal(parsed.durationMs, 7000);
});

test("parseMoovBox never throws on garbage", () => {
  assert.deepEqual(parseMoovBox(Buffer.from([1, 2, 3])), {
    durationMs: 0,
    handlerTypes: [],
  });
  const truncatedMvhd = Buffer.concat([VIDEO_AND_AUDIO_MOOV.subarray(8, 20)]);
  assert.equal(parseMoovBox(truncatedMvhd).durationMs, 0);
});

test("probeMp4File finds moov after a large mdat and skips payloads", async () => {
  const mdat = box("mdat", Buffer.alloc(64 * 1024));
  const { dir, filePath } = await writeTempFile(
    "video.mp4",
    Buffer.concat([box("ftyp", Buffer.alloc(16)), mdat, VIDEO_AND_AUDIO_MOOV]),
  );
  try {
    const probe = await probeMp4File(filePath);
    assert.ok(probe);
    assert.equal(probe.durationMs, 12345);
    assert.equal(probe.hasVideo, true);
    assert.equal(probe.hasAudio, true);
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
});

test("probeMp4File handles 64-bit (largesize) box headers", async () => {
  const { dir, filePath } = await writeTempFile(
    "large.mp4",
    Buffer.concat([
      box("ftyp", Buffer.alloc(16)),
      boxWithLargeSize("mdat", Buffer.alloc(1024)),
      VIDEO_AND_AUDIO_MOOV,
    ]),
  );
  try {
    const probe = await probeMp4File(filePath);
    assert.ok(probe);
    assert.equal(probe.durationMs, 12345);
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
});

test("probeMp4File reports audio-only muxes", async () => {
  const moov = box("moov", Buffer.concat([mvhdV0(1000, 402000), trakWith("soun")]));
  const { dir, filePath } = await writeTempFile(
    "audio.mp4",
    Buffer.concat([box("ftyp", Buffer.alloc(16)), moov]),
  );
  try {
    const probe = await probeMp4File(filePath);
    assert.ok(probe);
    assert.equal(probe.hasVideo, false);
    assert.equal(probe.hasAudio, true);
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
});

test("probeMp4File returns null for stubs and truncated files", async () => {
  const stub = await writeTempFile("stub.mp4", box("ftyp", Buffer.alloc(20)));
  try {
    assert.equal(await probeMp4File(stub.filePath), null);
  } finally {
    await rm(stub.dir, { recursive: true, force: true });
  }

  const fullMoov = VIDEO_AND_AUDIO_MOOV;
  const truncated = await writeTempFile(
    "cut.mp4",
    Buffer.concat([box("ftyp", Buffer.alloc(16)), fullMoov.subarray(0, 12)]),
  );
  try {
    assert.equal(await probeMp4File(truncated.filePath), null);
  } finally {
    await rm(truncated.dir, { recursive: true, force: true });
  }
});

test("probeMp4File returns null for a missing file", async () => {
  assert.equal(await probeMp4File(join(tmpdir(), "opennow-does-not-exist.mp4")), null);
});
