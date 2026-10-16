import { randomUUID } from "node:crypto";
import { createWriteStream } from "node:fs";
import {
  copyFile,
  mkdir,
  readFile,
  rename,
  stat,
  unlink,
  writeFile,
} from "node:fs/promises";
import { basename, join } from "node:path";
import type {
  RecordingAbortRequest,
  RecordingBeginRequest,
  RecordingBeginResult,
  RecordingChunkRequest,
  RecordingDeleteRequest,
  RecordingEntry,
  RecordingFinishRequest,
  RecordingInstallRequest,
} from "@shared/gfn";
import {
  getGalleryRootDirectory,
  getGalleryTempDirectory,
  getManagedGalleryRoots,
  pruneGalleryDiskLimit,
} from "./gallery";
import { walkGalleryCaptureFiles } from "./galleryFiles";
import {
  buildCaptureBaseName,
  resolveUniqueCapturePath,
  sanitizeGameFolderName,
  thumbnailPathForCapture,
} from "./galleryNaming";
import { dataUrlToBuffer } from "./mediaFiles";
import { resolveTrustedOpenNowMediaPath } from "../mediaPaths";
import { probeMp4File } from "./mp4Probe";

/**
 * Listing safety cap only — retention is governed by the gallery disk budget
 * (`galleryDiskLimitGb`). This keeps a pathological folder from flooding the
 * IPC channel and renderer state.
 */
const RECORDING_LIST_LIMIT = 500;

const VIDEO_EXTENSION = /\.(mp4|webm)$/i;

interface ActiveRecording {
  writeStream: ReturnType<typeof createWriteStream>;
  tempPath: string;
  mimeType: string;
}

const activeRecordings = new Map<string, ActiveRecording>();

export function extFromMimeType(mimeType: string): ".mp4" | ".webm" {
  return mimeType.startsWith("video/mp4") ? ".mp4" : ".webm";
}

/** Legacy names: `{stamp}-{title}-{rand}[-dur{ms}].{ext}`. */
function parseLegacyRecordingName(fileName: string): {
  durationMs: number;
  gameTitle?: string;
} {
  const durMatch = /-dur(\d+)\.(mp4|webm)$/i.exec(fileName);
  const titleMatch =
    /^[^-]+-[^-]+-([^-]+(?:-[^-]+)*?)-[a-f0-9]{6}(?:-dur\d+)?\.(mp4|webm)$/i.exec(
      fileName,
    );
  return {
    durationMs: durMatch ? Number(durMatch[1]) : 0,
    gameTitle: titleMatch ? titleMatch[1].replace(/-/g, " ") : undefined,
  };
}

async function readThumbnailDataUrl(filePath: string): Promise<string | undefined> {
  try {
    const thumbBuf = await readFile(thumbnailPathForCapture(filePath));
    return `data:image/jpeg;base64,${thumbBuf.toString("base64")}`;
  } catch {
    return undefined;
  }
}

/**
 * Gallery listing, newest first. Both layouts are supported:
 *  - official per-game folders (`<root>/<Game>/…`) → title from the folder
 *  - legacy flat `Recordings` dirs → title/duration from the old file name
 * MP4 entries get duration and track info from the file itself (`moov`
 * probe); files without a readable video track (stub artifacts, audio-only
 * muxes from interrupted sessions) stay hidden instead of surfacing as
 * broken zero-second clips. WebM fallback clips keep the legacy `-dur{ms}`
 * name suffix — there is no cheap in-process probe for that container.
 */
export async function listRecordings(): Promise<RecordingEntry[]> {
  const files = (await walkGalleryCaptureFiles(getManagedGalleryRoots())).filter((file) =>
    VIDEO_EXTENSION.test(file.fileName),
  );

  const loaded = await Promise.all(
    files.map(async (file): Promise<RecordingEntry | null> => {
      try {
        let gameTitle = file.gameTitle;
        let durationMs = 0;
        if (file.fileName.toLowerCase().endsWith(".mp4")) {
          const probe = await probeMp4File(file.filePath);
          if (!probe || !probe.hasVideo || probe.durationMs <= 0) return null;
          durationMs = probe.durationMs;
        } else {
          const legacy = parseLegacyRecordingName(file.fileName);
          durationMs = legacy.durationMs;
          gameTitle = gameTitle ?? legacy.gameTitle;
        }

        return {
          id: file.filePath,
          fileName: file.fileName,
          filePath: file.filePath,
          createdAtMs: file.createdAtMs,
          sizeBytes: file.sizeBytes,
          durationMs,
          gameTitle,
          thumbnailDataUrl: await readThumbnailDataUrl(file.filePath),
        };
      } catch {
        return null;
      }
    }),
  );

  return loaded
    .filter((item): item is RecordingEntry => item !== null)
    .sort((a, b) => b.createdAtMs - a.createdAtMs)
    .slice(0, RECORDING_LIST_LIMIT);
}

export async function beginRecording(
  input: RecordingBeginRequest,
): Promise<RecordingBeginResult> {
  const tempDir = getGalleryTempDirectory();
  await mkdir(tempDir, { recursive: true });
  const recordingId = randomUUID();
  const ext = extFromMimeType(input.mimeType);
  const tempPath = join(tempDir, `${recordingId}${ext}.tmp`);
  const writeStream = createWriteStream(tempPath);
  activeRecordings.set(recordingId, {
    writeStream,
    tempPath,
    mimeType: input.mimeType,
  });
  return { recordingId };
}

