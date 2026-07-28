import {
  StreamingEngineType,
  StreamStats,
  StreamConfig,
  StreamSession,
  StreamEvent,
  UserInput,
} from './streamingTypes';

export abstract class StreamingEngine {
  protected config: StreamConfig;
  protected session: StreamSession | null = null;
  protected isRunning = false;
  protected listeners: Map<string, Function[]> = new Map();

  constructor(config: StreamConfig) {
    this.config = config;
  }

  abstract initialize(): Promise<void>;
  abstract connect(session: StreamSession): Promise<void>;
  abstract disconnect(): Promise<void>;
  abstract getStreamRenderer(): HTMLElement | null;
  abstract getStats(): StreamStats;
  abstract updateConfig(newConfig: Partial<StreamConfig>): Promise<void>;
  abstract sendInput(input: UserInput): void;
  abstract isHealthy(): boolean;
  abstract handleSignalingEvent(event: any): Promise<void>;

  /**
   * Built-in event listener management
   */
  on(eventType: string, callback: (event: StreamEvent) => void): () => void {
    if (!this.listeners.has(eventType)) {
      this.listeners.set(eventType, []);
    }
    this.listeners.get(eventType)!.push(callback);

    return () => {
      const cbs = this.listeners.get(eventType)!;
      const idx = cbs.indexOf(callback);
      if (idx !== -1) {
        cbs.splice(idx, 1);
      }
    };
  }

  protected emit(eventType: string, data?: any): void {
    const event: StreamEvent = {
      type: eventType as any,
      data,
      timestamp: Date.now(),
    };
    this.listeners.get(eventType)?.forEach(cb => cb(event));
  }

  getEngineType(): StreamingEngineType {
    return this.config.type;
  }
}
