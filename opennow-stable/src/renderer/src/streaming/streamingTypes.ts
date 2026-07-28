/**
 * Shared types across all streaming engines
 */

export enum StreamingEngineType {
  WebRTC = 'webrtc',
  Native = 'native',
}

export interface StreamStats {
  fps: number;
  bitrate: number;
  latency: number;
  packetLoss: number;
  connectionQuality: 'excellent' | 'good' | 'fair' | 'poor';
  timestamp: number;
}

export interface StreamConfig {
  type: StreamingEngineType;
  resolution: string;
  fps: number;
  maxBitrateMbps: number;
  codec: string;
  enableAudio: boolean;
  colorQuality?: any;
  enableNativeMouseCapture?: boolean;
}

export interface StreamSession {
  sessionId: string;
  serverAddress: string;
  authToken: string;
  clientPort?: number;
  rawSession?: any;
}

export interface StreamEvent {
  type: 'connected' | 'disconnected' | 'error' | 'stats-update' | 'quality-change' | 'stream-paused' | 'stream-resumed';
  data?: any;
  timestamp: number;
}

export interface UserInput {
  type: 'keyboard' | 'mouse' | 'gamepad';
  payload: any;
}
