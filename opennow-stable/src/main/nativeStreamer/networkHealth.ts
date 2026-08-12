/**
 * Pure helpers for the native streamer's `[NetworkHealth]` log lines.
 *
 * The streamer logs measured receive stats roughly once a second, e.g.:
 *
 *   [NetworkHealth] server rtt=41ms loss=0.0200% rtcp=n/a (receiver-only)
 *   sinkDrop=0.00% sink=60.8fps bitrate=2.8Mbps rtcp_sent=...
 *
 * `bitrate=` is the received RTP byte rate — the closest native-side proxy for
 * the server's encoder rate. It is not a BWE estimate like the browser's
 * `availableIncomingBitrate`, so it fluctuates with scene complexity, but a
 * sustained jump right after a mid-session bitrate push is the only usable
 * signal that the server honored the new `vqos.bw.maximumBitrateKbps` cap
 * without waiting for the next offer/reconnect.
 */

export const NATIVE_BITRATE_PUSH_VERIFY_WINDOW_MS = 15_000;
export const NATIVE_BITRATE_PUSH_MOVE_THRESHOLD_MBPS = 1.0;

/** Extract the measured `bitrate=X.XMbps` value from a [NetworkHealth] line, or null when absent/unparseable. */
export function parseNetworkHealthBitrateMbps(text: string): number | null {
  const match = text.match(/bitrate=(\d+(?:\.\d+)?)Mbps/);
  if (!match) {
    return null;
  }
  const value = Number(match[1]);
  return Number.isFinite(value) ? value : null;
}

export interface NativeBitratePushVerification {
  /** Wall-clock of the mid-session cap push that armed this verification. */
  pushedAtMs: number;
  /** First measured bitrate after the push; null until the first usable [NetworkHealth] sample lands. */
  baselineMbps: number | null;
  /** 1-based push number within the session, for the log. */
  pushNumber: number;
}

export type NativeBitratePushEvent =
  | { kind: "baseline"; baselineMbps: number }
  | {
      kind: "verified";
      baselineMbps: number;
      currentMbps: number;
      elapsedMs: number;
      pushNumber: number;
    }
  | { kind: "unchanged"; baselineMbps: number; elapsedMs: number; pushNumber: number };

/**
 * Advance a pending push verification with one [NetworkHealth] sample.
 *
 * Returns:
 * - `{ kind: "baseline" }` on the first usable sample — the caller records the
 *   baseline and keeps waiting.
 * - `{ kind: "verified" }` when a later sample is at least `thresholdMbps`
 *   above the baseline within the window — the server honored the push.
 * - `{ kind: "unchanged" }` when the window expires without movement — the
 *   server likely applies the cap only on the next offer/reconnect.
 * - `null` while waiting (no usable sample yet, or sample within window below
 *   the threshold). Missing samples do not advance the window deadline, only
 *   the clock does, so a lagging health log cannot fake a timeout.
 */
export function stepNativeBitratePushVerification(
  verification: NativeBitratePushVerification,
  bitrateMbps: number | null,
  nowMs: number,
  windowMs = NATIVE_BITRATE_PUSH_VERIFY_WINDOW_MS,
  thresholdMbps = NATIVE_BITRATE_PUSH_MOVE_THRESHOLD_MBPS,
): NativeBitratePushEvent | null {
  const elapsedMs = nowMs - verification.pushedAtMs;
  if (verification.baselineMbps === null) {
    if (bitrateMbps === null) {
      return null;
    }
    return { kind: "baseline", baselineMbps: bitrateMbps };
  }
  if (bitrateMbps !== null && bitrateMbps >= verification.baselineMbps + thresholdMbps) {
    return {
      kind: "verified",
      baselineMbps: verification.baselineMbps,
      currentMbps: bitrateMbps,
      elapsedMs,
      pushNumber: verification.pushNumber,
    };
  }
  if (elapsedMs >= windowMs) {
    return {
      kind: "unchanged",
      baselineMbps: verification.baselineMbps,
      elapsedMs,
      pushNumber: verification.pushNumber,
    };
  }
  return null;
}
