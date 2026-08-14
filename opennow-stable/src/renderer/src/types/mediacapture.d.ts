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
