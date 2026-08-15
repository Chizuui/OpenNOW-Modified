import type { NativeStreamStats } from "@shared/gfn";

import { mapServerGpuType } from "../platforms/gfn/webrtc/streamStatsHelpers";
import type { StreamDiagnostics } from "../platforms/gfn/webrtcClient";
import { formatServerLocation } from "../utils/streamDiagnosticsFormat";

// Native stats events arrive roughly once a second. When a stats merge has
// no fresh RTT source (the local RTCP measurement stopped being refreshed
// AND the server field stopped being refreshed), the last ping is kept for
// only a few merges before it decays to 0 (HUD "--") — a one-off spike value
// must never stick in the HUD as a stale "current" ping.
// How many consecutive native stats samples (~1/s) without a FRESH RTT
// source before the held ping decays to 0 ("--"). Both native sources carry
// their own sample age (networkRttAgeMs / localRtcpRttAgeMs) and expire
// independently: the local RTCP measurement only updates while Receiver
// Reports keep flowing (~5s cadence), and the server stats_channel RTT only
// updates while frames keep arriving. When both are stale the ping is held
// for ~10 samples (~10s) — long enough to bridge a brief stats gap, short
// enough that a frozen spike never looks like the current ping.
const NATIVE_RTT_STALE_SAMPLE_LIMIT = 10;
let nativeRttStaleSamples = 0;

// Native jitter is a single source (rtpsession's locally computed
// interarrival jitter of the received video stream) and it has no competing
// second source, so it only needs a short hold while the native side stops
// reporting it during a stall before decaying to 0 ("--"). The native
// streamer already gates the value on RTP liveness (None once no RTP has
// arrived for 5s), so this counter only bridges the gap between the stream
// stalling and the native side dropping the field.
const NATIVE_JITTER_STALE_SAMPLE_LIMIT = 5;
let nativeJitterStaleSamples = 0;

// Native pre-decode jitter buffer depth (ms of buffered video the streamer
// intentionally holds before decoding, relayed as preDecodeJitterBufferMs).
// Same hold-then-decay as native jitter: the native side gates the value on
// RTP liveness (None once no RTP has arrived for 5s), so this counter only
// bridges the gap between the stream stalling and the native side dropping
// the field.
const NATIVE_JITTER_BUF_STALE_SAMPLE_LIMIT = 5;
let nativeJitterBufStaleSamples = 0;

export function defaultDiagnostics(): StreamDiagnostics {
  return {
    connectionState: "closed",
    inputReady: false,
    nativeRendererActive: false,
    nativeStackedRenderer: false,
    connectedGamepads: 0,
    resolution: "",
    codec: "",
    requestedCodec: "",
    hardwareAcceleration: "",
    colorCodec: "",
    isHdr: false,
    bitrateKbps: 0,
    targetBitrateKbps: 0,
    decodeFps: 0,
    receiveFps: 0,
    renderFps: 0,
    gameFps: undefined,
    packetsLost: 0,
    packetsReceived: 0,
    packetLossPercent: 0,
    jitterMs: 0,
    rttMs: 0,
    transportType: "unknown",
    localCandidateType: "",
    framesReceived: 0,
    framesDecoded: 0,
    framesDropped: 0,
    decodeTimeMs: 0,
    renderTimeMs: 0,
    jitterBufferDelayMs: 0,
    inputQueueBufferedBytes: 0,
    inputQueuePeakBufferedBytes: 0,
    partiallyReliableInputQueueBufferedBytes: 0,
    partiallyReliableInputQueuePeakBufferedBytes: 0,
    inputQueueDropCount: 0,
    inputQueueMaxSchedulingDelayMs: 0,
    partiallyReliableInputOpen: false,
    mouseMoveTransport: "reliable",
    mouseFlushIntervalMs: 0,
    mousePacketsPerSecond: 0,
    mouseResidualMagnitude: 0,
    mousePath: "none",
    mouseHopLatencyMs: undefined,
    nativeInputPath: undefined,
    nativeMouseDeltaLatencyUs: undefined,
    nativeServerBitrateKbps: undefined,
    nativePacketLossPercent: undefined,
    lagReason: "unknown",
    lagReasonDetail: "Waiting for stream stats",
    gpuType: "",
    serverGpuType: "",
    serverRegion: "",
    serverZone: "",
    decoderPressureActive: false,
    decoderRecoveryAttempts: 0,
    decoderRecoveryAction: "none",
    shaderActive: false,
    nativeRequestedFps: undefined,
    nativeCapsFramerate: undefined,
    nativeQueueMode: undefined,
    nativeFramesPendingToPresent: undefined,
    nativePartialFlushCount: undefined,
    nativeCompleteFlushCount: undefined,
    nativeTransitionSummary: undefined,
    nativeRequestedStreamingFeaturesSummary: undefined,
    nativeFinalizedStreamingFeaturesSummary: undefined,
    micState: "uninitialized",
    micEnabled: false,
  };
}

