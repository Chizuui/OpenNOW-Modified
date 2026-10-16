/// <reference types="node" />

import test from "node:test";
import assert from "node:assert/strict";
import { mkdir, mkdtemp, rm, stat, utimes, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import {
  cleanupStaleGalleryTemps,
  GalleryDiskLimitError,
  getGalleryUsage,
  pruneGalleryToLimit,
  walkGalleryCaptureFiles,
} from "./galleryFiles";

async function makeRoot(prefix: string): Promise<string> {
  return mkdtemp(join(tmpdir(), prefix));
}

test("walkGalleryCaptureFiles understands both layouts", async () => {
  const root = await makeRoot("opennow-gallery-");
  try {
    await mkdir(join(root, "Game A"), { recursive: true });
    await mkdir(join(root, "Recordings"), { recursive: true });
    await mkdir(join(root, "Screenshots"), { recursive: true });
    await mkdir(join(root, ".tmp"), { recursive: true });
    await writeFile(join(root, "Game A", "Game A 2026.01.01 - 10.00.00.00.mp4"), Buffer.alloc(100));
    await writeFile(join(root, "Game A", "Game A 2026.01.01 - 10.00.00.00-thumb.jpg"), Buffer.alloc(10));
    await writeFile(join(root, "Game A", "Game A 2026.01.01 - 10.00.01.00.png"), Buffer.alloc(50));
    await writeFile(
      join(root, "Recordings", "2024-01-01T00-00-00-000Z-old-game-ab12cd-dur1234.webm"),
      Buffer.alloc(80),
    );
    await writeFile(join(root, ".tmp", "uuid.mp4.tmp"), Buffer.alloc(7));
    await writeFile(join(root, "Game A", "notes.txt"), "ignored");

    const files = await walkGalleryCaptureFiles([root]);
    const byName = Object.fromEntries(files.map((file) => [file.fileName, file]));
    assert.equal(files.length, 3);
    assert.equal(byName["Game A 2026.01.01 - 10.00.00.00.mp4"].gameTitle, "Game A");
    assert.equal(byName["Game A 2026.01.01 - 10.00.00.00.mp4"].legacy, false);
    assert.equal(byName["Game A 2026.01.01 - 10.00.00.00.mp4"].sizeBytes, 100);
    assert.equal(byName["Game A 2026.01.01 - 10.00.01.00.png"].gameTitle, "Game A");
    assert.equal(
      byName["2024-01-01T00-00-00-000Z-old-game-ab12cd-dur1234.webm"].legacy,
      true,
    );
    assert.equal(
      byName["2024-01-01T00-00-00-000Z-old-game-ab12cd-dur1234.webm"].gameTitle,
      undefined,
    );
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("getGalleryUsage counts captures plus sidecar thumbnails", async () => {
  const root = await makeRoot("opennow-usage-");
  try {
    await mkdir(join(root, "Game"), { recursive: true });
    await writeFile(join(root, "Game", "capture.mp4"), Buffer.alloc(200));
    await writeFile(join(root, "Game", "capture-thumb.jpg"), Buffer.alloc(20));
    await mkdir(join(root, "Screenshots"), { recursive: true });
    await writeFile(join(root, "Screenshots", "shot.png"), Buffer.alloc(40));

    const usage = await getGalleryUsage([root]);
    assert.equal(usage.fileCount, 2);
    assert.equal(usage.usedBytes, 260);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("pruneGalleryToLimit removes oldest captures first and keeps the new one", async () => {
  const root = await makeRoot("opennow-prune-");
  try {
    await mkdir(join(root, "Game"), { recursive: true });
    const oldPath = join(root, "Game", "old.mp4");
    const newPath = join(root, "Game", "new.mp4");
    await writeFile(oldPath, Buffer.alloc(100));
    await writeFile(newPath, Buffer.alloc(100));
    await utimes(oldPath, new Date(2020, 0, 1), new Date(2020, 0, 1));
    await utimes(newPath, new Date(2026, 0, 1), new Date(2026, 0, 1));

    const result = await pruneGalleryToLimit([root], 150, newPath);
    assert.equal(result.prunedFiles, 1);
    await assert.rejects(stat(oldPath));
    await stat(newPath);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("pruneGalleryToLimit throws when the new capture alone cannot fit", async () => {
  const root = await makeRoot("opennow-prune-small-");
  try {
    await mkdir(join(root, "Game"), { recursive: true });
    const oldPath = join(root, "Game", "old.mp4");
    const newPath = join(root, "Game", "new.mp4");
    await writeFile(oldPath, Buffer.alloc(100));
    await writeFile(newPath, Buffer.alloc(100));
    await utimes(oldPath, new Date(2020, 0, 1), new Date(2020, 0, 1));
    await utimes(newPath, new Date(2026, 0, 1), new Date(2026, 0, 1));

    await assert.rejects(
      () => pruneGalleryToLimit([root], 50, newPath),
      GalleryDiskLimitError,
    );
    await assert.rejects(stat(oldPath));
    await assert.rejects(stat(newPath));
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("pruneGalleryToLimit is a no-op without a limit", async () => {
  const root = await makeRoot("opennow-prune-off-");
  try {
    await mkdir(join(root, "Game"), { recursive: true });
    const filePath = join(root, "Game", "keep.mp4");
    await writeFile(filePath, Buffer.alloc(100));
    const result = await pruneGalleryToLimit([root], 0, filePath);
    assert.equal(result.prunedFiles, 0);
    await stat(filePath);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("cleanupStaleGalleryTemps reaps only temp files", async () => {
  const root = await makeRoot("opennow-temps-");
  try {
    await mkdir(join(root, ".tmp"), { recursive: true });
    await mkdir(join(root, "Recordings"), { recursive: true });
    await writeFile(join(root, ".tmp", "uuid.mp4.tmp"), Buffer.alloc(5));
    await writeFile(join(root, "Recordings", "legacy-uuid.webm.tmp"), Buffer.alloc(5));
    const keep = join(root, "Recordings", "legacy-real.webm");
    await writeFile(keep, Buffer.alloc(5));

    const removed = await cleanupStaleGalleryTemps([root]);
    assert.equal(removed, 2);
    await assert.rejects(stat(join(root, ".tmp", "uuid.mp4.tmp")));
    await assert.rejects(stat(join(root, "Recordings", "legacy-uuid.webm.tmp")));
    await stat(keep);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
