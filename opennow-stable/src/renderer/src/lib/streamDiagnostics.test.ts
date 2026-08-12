import assert from "node:assert/strict";
import test from "node:test";

import { defaultDiagnostics, mergeNativeStreamStats } from "./streamDiagnostics";
import type { NativeStreamStats } from "@shared/gfn";

function baseNativeStats(): NativeStreamStats {
  return {
    codec: "H265",
    resolution: "1920x1080",
    hardwareAcceleration: "GStreamer native decode",
    bitrateKbps: 3000,
    targetBitrateKbps: 15000,
    bitratePerformancePercent: 20,
    decodedFps: 60,
    renderFps: 60,
    framesDecoded: 600,
    framesRendered: 600,
    zeroCopyD3D11: false,
    zeroCopyD3D12: false,
  };
}

test("mergeNativeStreamStats maps the raw server GPU code to the official rig name", () => {
  const merged = mergeNativeStreamStats(
    defaultDiagnostics(),
    { ...baseNativeStats(), serverGpuType: "2080d / T10" },
  );
  assert.equal(merged.serverGpuType, "GeForce RTX");
});

test("mergeNativeStreamStats keeps the previous server GPU when a sample carries none", () => {
  const merged = mergeNativeStreamStats(
    { ...defaultDiagnostics(), serverGpuType: "GeForce RTX 5080" },
    baseNativeStats(),
  );
  assert.equal(merged.serverGpuType, "GeForce RTX 5080");
});

test("mergeNativeStreamStats leaves serverGpuType empty when never reported", () => {
  const merged = mergeNativeStreamStats(
    defaultDiagnostics(),
    baseNativeStats(),
  );
  assert.equal(merged.serverGpuType, "");
});

test("RTT: uses the server RTT when it is the only fresh source", () => {
  const merged = mergeNativeStreamStats(
    defaultDiagnostics(),
    { ...baseNativeStats(), networkRttMs: 38, networkRttAgeMs: 500 },
  );
  assert.equal(merged.rttMs, 38);
});

test("RTT: prefers the fresher source when both local RTCP and server RTT are fresh", () => {
  // Local RTCP is older (4s since the last RR) than the server field (0.2s)
  // → the server RTT wins as the fresher measurement.
  const merged = mergeNativeStreamStats(
    defaultDiagnostics(),
    {
      ...baseNativeStats(),
      localRtcpRttMs: 40,
      localRtcpRttAgeMs: 4_000,
      networkRttMs: 38,
      networkRttAgeMs: 200,
    },
  );
  assert.equal(merged.rttMs, 38);
  // And the reverse: a fresh local RTCP beats a stale-ish server field.
  const merged2 = mergeNativeStreamStats(
    defaultDiagnostics(),
    {
      ...baseNativeStats(),
      localRtcpRttMs: 40,
      localRtcpRttAgeMs: 500,
      networkRttMs: 38,
      networkRttAgeMs: 9_000,
    },
  );
  assert.equal(merged2.rttMs, 40);
});

test("RTT: expires a local RTCP whose RR stream stopped, falling back to the server RTT", () => {
  // Local RTCP value is present but 30s old (rtpsession's have-rb stuck) —
  // it must NOT override the live server RTT.
  const merged = mergeNativeStreamStats(
    defaultDiagnostics(),
    {
      ...baseNativeStats(),
      localRtcpRttMs: 120,
      localRtcpRttAgeMs: 30_000,
      networkRttMs: 38,
      networkRttAgeMs: 400,
    },
  );
  assert.equal(merged.rttMs, 38);
});

test("RTT: holds the previous ping briefly when both sources go stale, then decays to 0", () => {
  // Start from a fresh sample (also resets the shared stale counter).
  let merged = mergeNativeStreamStats(
    defaultDiagnostics(),
    { ...baseNativeStats(), networkRttMs: 42, networkRttAgeMs: 100 },
  );
  assert.equal(merged.rttMs, 42);
  // Both sources stop reporting → hold for NATIVE_RTT_STALE_SAMPLE_LIMIT
  // merges (10), then decay to 0 ("--").
  const stale = baseNativeStats();
  for (let i = 0; i < 10; i++) {
    merged = mergeNativeStreamStats(merged, stale);
    assert.equal(merged.rttMs, 42, `held ping at merge ${i + 1}`);
  }
  merged = mergeNativeStreamStats(merged, stale);
  assert.equal(merged.rttMs, 0);
});

