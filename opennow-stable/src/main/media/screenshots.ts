import { app, type BrowserWindow } from "electron";
import { basename, join } from "node:path";
import { copyFile, mkdir, readFile, stat, unlink, writeFile } from "node:fs/promises";
import type {
  ScreenshotDeleteRequest,
  ScreenshotEntry,
  ScreenshotSaveAsRequest,
  ScreenshotSaveAsResult,
  ScreenshotSaveRequest,
} from "@shared/gfn";
import {
  buildScreenshotThumbnailDataUrl,
  getGalleryRootDirectory,
  getManagedGalleryRoots,
  pruneGalleryDiskLimit,
  writeScreenshotThumbnailSidecar,
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

/**
 * Listing safety cap only — retention is governed by the gallery disk budget
 * (`galleryDiskLimitGb`).
 */
const SCREENSHOT_LIST_LIMIT = 500;

const IMAGE_EXTENSION = /\.(png|jpe?g|webp)$/i;

export interface ScreenshotSaveAsDeps {
  dialog: Electron.Dialog;
  getMainWindow(): BrowserWindow | null;
}

export function buildScreenshotDataUrl(ext: string, buffer: Buffer): string {
  const mime =
    ext === "jpg" || ext === "jpeg"
      ? "image/jpeg"
      : ext === "webp"
        ? "image/webp"
        : "image/png";
  return `data:${mime};base64,${buffer.toString("base64")}`;
}

/**
 * Full-resolution data URL for a gallery screenshot — used by the preview
 * panel (one image at a time) and as the decode fallback for formats
 * `nativeImage` cannot resize (WebP).
 */
export async function readScreenshotDataUrl(rawPath: string): Promise<string | null> {
  const filePath = await resolveTrustedOpenNowMediaPath(rawPath);
  if (!filePath || !IMAGE_EXTENSION.test(filePath)) return null;
  try {
    const buffer = await readFile(filePath);
    const ext = /\.([^.]+)$/.exec(filePath)?.[1]?.toLowerCase() ?? "png";
    return buildScreenshotDataUrl(ext, buffer);
  } catch {
    return null;
  }
}

/**
 * Gallery listing, newest first. Both layouts are supported:
 *  - official per-game folders (`<root>/<Game>/…`) → gameTitle from the folder
 *  - legacy flat `Screenshots` dirs
 * Entries carry a small JPEG `thumbnailDataUrl` so a large library stays
 * light in renderer memory; the full-resolution image is read on demand.
 */
export async function listScreenshots(): Promise<ScreenshotEntry[]> {
  const files = (await walkGalleryCaptureFiles(getManagedGalleryRoots())).filter((file) =>
    IMAGE_EXTENSION.test(file.fileName),
  );

  const loaded = await Promise.all(
    files.map(async (file): Promise<ScreenshotEntry | null> => {
      try {
        if (file.sizeBytes <= 0) return null;
        const thumbnailDataUrl =
          (await buildScreenshotThumbnailDataUrl(file.filePath)) ??
          (await readScreenshotDataUrl(file.filePath));
        if (!thumbnailDataUrl) return null;

        return {
          id: file.filePath,
          fileName: file.fileName,
          filePath: file.filePath,
          createdAtMs: file.createdAtMs,
          sizeBytes: file.sizeBytes,
          gameTitle: file.gameTitle,
          thumbnailDataUrl,
        };
      } catch {
        return null;
      }
    }),
  );

  return loaded
    .filter((item): item is ScreenshotEntry => item !== null)
    .sort((a, b) => b.createdAtMs - a.createdAtMs)
    .slice(0, SCREENSHOT_LIST_LIMIT);
}

export async function saveScreenshot(
  input: ScreenshotSaveRequest,
): Promise<ScreenshotEntry> {
  const { ext, buffer } = dataUrlToBuffer(input.dataUrl);
  const gameFolder = sanitizeGameFolderName(input.gameTitle);
  const dir = join(getGalleryRootDirectory(), gameFolder);
  await mkdir(dir, { recursive: true });

  const base = buildCaptureBaseName(input.gameTitle, new Date());
  const filePath = await resolveUniqueCapturePath(dir, base, `.${ext}`);

  await writeFile(filePath, buffer);

  // Sidecar preview for the gallery strip (best-effort — if the bytes are not
  // decodable the listing falls back to the raw image data URL).
  const thumbnailDataUrl =
    (await writeScreenshotThumbnailSidecar(filePath, buffer)) ??
    buildScreenshotDataUrl(ext, buffer);

  // Disk budget (replaces the old 60-item listing cap): prune the oldest
  // captures when the gallery exceeds the configured limit; throws when this
  // file alone cannot fit, and the save is reported as failed.
  await pruneGalleryDiskLimit(filePath);

  const fileStats = await stat(filePath);
  return {
    id: filePath,
    fileName: basename(filePath),
    filePath,
    createdAtMs: fileStats.birthtimeMs || fileStats.mtimeMs || Date.now(),
    sizeBytes: fileStats.size,
    gameTitle: input.gameTitle,
    thumbnailDataUrl,
  };
}

export async function deleteScreenshot(
  input: ScreenshotDeleteRequest,
): Promise<void> {
  const filePath = await resolveTrustedOpenNowMediaPath(input.filePath);
  if (!filePath) {
    throw new Error("Invalid screenshot path");
  }
  await unlink(filePath);
  await unlink(thumbnailPathForCapture(filePath)).catch(() => undefined);
}

export async function saveScreenshotAs(
  input: ScreenshotSaveAsRequest,
  deps: ScreenshotSaveAsDeps,
): Promise<ScreenshotSaveAsResult> {
  const sourcePath = await resolveTrustedOpenNowMediaPath(input.filePath);
  if (!sourcePath) {
    throw new Error("Invalid screenshot path");
  }

  const saveDialogOptions = {
    title: "Save Screenshot",
    defaultPath: join(app.getPath("pictures"), basename(sourcePath)),
    filters: [
      { name: "PNG Image", extensions: ["png"] },
      { name: "JPEG Image", extensions: ["jpg", "jpeg"] },
      { name: "WebP Image", extensions: ["webp"] },
      { name: "All Files", extensions: ["*"] },
    ],
  };
  const mainWindow = deps.getMainWindow();
  const target =
    mainWindow && !mainWindow.isDestroyed()
      ? await deps.dialog.showSaveDialog(mainWindow, saveDialogOptions)
      : await deps.dialog.showSaveDialog(saveDialogOptions);

  if (target.canceled || !target.filePath) {
    return { saved: false };
  }

  await copyFile(sourcePath, target.filePath);
  return { saved: true, filePath: target.filePath };
}
