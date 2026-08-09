/**
 * Shared handlers for GFN's server-created `control_channel` data channel.
 *
 * Both transports route control-channel messages through the renderer-side
 * `nativeDataChannelRegistry`:
 *  - web mode: `GfnWebRtcClient.onControlChannelMessage` normalizes the raw
 *    data channel payload and dispatches it to the registry;
 *  - native mode: the native streamer relays `control_channel` verbatim and
 *    the main process re-emits it as `native-data-channel-message`, which
 *    dispatches to the same registry.
 *
 * Features register here once and work on both transports — the only
 * transport-specific bit is the `sendReply` callback handed in by the caller.
 */

import {
  CLIPBOARD_CLIENT_DATA_RESPONSE,
  CLIPBOARD_CLIENT_REMOVED_DATA,
  buildClipboardControlMessage,
  isClipboardServerDataRequest,
  parseClipboardControlMessage,
  validateClipboardText,
} from "./clipboardProtocol";
import { decodeBase64Utf8, encodeBase64Utf8 } from "../../lib/streamSessionHelpers";
import {
  registerNativeDataChannelHandler,
  type NativeDataChannelMessage,
} from "../../lib/nativeDataChannelRegistry";
import type { StreamTimeWarning } from "./webrtc/streamDiagnosticsTypes";

/** GFN control channel label the server creates (clipboard protocol + more). */
export const GFN_CONTROL_CHANNEL_LABEL = "control_channel";

/**
 * Transport-specific clipboard answer wiring. `sendReply` receives the
 * base64-encoded reply JSON and must deliver it back on the control channel
 * (web: `RTCDataChannel.send`; native: `sendNativeDataChannelMessage`).
 */
export interface ClipboardControlChannelHandlerOptions {
  /** Dynamic gate, evaluated per message (mirrors the clipboardPaste setting). */
  enabled: () => boolean;
  /** Clipboard payload cap in bytes. */
  maxBytes: number;
  /** Reads the host clipboard; returns ""/rejects → treated as no data. */
  readClipboardText: () => Promise<string>;
  /** Transport-specific reply sender (base64-encoded reply JSON). */
  sendReply: (payloadBase64: string) => void | Promise<void>;
  /** Called after answering (web keeps lastAdvertisedClipboardAvailable in sync). */
  onAnswered?: (text: string | null) => void;
}

async function handleClipboardControlMessage(
  message: NativeDataChannelMessage,
  options: ClipboardControlChannelHandlerOptions,
): Promise<void> {
  const payloadText = decodeBase64Utf8(message.payloadBase64);
  let parsed: unknown;
  try {
    parsed = JSON.parse(payloadText);
  } catch {
    return;
  }

  const clipboardPayload = parseClipboardControlMessage(parsed);
  if (!clipboardPayload || !isClipboardServerDataRequest(clipboardPayload)) {
    return;
  }

  try {
    const text = validateClipboardText(await options.readClipboardText(), options.maxBytes);
    const reply = buildClipboardControlMessage(
      text ? CLIPBOARD_CLIENT_DATA_RESPONSE : CLIPBOARD_CLIENT_REMOVED_DATA,
      { text, tracingData: clipboardPayload.tracingData },
    );
    await options.sendReply(encodeBase64Utf8(JSON.stringify(reply)));
    options.onAnswered?.(text);
  } catch (error) {
    console.warn("[NativeStreamer] Failed to answer clipboard request:", error);
  }
}

/**
 * Register the clipboard `SERVER_DATA_REQUEST` answerer on `control_channel`.
 * Shared by web (client registers when the control channel opens) and native
 * (useSignalingEvents registers per session). Returns an unregister function.
 */
export function installClipboardControlChannelHandler(
  options: ClipboardControlChannelHandlerOptions,
): () => void {
  return registerNativeDataChannelHandler(GFN_CONTROL_CHANNEL_LABEL, (message) => {
    if (!options.enabled()) {
      return;
    }
    void handleClipboardControlMessage(message, options);
  });
}

/**
 * Mirrors official client behavior from timerNotification -> StreamWarningType.
 */
export function mapTimerNotificationCode(rawCode: number): StreamTimeWarning["code"] | null {
  if (rawCode === 1 || rawCode === 2) {
    return 1;
  }
  if (rawCode === 4) {
    return 2;
  }
  if (rawCode === 6) {
    return 3;
  }
  return null;
}

export interface TimerNotificationHandlerOptions {
  onTimeWarning?: (warning: StreamTimeWarning) => void;
  log?: (line: string) => void;
}

/**
 * Register the `timerNotification` handler on `control_channel` (session timer
 * warnings). Currently wired by the web client; native can reuse it by passing
 * an `onTimeWarning` callback without touching the native streamer.
 */
export function installTimerNotificationHandler(
  options: TimerNotificationHandlerOptions,
): () => void {
  return registerNativeDataChannelHandler(GFN_CONTROL_CHANNEL_LABEL, (message) => {
    const payloadText = decodeBase64Utf8(message.payloadBase64);
    let parsed: unknown;
    try {
      parsed = JSON.parse(payloadText);
    } catch {
      return;
    }

    if (!parsed || typeof parsed !== "object" || !("timerNotification" in parsed)) {
      return;
    }

    const timerNotification = (parsed as { timerNotification?: unknown }).timerNotification;
    if (!timerNotification || typeof timerNotification !== "object") {
      return;
    }

    const rawCode = Number((timerNotification as { code?: unknown }).code);
    const mappedCode = mapTimerNotificationCode(rawCode);
    if (mappedCode === null) {
      options.log?.(`Control timer notification ignored: code=${rawCode}`);
      return;
    }

    const rawSecondsLeft = Number((timerNotification as { secondsLeft?: unknown }).secondsLeft);
    const secondsLeft =
      Number.isFinite(rawSecondsLeft) && rawSecondsLeft >= 0
        ? Math.floor(rawSecondsLeft)
        : undefined;
    options.log?.(
      `Control timer warning: rawCode=${rawCode} mappedCode=${mappedCode} secondsLeft=${secondsLeft ?? "n/a"}`,
    );
    options.onTimeWarning?.({ code: mappedCode, secondsLeft });
  });
}
