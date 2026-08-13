// Ambient types for APIs Chromium ships but TS 6.0's DOM lib lacks.
//
// MediaStreamTrackGenerator is the mediacapture-transform entry point that
// turns WebCodecs frames written to a WritableStream into a MediaStreamTrack
// a MediaRecorder can consume. FileReaderSync is worker-only, so TS declares
// it in lib.webworker, which this DOM project does not pull in.
interface MediaStreamTrackGeneratorInit {
  kind: "video" | "audio";
}

declare class MediaStreamTrackGenerator {
  constructor(init: MediaStreamTrackGeneratorInit);
  /** VideoFrames written here are exposed as a track on `readable`. */
  readonly writable: WritableStream<VideoFrame>;
  readonly readable: MediaStreamTrack;
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
