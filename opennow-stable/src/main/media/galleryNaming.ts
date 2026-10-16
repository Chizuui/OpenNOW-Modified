/**
 * Capture-gallery naming and layout helpers.
 *
 * New captures follow the official GeForce NOW layout — one folder per game
 * under the gallery root, files stamped with the local time:
 *
 *   <root>/<Game>/<Game> YYYY.MM.DD - HH.mm.ss.XX<ext>
 *
 * `XX` is a two-digit centisecond tail; when that name already exists it is
 * bumped until the path is free. Everything here is plain string/path work
 * with fs existence checks, so recordings and screenshots stay on one naming
 * rule and the behavior is unit-testable without Electron.
 */
import { access } from "node:fs/promises";
import { join } from "node:path";

// Control characters are intentional here: a Windows path component must
// strip them along with the reserved punctuation.
// eslint-disable-next-line no-control-regex
const ILLEGAL_PATH_CHARS = /[<>:"/\\|?*\u0000-\u001f]/g;
const RESERVED_COMPONENT_NAMES = /^(con|prn|aux|nul|com[1-9]|lpt[1-9])$/i;
const CAPTURE_EXTENSION = /\.(mp4|webm|png|jpe?g|webp)$/i;

/** Folder used when a capture carries no usable game title. */
export const GALLERY_FALLBACK_GAME_NAME = "OpenNOW";

/**
 * Display-preserving folder name for a game title (case and spaces survive,
 * unlike the legacy `sanitizeTitleForFileName`). Windows forbids a fixed set
 * of characters inside path components and rejects reserved device names plus
 * trailing dots/spaces, so all of those are normalized away.
 */
export function sanitizeGameFolderName(gameTitle: string | undefined): string {
  const replaced = (gameTitle ?? "").replace(ILLEGAL_PATH_CHARS, " ");
  const collapsed = replaced.replace(/\s+/g, " ").trim().replace(/[. ]+$/g, "");
  if (!collapsed) return GALLERY_FALLBACK_GAME_NAME;
  const bounded = collapsed.slice(0, 80).replace(/[. ]+$/g, "");
  const safe = bounded || GALLERY_FALLBACK_GAME_NAME;
  return RESERVED_COMPONENT_NAMES.test(safe) ? `_${safe}` : safe;
}

function pad2(value: number): string {
  return String(value).padStart(2, "0");
}

/** `Game 2026.10.16 - 14.05.09.42` — the official stamp plus a centisecond tail. */
export function buildCaptureBaseName(gameTitle: string | undefined, date: Date): string {
  const game = sanitizeGameFolderName(gameTitle);
  const day = `${date.getFullYear()}.${pad2(date.getMonth() + 1)}.${pad2(date.getDate())}`;
  const time = `${pad2(date.getHours())}.${pad2(date.getMinutes())}.${pad2(date.getSeconds())}`;
  const centis = pad2(Math.floor(date.getMilliseconds() / 10));
  return `${game} ${day} - ${time}.${centis}`;
}

async function pathExists(path: string): Promise<boolean> {
  try {
    await access(path);
    return true;
  } catch {
    return false;
  }
}

/**
 * Free path in `dir` for `base + ext`. The two-digit tail is bumped first so
 * two captures landing in the same centisecond still differ; a random tail is
 * the last-resort fallback for pathological directories.
 */
export async function resolveUniqueCapturePath(
  dir: string,
  base: string,
  ext: string,
): Promise<string> {
  const head = base.slice(0, -2);
  const start = Number.parseInt(base.slice(-2), 10);
  if (Number.isFinite(start)) {
    for (let i = 0; i < 100; i += 1) {
      const candidate = join(dir, `${head}${pad2((start + i) % 100)}${ext}`);
      if (!(await pathExists(candidate))) return candidate;
    }
  }
  for (let i = 0; i < 1000; i += 1) {
    const candidate = join(dir, `${base}-${Math.random().toString(16).slice(2, 6)}${ext}`);
    if (!(await pathExists(candidate))) return candidate;
  }
  throw new Error("Could not allocate a capture file name");
}

/** `-thumb.jpg` sidecar beside a capture (videos and screenshots share it). */
export function thumbnailPathForCapture(filePath: string): string {
  return `${filePath.replace(CAPTURE_EXTENSION, "")}-thumb.jpg`;
}
