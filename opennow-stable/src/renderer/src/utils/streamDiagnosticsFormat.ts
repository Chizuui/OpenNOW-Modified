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
  tyo: "Japan",
  osa: "Japan",
  sin: "Singapore",
  hkg: "Hong Kong",
  syd: "Australia",
  seo: "South Korea",
  tpe: "Taiwan",
  kul: "Malaysia",
  bom: "India",
  fra: "Germany",
  ams: "Netherlands",
  par: "France",
  lhr: "United Kingdom",
  lon: "United Kingdom",
  mad: "Spain",
  mil: "Italy",
  arn: "Sweden",
  waw: "Poland",
  lax: "US West",
  sjc: "US West",
  sea: "US West",
  pdx: "US West",
  iad: "US East",
  ash: "US East",
  atl: "US East",
  ord: "US Central",
  dfw: "US Central",
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
  const zoneCode = (zone || "").trim();
  let host = (hostname || "").trim();
  try {
    host = new URL(host.includes("://") ? host : `https://${host}`).hostname;
  } catch {
    host = host.replace(/^https?:\/\//i, "").split("/")[0];
  }

  // Scan every 3-letter token in the zone id + hostname and pick the first one
  // that is a known city code. This avoids false positives like "yes"/"net".
  const source = `${zoneCode} ${host}`.toLowerCase();
  const tokens = source.match(/[a-z]{3}/g) ?? [];
  let city: string | undefined;
  for (const tok of tokens) {
    if (tok in SERVER_CITY_LABELS) {
      city = tok;
      break;
    }
  }
  const country = city ? SERVER_CITY_LABELS[city] : undefined;

  if (country && city) {
    return `${country} (${city.toUpperCase()})`;
  }
  if (zoneCode && zoneCode.toLowerCase() !== "prod") {
    return zoneCode.toUpperCase();
  }
  return hostname || "--";
}
