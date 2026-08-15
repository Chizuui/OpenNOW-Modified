import type {
  CodecPreference,
  ColorQuality,
  FallbackCodecPreference,
  JitterBufferMode,
  NativeTransitionDiagnostics,
  StreamClientMode,
  VideoAccelerationPreference,
} from "./stream";
import type {
  NativePacingMode,
  NativeStreamerBackendPreference,
  NativeStreamerFeatureMode,
  NativeVideoBackendPreference,
  StreamTransportMode,
} from "./nativeStreamer";
import { DEFAULT_KEYBOARD_LAYOUT, type GameLanguage, type KeyboardLayout } from "./keyboard";
import { DEFAULT_VIDEO_SHADER_SETTINGS, type VideoShaderSettings } from "./videoShader";
import type { UpdateChannel } from "./updater";
import { normalizeFallbackCodecPreference, normalizeStreamPreferences } from "./stream";

export type AppAccentColor = "green" | "blue" | "violet" | "amber" | "rose";
export type AppTheme = "light" | "dark" | "auto";
export type MicrophoneMode = "disabled" | "push-to-talk" | "voice-activity";
/** On-screen corner for the in-stream stats overlay. */
export type StatsOverlayPosition = "bottom-left" | "bottom-right" | "top-left" | "top-right";
export type AspectRatio = "16:9" | "16:10" | "21:9" | "32:9";
export type ErrorReportingConsent = "unset" | "granted" | "denied";
export type RuntimePlatform =
  | "aix"
  | "android"
  | "cygwin"
  | "darwin"
  | "freebsd"
  | "haiku"
  | "linux"
  | "netbsd"
  | "openbsd"
  | "sunos"
  | "win32"
  | "unknown";

export type MacOsMicrophoneAccessStatus = "not-determined" | "granted" | "denied" | "restricted" | "unknown";

export interface MicrophonePermissionResult {
  platform: RuntimePlatform;
  isMacOs: boolean;
  status: MacOsMicrophoneAccessStatus | "not-applicable";
  granted: boolean;
  canRequest: boolean;
  shouldUseBrowserApi: boolean;
}

