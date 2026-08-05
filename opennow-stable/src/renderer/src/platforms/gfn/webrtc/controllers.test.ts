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
  // no keyframes, no bitrate churn, and the jitter buffer keeps its normal
  // floor so transient jitter is absorbed instead of turning into frame drops.
  await controller.recover(pressureSignal);
  await controller.recover(pressureSignal);
  await controller.recover(pressureSignal);
  assert.equal(keyframeRequests, 0);
  assert.ok(
    logs.some((line) => line.includes("video=35ms audio=50ms")),
    "backlog/drop pressure keeps the normal jitter floor",
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

test("drop burst requests a keyframe immediately without the multi-poll debounce", async () => {
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

  const burstSignal: DecoderPressureSignal = {
    active: true,
    reason: "drop_burst",
    backlogFrames: 12,
    dropRatePercent: 4,
  };

  // A single drop_burst sample must trigger an immediate keyframe (the picture
  // is already frozen; waiting ~3s of polls would leave the stutter visible).
  await controller.recover(burstSignal);
  assert.equal(keyframeRequests, 1);
  assert.ok(
    logs.some((line) => line.includes("keyframe requested (reason=drop_burst")),
    "drop burst requests a keyframe",
  );

  // The keyframe cooldown still throttles repeated bursts.
  await controller.recover(burstSignal);
  assert.equal(keyframeRequests, 1);

  const stableSignal: DecoderPressureSignal = {
    active: false,
    reason: "stable",
    backlogFrames: 0,
    dropRatePercent: 0,
  };
  for (let index = 0; index < 6; index++) {
    await controller.recover(stableSignal);
  }
  assert.deepEqual(states.at(-1), {
    active: false,
    recoveryAttempts: 0,
    recoveryAction: "none",
  });
});

function makeFakeReceiver(kind: "video" | "audio"): RTCRtpReceiver {
  const receiver: Record<string, unknown> = {
    jitterBufferTarget: undefined,
    playoutDelayHint: undefined,
    track: { kind, contentHint: "" },
  };
  return receiver as unknown as RTCRtpReceiver;
}

test("jitter buffer floor grows with measured RTT and clamps to bounds", () => {
  const logs: string[] = [];
  const videoReceiver = makeFakeReceiver("video");
  const audioReceiver = makeFakeReceiver("audio");
  const controller = new DecoderPressureController({
    log: (message) => logs.push(message),
    getPeerConnection: () => null,
    getControlChannel: () => null,
    requestSignalingKeyframe: async () => {},
    onStateChange: () => {},
    now: () => 0,
  });
  controller.configureReceiver(videoReceiver, "video");
  controller.configureReceiver(audioReceiver, "audio");

  // Low RTT stays at the balanced preset floor; the adaptive floor never
  // shrinks below the preset base.
  controller.updateJitterFloorFromRtt(20);
  assert.equal(videoReceiver.jitterBufferTarget, 35);
  assert.equal(audioReceiver.jitterBufferTarget, 50);

  controller.updateJitterFloorFromRtt(160);
  assert.equal(videoReceiver.jitterBufferTarget, 80);
  assert.equal(audioReceiver.jitterBufferTarget, 95);
  assert.ok(
    logs.some((line) => line.includes("video=80ms audio=95ms")),
    "RTT 160ms -> 80ms video floor (0.5x)",
  );

  // Deadband: a 2ms floor swing (80 -> 82) must NOT re-apply the receiver target.
  const logCountBefore = logs.length;
  controller.updateJitterFloorFromRtt(164);
  assert.equal(
    logs.length,
    logCountBefore,
    "small RTT swings inside the deadband do not churn the jitter target",
  );

  controller.updateJitterFloorFromRtt(400);
  assert.equal(videoReceiver.jitterBufferTarget, 100);
  assert.equal(audioReceiver.jitterBufferTarget, 115);
  assert.ok(
    logs.some((line) => line.includes("video=100ms audio=115ms")),
    "very high RTT caps at the 100ms maximum",
  );
});

test("jitter buffer preset switch applies new floors live and resets the RTT floor", () => {
  const logs: string[] = [];
  const videoReceiver = makeFakeReceiver("video");
  const audioReceiver = makeFakeReceiver("audio");
  const controller = new DecoderPressureController({
    log: (message) => logs.push(message),
    getPeerConnection: () => null,
    getControlChannel: () => null,
    requestSignalingKeyframe: async () => {},
    onStateChange: () => {},
    now: () => 0,
  });
  controller.configureReceiver(videoReceiver, "video");
  controller.configureReceiver(audioReceiver, "audio");

  // Default preset is balanced.
  assert.equal(videoReceiver.jitterBufferTarget, 35);
  assert.equal(audioReceiver.jitterBufferTarget, 50);

  // Switching to "low" applies to receivers immediately.
  controller.setJitterBufferMode("low");
  assert.equal(videoReceiver.jitterBufferTarget, 20);
  assert.equal(audioReceiver.jitterBufferTarget, 35);

  // RTT scaling never drops below the preset floor.
  controller.updateJitterFloorFromRtt(20);
  assert.equal(videoReceiver.jitterBufferTarget, 20);
  assert.equal(audioReceiver.jitterBufferTarget, 35);

  // "smooth" provides large headroom.
  controller.setJitterBufferMode("smooth");
  assert.equal(videoReceiver.jitterBufferTarget, 70);
  assert.equal(audioReceiver.jitterBufferTarget, 100);

  // Re-setting the same mode is a no-op.
  const logCountBefore = logs.length;
  controller.setJitterBufferMode("smooth");
  assert.equal(logs.length, logCountBefore);
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
