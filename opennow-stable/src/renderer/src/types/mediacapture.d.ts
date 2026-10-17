// Ambient types for APIs Chromium ships but TS 6.0's DOM lib lacks.
//
// MediaStreamTrackGenerator is the mediacapture-transform entry point that
// turns WebCodecs frames written to a WritableStream into a MediaStreamTrack
// a MediaRecorder can consume. FileReaderSync is worker-only, so TS declares
// it in lib.webworker, which this DOM project does not pull in.
interface MediaStreamTrackGeneratorInit {
  kind: "video" | "audio";
}

// Chromium's current IDL (unchanged since ~M100) makes the generator itself
// the track: `interface MediaStreamTrackGenerator : MediaStreamTrack` — the
// object you construct IS the MediaStreamTrack, so it can be handed straight
// to `new MediaStream([...])`. The legacy `readable` accessor (Chrome 94-99
// era, pre-M100) was removed years ago; it is declared as an optional member
// only so runtime code can still probe for both API shapes.
declare class MediaStreamTrackGenerator extends MediaStreamTrack {
  constructor(init: MediaStreamTrackGeneratorInit);
  /** VideoFrames written here are pushed into the generator's own track. */
  readonly writable: WritableStream<VideoFrame>;
  /** Legacy (removed in Chromium ~M100): older runtimes exposed the track here. */
  readonly readable?: MediaStreamTrack;
}

// MediaStreamTrackProcessor (mediacapture-transform) reads a live track
// through a ReadableStream with live-edge drop semantics: stale frames are
// dropped for a slow consumer and the track is never backpressured. The
// WebCodecs recorder uses it to ingest the live stream without touching the
// decode pipeline.
interface MediaStreamTrackProcessorInit {
  track: MediaStreamTrack;
  /** Frames held before live-edge drops kick in (default 1). */
  maxBufferSize?: number;
}

declare class MediaStreamTrackProcessor<T = VideoFrame> {
  constructor(init: MediaStreamTrackProcessorInit);
  readonly readable: ReadableStream<T>;
}

interface FileReaderSync {
  readonly error: DOMException | null;
  readAsArrayBuffer(blob: Blob): ArrayBuffer;
  readAsBinaryString(blob: Blob): string;
  readAsDataURL(blob: Blob): string;
  readAsText(blob: Blob, encoding?: string): string;
}

declare var FileReaderSync: {
  prototype: FileReaderSync;
  new (): FileReaderSync;
};
