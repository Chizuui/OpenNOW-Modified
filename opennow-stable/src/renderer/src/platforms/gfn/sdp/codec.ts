import type { VideoCodec } from "@shared/gfn";

function normalizeCodec(name: string): string {
  const upper = name.toUpperCase();
  return upper === "HEVC" ? "H265" : upper;
}

export function rewriteH265TierFlag(
  sdp: string,
  tierFlag: 0 | 1,
): { sdp: string; replacements: number } {
  const lineEnding = sdp.includes("\r\n") ? "\r\n" : "\n";
  const lines = sdp.split(/\r?\n/);

  const h265Payloads = new Set<string>();
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

    const [, rest = ""] = line.split(":", 2);
    const [pt = "", codecPart = ""] = rest.split(/\s+/, 2);
    const codecName = normalizeCodec((codecPart.split("/")[0] ?? "").trim());
    if (pt && codecName === "H265") {
      h265Payloads.add(pt);
    }
  }

  if (h265Payloads.size === 0) {
    return { sdp, replacements: 0 };
  }

  let replacements = 0;
  const rewritten = lines.map((line) => {
    if (!line.startsWith("a=fmtp:")) {
      return line;
    }

    const [, rest = ""] = line.split(":", 2);
    const [pt = ""] = rest.split(/\s+/, 1);
    if (!pt || !h265Payloads.has(pt)) {
      return line;
    }

    const next = line.replace(/tier-flag=1/gi, `tier-flag=${tierFlag}`);
    if (next !== line) {
      replacements += 1;
    }
    return next;
  });

  return {
    sdp: rewritten.join(lineEnding),
    replacements,
  };
}

export function rewriteH265LevelIdByProfile(
  sdp: string,
  maxLevelByProfile: Partial<Record<1 | 2, number>>,
): { sdp: string; replacements: number } {
  const lineEnding = sdp.includes("\r\n") ? "\r\n" : "\n";
  const lines = sdp.split(/\r?\n/);

  const h265Payloads = new Set<string>();
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

    const [, rest = ""] = line.split(":", 2);
    const [pt = "", codecPart = ""] = rest.split(/\s+/, 2);
    const codecName = normalizeCodec((codecPart.split("/")[0] ?? "").trim());
    if (pt && codecName === "H265") {
      h265Payloads.add(pt);
    }
  }

  if (h265Payloads.size === 0) {
    return { sdp, replacements: 0 };
  }

  let replacements = 0;
  const rewritten = lines.map((line) => {
    if (!line.startsWith("a=fmtp:")) {
      return line;
    }

    const [, rest = ""] = line.split(":", 2);
    const [pt = "", params = ""] = rest.split(/\s+/, 2);
    if (!pt || !params || !h265Payloads.has(pt)) {
      return line;
    }

    const profileMatch = params.match(/(?:^|;)\s*profile-id=(\d+)/i);
    const levelMatch = params.match(/(?:^|;)\s*level-id=(\d+)/i);
    if (!profileMatch?.[1] || !levelMatch?.[1]) {
      return line;
    }

    const profileNum = Number.parseInt(profileMatch[1], 10) as 1 | 2;
    const offeredLevel = Number.parseInt(levelMatch[1], 10);
    const maxLevel = maxLevelByProfile[profileNum];
    if (!Number.isFinite(offeredLevel) || !maxLevel || offeredLevel <= maxLevel) {
      return line;
    }

    const next = line.replace(/(level-id=)(\d+)/i, `$1${maxLevel}`);
    if (next !== line) {
      replacements += 1;
    }
    return next;
  });

  return {
    sdp: rewritten.join(lineEnding),
    replacements,
  };
}

interface PreferCodecOptions {
  preferHevcProfileId?: 1 | 2;
  /**
   * Soft filtering: keep every payload type in the video m-line and only
   * reorder so the preferred codec comes first. When false (default) the
   * m-line is stripped down to the preferred codec (+ its RTX).
   *
   * Soft mode is the GFN-web behavior and is required when the requested
   * codec may not be receivable on this device: with a hard filter the
   * answer would reject the whole video m-line (port 0, dropped from the
   * BUNDLE group) and the session would hang on "Waiting for game video..."
   * instead of falling back to a codec the browser can actually decode.
   */
  keepFallbacks?: boolean;
  /**
   * User-pinned fallback codec (web mode). When `keepFallbacks` is set, this
   * codec's payload types are ordered directly after the preferred codec's so
   * the answer prefers it whenever the requested codec cannot be negotiated.
   * Ignored when equal to the preferred codec or when `keepFallbacks` is
   * false (the m-line then only carries the preferred codec anyway).
   */
  fallbackCodec?: VideoCodec;
}