export function mergeNativeStreamStats(
  current: StreamDiagnostics,
  stats: NativeStreamStats,
): StreamDiagnostics {
  const sinkDropped = stats.sinkDropped ?? 0;
  const sinkRendered = stats.sinkRendered ?? stats.framesRendered;
  const totalSinkFrames = sinkRendered + sinkDropped;
  const dropPercent = totalSinkFrames > 0 ? (sinkDropped / totalSinkFrames) * 100 : 0;
  const hardwareAcceleration = [
    stats.hardwareAcceleration || "GStreamer native decode",
    stats.zeroCopy && stats.memoryMode ? `${stats.memoryMode} zero-copy` : "",
    !stats.zeroCopy && stats.memoryMode ? stats.memoryMode : "",
    !stats.memoryMode && stats.zeroCopyD3D12 ? "D3D12 zero-copy" : "",
    !stats.memoryMode && stats.zeroCopyD3D11 ? "D3D11 zero-copy" : "",
  ].filter(Boolean).join(" · ");

  // Live native ping: prefer the FRESHEST source. Each native sample stamps
  // its sources with ages (networkRttAgeMs / localRtcpRttAgeMs); a source is
  // only "fresh" while its age stays under NATIVE_RTT_SOURCE_FRESH_AGE_MS,
  // and when both are fresh the newer one wins. This is the fix for the
  // frozen-ping symptom: rtpsession's have-rb sticks once set, so without the
  // local age gate the local RTCP value would override the server RTT
  // forever (even after the RR stream stopped); and the server RTT from the
  // stats channel can itself go quiet, so it gets the same age gate.
  const NATIVE_RTT_SOURCE_FRESH_AGE_MS = 15_000;
  const localAgeMs = stats.localRtcpRttAgeMs ?? 0;
  const serverAgeMs = stats.networkRttAgeMs ?? 0;
  const freshLocalRtcp = stats.localRtcpRttMs !== undefined
    && stats.localRtcpRttMs > 0
    && localAgeMs <= NATIVE_RTT_SOURCE_FRESH_AGE_MS;
  const freshServerRtt = stats.networkRttMs !== undefined
    && stats.networkRttMs > 0
    && serverAgeMs <= NATIVE_RTT_SOURCE_FRESH_AGE_MS;
  let rttMs: number;
  if (freshLocalRtcp || freshServerRtt) {
    nativeRttStaleSamples = 0;
    if (freshLocalRtcp && freshServerRtt) {
      // Both sources are alive — prefer the fresher of the two; ties go to
      // the local RTCP measurement (the native LSR/DLSR computation).
      rttMs = localAgeMs <= serverAgeMs ? stats.localRtcpRttMs! : stats.networkRttMs!;
    } else {
      rttMs = freshLocalRtcp ? stats.localRtcpRttMs! : stats.networkRttMs!;
    }
  } else if (current.rttMs > 0 && nativeRttStaleSamples < NATIVE_RTT_STALE_SAMPLE_LIMIT) {
    nativeRttStaleSamples += 1;
    rttMs = current.rttMs;
  } else {
    nativeRttStaleSamples = 0;
    rttMs = 0;
  }

  // Native jitter (rtpsession RFC 3550 interarrival jitter of the received
  // video stream, relayed as localJitterMs). Hold the previous value briefly
  // when a sample carries none (stream stalling), then decay to 0 so a
  // frozen jitter never looks like the current network state.
  let jitterMs: number;
  if (stats.localJitterMs !== undefined && stats.localJitterMs > 0) {
    nativeJitterStaleSamples = 0;
    jitterMs = stats.localJitterMs;
  } else if (current.jitterMs > 0 && nativeJitterStaleSamples < NATIVE_JITTER_STALE_SAMPLE_LIMIT) {
    nativeJitterStaleSamples += 1;
    jitterMs = current.jitterMs;
  } else {
    nativeJitterStaleSamples = 0;
    jitterMs = 0;
  }

  // Native pre-decode jitter buffer depth (ms of buffered video). Hold the
  // previous value briefly while the native side stops reporting it during a
  // stall, then decay to 0 so a dead session does not keep showing a depth.
  let jitterBufferDelayMs: number;
  if (stats.preDecodeJitterBufferMs !== undefined && stats.preDecodeJitterBufferMs > 0) {
    nativeJitterBufStaleSamples = 0;
    jitterBufferDelayMs = stats.preDecodeJitterBufferMs;
  } else if (current.jitterBufferDelayMs > 0
    && nativeJitterBufStaleSamples < NATIVE_JITTER_BUF_STALE_SAMPLE_LIMIT) {
    nativeJitterBufStaleSamples += 1;
    jitterBufferDelayMs = current.jitterBufferDelayMs;
  } else {
    nativeJitterBufStaleSamples = 0;
    jitterBufferDelayMs = 0;
  }

  return {
    ...current,
    connectionState: "connected",
    inputReady: current.inputReady,
    nativeRendererActive: true,
    resolution: stats.resolution || current.resolution,
    codec: stats.codec || current.codec,
    hardwareAcceleration,
    bitrateKbps: stats.bitrateKbps,
    targetBitrateKbps: stats.targetBitrateKbps,
    decodeFps: Math.round(stats.decodedFps),
    receiveFps: Math.round(stats.decodedFps),
    renderFps: Math.round(stats.renderFps),
    // Server-reported game FPS (stats_channel relayed through the native
    // streamer) so the HUD "GAME" number shows the real game rate in native
    // sessions instead of falling back to the local decode rate.
    gameFps: stats.gameFps !== undefined && stats.gameFps > 0
      ? stats.gameFps
      : current.gameFps,
    decodeTimeMs: stats.decodeTimeMs !== undefined && stats.decodeTimeMs > 0
      ? stats.decodeTimeMs
      : current.decodeTimeMs,
    // Native ping source, in preference order: (1) the FRESHEST of the
    // locally computed RTCP LSR/DLSR round-trip and the server-reported
    // stats_channel RTT (each gated by its own sample age); (2) the previous
    // value, only while it is fresh (see NATIVE_RTT_STALE_SAMPLE_LIMIT).
    // 0 = no fresh source → HUD keeps "--".
    rttMs,
    // Real receive-side jitter for native sessions (rtpsession source-stats)
    // instead of the 0 the HUD showed before; 0 → HUD "<0.1ms"/"--".
    jitterMs,
    // Native analogue of the WebRTC JitterBuf metric: the pre-decode jitter
    // buffer depth (ms) the streamer holds before decoding; 0 → HUD "--".
    jitterBufferDelayMs,
    framesReceived: stats.framesDecoded,
    framesDecoded: stats.framesDecoded,
    framesDropped: sinkDropped,
    // Server-reported network packet loss wins over the local sink-drop
    // percentage when available (the latter is render drop, not network loss).
    packetLossPercent: stats.networkPacketLossPercent !== undefined
      ? stats.networkPacketLossPercent
      : dropPercent,
    // Raw server-reported loss kept separate so the native HUD banner can key
    // off the stats_channel field directly (see NATIVE_PACKET_LOSS_BANNER_PERCENT).
    nativePacketLossPercent: stats.networkPacketLossPercent,
    inputQueueBufferedBytes: 0,
    inputQueuePeakBufferedBytes: 0,
    partiallyReliableInputQueueBufferedBytes: 0,
    partiallyReliableInputQueuePeakBufferedBytes: 0,
    inputQueueDropCount: 0,
    inputQueueMaxSchedulingDelayMs: 0,
    mousePacketsPerSecond: 0,
    mouseResidualMagnitude: 0,
    // Native streamer reports the capture path it is actually using; when it
    // owns RawInput (sink-native / internal / external) the renderer mouse
    // sources stand down, so mirror that as the effective path.
    mousePath: stats.inputPath === "sink-native" || stats.inputPath === "internal"
        || stats.inputPath === "external"
      ? stats.inputPath
      : current.mousePath,
    nativeInputPath: stats.inputPath,
    nativeMouseDeltaLatencyUs: stats.mouseDeltaLatencyUs,
    nativeServerBitrateKbps: stats.networkBitrateKbps,
    // Mirror the web client's network-lag bands so native sessions surface the
    // same warn state (alert dot / warn styling) from the server-reported
    // RTT/loss: ≥1% loss or ≥75ms RTT = network lag, else render-drop / stable.
    lagReason: (stats.networkPacketLossPercent !== undefined
        && stats.networkPacketLossPercent >= 1)
      || rttMs >= 75
      ? "network"
      : (dropPercent > 1 ? "render" : "stable"),
    lagReasonDetail: stats.lastTransitionSummary
      ? `Native bitrate ${stats.bitratePerformancePercent.toFixed(0)}% of target · ${stats.lastTransitionSummary}`
      : `Native bitrate ${stats.bitratePerformancePercent.toFixed(0)}% of target`,
    decoderPressureActive: false,
    nativeRequestedFps: stats.requestedFps,
    nativeCapsFramerate: stats.capsFramerate,
    nativeQueueMode: stats.queueMode,
    nativeFramesPendingToPresent: stats.framesPendingToPresent,
    // Duplicate-frame detector: unique vs total decoded frames (see the type
    // docs). Only reported by the native streamer; absent in WebRTC sessions.
    nativeDuplicateFramesSeen: stats.duplicateFramesSeen,
    nativeDuplicateFramesUnique: stats.duplicateFramesUnique,
    nativePartialFlushCount: stats.partialFlushCount,
    nativeCompleteFlushCount: stats.completeFlushCount,
    nativeTransitionSummary: stats.lastTransitionSummary,
    nativeRequestedStreamingFeaturesSummary: stats.requestedStreamingFeaturesSummary,
    nativeFinalizedStreamingFeaturesSummary: stats.finalizedStreamingFeaturesSummary,
    // Server GPU (raw CloudMatch code like "2080d / T10", stamped by the main
    // process onto native stats) mapped to the official rig name so the HUD
    // shows "GeForce RTX" instead of falling back to the local GPU. Keep the
    // previous value when a stats sample carries none.
    serverGpuType: stats.serverGpuType
      ? mapServerGpuType(stats.serverGpuType)
      : current.serverGpuType,
    // Zone LB hostname (datacenter code) stamped by the main process; resolve
    // the official-style region label once and keep it across samples so the
    // HUD shows "Malaysia (NP-KUL-01)" in native sessions too.
    serverLocationLabel: stats.serverLocation
      ? formatServerLocation("", stats.serverLocation)
      : current.serverLocationLabel,
    serverRegion: stats.serverLocation || current.serverRegion,
  };
}
