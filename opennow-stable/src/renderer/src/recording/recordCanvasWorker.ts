/**
 * Recording downscale worker.
 *
 * The canvas-downscale recording path used to run `drawImage` into a DOM
 * canvas on the renderer main thread — the same thread that runs the WebRTC
 * decoder — which stuttered the stream on weak devices. This worker owns the
 * downscale instead: the main thread hands it a GPU-backed VideoFrame, it
 * draws into an OffscreenCanvas at record resolution and returns a
 * GPU-backed ImageBitmap, which the main thread feeds to a
 * MediaStreamTrackGenerator consumed by MediaRecorder. The only main-thread
 * work left is the frame handoff — no synchronous drawImage, no canvas
 * rasterize timer.
 *
 * The thumbnail is produced here too (a tiny OffscreenCanvas + JPEG encode),
 * so the first chunk doesn't pay for a full-res draw on the main thread.
 */

export type RecordCanvasWorkerInitMessage = {
  type: "init";
  width: number;
  height: number;
  thumbWidth: number;
  thumbHeight: number;
};

export type RecordCanvasWorkerFrameMessage = {
  type: "frame";
  frame: VideoFrame;
  /** Microsecond timestamp from the main thread, echoed back with the bitmap. */
  timestamp: number;
};

export type RecordCanvasWorkerInboundMessage =
  | RecordCanvasWorkerInitMessage
  | RecordCanvasWorkerFrameMessage;

export type RecordCanvasWorkerBitmapMessage = {
  type: "bitmap";
  bitmap: ImageBitmap;
  timestamp: number;
};

export type RecordCanvasWorkerThumbMessage = {
  type: "thumb";
  dataUrl: string;
};

export type RecordCanvasWorkerOutboundMessage =
  | RecordCanvasWorkerBitmapMessage
  | RecordCanvasWorkerThumbMessage;

// Minimal worker-global typing: the DOM lib's `self` (Window) is a lie inside
// a dedicated worker, and pulling lib.webworker into this DOM project would
// collide with the DOM declarations.
declare const self: {
  onmessage: ((event: MessageEvent<RecordCanvasWorkerInboundMessage>) => void) | null;
  postMessage(message: RecordCanvasWorkerOutboundMessage, transfer: Transferable[]): void;
};

let canvas: OffscreenCanvas | null = null;
let ctx: OffscreenCanvasRenderingContext2D | null = null;
let thumbCanvas: OffscreenCanvas | null = null;
let thumbCtx: OffscreenCanvasRenderingContext2D | null = null;
let thumbSent = false;

self.onmessage = (event: MessageEvent<RecordCanvasWorkerInboundMessage>) => {
  const message = event.data;

  if (message.type === "init") {
    canvas = new OffscreenCanvas(message.width, message.height);
    ctx = canvas.getContext("2d");
    thumbCanvas = new OffscreenCanvas(message.thumbWidth, message.thumbHeight);
    thumbCtx = thumbCanvas.getContext("2d");
    thumbSent = false;
    return;
  }

  if (message.type === "frame") {
    const frame = message.frame;
    if (ctx && canvas) {
      ctx.drawImage(frame, 0, 0, canvas.width, canvas.height);
      // transferToImageBitmap detaches the canvas bitmap (it becomes blank),
      // so the thumbnail must come from the frame, not the canvas.
      const bitmap = canvas.transferToImageBitmap();
      self.postMessage(
        { type: "bitmap", bitmap, timestamp: message.timestamp } satisfies RecordCanvasWorkerBitmapMessage,
        [bitmap],
      );
      if (!thumbSent && thumbCtx && thumbCanvas) {
        thumbCtx.drawImage(frame, 0, 0, thumbCanvas.width, thumbCanvas.height);
        thumbCanvas
          .convertToBlob({ type: "image/jpeg", quality: 0.72 })
          .then((blob) => {
            const dataUrl = new FileReaderSync().readAsDataURL(blob);
            self.postMessage({ type: "thumb", dataUrl } satisfies RecordCanvasWorkerThumbMessage, []);
          })
          .catch(() => undefined);
        thumbSent = true;
      }
    }
    frame.close();
  }
};