test("Jitter: maps the native rtpsession jitter to the HUD field", () => {
  const merged = mergeNativeStreamStats(
    defaultDiagnostics(),
    { ...baseNativeStats(), localJitterMs: 3 },
  );
  assert.equal(merged.jitterMs, 3);
});

test("Jitter: holds the previous value briefly when native stops reporting, then decays to 0", () => {
  // Start from a fresh sample (also resets the shared stale counter).
  let merged = mergeNativeStreamStats(
    defaultDiagnostics(),
    { ...baseNativeStats(), localJitterMs: 4 },
  );
  assert.equal(merged.jitterMs, 4);
  // Native side stops reporting (stream stalled) → hold for
  // NATIVE_JITTER_STALE_SAMPLE_LIMIT merges (5), then decay to 0 ("--").
  const stale = baseNativeStats();
  for (let i = 0; i < 5; i++) {
    merged = mergeNativeStreamStats(merged, stale);
    assert.equal(merged.jitterMs, 4, `held jitter at merge ${i + 1}`);
  }
  merged = mergeNativeStreamStats(merged, stale);
  assert.equal(merged.jitterMs, 0);
});

test("Jitter: a fresh value resets the stale counter", () => {
  let merged = mergeNativeStreamStats(
    defaultDiagnostics(),
    { ...baseNativeStats(), localJitterMs: 4 },
  );
  // Two stale merges, then a fresh value must win and reset the counter.
  merged = mergeNativeStreamStats(merged, baseNativeStats());
  merged = mergeNativeStreamStats(merged, baseNativeStats());
  merged = mergeNativeStreamStats(merged, { ...baseNativeStats(), localJitterMs: 2 });
  assert.equal(merged.jitterMs, 2);
  // And after the reset, the hold window is back to full.
  const stale = baseNativeStats();
  for (let i = 0; i < 5; i++) {
    merged = mergeNativeStreamStats(merged, stale);
    assert.equal(merged.jitterMs, 2, `held jitter at merge ${i + 1}`);
  }
  merged = mergeNativeStreamStats(merged, stale);
  assert.equal(merged.jitterMs, 0);
});

test("JitterBuf: maps the native pre-decode jitter buffer depth to the HUD field", () => {
  const merged = mergeNativeStreamStats(
    defaultDiagnostics(),
    { ...baseNativeStats(), preDecodeJitterBufferMs: 116 },
  );
  assert.equal(merged.jitterBufferDelayMs, 116);
});

test("JitterBuf: holds the previous depth briefly when native stops reporting, then decays to 0", () => {
  let merged = mergeNativeStreamStats(
    defaultDiagnostics(),
    { ...baseNativeStats(), preDecodeJitterBufferMs: 116 },
  );
  assert.equal(merged.jitterBufferDelayMs, 116);
  // Native side stops reporting (stream stalled) → hold for
  // NATIVE_JITTER_BUF_STALE_SAMPLE_LIMIT merges (5), then decay to 0 ("--").
  const stale = baseNativeStats();
  for (let i = 0; i < 5; i++) {
    merged = mergeNativeStreamStats(merged, stale);
    assert.equal(merged.jitterBufferDelayMs, 116, `held depth at merge ${i + 1}`);
  }
  merged = mergeNativeStreamStats(merged, stale);
  assert.equal(merged.jitterBufferDelayMs, 0);
});

test("JitterBuf: a fresh depth resets the stale counter", () => {
  let merged = mergeNativeStreamStats(
    defaultDiagnostics(),
    { ...baseNativeStats(), preDecodeJitterBufferMs: 116 },
  );
  merged = mergeNativeStreamStats(merged, baseNativeStats());
  merged = mergeNativeStreamStats(merged, baseNativeStats());
  merged = mergeNativeStreamStats(merged, { ...baseNativeStats(), preDecodeJitterBufferMs: 66 });
  assert.equal(merged.jitterBufferDelayMs, 66);
  // And after the reset, the hold window is back to full.
  const stale = baseNativeStats();
  for (let i = 0; i < 5; i++) {
    merged = mergeNativeStreamStats(merged, stale);
    assert.equal(merged.jitterBufferDelayMs, 66, `held depth at merge ${i + 1}`);
  }
  merged = mergeNativeStreamStats(merged, stale);
  assert.equal(merged.jitterBufferDelayMs, 0);
});