export interface Settings {
  resolution: string;
  aspectRatio: AspectRatio;
  posterSizeScale: number;
  fps: number;
  maxBitrateMbps: number;
  /** Web-mode jitter buffer aggressiveness: low latency / balanced / smooth */
  jitterBufferMode: JitterBufferMode;
  /** Recording video bitrate in Mbps; null means let MediaRecorder choose automatically */
  recordingBitrateMbps: number | null;
  recordingResolution: string;
  recordingFps: number;
  /**
   * Mix the live microphone into encoded-bitstream recordings (the GFN
   * parity recorder). The mic is captured as raw PCM and mixed into the
   * recording's audio track inside the worker — only the audio track is
   * re-encoded; the video bitstream stays untouched. Requires an active mic.
   */
  recordingMixMic: boolean;
  streamClientMode: StreamClientMode;
  nativeStreamerBackend: NativeStreamerBackendPreference;
  nativeVideoBackend: NativeVideoBackendPreference;
  nativeStreamerExecutablePath: string;
  nativeCloudGsyncMode: NativeStreamerFeatureMode;
  /**
   * Present-limiter pacing mode applied to the native streamer (`auto` |
   * `stream` | `vrr` | `off` | an fps like `120`) — the GFN NVST p-f pacing
   * framework analogue. Applied at session start and on change.
   */
  nativePacingMode: NativePacingMode;
  nativeD3dFullscreenMode: NativeStreamerFeatureMode;
  nativeExternalRenderer: boolean;
  /** GFN-style stacked video window behind a transparent Electron shell */
  nativeStackedRenderer: boolean;
  transportMode: StreamTransportMode;
  showNativeStreamerStats: boolean;
  codec: CodecPreference;
  /**
   * Codec used when the primary (`codec`) selection cannot be negotiated on
   * the server (web mode). `"auto"` keeps every supported GFN primary as a
   * fallback; a concrete value pins that codec first.
   */
  fallbackCodec: FallbackCodecPreference;
  decoderPreference: VideoAccelerationPreference;
  encoderPreference: VideoAccelerationPreference;
  colorQuality: ColorQuality;
  region: string;
  sessionProxyEnabled: boolean;
  sessionProxyUrl: string;
  clipboardPaste: boolean;
  /** Enable experimental gyroscope controller input mapping */
  enableGyroscopeControls: boolean;
  /** macOS-only workaround that restores Chromium's older HID path for Steam Controller compatibility */
  steamControllerCompatibilityMode: boolean;
  /** Use the WebRTC cursor_channel overlay instead of leaving cursor rendering to the stream. */
  nativeCursorOverlay: boolean;
  /** Opt-in stacked sink-native RawInput capture (mouse + keyboard bypass the Electron bridge). */
  nativeSinkInputCapture: boolean;
  mouseSensitivity: number;
  mouseAcceleration: number;
  shortcutToggleStats: string;
  shortcutTogglePointerLock: string;
  shortcutToggleFullscreen: string;
  shortcutStopStream: string;
  shortcutToggleAntiAfk: string;
  shortcutToggleMicrophone: string;
  shortcutScreenshot: string;
  shortcutToggleRecording: string;
  /** Cycle the native present-limiter pacing mode (auto → stream → vrr → off) while streaming. */
  shortcutCyclePacing: string;
  microphoneMode: MicrophoneMode;
  microphoneDeviceId: string;
  hideStreamButtons: boolean;
  showAntiAfkIndicator: boolean;
  showStatsOnLaunch: boolean;
  /** On-screen corner for the in-stream stats overlay. */
  statsOverlayPosition: StatsOverlayPosition;
  /** Skip the free-tier queue server selection modal and launch with default routing */
  hideServerSelector: boolean;
  /** Desktop UI accent preset */
  appAccentColor: AppAccentColor;
  /** UI Theme */
  appTheme: AppTheme;
  /** Use translucent overlays for settings and navbars */
  translucentUI: boolean;
  /** Use the large-screen controller-oriented shell and library layout */
  controllerMode: boolean;
  /** Launch fullscreen with Controller Mode enabled, like GeForce NOW's TV mode */
  launchInConsoleMode: boolean;
  autoFullScreen: boolean;
  favoriteGameIds: string[];
  sessionCounterEnabled: boolean;
  /** Also show the session-limit countdown in the stats overlay while streaming */
  showSessionTimeRemainingInStatsOverlay: boolean;
  sessionClockShowEveryMinutes: number;
  sessionClockShowDurationSeconds: number;
  windowWidth: number;
  windowHeight: number;
  /** Keyboard layout for mapping physical keys inside the remote session */
  keyboardLayout: KeyboardLayout;
  /** In-game language setting (sent to GFN servers via languageCode parameter) */
  gameLanguage: GameLanguage;
  /** User opt-in for NVIDIA's per-game in-game graphics/settings persistence. */
  enablePersistingInGameSettings: boolean;
  /** Experimental request for Low Latency, Low Loss, Scalable throughput on new sessions */
  enableL4S: boolean;
  /**
   * Advertise OpenNOW as the official Steam Deck GFN client via nv-device-* headers
   * and clientPlatformName (does not switch OAuth client ID).
   */
  identifyAsSteamDeck: boolean;
  /** Request Cloud G-Sync / Variable Refresh Rate on new sessions */
  enableCloudGsync: boolean;
  /** Hidden diagnostics for native transition recovery and 240 FPS server-side stream changes */
  nativeTransitionDiagnostics?: NativeTransitionDiagnostics;
  /** Show the currently streaming game as Discord Rich Presence activity */
  discordRichPresence: boolean;
  /** Automatically check GitHub Releases for app updates in the background */
  autoCheckForUpdates: boolean;
  /** Release channel used for application updates */
  updateChannel: UpdateChannel;
  /** When true, pressing Escape will exit fullscreen; when false Escape is sent to the game while pointer-locked */
  allowEscapeToExitFullscreen?: boolean;
  /** Last version for which the release highlights modal was acknowledged (empty = never) */
  lastSeenReleaseHighlightsVersion: string;
  /** Client-side GPU post-processing shaders applied to the stream (web client mode) */
  videoShader: VideoShaderSettings;
  /**
   * First-run consent for anonymous error reporting.
   * `"unset"` shows the one-time prompt; only `"granted"` enables exception capture.
   */
  errorReportingConsent: ErrorReportingConsent;
  /** Anonymous install UUID used as PostHog distinct ID (empty until first grant or feedback) */
  telemetryInstallId: string;
  lowPerformanceMode: boolean;
}