export async function appendRecordingChunk(
  input: RecordingChunkRequest,
): Promise<void> {
  const rec = activeRecordings.get(input.recordingId);
  if (!rec) {
    throw new Error("Unknown recording id");
  }
  await new Promise<void>((resolve, reject) => {
    rec.writeStream.write(Buffer.from(input.chunk), (err) => {
      if (err) reject(err);
      else resolve();
    });
  });
}

/**
 * Native streamer handoff: the finalized MP4 already exists as a complete
 * file (written by the offline remux worker in its temp dir). Close the
 * (empty) chunk stream, then move the file into this recording's temp slot
 * so `finishRecording`'s rename to the final name works unchanged. The move
 * is copy + unlink — the native temp dir and the gallery root can live on
 * different volumes, where a plain rename would fail with EXDEV.
 */
export async function installRecordingFile(
  input: RecordingInstallRequest,
): Promise<void> {
  const rec = activeRecordings.get(input.recordingId);
  if (!rec) {
    throw new Error("Unknown recording id");
  }
  // Close the never-written chunk stream so the temp path can be replaced.
  await new Promise<void>((resolve, reject) => {
    rec.writeStream.end((err?: Error | null) => {
      if (err) reject(err);
      else resolve();
    });
  });
  await copyFile(input.sourcePath, rec.tempPath);
  // Best-effort: the source lives in the OS temp dir; if the unlink fails
  // the OS will reap it eventually.
  await unlink(input.sourcePath).catch(() => undefined);
}

export async function finishRecording(
  input: RecordingFinishRequest,
): Promise<RecordingEntry> {
  const rec = activeRecordings.get(input.recordingId);
  if (!rec) {
    throw new Error("Unknown recording id");
  }
  activeRecordings.delete(input.recordingId);

  // The native handoff (`installRecordingFile`) already ended the (empty)
  // chunk stream before copying the finalized file into the temp slot; on
  // the plain chunk path the stream is still open. Calling end() on an
  // already-finished stream throws ERR_STREAM_ALREADY_FINISHED and kills
  // the finish handler after the file was renamed — so only end what is
  // still writable.
  if (!rec.writeStream.writableEnded) {
    await new Promise<void>((resolve, reject) => {
      rec.writeStream.end((err?: Error | null) => {
        if (err) reject(err);
        else resolve();
      });
    });
  }

  const ext = extFromMimeType(rec.mimeType);
  const gameFolder = sanitizeGameFolderName(input.gameTitle);
  const dir = join(getGalleryRootDirectory(), gameFolder);
  await mkdir(dir, { recursive: true });

  // MP4s carry their duration in the `moov` box, so the official file name
  // needs no suffix. The WebM fallback keeps the legacy `-dur{ms}` marker so
  // its listing duration survives without a container probe.
  const durSuffix =
    ext === ".webm" && input.durationMs > 0
      ? `-dur${Math.round(input.durationMs)}`
      : "";
  const base = buildCaptureBaseName(input.gameTitle, new Date());
  const finalPath = await resolveUniqueCapturePath(dir, base, `${durSuffix}${ext}`);

  await rename(rec.tempPath, finalPath);

  // Save thumbnail if provided (best-effort — never fail the recording).
  let thumbnailDataUrl: string | undefined;
  if (input.thumbnailDataUrl) {
    try {
      const { buffer } = dataUrlToBuffer(input.thumbnailDataUrl);
      await writeFile(thumbnailPathForCapture(finalPath), buffer);
      thumbnailDataUrl = input.thumbnailDataUrl;
    } catch {
      // Thumbnail save is best-effort — don't fail the recording
    }
  }

  // Disk budget (replaces the old 20-item cap): prune the oldest captures
  // when the gallery exceeds the configured limit. Throws when the new file
  // alone cannot fit, and the renderer surfaces that as a save failure.
  await pruneGalleryDiskLimit(finalPath);

  const fileStats = await stat(finalPath);
  return {
    id: finalPath,
    fileName: basename(finalPath),
    filePath: finalPath,
    createdAtMs: fileStats.birthtimeMs || fileStats.mtimeMs || Date.now(),
    sizeBytes: fileStats.size,
    durationMs: input.durationMs,
    gameTitle: input.gameTitle,
    thumbnailDataUrl,
  };
}

export async function abortRecording(
  input: RecordingAbortRequest,
): Promise<void> {
  const rec = activeRecordings.get(input.recordingId);
  if (!rec) {
    return;
  }
  activeRecordings.delete(input.recordingId);
  rec.writeStream.destroy();
  await unlink(rec.tempPath).catch(() => undefined);
}

export async function deleteRecording(
  input: RecordingDeleteRequest,
): Promise<void> {
  const filePath = await resolveTrustedOpenNowMediaPath(input.filePath);
  if (!filePath) {
    throw new Error("Invalid recording path");
  }
  await unlink(filePath);
  await unlink(thumbnailPathForCapture(filePath)).catch(() => undefined);
}

export async function getRecordingFilePath(rawPath: string): Promise<string> {
  const filePath = await resolveTrustedOpenNowMediaPath(rawPath);
  if (!filePath) {
    throw new Error("Invalid recording path");
  }
  return filePath;
}
