/**
 * Renderer-side registry for server-initiated messages relayed from the native
 * streamer's remote WebRTC data channels. The native streamer relays every
 * non-native channel verbatim (`data-channel-message` → main → renderer), so
 * features can register a per-label handler here without touching the native
 * streamer — GFN's clipboard `control_channel`, notifications, timers, etc.
 *
 * Handlers receive the decoded (base64) payload and may reply asynchronously
 * via `window.openNow.sendNativeDataChannelMessage(label, payloadBase64)`.
 */

export interface NativeDataChannelMessage {
  label: string;
  payloadBase64: string;
}

export type NativeDataChannelHandler = (message: NativeDataChannelMessage) => void;

const REGISTERED_HANDLERS = new Map<string, Set<NativeDataChannelHandler>>();

/**
 * Register a handler for messages on the given remote data channel label.
 * Returns an unregister function. Multiple handlers per label are allowed and
 * all run for each message.
 */
export function registerNativeDataChannelHandler(
  label: string,
  handler: NativeDataChannelHandler,
): () => void {
  let handlers = REGISTERED_HANDLERS.get(label);
  if (!handlers) {
    handlers = new Set();
    REGISTERED_HANDLERS.set(label, handlers);
  }
  handlers.add(handler);
  return () => {
    handlers!.delete(handler);
    if (handlers!.size === 0) {
      REGISTERED_HANDLERS.delete(label);
    }
  };
}

/** Route one relayed native data channel message to its registered handlers. */
export function dispatchNativeDataChannelMessage(message: NativeDataChannelMessage): void {
  const handlers = REGISTERED_HANDLERS.get(message.label);
  if (!handlers) {
    return;
  }
  for (const handler of handlers) {
    try {
      handler(message);
    } catch (error) {
      console.warn(
        `[NativeStreamer] Data channel handler for "${message.label}" threw:`,
        error,
      );
    }
  }
}

/** Whether any handler is registered for the label (handy for diagnostics). */
export function hasNativeDataChannelHandler(label: string): boolean {
  return (REGISTERED_HANDLERS.get(label)?.size ?? 0) > 0;
}
