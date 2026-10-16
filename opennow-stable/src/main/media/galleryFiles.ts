/**
 * Filesystem-side gallery helpers shared by recordings, screenshots and the
 * disk-budget pass.
 *
 * Understands both layouts under the managed roots (the configured gallery
 * root plus the legacy `Pictures/OpenNOW` root):
 *   - official per-game folders: `<root>/<Game>/<capture files>`
 *   - legacy flat dirs: `<root>/Recordings/*` and `<root>/Screenshots/*`
 *
 * Also prunes the oldest captures when the disk budget is exceeded and reaps
 * temp files left behind by crashed recordings. Deliberately free of Electron
 * imports so it can run (and be tested) against plain temp directories.
 */
import { readdir, stat, unlink } from "node:fs/promises";
import type { Dirent } from "node:fs";
import { join, resolve } from "node:path";
import { thumbnailPathForCapture } from "./galleryNaming";

const VIDEO_FILE = /\.(mp4|webm)$/i;
const IMAGE_FILE = /\.(png|jpe?g|webp)$/i;
const THUMBNAIL_FILE = /-thumb\.jpe?g$/i;

export interface GalleryCaptureFile {
  filePath: string;
  fileName: string;
  /** Per-game folder name in the official layout; undefined for legacy dirs. */
  gameTitle?: string;
  /** True when the file lives in a legacy `Recordings`/`Screenshots` dir. */
  legacy: boolean;
  sizeBytes: number;
  /** Birth time when the filesystem provides it, modification time otherwise. */
  createdAtMs: number;
  mtimeMs: number;
}

export interface GalleryPruneResult {
  prunedFiles: number;
  freedBytes: number;
}

export interface GalleryUsage {
  usedBytes: number;
  fileCount: number;
}

/** The just-saved capture does not fit the disk budget even after pruning. */
export class GalleryDiskLimitError extends Error {
  constructor() {
    super("Gallery disk limit reached");
    this.name = "GalleryDiskLimitError";
  }
}

async function readDirSafe(dir: string): Promise<Dirent[]> {
  try {
    return await readdir(dir, { withFileTypes: true });
  } catch {
    return [];
  }
}

function isCaptureName(name: string): boolean {
  if (THUMBNAIL_FILE.test(name)) return false;
  return VIDEO_FILE.test(name) || IMAGE_FILE.test(name);
}

async function collectDirFiles(
  dir: string,
  legacy: boolean,
  gameTitle: string | undefined,
  out: GalleryCaptureFile[],
): Promise<void> {
  for (const entry of await readDirSafe(dir)) {
    if (!entry.isFile() || !isCaptureName(entry.name)) continue;
    const filePath = join(dir, entry.name);
    try {
      const stats = await stat(filePath);
      if (!stats.isFile()) continue;
      out.push({
        filePath,
        fileName: entry.name,
        gameTitle,
        legacy,
        sizeBytes: stats.size,
        createdAtMs: stats.birthtimeMs || stats.mtimeMs,
        mtimeMs: stats.mtimeMs,
      });
    } catch {
      // Unreadable file: skip — the gallery must not fail on one bad entry.
    }
  }
}

/**
 * Every capture file under the managed roots. Dot-directories (`.tmp`) are
 * skipped; the legacy `Recordings`/`Screenshots` dirs are scanned as legacy
 * entries and never treated as game folders.
 */
export async function walkGalleryCaptureFiles(
  roots: readonly string[],
): Promise<GalleryCaptureFile[]> {
  const out: GalleryCaptureFile[] = [];
  for (const root of roots) {
    for (const entry of await readDirSafe(root)) {
      if (!entry.isDirectory()) continue;
      const name = entry.name;
      if (!name || name.startsWith(".")) continue;
      const dir = join(root, name);
      if (name === "Recordings" || name === "Screenshots") {
        await collectDirFiles(dir, true, undefined, out);
      } else {
        await collectDirFiles(dir, false, name, out);
      }
    }
  }
  return out;
}

async function sidecarThumbnailSize(filePath: string): Promise<number> {
  try {
    return (await stat(thumbnailPathForCapture(filePath))).size;
  } catch {
    return 0;
  }
}

async function removeCaptureFile(file: GalleryCaptureFile): Promise<void> {
  await unlink(file.filePath).catch(() => undefined);
  await unlink(thumbnailPathForCapture(file.filePath)).catch(() => undefined);
}

function samePath(a: string, b: string): boolean {
  const [pa, pb] = [resolve(a), resolve(b)];
  return process.platform === "win32"
    ? pa.toLowerCase() === pb.toLowerCase()
    : pa === pb;
}

/**
 * Delete oldest captures until the managed roots fit `limitBytes`.
 *
 * The just-saved file (when provided) is kept as long as possible; if even it
 * does not fit, it is removed and `GalleryDiskLimitError` is thrown so the
 * caller can surface a clear "disk limit" failure instead of silently losing
 * the capture.
 */
export async function pruneGalleryToLimit(
  roots: readonly string[],
  limitBytes: number,
  keepFilePath: string | null,
): Promise<GalleryPruneResult> {
  const result: GalleryPruneResult = { prunedFiles: 0, freedBytes: 0 };
  if (!Number.isFinite(limitBytes) || limitBytes <= 0) return result;

  const files = await walkGalleryCaptureFiles(roots);
  let total = files.reduce((sum, file) => sum + file.sizeBytes, 0);
  for (const file of files) total += await sidecarThumbnailSize(file.filePath);
  if (total <= limitBytes) return result;

  files.sort((a, b) => a.mtimeMs - b.mtimeMs);
  for (const file of files) {
    if (total <= limitBytes) return result;
    if (keepFilePath && samePath(file.filePath, keepFilePath)) continue;
    const freed = file.sizeBytes + (await sidecarThumbnailSize(file.filePath));
    await removeCaptureFile(file);
    total -= freed;
    result.prunedFiles += 1;
    result.freedBytes += freed;
  }

  if (total > limitBytes && keepFilePath) {
    const keep = files.find((file) => samePath(file.filePath, keepFilePath));
    if (keep) {
      await removeCaptureFile(keep);
      result.prunedFiles += 1;
      result.freedBytes += keep.sizeBytes + (await sidecarThumbnailSize(keep.filePath));
    }
    throw new GalleryDiskLimitError();
  }

  return result;
}

/**
 * Total bytes (captures + sidecar thumbnails) under the managed roots.
 */
export async function getGalleryUsage(roots: readonly string[]): Promise<GalleryUsage> {
  const files = await walkGalleryCaptureFiles(roots);
  let usedBytes = files.reduce((sum, file) => sum + file.sizeBytes, 0);
  for (const file of files) usedBytes += await sidecarThumbnailSize(file.filePath);
  return { usedBytes, fileCount: files.length };
}

/**
 * Reap temp files left behind by recordings that crashed or were killed
 * mid-write. Runs at app start (nothing is active then); in-flight recordings
 * live in `<root>/.tmp` and are never listed, so stale files are invisible
 * until they are removed here.
 */
export async function cleanupStaleGalleryTemps(roots: readonly string[]): Promise<number> {
  let removed = 0;
  for (const root of roots) {
    const dirs = [join(root, ".tmp"), join(root, "Recordings")];
    for (const dir of dirs) {
      for (const entry of await readDirSafe(dir)) {
        if (!entry.isFile() || !entry.name.endsWith(".tmp")) continue;
        try {
          await unlink(join(dir, entry.name));
          removed += 1;
        } catch {
          // Best effort: a locked temp file is retried on the next start.
        }
      }
    }
  }
  return removed;
}
