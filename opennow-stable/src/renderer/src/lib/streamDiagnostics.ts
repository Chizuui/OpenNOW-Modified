import type { NativeStreamStats } from "@shared/gfn";

import type { StreamDiagnostics } from "../platforms/gfn/webrtcClient";

// Native stats events arrive roughly once a second. When a stats merge has
// no fresh RTT source (no local RTCP measurement yet and the server field
// is 0/absent), the last ping is kept for only a few merges before it
// decays to 0 (HUD "--") — a one-off spike value must never stick in the
// HUD as a stale "current" ping.
const NATIVE_RTT_STALE_SAMPLE_LIMIT = 5;
let nativeRttStaleSamples = 0;

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
    mouseFlushIntervalMs: 8,
    mousePacketsPerSecond: 0,
    mouseResidualMagnitude: 0,
    mouseAdaptiveFlushActive: false,
    mousePath: "none",
    mouseHopLatencyMs: undefined,
    nativeInputPath: undefined,
    nativeMouseDeltaLatencyUs: undefined,
    nativeServerBitrateKbps: undefined,
    nativePacketLossPercent: undefined,
    lagReason: "unknown",
    lagReasonDetail: "Waiting for stream stats",
    gpuType: "",
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

  // Live native ping: prefer the local RTCP measurement, then the server
  // stats_channel field. When neither reports anything for a few consecutive
  // merges, the previous value decays to 0 ("--") instead of sticking.
  const freshLocalRtcp = stats.localRtcpRttMs !== undefined && stats.localRtcpRttMs > 0;
  const freshServerRtt = stats.networkRttMs !== undefined && stats.networkRttMs > 0;
  let rttMs: number;
  if (freshLocalRtcp || freshServerRtt) {
    nativeRttStaleSamples = 0;
    rttMs = freshLocalRtcp ? stats.localRtcpRttMs! : stats.networkRttMs!;
  } else if (current.rttMs > 0 && nativeRttStaleSamples < NATIVE_RTT_STALE_SAMPLE_LIMIT) {
    nativeRttStaleSamples += 1;
    rttMs = current.rttMs;
  } else {
    nativeRttStaleSamples = 0;
    rttMs = 0;
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
    // Native ping source, in preference order: (1) locally computed RTCP
    // LSR/DLSR round-trip from Receiver Reports the server sends about our
    // outgoing RTP — the live local measurement (the native pipeline always
    // negotiates the mic m-line sendonly so RTP flows and RTCP RRs come
    // back); (2) the server-reported stats_channel RTT; (3) the previous
    // value, only while it is fresh (see NATIVE_RTT_STALE_SAMPLE_LIMIT).
    // 0 = no fresh source → HUD keeps "--".
    rttMs,
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
    mouseAdaptiveFlushActive: false,
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
    nativePartialFlushCount: stats.partialFlushCount,
    nativeCompleteFlushCount: stats.completeFlushCount,
    nativeTransitionSummary: stats.lastTransitionSummary,
    nativeRequestedStreamingFeaturesSummary: stats.requestedStreamingFeaturesSummary,
    nativeFinalizedStreamingFeaturesSummary: stats.finalizedStreamingFeaturesSummary,
  };
}