export const SHORTCUT_SETTING_KEYS = [
  "shortcutToggleStats",
  "shortcutTogglePointerLock",
  "shortcutToggleFullscreen",
  "shortcutStopStream",
  "shortcutToggleAntiAfk",
  "shortcutToggleMicrophone",
  "shortcutScreenshot",
  "shortcutToggleRecording",
  "shortcutCyclePacing",
] as const satisfies readonly (keyof Settings)[];

export type ShortcutSettingKey = typeof SHORTCUT_SETTING_KEYS[number];
export type ShortcutSettings = Pick<Settings, ShortcutSettingKey>;

export const DEFAULT_SHORTCUT_SETTINGS: Readonly<ShortcutSettings> = Object.freeze({
  shortcutToggleStats: "Ctrl+N",
  shortcutTogglePointerLock: "F8",
  shortcutToggleFullscreen: "F10",
  shortcutStopStream: "Ctrl+Shift+Q",
  shortcutToggleAntiAfk: "Ctrl+Shift+K",
  shortcutToggleMicrophone: "Ctrl+Shift+M",
  shortcutScreenshot: "F11",
  shortcutToggleRecording: "F12",
  shortcutCyclePacing: "Ctrl+Shift+P",
});

export interface PlatformShortcutDefaults {
  bindings: ShortcutSettings;
  sidebarToggle: string;
  sidebarToggleAliases: string[];
}

export function resolveRuntimePlatform(platform: string): RuntimePlatform {
  const normalized = platform.trim().toLowerCase();
  const exactPlatforms: readonly RuntimePlatform[] = [
    "aix",
    "android",
    "cygwin",
    "darwin",
    "freebsd",
    "haiku",
    "linux",
    "netbsd",
    "openbsd",
    "sunos",
    "win32",
  ];
  if (exactPlatforms.includes(normalized as RuntimePlatform)) {
    return normalized as RuntimePlatform;
  }
  if (normalized.includes("mac")) return "darwin";
  if (normalized.includes("win")) return "win32";
  if (normalized.includes("linux")) return "linux";
  return "unknown";
}

export function createPlatformShortcutDefaults(platform: string): PlatformShortcutDefaults {
  const isMacOs = resolveRuntimePlatform(platform) === "darwin";
  const sidebarToggle = isMacOs ? "Meta+G" : "Ctrl+G";
  return {
    bindings: { ...DEFAULT_SHORTCUT_SETTINGS },
    sidebarToggle,
    sidebarToggleAliases: isMacOs ? [sidebarToggle] : [sidebarToggle, "Ctrl+Shift+G"],
  };
}

