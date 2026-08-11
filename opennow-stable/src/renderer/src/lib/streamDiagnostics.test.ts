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
