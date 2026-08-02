export function getRttColor(rttMs: number): string {
  if (rttMs <= 0) return "var(--ink-muted)";
  if (rttMs < 30) return "var(--success)";
  if (rttMs < 60) return "var(--warning)";
  return "var(--error)";
}

export function getPacketLossColor(lossPercent: number): string {
  if (lossPercent <= 0.15) return "var(--success)";
  if (lossPercent < 1) return "var(--warning)";
  return "var(--error)";
}

export function getTimingColor(valueMs: number, goodMax: number, warningMax: number): string {
  if (valueMs <= 0) return "var(--ink-muted)";
  if (valueMs <= goodMax) return "var(--success)";
  if (valueMs <= warningMax) return "var(--warning)";
  return "var(--error)";
}

export function getInputQueueColor(bufferedBytes: number, dropCount: number): string {
  if (dropCount > 0 || bufferedBytes >= 65536) return "var(--error)";
  if (bufferedBytes >= 32768) return "var(--warning)";
  return "var(--success)";
}

export function getBitratePerformanceColor(percent: number): string {
  if (percent <= 0) return "var(--ink-muted)";
  if (percent >= 70 && percent <= 110) return "var(--success)";
  if (percent >= 45 && percent < 130) return "var(--warning)";
  return "var(--error)";
}

export function formatBitrate(kbps: number): string {
  if (kbps >= 1000) return `${(kbps / 1000).toFixed(1)} Mbps`;
  return `${kbps.toFixed(0)} kbps`;
}

// Map the 3-letter GFN/CloudMatch city code (embedded in the zone id or the
// server hostname) to a human country/city label, mirroring how the official
// client shows e.g. "Japan (NP-TYO-01)".
const SERVER_CITY_LABELS: Record<string, string> = {
  // PrintedWaste queue data + known GFN city codes.
  dal: "US Central",
  ash: "US East",
  chi: "US Central",
  nwk: "US East",
  pdx: "US West",
  atl: "US East",
  mia: "US East",
  lax: "US West",
  phx: "US West",
  sjc: "US West",
  sjc6: "US West",
  ams: "Netherlands",
  frk: "Germany",
  fra: "Germany",
  par: "France",
  lon: "United Kingdom",
  lhr: "United Kingdom",
  sth: "Sweden",
  arn: "Sweden",
  sof: "Bulgaria",
  waw: "Poland",
  bom: "India",
  tyo: "Japan",
  osa: "Japan",
  mon: "Canada",
  yyz: "Canada",
  sel: "South Korea",
  seo: "South Korea",
  bkk: "Thailand",
  kul: "Malaysia",
  sin: "Singapore",
  hkg: "Hong Kong",
  tpe: "Taiwan",
  syd: "Australia",
  mad: "Spain",
  mil: "Italy",
  yto: "Canada",
  gru: "Brazil",
  sao: "Brazil",
};

/**
 * Turn a raw zone id (e.g. "NP-TYO-01") and/or server hostname
 * (e.g. "npa-yes-kul-01.yes.geforcenow.nvidiagrid.net" or
 * "183-78-14-232.yes.geforcenow.nvidiagrid.net") into a friendly location label
 * like "Malaysia (KUL)". Falls back to the zone code, then the hostname.
 */
export function formatServerLocation(zone: string, hostname: string): string {
  const zoneCode = (zone || "").trim().toLowerCase();
  let host = (hostname || "").trim();
  // Strip port first (e.g. "host:443" → "host") so URL parser works cleanly.
  host = host.replace(/:\d+$/, "");
  try {
    host = new URL(host.includes("://") ? host : `https://${host}`).hostname;
  } catch {
    host = host.replace(/^https?:\/\//i, "").split("/")[0];
  }

  // 1. Coba parse dari zoneCode dulu, e.g. "NP-SJC-01"
  if (zoneCode && zoneCode !== "prod") {
    const tokens = zoneCode.split("-");
    for (const tok of tokens) {
      if (tok in SERVER_CITY_LABELS) {
        const city = tok;
        const country = SERVER_CITY_LABELS[city];
        return `${country} (${city.toUpperCase()})`;
      }
    }
  }

  // 2. Parse dari hostname: scan semua token hyphen di semua dot-segments
  // e.g. "np-tyo-01.cloudmatchbeta.nvidiagrid.net" → tokens: [np,tyo,01,cloudmatchbeta,...]
  // e.g. "npa-yes-kul-01.yes.geforcenow.nvidiagrid.net" → tokens: [...,kul,...]
  const tokens = host.toLowerCase().split(/[.\-]/);
  let city: string | undefined;
  for (const tok of tokens) {
    if (tok in SERVER_CITY_LABELS) {
      city = tok;
      break;
    }
  }
  const country = city ? SERVER_CITY_LABELS[city] : undefined;

  if (country && city) {
    // Append the numeric server index when present, e.g. "np-tyo-01" → "TYO-01".
    const idx = tokens.indexOf(city);
    const next = tokens[idx + 1];
    const suffix = next && /^\d+$/.test(next) ? `-${next}` : "";
    return `${country} (${city.toUpperCase()}${suffix})`;
  }

  // 3. Fallback
  if (zoneCode && zoneCode !== "prod") {
    return zoneCode.toUpperCase();
  }
  // Fallback: ambil token pertama hostname jika bukan IP address
  const firstHostToken = host.split(".")[0];
  if (firstHostToken && !/^(?:\d+[.-]){2,3}\d+$/.test(firstHostToken)) {
    return firstHostToken.toUpperCase();
  }
  return "--";
}
