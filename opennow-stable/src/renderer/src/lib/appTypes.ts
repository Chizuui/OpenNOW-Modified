import type { StreamTimeWarning } from "../platforms/gfn/webrtcClient";

export type StreamStatus = "idle" | "queue" | "setup" | "starting" | "connecting" | "streaming";
export type StreamLoadingStatus = "queue" | "setup" | "starting" | "connecting";

/** Stats overlay display mode. Cycled by the toggle-stats shortcut:
 *  off → compact → full → off. */
export type StatsOverlayMode = "off" | "compact" | "full";

/** On-screen corner for the stats overlay. */
export type StatsOverlayPosition =
  | "bottom-left"
  | "bottom-right"
  | "top-left"
  | "top-right";

export type StreamWarningState = {
  code: StreamTimeWarning["code"];
  message: string;
  tone: "warn" | "critical";
  secondsLeft?: number;
};

export type LocalSessionTimerWarningState = {
  stage: "free-tier-30m" | "free-tier-15m" | "free-tier-final-minute";
  shownAtMs: number;
};

export type LaunchErrorState = {
  stage: StreamLoadingStatus;
  title: string;
  description: string;
  codeLabel?: string;
  action?: "persistent-storage-settings";
  actionLabel?: string;
};
