/// <reference types="node" />

import test from "node:test";
import assert from "node:assert/strict";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import {
  buildCaptureBaseName,
  GALLERY_FALLBACK_GAME_NAME,
  resolveUniqueCapturePath,
  sanitizeGameFolderName,
  thumbnailPathForCapture,
} from "./galleryNaming";

test("sanitizeGameFolderName keeps display formatting", () => {
  assert.equal(sanitizeGameFolderName("007 First Light"), "007 First Light");
  assert.equal(sanitizeGameFolderName("  Cyberpunk  2077  "), "Cyberpunk 2077");
  assert.equal(
    sanitizeGameFolderName("Tom's Game: Special/ Edition?"),
    "Tom's Game Special Edition",
  );
  assert.equal(sanitizeGameFolderName("Trailing dot."), "Trailing dot");
});

test("sanitizeGameFolderName falls back and guards reserved names", () => {
  assert.equal(sanitizeGameFolderName(undefined), GALLERY_FALLBACK_GAME_NAME);
  assert.equal(sanitizeGameFolderName("   "), GALLERY_FALLBACK_GAME_NAME);
  assert.equal(sanitizeGameFolderName("//"), GALLERY_FALLBACK_GAME_NAME);
  assert.equal(sanitizeGameFolderName("CON"), "_CON");
  assert.equal(sanitizeGameFolderName("com1"), "_com1");
  assert.equal(sanitizeGameFolderName("console"), "console");
});

test("buildCaptureBaseName uses the official local-time stamp", () => {
  const date = new Date(2026, 9, 16, 14, 5, 9, 420);
  assert.equal(
    buildCaptureBaseName("007 First Light", date),
    "007 First Light 2026.10.16 - 14.05.09.42",
  );
  // Centiseconds truncate (not round): 399 ms -> 39.
  assert.equal(
    buildCaptureBaseName(undefined, new Date(2026, 0, 2, 3, 4, 5, 399)),
    `${GALLERY_FALLBACK_GAME_NAME} 2026.01.02 - 03.04.05.39`,
  );
});

test("resolveUniqueCapturePath bumps the centisecond tail", async () => {
  const dir = await mkdtemp(join(tmpdir(), "opennow-naming-"));
  try {
    const base = "Game 2026.10.16 - 14.05.09.42";
    const head = base.slice(0, -2);
    await writeFile(join(dir, `${base}.mp4`), "");
    await writeFile(join(dir, `${head}43.mp4`), "");
    const resolved = await resolveUniqueCapturePath(dir, base, ".mp4");
    assert.equal(resolved, join(dir, `${head}44.mp4`));
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
});

test("resolveUniqueCapturePath returns the base name when free", async () => {
  const dir = await mkdtemp(join(tmpdir(), "opennow-naming-"));
  try {
    const base = "Game 2026.10.16 - 14.05.09.07";
    const resolved = await resolveUniqueCapturePath(dir, base, ".png");
    assert.equal(resolved, join(dir, `${base}.png`));
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
});

test("thumbnailPathForCapture strips the capture extension", () => {
  assert.equal(
    thumbnailPathForCapture(join("C:", "gallery", "Game A 1.mp4")),
    join("C:", "gallery", "Game A 1-thumb.jpg"),
  );
  assert.equal(
    thumbnailPathForCapture(join("C:", "gallery", "Game A 1-dur1234.webm")),
    join("C:", "gallery", "Game A 1-dur1234-thumb.jpg"),
  );
  assert.equal(
    thumbnailPathForCapture(join("C:", "gallery", "Shot 2.png")),
    join("C:", "gallery", "Shot 2-thumb.jpg"),
  );
});
