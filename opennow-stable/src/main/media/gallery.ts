/**
 * Gallery root resolution plus the Electron-dependent thumbnail helpers.
 *
 * Roots are resolved lazily on every call so a settings change applies to the
 * next capture immediately. The legacy default root (`Pictures/OpenNOW`) is
 * always managed alongside the configured root: existing libraries keep
 * listing/playing/deleting even after the user moves the gallery, and
 * `mediaPaths` keeps trusting both for playback and previews.
 */
import { app, nativeImage } from "electron";
import { readFile, writeFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import { getSettingsManager } from "../settings";
import { thumbnailPathForCapture } from "./galleryNaming";
import { pruneGalleryToLimit } from "./galleryFiles";

/** Legacy and fallback root — also the parent of the legacy flat dirs. */
export function getDefaultGalleryRootDirectory(): string {
  return join(app.getPath("pictures"), "OpenNOW");
}

/** Configured gallery root; empty setting keeps the default. */
export function getGalleryRootDirectory(): string {
  const raw = getSettingsManager().get("galleryDirectory");
  const trimmed = typeof raw === "string" ? raw.trim() : "";
  return trimmed ? resolve(trimmed) : getDefaultGalleryRootDirectory();
}

/** In-flight recording temps; never listed and reaped at startup. */
export function getGalleryTempDirectory(): string {
  return join(getGalleryRootDirectory(), ".tmp");
}

/** Deduped roots whose captures the app writes, serves and counts. */
export function getManagedGalleryRoots(): string[] {
  const seen = new Set<string>();
  return [getGalleryRootDirectory(), getDefaultGalleryRootDirectory()].filter((root) => {
    const key = process.platform === "win32" ? root.toLowerCase() : root;
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}

/** Configured disk budget in bytes; 0 = unlimited. */
export function getGalleryDiskLimitBytes(): number {
  const gb = getSettingsManager().get("galleryDiskLimitGb");
  const value = typeof gb === "number" && Number.isFinite(gb) && gb > 0 ? gb : 0;
  return value > 0 ? value * 1024 ** 3 : 0;
}

/**
 * Enforce the gallery disk budget after a save. Prunes the oldest captures
 * first; the just-saved file (when given) is the last thing removed, and if
 * even it cannot fit the underlying `GalleryDiskLimitError` propagates so the
 * save is reported as failed instead of silently dropping the capture.
 */
export async function pruneGalleryDiskLimit(keepFilePath: string | null): Promise<void> {
  const limitBytes = getGalleryDiskLimitBytes();
  if (limitBytes <= 0) return;
  const result = await pruneGalleryToLimit(getManagedGalleryRoots(), limitBytes, keepFilePath);
  if (result.prunedFiles > 0) {
    console.log(
      `[gallery] disk budget: pruned ${result.prunedFiles} capture(s), freed ${Math.round(
        result.freedBytes / (1024 * 1024),
      )} MB`,
    );
  }
}

export const GALLERY_THUMBNAIL_WIDTH = 480;

/**
 * Small JPEG data URL for a gallery image: sidecar first, then a fresh resize
 * from the source (nativeImage handles PNG/JPEG in the main process). Returns
 * `null` when the image cannot be decoded (e.g. WebP) so the caller can fall
 * back to the raw bytes.
 */
export async function buildScreenshotThumbnailDataUrl(
  filePath: string,
): Promise<string | null> {
  const sidecar = thumbnailPathForCapture(filePath);
  try {
    const cached = await readFile(sidecar);
    return `data:image/jpeg;base64,${cached.toString("base64")}`;
  } catch {
    // No sidecar yet — generate + persist one below.
  }
  try {
    const source = await readFile(filePath);
    const image = nativeImage.createFromBuffer(source);
    if (image.isEmpty()) return null;
    const jpeg =
      image.getSize().width > GALLERY_THUMBNAIL_WIDTH
        ? image.resize({ width: GALLERY_THUMBNAIL_WIDTH }).toJPEG(72)
        : image.toJPEG(72);
    if (!jpeg.length) return null;
    await writeFile(sidecar, jpeg).catch(() => undefined);
    return `data:image/jpeg;base64,${jpeg.toString("base64")}`;
  } catch {
    return null;
  }
}

/**
 * Sidecar thumbnail straight from freshly captured image bytes; returns the
 * data URL for the new entry (or null when the bytes are not decodable).
 */
export async function writeScreenshotThumbnailSidecar(
  filePath: string,
  imageBytes: Buffer,
): Promise<string | null> {
  try {
    const image = nativeImage.createFromBuffer(imageBytes);
    if (image.isEmpty()) return null;
    const jpeg =
      image.getSize().width > GALLERY_THUMBNAIL_WIDTH
        ? image.resize({ width: GALLERY_THUMBNAIL_WIDTH }).toJPEG(72)
        : image.toJPEG(72);
    if (!jpeg.length) return null;
    await writeFile(thumbnailPathForCapture(filePath), jpeg);
    return `data:image/jpeg;base64,${jpeg.toString("base64")}`;
  } catch {
    return null;
  }
}
