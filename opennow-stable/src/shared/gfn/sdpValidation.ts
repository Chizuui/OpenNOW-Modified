import type { VideoCodec } from "./stream";

function normalizeCodec(name: string): string {
  const upper = name.toUpperCase();
  return upper === "HEVC" ? "H265" : upper;
}

/**
 * Extract the video codec actually negotiated in a (local answer) SDP.
 * Walks the video m-line payload types in order and returns the first known
 * primary codec name. Returns `null` when the m-line is missing/rejected
 * (port 0) or carries no recognizable primary codec.
 *
 * Canonical implementation shared by:
 * - the renderer WebRTC path (via `sdp/codec.ts` re-export), which validates
 *   the Chromium `createAnswer` output and auto-falls-back to the next codec;
 * - the main-process native streamer path, which rejects an answer that
 *   dropped the video m-line so `SignalingCoordinator` falls back to web mode
 *   instead of hanging on "Waiting for game video...".
 *
 * Mirrors the Rust reference implementation (`extract_negotiated_video_codec`).
 */
export function extractNegotiatedVideoCodec(sdp: string): VideoCodec | null {
  const lines = sdp.split(/\r?\n/);
  const codecByPayloadType = new Map<string, string>();
  let inVideoSection = false;

  for (const line of lines) {
    if (line.startsWith("m=video")) {
      inVideoSection = true;
      continue;
    }
    if (line.startsWith("m=") && inVideoSection) {
      inVideoSection = false;
    }
    if (!inVideoSection || !line.startsWith("a=rtpmap:")) {
      continue;
    }

    const [, rest = ""] = line.split("a=rtpmap:");
    const [pt, codecPart] = rest.split(/\s+/, 2);
    const codecName = normalizeCodec((codecPart ?? "").split("/")[0] ?? "");
    if (pt && codecName) {
      codecByPayloadType.set(pt, codecName);
    }
  }

  for (const line of lines) {
    if (!line.startsWith("m=video")) {
      continue;
    }
    const payloads = line.split(/\s+/).slice(3);
    for (const pt of payloads) {
      const codec = codecByPayloadType.get(pt);
      if (codec === "H264") return "H264";
      if (codec === "H265") return "H265";
      if (codec === "AV1") return "AV1";
    }
  }

  return null;
}

/**
 * True when the SDP answer still carries a decodable video m-line with a
 * recognizable primary codec. A false result means the answer dropped the
 * whole video m-line (port 0 / missing from the BUNDLE group), which would
 * leave the session stuck on "Waiting for game video...".
 */
export function answerHasVideoCodec(sdp: string): boolean {
  return extractNegotiatedVideoCodec(sdp) !== null;
}
