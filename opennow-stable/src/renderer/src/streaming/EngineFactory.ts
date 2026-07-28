import { StreamingEngine } from './StreamingEngine';
import { StreamConfig, StreamingEngineType } from './streamingTypes';
import { WebRTCEngine } from './webrtc/WebRTCEngine';
import { NativeEngine } from './native/NativeEngine';
import { GfnWebRtcClient } from '../gfn/webrtcClient';

export class EngineFactory {
  /**
   * Create and initialize appropriate streaming engine.
   * Tries Native first, falls back to WebRTC on error.
   */
  static async create(config: StreamConfig, client: GfnWebRtcClient, nativeOptions?: any): Promise<StreamingEngine> {
    if (config.type === StreamingEngineType.Native) {
      try {
        const engine = new NativeEngine(config, client, nativeOptions);
        await engine.initialize();
        return engine;
      } catch (err) {
        console.warn('[EngineFactory] Native engine init failed, falling back to WebRTC:', err);
        // Fall through to WebRTC
      }
    }

    // Default to WebRTC
    const engine = new WebRTCEngine(config, client);
    await engine.initialize();
    return engine;
  }

  /**
   * Validate config for specific engine type
   */
  static validateConfig(config: StreamConfig): boolean {
    if (config.type === StreamingEngineType.Native) {
      const supportedCodecs = ['h264', 'h265'];
      if (!supportedCodecs.includes(config.codec)) {
        console.warn(
          `[EngineFactory] Native engine may not support ${config.codec}`
        );
      }
    }

    const validResolutions = ['1080p', '1440p', '4k'];
    if (!validResolutions.includes(config.resolution)) {
      // Don't throw for other custom resolutions since GFN can dynamically support others
      console.warn(`[EngineFactory] Uncommon resolution: ${config.resolution}`);
    }

    return true;
  }
}
