import { StreamingEngine } from '../StreamingEngine';
import { StreamConfig, StreamSession, StreamStats, UserInput } from '../streamingTypes';
import { GfnWebRtcClient } from '../../gfn/webrtcClient';

export class WebRTCEngine extends StreamingEngine {
  private client: GfnWebRtcClient;
  private lastStats: StreamStats;

  constructor(config: StreamConfig, client: GfnWebRtcClient) {
    super(config);
    this.client = client;
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
    console.log('[WebRTCEngine] Initialized');
  }

  async connect(session: StreamSession): Promise<void> {
    this.session = session;
    console.log('[WebRTCEngine] Connecting signaling...');
    await window.openNow.connectSignaling({
      sessionId: session.sessionId,
      signalingServer: session.serverAddress,
    });
  }

  async disconnect(): Promise<void> {
    console.log('[WebRTCEngine] Disconnecting...');
    this.isRunning = false;
    await window.openNow.disconnectSignaling();
  }

  getStreamRenderer(): HTMLElement | null {
    // Return the video element from client options
    return (this.client as any).options?.videoElement || null;
  }

  getStats(): StreamStats {
    const diag = (this.client as any).diagnostics;
    if (diag) {
      return {
        fps: diag.renderFps || diag.decodeFps || 0,
        bitrate: diag.bitrateKbps || 0,
        latency: diag.rttMs || 0,
        packetLoss: diag.packetLossPercent || 0,
        connectionQuality: this.assessQuality(diag.renderFps || diag.decodeFps || 0, diag.rttMs || 0),
        timestamp: Date.now(),
      };
    }
    return this.lastStats;
  }

  async updateConfig(newConfig: Partial<StreamConfig>): Promise<void> {
    this.config = { ...this.config, ...newConfig };
  }

  sendInput(input: UserInput): void {
    console.log('[WebRTCEngine] sendInput (handled by GfnWebRtcClient internally)', input);
  }

  isHealthy(): boolean {
    return this.isRunning;
  }

  async handleSignalingEvent(event: any): Promise<void> {
    if (event.type === 'offer') {
      console.log('[WebRTCEngine] Handling SDP Offer...');
      const activeSession = this.session ? {
        sessionId: this.session.sessionId,
        serverIp: this.session.serverAddress,
        signalingServer: this.session.serverAddress,
        iceServers: [],
      } : null;

      if (activeSession) {
        await this.client.handleOffer(event.sdp, activeSession as any, {
          codec: this.config.codec as any,
          resolution: this.config.resolution,
          fps: this.config.fps,
          maxBitrateKbps: this.config.maxBitrateMbps * 1000,
          colorQuality: this.config.colorQuality || 'standard',
        });
      }
    } else if (event.type === 'remote-ice') {
      console.log('[WebRTCEngine] Adding Remote ICE candidate...');
      await this.client.addRemoteCandidate(event.candidate);
    }
  }

  getClientInstance(): GfnWebRtcClient | null {
    return this.client;
  }

  private assessQuality(fps: number, rtt: number): 'excellent' | 'good' | 'fair' | 'poor' {
    if (fps >= 55 && rtt < 50) return 'excellent';
    if (fps >= 40 && rtt < 100) return 'good';
    if (fps >= 20 && rtt < 200) return 'fair';
    return 'poor';
  }
}
