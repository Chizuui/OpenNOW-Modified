/// <reference types="node" />

import test from "node:test";
import assert from "node:assert/strict";

import {
  DecoderPressureController,
  isVideoFreezeEligible,
  shouldRequestLossKeyframe,
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
  let now = 0;
  const controller = new DecoderPressureController({
    log: (message) => logs.push(message),
    getPeerConnection: () => null,
    getControlChannel: () => null,
    requestSignalingKeyframe: async () => {},
    onStateChange: () => {},
    now: () => now,
  });
  controller.configureReceiver(videoReceiver, "video");
  controller.configureReceiver(audioReceiver, "audio");

  // Low RTT stays at the balanced preset floor; the adaptive floor never
  // shrinks below the preset base.
  controller.updateJitterFloorFromRtt(20);
  assert.equal(videoReceiver.jitterBufferTarget, 35);
  assert.equal(audioReceiver.jitterBufferTarget, 50);

  // A raw RTT spike (160ms vs a ~55ms EMA) is a jitter burst in flight: the
  // floor is pinned at the maximum and held (native streamer port).
  now += 1000;
  controller.updateJitterFloorFromRtt(160);
  assert.equal(videoReceiver.jitterBufferTarget, 100);
  assert.equal(audioReceiver.jitterBufferTarget, 115);
  assert.ok(
    logs.some((line) => line.includes("video=100ms audio=115ms")),
    "RTT spike pins the floor at the 100ms maximum",
  );

  // Deadband: the floor is already pinned at MAX, so no re-apply is needed.
  const logCountBefore = logs.length;
  now += 1000;
  controller.updateJitterFloorFromRtt(164);
  assert.equal(
    logs.length,
    logCountBefore,
    "RTT swings inside the deadband do not churn the jitter target",
  );

  now += 1000;
  controller.updateJitterFloorFromRtt(400);
  assert.equal(videoReceiver.jitterBufferTarget, 100);
  assert.equal(audioReceiver.jitterBufferTarget, 115);
  assert.ok(
    logs.some((line) => line.includes("video=100ms audio=115ms")),
    "very high RTT caps at the 100ms maximum",
  );
});

test("jitter buffer RTT ramp applies the 0.5x floor when RTT is stable", () => {
  const videoReceiver = makeFakeReceiver("video");
  const audioReceiver = makeFakeReceiver("audio");
  const controller = new DecoderPressureController({
    log: () => {},
    getPeerConnection: () => null,
    getControlChannel: () => null,
    requestSignalingKeyframe: async () => {},
    onStateChange: () => {},
    now: () => 0,
  });
  controller.configureReceiver(videoReceiver, "video");
  controller.configureReceiver(audioReceiver, "audio");

  // First sample initializes the spike-detection EMA, so a stable 100ms link
  // applies the 0.5x ramp floor without tripping the burst hold.
  controller.updateJitterFloorFromRtt(100);
  assert.equal(videoReceiver.jitterBufferTarget, 50);
  assert.equal(audioReceiver.jitterBufferTarget, 65);
});

test("packet loss raises the jitter floor before RTT climbs", () => {
  const videoReceiver = makeFakeReceiver("video");
  const audioReceiver = makeFakeReceiver("audio");
  const controller = new DecoderPressureController({
    log: () => {},
    getPeerConnection: () => null,
    getControlChannel: () => null,
    requestSignalingKeyframe: async () => {},
    onStateChange: () => {},
    now: () => 0,
  });
  controller.configureReceiver(videoReceiver, "video");
  controller.configureReceiver(audioReceiver, "audio");

  // 0.2% loss (>= 0.1%) raises the floor to the MID level even at low RTT.
  controller.updateJitterFloorFromRtt(20, 0.2);
  assert.equal(videoReceiver.jitterBufferTarget, 70);
  assert.equal(audioReceiver.jitterBufferTarget, 85);

  // 0.6% loss (>= 0.5%) pins the floor at the maximum.
  controller.updateJitterFloorFromRtt(20, 0.6);
  assert.equal(videoReceiver.jitterBufferTarget, 100);
  assert.equal(audioReceiver.jitterBufferTarget, 115);

  // Sub-threshold loss returns to the preset floor.
  controller.updateJitterFloorFromRtt(20, 0.05);
  assert.equal(videoReceiver.jitterBufferTarget, 35);
  assert.equal(audioReceiver.jitterBufferTarget, 50);
});

