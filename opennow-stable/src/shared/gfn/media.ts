export interface ScreenshotSaveRequest {
  dataUrl: string;
  gameTitle?: string;
}

export interface ScreenshotDeleteRequest {
  id: string;
}

export interface ScreenshotSaveAsRequest {
  id: string;
}

export interface ScreenshotSaveAsResult {
  saved: boolean;
  filePath?: string;
}

export interface ScreenshotEntry {
  id: string;
  fileName: string;
  filePath: string;
  createdAtMs: number;
  sizeBytes: number;
  dataUrl: string;
}

export interface RecordingEntry {
  id: string;
  fileName: string;
  filePath: string;
  createdAtMs: number;
  sizeBytes: number;
  durationMs: number;
  gameTitle?: string;
  thumbnailDataUrl?: string;
}

export interface RecordingBeginRequest {
  mimeType: string;
}

export interface RecordingBeginResult {
  recordingId: string;
}

export interface RecordingChunkRequest {
  recordingId: string;
  chunk: ArrayBuffer;
}

/**
 * Native streamer handoff: the finalized MP4 already exists as a complete
 * file (written by the offline remux worker). Electron installs it into the
 * recording's temp slot so `finishRecording`'s rename to the final name
 * works unchanged — replacing the chunk-append path.
 */
export interface RecordingInstallRequest {
  recordingId: string;
  /** Absolute path of the completed MP4 in the native streamer's temp dir. */
  sourcePath: string;
}

export interface RecordingFinishRequest {
  recordingId: string;
  durationMs: number;
  gameTitle?: string;
  thumbnailDataUrl?: string;
}

export interface RecordingAbortRequest {
  recordingId: string;
}

export interface RecordingDeleteRequest {
  id: string;
}

export interface MediaListingEntry {
  id: string;
  fileName: string;
  filePath: string;
  createdAtMs: number;
  sizeBytes: number;
  gameTitle?: string;
  durationMs?: number;
  thumbnailDataUrl?: string;
  dataUrl?: string;
}

export interface MediaListingResult {
  screenshots: MediaListingEntry[];
  videos: MediaListingEntry[];
}
