/// <reference types="node" />

import test from "node:test";
import assert from "node:assert/strict";

import {
  DecoderPressureController,
  type DecoderPressureSignal,
  type DecoderPressureState,
} from "./decoderPressureController";
import {
  selectGamepadPollIntervalMs,
  shouldSendGamepadPacket,
} from "./gamepadController";
import { InputChannelPolicyController } from "./inputChannelPolicy";

const pressureSignal: DecoderPressureSignal = {
  active: true,
  reason: "backlog_and_drop",
  backlogFrames: 50,
  dropRatePercent: 7,
};

test("decoder recovery tracks pressure but leaves the stream alone for backlog/drop pressure", async () => {
  const states: DecoderPressureState[] = [];
  const logs: string[] = [];
  let keyframeRequests = 0;
  const controller = new DecoderPressureController({
    log: (message) => logs.push(message),
    getPeerConnection: () => null,
    getControlChannel: () => null,
    requestSignalingKeyframe: async () => {
      keyframeRequests++;
    },
    onStateChange: (state) => states.push(state),
    now: () => 2_000,
  });

  // Non-severe pressure (backlog + drops) must NOT interrupt the stream:
  // no keyframes, no bitrate churn, and the jitter buffer stays adaptive
  // so transient jitter is absorbed instead of turning into frame drops.
  await controller.recover(pressureSignal);
  await controller.recover(pressureSignal);
  await controller.recover(pressureSignal);
  assert.equal(keyframeRequests, 0);
  assert.ok(
    logs.some((line) => line.includes("video=adaptive audio=adaptive")),
    "backlog/drop pressure keeps adaptive jitter targets",
  );
  assert.deepEqual(states.at(-1), {
    active: true,
    recoveryAttempts: 0,
    recoveryAction: "none",
  });

  const stableSignal = { ...pressureSignal, active: false, reason: "stable" };
  for (let index = 0; index < 5; index++) {
    await controller.recover(stableSignal);
  }
  assert.equal(states.at(-1)?.active, true);

  await controller.recover(stableSignal);
  assert.deepEqual(states.at(-1), {
    active: false,
    recoveryAttempts: 0,
    recoveryAction: "none",
  });
});

test("decoder recovery requests a keyframe only on a severe decode stall", async () => {
  const states: DecoderPressureState[] = [];
  const logs: string[] = [];
  let keyframeRequests = 0;
  const controller = new DecoderPressureController({
    log: (message) => logs.push(message),
    getPeerConnection: () => null,
    getControlChannel: () => null,
    requestSignalingKeyframe: async () => {
      keyframeRequests++;
    },
    onStateChange: (state) => states.push(state),
    now: () => 2_000,
  });

  const stallSignal: DecoderPressureSignal = {
    active: true,
    reason: "severe_stall",
    backlogFrames: 200,
    dropRatePercent: 0,
  };

  await controller.recover(stallSignal);
  await controller.recover(stallSignal);
  assert.equal(keyframeRequests, 0);

  await controller.recover(stallSignal);
  assert.equal(keyframeRequests, 1);
  assert.ok(
    logs.some((line) => line.includes("video=30ms audio=32ms")),
    "severe stall pins explicit low-latency jitter targets",
  );
  assert.deepEqual(states.at(-1), {
    active: true,
    recoveryAttempts: 1,
    recoveryAction: "signaling_keyframe",
  });
});

test("input policy preserves native, partially-reliable, and fallback routes", () => {
  const nativePackets: Array<{ payload: Uint8Array; partiallyReliable: boolean }> = [];
  const reliablePackets: Uint8Array[] = [];
  const channelPackets: Uint8Array[] = [];
  let nativeActive = true;
  let channelOpen = true;
  const channel = {
    get readyState() {
      return channelOpen ? "open" : "closed";
    },
    send: (payload: Uint8Array) => channelPackets.push(payload),
  } as unknown as RTCDataChannel;
  const controller = new InputChannelPolicyController(
    {
      partialReliableThresholdMs: 300,
      hidDeviceMask: 0xffff,
      enablePartiallyReliableTransferGamepad: 0xffff,
      enablePartiallyReliableTransferHid: 0xffff,
    },
    {
      isNativeInputActive: () => nativeActive,
      getPartiallyReliableChannel: () => channel,
      sendNativeInput: (payload, partiallyReliable) => {
        nativePackets.push({ payload, partiallyReliable });
      },
      sendReliable: (payload) => reliablePackets.push(payload),
    },
  );
  const payload = new Uint8Array([1, 2, 3]);

  controller.sendPartiallyReliable(payload);
  assert.deepEqual(nativePackets, [{ payload, partiallyReliable: true }]);

  nativeActive = false;
  controller.sendPartiallyReliable(payload);
  assert.equal(channelPackets.length, 1);

  channelOpen = false;
  controller.sendPartiallyReliable(payload);
  assert.deepEqual(reliablePackets, [payload]);
});

test("gamepad polling and keepalive decisions preserve adaptive timing", () => {
  assert.equal(selectGamepadPollIntervalMs({
    inputReady: false,
    visible: true,
    connectedCount: 1,
    inputBlocked: false,
  }), 100);
  assert.equal(selectGamepadPollIntervalMs({
    inputReady: true,
    visible: true,
    connectedCount: 1,
    inputBlocked: true,
  }), 16);
  assert.equal(selectGamepadPollIntervalMs({
    inputReady: true,
    visible: true,
    connectedCount: 1,
    inputBlocked: false,
  }), 4);
  assert.equal(shouldSendGamepadPacket(false, 99), false);
  assert.equal(shouldSendGamepadPacket(false, 100), true);
  assert.equal(shouldSendGamepadPacket(true, 0), true);
});