test("RTT spike holds the floor at MAX for the burst window then decays", () => {
  const videoReceiver = makeFakeReceiver("video");
  const audioReceiver = makeFakeReceiver("audio");
  let now = 0;
  const controller = new DecoderPressureController({
    log: () => {},
    getPeerConnection: () => null,
    getControlChannel: () => null,
    requestSignalingKeyframe: async () => {},
    onStateChange: () => {},
    now: () => now,
  });
  controller.configureReceiver(videoReceiver, "video");
  controller.configureReceiver(audioReceiver, "audio");

  controller.updateJitterFloorFromRtt(20);
  assert.equal(videoReceiver.jitterBufferTarget, 35);

  // Spike -> floor pinned at MAX for JITTER_BURST_HOLD_MS (4s).
  now += 1000;
  controller.updateJitterFloorFromRtt(160);
  assert.equal(videoReceiver.jitterBufferTarget, 100);

  // A healthy sample inside the hold window keeps the MAX floor: the burst in
  // flight is still being absorbed and the next burst would leak through if
  // the buffer shrank early.
  now += 2000; // 3000ms < 5000ms hold-until
  controller.updateJitterFloorFromRtt(30);
  assert.equal(
    videoReceiver.jitterBufferTarget,
    100,
    "burst hold keeps the floor deep through the spike cluster",
  );

  // After the hold expires, the floor decays back to the RTT ramp.
  now += 5000;
  controller.updateJitterFloorFromRtt(30);
  assert.equal(videoReceiver.jitterBufferTarget, 35);
  assert.equal(audioReceiver.jitterBufferTarget, 50);
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

test("video freeze watchdog qualifies only when no frame was presented while playing", () => {
  // Frame seen 700ms ago, playing, visible, ready → frozen.
  assert.equal(isVideoFreezeEligible({
    lastFrameAtMs: 1000,
    nowMs: 1700,
    timeoutMs: 600,
    paused: false,
    hidden: false,
    readyState: 3,
  }), true);
  // A fresh frame (within the window) is not frozen.
  assert.equal(isVideoFreezeEligible({
    lastFrameAtMs: 1500,
    nowMs: 1700,
    timeoutMs: 600,
    paused: false,
    hidden: false,
    readyState: 3,
  }), false);
  // No frame seen yet (connection starting) must not trigger.
  assert.equal(isVideoFreezeEligible({
    lastFrameAtMs: 0,
    nowMs: 1700,
    timeoutMs: 600,
    paused: false,
    hidden: false,
    readyState: 3,
  }), false);
  // User paused / tab hidden / no media data → never a freeze.
  assert.equal(isVideoFreezeEligible({
    lastFrameAtMs: 1000,
    nowMs: 1700,
    timeoutMs: 600,
    paused: true,
    hidden: false,
    readyState: 3,
  }), false);
  assert.equal(isVideoFreezeEligible({
    lastFrameAtMs: 1000,
    nowMs: 1700,
    timeoutMs: 600,
    paused: false,
    hidden: true,
    readyState: 3,
  }), false);
  assert.equal(isVideoFreezeEligible({
    lastFrameAtMs: 1000,
    nowMs: 1700,
    timeoutMs: 600,
    paused: false,
    hidden: false,
    readyState: 1,
  }), false);
});

test("loss keyframe needs the threshold held for consecutive polls", () => {
  // Below threshold → never.
  assert.equal(shouldRequestLossKeyframe(1.5, 5), false);
  // At threshold but not enough consecutive polls yet → no.
  assert.equal(shouldRequestLossKeyframe(2.5, 1), false);
  // Sustained above threshold → yes.
  assert.equal(shouldRequestLossKeyframe(2.5, 2), true);
  assert.equal(shouldRequestLossKeyframe(8.0, 3), true);
});

test("network loss triggers a keyframe only after sustained loss, with cooldown", async () => {
  const logs: string[] = [];
  let keyframeRequests = 0;
  let nowMs = 2_000;
  const controller = new DecoderPressureController({
    log: (message) => logs.push(message),
    getPeerConnection: () => null,
    getControlChannel: () => null,
    requestSignalingKeyframe: async () => {
      keyframeRequests++;
    },
    onStateChange: () => undefined,
    now: () => nowMs,
  });

  // One lossy poll alone must NOT interrupt the stream.
  controller.reportPacketLoss(2.5);
  assert.equal(keyframeRequests, 0);
  // Second consecutive lossy poll crosses the threshold → keyframe.
  controller.reportPacketLoss(2.5);
  await flushMicrotasks();
  assert.equal(keyframeRequests, 1);
  assert.ok(
    logs.some((line) => line.includes("reason=network_loss")),
    "sustained loss requests a keyframe",
  );
  // Cooldown throttles a third poll even while loss persists.
  controller.reportPacketLoss(2.5);
  await flushMicrotasks();
  assert.equal(keyframeRequests, 1);
  // A clean poll resets the consecutive counter; loss must re-accumulate.
  controller.reportPacketLoss(0.1);
  controller.reportPacketLoss(2.5);
  await flushMicrotasks();
  assert.equal(keyframeRequests, 1, "one lossy poll after a clean reset does not re-trigger");
  // After the cooldown passes, sustained loss can request again.
  nowMs = 4_000;
  controller.reportPacketLoss(2.5);
  await flushMicrotasks();
  assert.equal(keyframeRequests, 2);
  // And the cooldown throttles a third poll immediately after.
  controller.reportPacketLoss(2.5);
  await flushMicrotasks();
  assert.equal(keyframeRequests, 2);
});

/** Drain the promise chain of the async keyframe path before asserting. */
function flushMicrotasks(): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, 0));
}

test("fast freeze watchdog signal requests an immediate keyframe like a drop burst", async () => {
  const states: DecoderPressureState[] = [];
  let keyframeRequests = 0;
  const controller = new DecoderPressureController({
    log: () => undefined,
    getPeerConnection: () => null,
    getControlChannel: () => null,
    requestSignalingKeyframe: async () => {
      keyframeRequests++;
    },
    onStateChange: (state) => states.push(state),
    now: () => 2_000,
  });

  // A single video_freeze sample is urgent — no multi-poll debounce.
  await controller.recover({
    active: true,
    reason: "video_freeze",
    backlogFrames: 0,
    dropRatePercent: 0,
  });
  assert.equal(keyframeRequests, 1);
  assert.deepEqual(states.at(-1), {
    active: true,
    recoveryAttempts: 1,
    recoveryAction: "signaling_keyframe",
  });
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
