/// <reference types="node" />

import test from "node:test";
import assert from "node:assert/strict";

import type { SessionInfo } from "@shared/gfn";
import { mergePolledSessionState } from "./queueAds";

function baseSession(overrides: Partial<SessionInfo> = {}): SessionInfo {
  return {
    sessionId: "session-1",
    status: 2,
    zone: "prod",
    serverIp: "server",
    signalingServer: "signal",
    signalingUrl: "wss://signal/nvst/",
    iceServers: [],
    ...overrides,
  };
}

test("mergePolledSessionState keeps a session-stable gpuType when the next poll omits it", () => {
  const previous = baseSession({ gpuType: "RTX" });
  const next = baseSession({ gpuType: undefined });

  const merged = mergePolledSessionState(previous, next);

  assert.equal(merged.gpuType, "RTX");
});

test("mergePolledSessionState lets a later gpuType win over the previous value", () => {
  const previous = baseSession({ gpuType: "RTX" });
  const next = baseSession({ gpuType: "RTX 5080" });

  const merged = mergePolledSessionState(previous, next);

  assert.equal(merged.gpuType, "RTX 5080");
});

test("mergePolledSessionState preserves gpuType for sessions still in setup", () => {
  const previous = baseSession({ status: 1, gpuType: "RTX" });
  const next = baseSession({ status: 1 });

  const merged = mergePolledSessionState(previous, next);

  assert.equal(merged.gpuType, "RTX");
});

test("mergePolledSessionState leaves gpuType empty when neither poll reports one", () => {
  const previous = baseSession();
  const next = baseSession();

  const merged = mergePolledSessionState(previous, next);

  assert.equal(merged.gpuType, undefined);
});

test("mergePolledSessionState keeps the zone hostname when the seat-assigned poll drops it", () => {
  const previous = baseSession({ serverLocation: "npa-yes-kul-01.yes.geforcenow.nvidiagrid.net" });
  const next = baseSession({ serverLocation: undefined });

  const merged = mergePolledSessionState(previous, next);

  assert.equal(merged.serverLocation, "npa-yes-kul-01.yes.geforcenow.nvidiagrid.net");
});

test("mergePolledSessionState lets a later zone hostname win over the previous value", () => {
  const previous = baseSession({ serverLocation: "np-lax-01.cloudmatchbeta.nvidiagrid.net" });
  const next = baseSession({ serverLocation: "npa-yes-kul-01.yes.geforcenow.nvidiagrid.net" });

  const merged = mergePolledSessionState(previous, next);

  assert.equal(merged.serverLocation, "npa-yes-kul-01.yes.geforcenow.nvidiagrid.net");
});

test("mergePolledSessionState preserves serverLocation for sessions still in setup", () => {
  const previous = baseSession({ status: 1, serverLocation: "np-tyo-01.cloudmatchbeta.nvidiagrid.net" });
  const next = baseSession({ status: 1 });

  const merged = mergePolledSessionState(previous, next);

  assert.equal(merged.serverLocation, "np-tyo-01.cloudmatchbeta.nvidiagrid.net");
});