export function createDefaultSettings(platform: string): Settings {
  const shortcuts = createPlatformShortcutDefaults(platform);
  return {
    resolution: "1920x1080",
    aspectRatio: "16:9",
    posterSizeScale: 1.05,
    fps: 60,
    maxBitrateMbps: 75,
    jitterBufferMode: "balanced",
    recordingBitrateMbps: null,
    // Safe defaults matching the canvas recorder's low-encode budget: MediaRecorder
    // encodes on the same main thread as the WebRTC decoder, so 720p30 is the
    // default that avoids dropping stream FPS on weak machines. Users can raise
    // resolution/FPS explicitly (settings or in-game Media tab) at their own cost.
    recordingResolution: "720p",
    recordingFps: 30,
    recordingMixMic: false,
    streamClientMode: "web",
    nativeStreamerBackend: "gstreamer",
    nativeVideoBackend: "auto",
    nativeStreamerExecutablePath: "",
    nativeCloudGsyncMode: "auto",
    nativePacingMode: "auto",
    nativeD3dFullscreenMode: "auto",
    nativeExternalRenderer: false,
    nativeStackedRenderer: false,
    transportMode: "webrtc",
    showNativeStreamerStats: false,
    codec: DEFAULT_STREAM_PREFERENCES.codec,
    fallbackCodec: DEFAULT_STREAM_PREFERENCES.fallbackCodec,
    decoderPreference: "auto",
    encoderPreference: "auto",
    colorQuality: DEFAULT_STREAM_PREFERENCES.colorQuality,
    region: "",
    sessionProxyEnabled: false,
    sessionProxyUrl: "",
    clipboardPaste: false,
    enableGyroscopeControls: false,
    steamControllerCompatibilityMode: false,
    nativeCursorOverlay: true,
    // Default ON: sink-native RawInput (WM_INPUT on the video sink window)
    // bypasses the Electron renderer bridge entirely — mouse deltas travel
    // sink wndproc → input thread → data channel with no renderer hops, which
    // is what makes input feel 1:1 in the native streamer.
    nativeSinkInputCapture: true,
    mouseSensitivity: 1,
    mouseAcceleration: 1,
    ...shortcuts.bindings,
    microphoneMode: "disabled",
    microphoneDeviceId: "",
    hideStreamButtons: false,
    showAntiAfkIndicator: true,
    showStatsOnLaunch: false,
    statsOverlayPosition: "bottom-left",
    hideServerSelector: false,
    appAccentColor: "green",
    appTheme: "auto",
    translucentUI: false,
    controllerMode: false,
    launchInConsoleMode: false,
    autoFullScreen: false,
    favoriteGameIds: [],
    sessionCounterEnabled: false,
    showSessionTimeRemainingInStatsOverlay: false,
    sessionClockShowEveryMinutes: 60,
    sessionClockShowDurationSeconds: 30,
    windowWidth: 1400,
    windowHeight: 900,
    keyboardLayout: DEFAULT_KEYBOARD_LAYOUT,
    gameLanguage: "en_US",
    enablePersistingInGameSettings: false,
    enableL4S: false,
    identifyAsSteamDeck: false,
    enableCloudGsync: false,
    nativeTransitionDiagnostics: undefined,
    discordRichPresence: false,
    autoCheckForUpdates: true,
    updateChannel: "stable",
    allowEscapeToExitFullscreen: false,
    lastSeenReleaseHighlightsVersion: "",
    videoShader: { ...DEFAULT_VIDEO_SHADER_SETTINGS },
    errorReportingConsent: "unset",
    telemetryInstallId: "",
    lowPerformanceMode: false,
  };
}

export const DEFAULT_STREAM_PREFERENCES: Readonly<Pick<Settings, "codec" | "colorQuality" | "fallbackCodec">> = Object.freeze({
  codec: "auto",
  fallbackCodec: "auto",
  colorQuality: "8bit_420",
});

export function getDefaultStreamPreferences(): Pick<Settings, "codec" | "colorQuality" | "fallbackCodec"> {
  const normalized = normalizeStreamPreferences(
    DEFAULT_STREAM_PREFERENCES.codec,
    DEFAULT_STREAM_PREFERENCES.colorQuality,
  );
  return {
    codec: normalized.codec,
    fallbackCodec: normalizeFallbackCodecPreference(DEFAULT_STREAM_PREFERENCES.fallbackCodec),
    colorQuality: normalized.colorQuality,
  };
}
