/**
 * Munge an SDP answer to inject bitrate limits and optimize audio codec params.
 * 
 * This matches what the official GFN browser client does:
 * 1. Adds "b=AS:<kbps>" after each m= line to signal our max receive bitrate
 * 2. Adds "stereo=1" to the opus fmtp line for stereo audio support
 * 
 * These are hints to the server encoder — they don't enforce limits client-side
 * but help the server avoid overshooting our link capacity.
 */
export function mungeAnswerSdp(sdp: string, maxBitrateKbps: number): string {
  const lineEnding = sdp.includes("\r\n") ? "\r\n" : "\n";
  const lines = sdp.split(/\r?\n/);
  const result: string[] = [];

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    result.push(line);

    // After each m= line, inject b=AS: if not already present
    if (line.startsWith("m=video") || line.startsWith("m=audio")) {
      const bitrateForSection = line.startsWith("m=video")
        ? maxBitrateKbps
        : 128; // 128 kbps for audio is plenty for opus stereo
      const nextLine = lines[i + 1] ?? "";
      if (!nextLine.startsWith("b=")) {
        result.push(`b=AS:${bitrateForSection}`);
      }
    }

    // Append stereo=1 to opus fmtp line if not already present
    if (line.startsWith("a=fmtp:") && line.includes("minptime=") && !line.includes("stereo=1")) {
      // Replace the line we just pushed with the stereo-augmented version
      result[result.length - 1] = line + ";stereo=1";
    }
  }

  console.log(`[SDP] mungeAnswerSdp: injected b=AS:${maxBitrateKbps} for video, b=AS:128 for audio, stereo=1 for opus`);
  return result.join(lineEnding);
}

interface RedAudioSection {
  mid?: string;
  payload: string;
  rtpmap: string;
  fmtp: string[];
  rtcpFb: string[];
}

/**
 * Re-inject the server's RED audio redundancy payload into the WebRTC answer.
 *
 * GFN servers advertise RED for the game-audio m-line (`a=rtpmap:63
 * red/48000/2` + `a=fmtp:63 111/111`) so a redundant copy of each Opus packet
 * survives one lost RTP packet — the same pattern the official client
 * negotiates. When the engine dropped RED from its answer despite supporting
 * it, mirror the offer's RED payload back into the matching audio m-line.
 * `redSupported` gates the injection on the engine actually being able to
 * unwrap RED — never negotiate a payload the receive path cannot decode.
 */
export function ensureAudioRedInAnswer(
  answerSdp: string,
  offerSdp: string,
  redSupported: boolean,
): string {
  if (!redSupported) return answerSdp;
  const lineEnding = answerSdp.includes("\r\n") ? "\r\n" : "\n";

  // Collect the offer's RED audio sections, keyed by a=mid so the right
  // answer section is patched (the mic uplink m-line never advertises RED, so
  // it is naturally skipped).
  const redSections: RedAudioSection[] = [];
  {
    const offerLines = offerSdp.split(/\r?\n/);
    for (let i = 0; i < offerLines.length; i++) {
      if (!offerLines[i].startsWith("m=audio")) continue;
      let j = i + 1;
      let mid: string | undefined;
      let red: RedAudioSection | undefined;
      const fmtp: Array<{ pt: string; line: string }> = [];
      const rtcpFb: Array<{ pt: string; line: string }> = [];
      while (j < offerLines.length && !offerLines[j].startsWith("m=")) {
        const line = offerLines[j];
        const midMatch = /^a=mid:(.*)$/.exec(line);
        if (midMatch) mid = midMatch[1];
        const rtpmapMatch = /^a=rtpmap:(\d+)\s+red\//.exec(line);
        if (rtpmapMatch) {
          red = { payload: rtpmapMatch[1], rtpmap: line, fmtp: [], rtcpFb: [] };
        }
        const fmtpMatch = /^a=fmtp:(\d+)\s+/.exec(line);
        if (fmtpMatch) fmtp.push({ pt: fmtpMatch[1], line });
        const fbMatch = /^a=rtcp-fb:(\d+)\s+/.exec(line);
        if (fbMatch) rtcpFb.push({ pt: fbMatch[1], line });
        j++;
      }
      if (red) {
        const payload = red.payload;
        red.mid = mid;
        red.fmtp = fmtp.filter((f) => f.pt === payload).map((f) => f.line);
        red.rtcpFb = rtcpFb.filter((f) => f.pt === payload).map((f) => f.line);
        redSections.push(red);
      }
      i = j - 1;
    }
  }
  if (redSections.length === 0) return answerSdp;

  // Find answer audio sections that need RED, then patch bottom-up so the
  // insertion indices stay valid.
  const lines = answerSdp.split(/\r?\n/);
  const patches: Array<{ sectionStart: number; insertAt: number; section: RedAudioSection }> = [];
  for (let i = 0; i < lines.length; i++) {
    if (!lines[i].startsWith("m=audio")) continue;
    let j = i + 1;
    let mid: string | undefined;
    while (j < lines.length && !lines[j].startsWith("m=")) {
      const m = /^a=mid:(.*)$/.exec(lines[j]);
      if (m) mid = m[1];
      j++;
    }
    const section = redSections.find((s) => s.mid !== undefined && s.mid === mid);
    if (section) {
      const payloads = lines[i].split(/\s+/);
      const rtpmapPresent = lines.slice(i + 1, j).some((l) => l === section.rtpmap);
      if (!payloads.includes(section.payload) && !rtpmapPresent) {
        patches.push({ sectionStart: i, insertAt: j, section });
      }
    }
    i = j - 1;
  }
  if (patches.length === 0) return answerSdp;

  for (const patch of patches.reverse()) {
    lines[patch.sectionStart] += ` ${patch.section.payload}`;
    const extra = [patch.section.rtpmap, ...patch.section.fmtp, ...patch.section.rtcpFb];
    lines.splice(patch.insertAt, 0, ...extra);
  }
  return lines.join(lineEnding);
}
