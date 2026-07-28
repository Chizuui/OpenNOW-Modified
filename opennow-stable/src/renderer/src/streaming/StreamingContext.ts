import { StreamingEngine } from './StreamingEngine';
import { StreamConfig, StreamSession } from './streamingTypes';

export class StreamingContext {
  private engine: StreamingEngine | null = null;
  private config: StreamConfig;
  private session: StreamSession | null = null;
  private listeners: Map<string, Function[]> = new Map();

  constructor(initialConfig: StreamConfig) {
    this.config = initialConfig;
  }

  /**
   * Set or swap the streaming engine at runtime
   */
  async setEngine(engine: StreamingEngine): Promise<void> {
    if (this.engine) {
      try {
        await this.engine.disconnect();
      } catch (err) {
        console.warn('Error disconnecting previous engine:', err);
      }
    }

    this.engine = engine;
    this.emit('engine-changed', { engineType: engine.getEngineType() });
  }

  /**
   * Connect with current engine
   */
  async connect(session: StreamSession): Promise<void> {
    if (!this.engine) {
      throw new Error('No streaming engine configured');
    }

    try {
      this.session = session;
      await this.engine.connect(session);
      this.emit('connected', { session });
    } catch (err) {
      this.emit('error', { error: err, stage: 'connect' });
      throw err;
    }
  }

  /**
   * Disconnect and cleanup
   */
  async disconnect(): Promise<void> {
    if (!this.engine) return;

    try {
      await this.engine.disconnect();
      this.session = null;
      this.emit('disconnected', {});
    } catch (err) {
      this.emit('error', { error: err, stage: 'disconnect' });
      throw err;
    }
  }

  /**
   * Listener management
   */
  on(event: string, callback: Function): () => void {
    if (!this.listeners.has(event)) {
      this.listeners.set(event, []);
    }
    this.listeners.get(event)!.push(callback);

    return () => {
      const cbs = this.listeners.get(event)!;
      const idx = cbs.indexOf(callback);
      if (idx !== -1) {
        cbs.splice(idx, 1);
      }
    };
  }

  private emit(event: string, data: any): void {
    this.listeners.get(event)?.forEach(cb => cb(data));
  }

  getEngine(): StreamingEngine | null {
    return this.engine;
  }

  getSession(): StreamSession | null {
    return this.session;
  }

  getConfig(): StreamConfig {
    return this.config;
  }

  updateConfig(newConfig: Partial<StreamConfig>): void {
    this.config = { ...this.config, ...newConfig };
    this.emit('config-updated', { config: this.config });
  }
}
