/**
 * Minimal ISO-BMFF (MP4) probe.
 *
 * The gallery needs duration and track information straight from the file:
 * the official naming scheme carries no duration suffix, and the legacy
 * name-encoded duration made broken artifacts (moov-less stubs and audio-only
 * muxes produced by interrupted sessions) indistinguishable from recordings.
 *
 * Only the top-level box chain and the `moov` payload are read — `mdat` is
 * skipped by seeking, so probing a multi-GB recording costs a few small
 * reads. Every structure is bounds-checked; malformed input reports `null`
 * instead of throwing.
 */
import { open } from "node:fs/promises";

export interface Mp4ProbeResult {
  durationMs: number;
  hasVideo: boolean;
  hasAudio: boolean;
}

/**
 * Real `moov` boxes are kilobytes; the cap keeps a corrupt size field from
 * asking for a multi-gigabyte allocation.
 */
const MAX_MOOV_BYTES = 16 * 1024 * 1024;

interface BoxBounds {
  type: string;
  contentStart: number;
  contentEnd: number;
}

function readBoxHeader(buf: Buffer, offset: number, end: number): BoxBounds | null {
  if (offset + 8 > end) return null;
  let size = buf.readUInt32BE(offset);
  const type = buf.toString("latin1", offset + 4, offset + 8);
  let headerSize = 8;
  if (size === 1) {
    if (offset + 16 > end) return null;
    const big = buf.readBigUInt64BE(offset + 8);
    if (big > BigInt(Number.MAX_SAFE_INTEGER)) return null;
    size = Number(big);
    headerSize = 16;
  } else if (size === 0) {
    size = end - offset;
  }
  if (size < headerSize || offset + size > end) return null;
  return { type, contentStart: offset + headerSize, contentEnd: offset + size };
}

function visitBoxes(
  buf: Buffer,
  start: number,
  end: number,
  visit: (box: BoxBounds) => void,
): void {
  let offset = start;
  while (offset < end) {
    const box = readBoxHeader(buf, offset, end);
    if (!box) return; // truncated / malformed: stop scanning this level
    visit(box);
    if (box.contentEnd <= offset) return; // defensive: require forward progress
    offset = box.contentEnd;
  }
}

function readU32(buf: Buffer, offset: number, end: number): number | null {
  if (offset < 0 || offset + 4 > end) return null;
  return buf.readUInt32BE(offset);
}

function readU64(buf: Buffer, offset: number, end: number): bigint | null {
  if (offset < 0 || offset + 8 > end) return null;
  return buf.readBigUInt64BE(offset);
}

/**
 * Movie duration (from `mvhd`) and the track handler types (`vide`/`soun`)
 * inside a standalone `moov` payload. Exported for unit tests.
 */
export function parseMoovBox(moov: Buffer): { durationMs: number; handlerTypes: string[] } {
  let durationMs = 0;
  const handlerTypes: string[] = [];

  const readMvhd = (box: BoxBounds): void => {
    const version = moov[box.contentStart] ?? 0;
    if (version === 1) {
      const timescale = readU32(moov, box.contentStart + 20, box.contentEnd);
      const duration = readU64(moov, box.contentStart + 24, box.contentEnd);
      if (timescale && duration !== null) {
        durationMs = Math.round((Number(duration) * 1000) / timescale);
      }
    } else {
      const timescale = readU32(moov, box.contentStart + 12, box.contentEnd);
      const duration = readU32(moov, box.contentStart + 16, box.contentEnd);
      if (timescale && duration !== null) {
        durationMs = Math.round((duration * 1000) / timescale);
      }
    }
  };

  const readHdlr = (box: BoxBounds): void => {
    // content: version/flags(4) + pre_defined(4) + handler_type(4)
    if (box.contentStart + 12 > box.contentEnd) return;
    const handler = moov.toString("latin1", box.contentStart + 8, box.contentStart + 12);
    if (handler) handlerTypes.push(handler);
  };

  visitBoxes(moov, 0, moov.length, (box) => {
    if (box.type === "mvhd" && durationMs === 0) readMvhd(box);
    if (box.type !== "trak") return;
    // moov > trak > mdia > hdlr
    visitBoxes(moov, box.contentStart, box.contentEnd, (trakChild) => {
      if (trakChild.type !== "mdia") return;
      visitBoxes(moov, trakChild.contentStart, trakChild.contentEnd, (mdiaChild) => {
        if (mdiaChild.type === "hdlr") readHdlr(mdiaChild);
      });
    });
  });

  return { durationMs: Math.max(0, durationMs), handlerTypes };
}

/**
 * Probe an MP4 file on disk. Returns `null` when the file has no readable
 * `moov` (stub artifacts, truncated writes) — callers treat that as "broken,
 * hide from the gallery".
 */
export async function probeMp4File(filePath: string): Promise<Mp4ProbeResult | null> {
  let handle: Awaited<ReturnType<typeof open>> | null = null;
  try {
    handle = await open(filePath, "r");
    const { size } = await handle.stat();
    if (size <= 0) return null;

    let pos = 0;
    while (pos + 8 <= size) {
      const header = Buffer.alloc(16);
      const { bytesRead } = await handle.read(header, 0, 16, pos);
      if (bytesRead < 8) return null;

      let boxSize = header.readUInt32BE(0);
      const type = header.toString("latin1", 4, 8);
      let headerSize = 8;
      if (boxSize === 1) {
        if (bytesRead < 16) return null;
        const big = header.readBigUInt64BE(8);
        if (big > BigInt(Number.MAX_SAFE_INTEGER)) return null;
        boxSize = Number(big);
        headerSize = 16;
      } else if (boxSize === 0) {
        boxSize = size - pos;
      }
      if (boxSize < headerSize || pos + boxSize > size) return null;

      if (type === "moov") {
        const moovSize = boxSize - headerSize;
        if (moovSize <= 0 || moovSize > MAX_MOOV_BYTES) return null;
        const moov = Buffer.alloc(moovSize);
        await handle.read(moov, 0, moovSize, pos + headerSize);
        const parsed = parseMoovBox(moov);
        return {
          durationMs: parsed.durationMs,
          hasVideo: parsed.handlerTypes.includes("vide"),
          hasAudio: parsed.handlerTypes.includes("soun"),
        };
      }

      pos += boxSize;
    }
    return null;
  } catch {
    return null;
  } finally {
    await handle?.close().catch(() => undefined);
  }
}
