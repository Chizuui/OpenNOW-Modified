/**
 * GFN `stats_channel` binary stats protocol — mirrors the official clients
 * (play.geforcenow.com web AND the Windows app bundle, which share the same
 * dispatcher).
 *
 * The first byte is a TYPE discriminator:
 *   - 3: 1-byte header + stats payload (payload starts at byte 1)
 *   - 4: stats payload directly (payload starts at byte 0)
 * anything else is dropped (the official clients log an error).
 *
 * The payload then starts with the protocol VERSION (must be >= 4):
 *   - `avgGameFps` (server-side game render FPS) is a little-endian float64
 *     at payload offset 25.
 *   - version 5 only appends an l4sState byte at payload offset 73, which is
 *     ignored here (offset 25 is unchanged between v4 and v5).
 *   - versions < 4 carry no avgGameFps and are dropped.
 */
export interface StatsChannelGameFps {
  /** Stats protocol version (>= 4). */
  version: number;
  /** Rounded server-side game render FPS. */
  fps: number;
}

export function parseStatsChannelGameFps(buf: ArrayBuffer): StatsChannelGameFps | null {
  const bytes = new Uint8Array(buf);
  if (bytes.length < 1) return null;
  let offset = 0;
  if (bytes[0] === 3) {
    if (bytes.length < 2) return null;
    offset = 1;
  } else if (bytes[0] !== 4) {
    return null;
  }
  if (bytes.length - offset < 33) return null;
  try {
    const view = new DataView(buf);
    const version = view.getUint8(offset);
    if (version < 4) return null;
    const avgGameFps = view.getFloat64(offset + 25, true); // little-endian
    if (!Number.isFinite(avgGameFps) || avgGameFps <= 0 || avgGameFps > 360) {
      return null;
    }
    return { version, fps: Math.round(avgGameFps) };
  } catch {
    return null;
  }
}
