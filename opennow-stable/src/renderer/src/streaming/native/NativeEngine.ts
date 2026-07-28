import { StreamingEngine } from '../StreamingEngine';
import { StreamConfig, StreamSession, StreamStats, UserInput } from '../streamingTypes';
import { GfnWebRtcClient } from '../../gfn/webrtcClient';
import { buildNativeStreamerSessionContext } from '@shared/gfn';

export interface NativeEngineOptions {
  settings: any;
  shortcuts: any;
}

export class NativeEngine extends StreamingEngine {
  private client: GfnWebRtcClient;
  private options: NativeEngineOptions;
  private lastStats: StreamStats;
  private nativeInputProtocolVersion: number | null = null;

  constructor(config: StreamConfig, client: GfnWebRtcClient, options: NativeEngineOptions) {
    super(config);
    this.client = client;
    this.options = options;
    this.lastStats = {
      fps: 0,
      bitrate: 0,
      latency: 0,
      packetLoss: 0,
      connectionQuality: 'poor',
      timestamp: Date.now(),
    };
  }

  async initialize(): Promise<void> {
    console.log('[NativeEngine] Initializing native streamer engine...');
    // Probe native status
    const status = await window.openNow.getNativeStreamerStatus();
    if (!status.detected) {
      throw new Error(`Native streamer not detected: ${status.message}`);
    }
    console.log('[NativeEngine] Native streamer status:', status.message);
  }

  async connect(session: StreamSession): Promise<void> {
    this.session = session;
    console.log('[NativeEngine] Preparing connect with session:', session.sessionId);

    const rawSession = session.rawSession;
    if (!rawSession) {
      throw new Error('[NativeEngine] Full GFN session info is required for native streaming');
    }

    // Build context required by main process NativeStreamerManager
    const nativeStreamerContext = buildNativeStreamerSessionContext(
      rawSession,
      {
        ...this.options.settings,
        nativeStreamerBackend: "gstreamer",
      },
      {
        toggleStats: this.options.shortcuts.toggleStats,
        togglePointerLock: this.options.shortcuts.togglePointerLock,
        toggleFullscreen: this.options.shortcuts.toggleFullscreen,
        stopStream: this.options.shortcuts.stopStream,
        toggleAntiAfk: this.options.shortcuts.toggleAntiAfk,
        toggleMicrophone: this.options.shortcuts.toggleMicrophone,
        screenshot: this.options.shortcuts.screenshot,
                toggleRecording: this.options.shortcuts.recording,
      }
    );

    console.log('[NativeEngine] Connecting signaling with native context...');
    await window.openNow.connectSignaling({
      sessionId: session.sessionId,
      signalingServer: session.serverAddress,
      nativeStreamer: nativeStreamerContext,
    });
  }

  async disconnect(): Promise<void> {
    console.log('[NativeEngine] Disconnecting signaling and native streamer...');
    this.isRunning = false;
    await window.openNow.disconnectSignaling();
  }

  getStreamRenderer(): HTMLElement | null {
    // Return the video element from client options for input capture
    return (this.client as any).options?.videoElement || null;
  }

  getStats(): StreamStats {
    return this.lastStats;
  }

  async updateConfig(newConfig: Partial<StreamConfig>): Promise<void> {
    this.config = { ...this.config, ...newConfig };
    if (newConfig.maxBitrateMbps !== undefined) {
      window.openNow.setSetting('maxBitrateMbps', newConfig.maxBitrateMbps);
    }
  }

  sendInput(input: UserInput): void {
    // Send input packet to main process via IPC
    window.openNow.sendNativeInput({
      payload: input.payload,
      partiallyReliable: input.type === 'mouse' && input.payload?.partiallyReliable,
    });
  }

  isHealthy(): boolean {
    return this.isRunning;
  }

  getClientInstance(): GfnWebRtcClient | null {
    return this.client;
  }

  async handleSignalingEvent(event: any): Promise<void> {
    if (event.type === 'native-stream-started') {
      console.log('[NativeEngine] Stream started event');
      this.isRunning = true;
      this.activateNativeInput();
      this.emit('connected');
    } else if (event.type === 'native-input-ready') {
      console.log('[NativeEngine] Input bridge ready event:', event.protocolVersion);
      this.nativeInputProtocolVersion = event.protocolVersion;
      this.client.setNativeInputProtocolVersion(event.protocolVersion);
      this.isRunning = true;
      this.activateNativeInput();
      this.emit('connected');
    } else if (event.type === 'native-stream-stats') {
      const stats = event.stats;
      this.lastStats = {
        fps: stats.renderFps || stats.fps || 0,
        bitrate: stats.bitrateKbps || 0,
        latency: stats.rttMs || stats.latency || 0,
        packetLoss: stats.packetLossPercent || 0,
        connectionQuality: this.assessQuality(stats.renderFps || stats.fps || 0, stats.rttMs || stats.latency || 0),
        timestamp: Date.now(),
      };
      this.emit('stats-update', this.lastStats);
    } else if (event.type === 'native-stream-stopped') {
      console.warn('[NativeEngine] Stream stopped event:', event.reason);
      this.isRunning = false;
      this.emit('disconnected', event.reason);
    } else if (event.type === 'disconnected') {
      console.warn('[NativeEngine] Signaling disconnected event:', event.reason);
      this.isRunning = false;
      this.emit('disconnected', event.reason);
    } else if (event.type === 'error') {
      console.error('[NativeEngine] Error event:', event.message);
      this.emit('error', event.message);
    } else if (event.type === 'native-shortcut') {
      this.emit('native-shortcut', event.action);
    } else if (event.type === 'native-clipboard-paste') {
      this.emit('native-clipboard-paste');
    } else if (event.type === 'native-input-capture-changed') {
      this.emit('native-input-capture-changed', event.captured);
    } else if (event.type === 'native-stream-transition') {
      this.emit('native-stream-transition', event.transition);
    }
  }

  private activateNativeInput(): void {
    this.client.activateNativeInput(this.nativeInputProtocolVersion ?? undefined, {
      codec: this.config.codec as any,
      colorQuality: this.options.settings.colorQuality,
      resolution: this.config.resolution,
      fps: this.config.fps,
      maxBitrateKbps: this.config.maxBitrateMbps * 1000,
    });
  }

  private assessQuality(fps: number, rtt: number): 'excellent' | 'good' | 'fair' | 'poor' {
    if (fps >= 55 && rtt < 50) return 'excellent';
    if (fps >= 40 && rtt < 100) return 'good';
    if (fps >= 20 && rtt < 200) return 'fair';
    return 'poor';
  }
}
