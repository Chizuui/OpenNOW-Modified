import type { NativeStreamStats } from "@shared/gfn";

import type { StreamDiagnostics } from "../platforms/gfn/webrtcClient";

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
    // outgoing RTP — the requested local measurement, active once the
    // pipeline sends RTP; (2) the server-reported stats_channel RTT (the
    // server's own LSR/DLSR measurement of OUR receiver reports); (3) the
    // previous value. 0/undefined = no source yet → HUD keeps "--".
    rttMs: stats.localRtcpRttMs !== undefined && stats.localRtcpRttMs > 0
      ? stats.localRtcpRttMs
      : (stats.networkRttMs !== undefined && stats.networkRttMs > 0
        ? stats.networkRttMs
        : current.rttMs),
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
      || (stats.networkRttMs !== undefined && stats.networkRttMs >= 75)
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