/**
 * Extract the video codec actually negotiated in a (local answer) SDP.
 * Mirrors the Rust reference implementation (`extract_negotiated_video_codec`):
 * walks the video m-line payload types in order and returns the first known
 * codec name. Returns null when the m-line is missing/rejected (port 0) or
 * carries no recognizable primary codec.
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

export function preferCodec(sdp: string, codec: VideoCodec, options?: PreferCodecOptions): string {
  console.log(`[SDP] preferCodec: filtering SDP for codec "${codec}"`);
  const lineEnding = sdp.includes("\r\n") ? "\r\n" : "\n";
  const lines = sdp.split(/\r?\n/);

  let inVideoSection = false;
  const payloadTypesByCodec = new Map<string, string[]>();
  const codecByPayloadType = new Map<string, string>();
  const rtxAptByPayloadType = new Map<string, string>();
  const fmtpByPayloadType = new Map<string, string>();

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
    if (!pt || !codecName) {
      continue;
    }

    const list = payloadTypesByCodec.get(codecName) ?? [];
    list.push(pt);
    payloadTypesByCodec.set(codecName, list);
    codecByPayloadType.set(pt, codecName);

    continue;
  }

  // Parse RTX apt mappings from fmtp lines so we can keep RTX for chosen codec payloads
  inVideoSection = false;
  for (const line of lines) {
    if (line.startsWith("m=video")) {
      inVideoSection = true;
      continue;
    }
    if (line.startsWith("m=") && inVideoSection) {
      inVideoSection = false;
    }
    if (!inVideoSection || !line.startsWith("a=fmtp:")) {
      continue;
    }

    const [, rest = ""] = line.split(":", 2);
    const [pt = "", params = ""] = rest.split(/\s+/, 2);
    if (!pt || !params) {
      continue;
    }

    const aptMatch = params.match(/(?:^|;)\s*apt=(\d+)/i);
    if (aptMatch?.[1]) {
      rtxAptByPayloadType.set(pt, aptMatch[1]);
    }
    fmtpByPayloadType.set(pt, params);
  }

  // Log all codecs found in the SDP
  for (const [name, pts] of payloadTypesByCodec.entries()) {
    console.log(`[SDP] preferCodec: found codec ${name} with payload types [${pts.join(", ")}]`);
  }

  const preferredPayloads = payloadTypesByCodec.get(codec) ?? [];
  if (preferredPayloads.length === 0) {
    console.log(`[SDP] preferCodec: codec "${codec}" NOT found in offer — returning SDP unmodified`);
    return sdp;
  }

  // H265 often appears with multiple profiles in one offer.
  // Prefer profile-id=1 first (widest decoder compatibility), then others.
  const orderedPreferredPayloads = codec === "H265" && options?.preferHevcProfileId
    ? [...preferredPayloads].sort((a, b) => {
      const pa = fmtpByPayloadType.get(a) ?? "";
      const pb = fmtpByPayloadType.get(b) ?? "";
      const score = (fmtp: string): number => {
        const profile = fmtp.match(/(?:^|;)\s*profile-id=(\d+)/i)?.[1];
        if (profile === String(options.preferHevcProfileId)) return 0;
        if (!profile) return 1;
        return 2;
      };
      return score(pa) - score(pb);
    })
    : preferredPayloads;

  const preferred = new Set(orderedPreferredPayloads);

  const allowed = new Set<string>(preferred);

  // Keep RTX payloads linked to preferred payloads (apt mapping)
  for (const [rtxPt, apt] of rtxAptByPayloadType.entries()) {
    if (preferred.has(apt) && codecByPayloadType.get(rtxPt) === "RTX") {
      allowed.add(rtxPt);
    }
  }

  // Do NOT keep FLEXFEC/RED/ULPFEC during hard codec filtering.
  // Chromium can otherwise negotiate a "video" m-line with only FEC payloads
  // when primary codec intersection fails, causing black video with live audio.
  // Soft mode (keepFallbacks) intentionally keeps them: primary codecs stay in
  // the m-line too, so the intersection can never collapse to FEC-only.
  if (options?.keepFallbacks) {
    for (const pts of payloadTypesByCodec.values()) {
      for (const pt of pts) {
        allowed.add(pt);
      }
    }
  }

  console.log(`[SDP] preferCodec: preferred ordered payloads [${orderedPreferredPayloads.join(", ")}] for ${codec}`);
  console.log(`[SDP] preferCodec: keeping payload types [${Array.from(allowed).join(", ")}] for ${codec}${options?.keepFallbacks ? " (keepFallbacks: reorder only)" : ""}`);

  const filtered: string[] = [];
  inVideoSection = false;

  for (const line of lines) {
    if (line.startsWith("m=video")) {
      inVideoSection = true;
      const parts = line.split(/\s+/);
      const header = parts.slice(0, 3);
      const available = options?.keepFallbacks
        ? parts.slice(3)
        : parts.slice(3).filter((pt) => allowed.has(pt));
      const ordered: string[] = [];

      for (const pt of orderedPreferredPayloads) {
        if (available.includes(pt)) {
          ordered.push(pt);
        }
      }
      const fallbackPayloads =
        options?.keepFallbacks && options.fallbackCodec && options.fallbackCodec !== codec
          ? (payloadTypesByCodec.get(options.fallbackCodec) ?? [])
          : [];
      for (const pt of fallbackPayloads) {
        if (!preferred.has(pt) && available.includes(pt) && !ordered.includes(pt)) {
          ordered.push(pt);
        }
      }
      for (const pt of available) {
        if (!preferred.has(pt) && !ordered.includes(pt)) {
          ordered.push(pt);
        }
      }

      filtered.push(ordered.length > 0 ? [...header, ...ordered].join(" ") : line);
      continue;
    }

    if (line.startsWith("m=") && inVideoSection) {
      inVideoSection = false;
    }

    if (inVideoSection) {
      if (
        line.startsWith("a=rtpmap:") ||
        line.startsWith("a=fmtp:") ||
        line.startsWith("a=rtcp-fb:")
      ) {
        if (options?.keepFallbacks) {
          // Soft mode: every payload remains in the m-line, so keep every
          // associated attribute untouched.
          filtered.push(line);
          continue;
        }
        const [, rest = ""] = line.split(":", 2);
        const [pt = ""] = rest.split(/\s+/, 1);
        if (pt && !allowed.has(pt)) {
          continue;
        }
      }
    }

    filtered.push(line);
  }

  return filtered.join(lineEnding);
}
