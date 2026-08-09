import type { NativeCloudGsyncCapabilities } from "../cloudGsync";
import type {
  AuthDeviceLoginAttemptRequest,
  AuthDeviceLoginChallenge,
  AuthDeviceLoginPollRequest,
  AuthDeviceLoginPollResult,
  AuthDeviceLoginStartRequest,
  AuthLoginRequest,
  AuthSession,
  AuthSessionRequest,
  AuthSessionResult,
  LoginProvider,
  SavedAccount,
} from "./auth";
import type {
  CatalogBrowseRequest,
  CatalogBrowseResult,
  DirectLaunchRequest,
  GameAccountConnectionsResult,
  GameAccountOperationRequest,
  GameAccountOperationResult,
  GameInfo,
  GamePanelResult,
  GamesFetchRequest,
  MarkGameOwnedRequest,
  MarkGameOwnedResult,
  PersistentStorageLocationsFetchRequest,
  PersistentStorageLocationsResult,
  PersistentStorageResetRequest,
  PersistentStorageResetResult,
  PingResult,
  RegionsFetchRequest,
  ResolveLaunchIdRequest,
  ResolveStoreUrlRequest,
  StreamRegion,
  SubscriptionFetchRequest,
} from "./catalog";
import type { MicrophonePermissionResult, Settings } from "./settings";
import type {
  ActiveSessionInfo,
  SessionAdReportRequest,
  SessionClaimRequest,
  SessionConflictChoice,
  SessionCreateRequest,
  SessionInfo,
  SessionPollRequest,
  SessionStopRequest,
} from "./session";
import type {
  IceCandidatePayload,
  KeyframeRequest,
  MainToRendererSignalingEvent,
  NativeInputPacket,
  NativeRenderSurfaceUpdate,
  NativeStreamerShortcutBindings,
  SendAnswerRequest,
  SignalingConnectRequest,
} from "./signaling";
import type { NativeStreamerStatus } from "./nativeStreamer";
import type { SubscriptionInfo } from "./subscription";
import type { ThankYouDataResult } from "./thankYou";
import type { AppUpdaterState, ReleaseHighlightsPayload } from "./updater";
import type {
  MediaListingResult,
  RecordingAbortRequest,
  RecordingBeginRequest,
  RecordingBeginResult,
  RecordingChunkRequest,
  RecordingDeleteRequest,
  RecordingEntry,
  RecordingFinishRequest,
  ScreenshotDeleteRequest,
  ScreenshotEntry,
  ScreenshotSaveAsRequest,
  ScreenshotSaveAsResult,
  ScreenshotSaveRequest,
} from "./media";
import type { PrintedWasteQueueData, PrintedWasteServerMapping } from "./printedWaste";

/**
 * Normalized snapshot of the Chromium GPU process (chrome://gpu equivalent)
 * used to describe the *actual* video decode/encode backends instead of
 * guessing from the platform.
 */
export interface GpuBackendInfo {
  /** Active GPU model name, e.g. "Intel(R) UHD Graphics". */
  gpuName: string | null;
  /** GPU vendor string, e.g. "Intel", "NVIDIA", "AMD". */
  vendorName: string | null;
  /** Active GPU driver version. */
  driverVersion: string | null;
  /** Whether the GPU process reports video decode as hardware accelerated. */
  decodeAccelerated: boolean | null;
  /** Whether the GPU process reports video encode as hardware accelerated. */
  encodeAccelerated: boolean | null;
  /** Codecs the GPU process reports as hardware-decodable (H264, H265, AV1...). */
  hardwareDecodeCodecs: string[];
  /** Codecs the GPU process reports as hardware-encodable. */
  hardwareEncodeCodecs: string[];
}

/** A raw mouse event captured by the native addon (Windows RawInput). */
export interface NativeMouseEventPayload {
  /** 0 = relative move, 1 = button, 2 = wheel */
  kind: number;
  /** move: relative delta X */
  dx: number;
  /** move: relative delta Y */
  dy: number;
  /** button: 0=left 1=right 2=middle 3=x1 4=x2 */
  button: number;
  /** button: 1=down 0=up */
  state: number;
  /** wheel: signed notches (multiples of 120) */
  wheel: number;
}

export interface OpenNowApi {
  getAuthSession(input?: AuthSessionRequest): Promise<AuthSessionResult>;
  getLoginProviders(): Promise<LoginProvider[]>;
  getRegions(input?: RegionsFetchRequest): Promise<StreamRegion[]>;
  login(input: AuthLoginRequest): Promise<AuthSession>;
  startDeviceLogin(input: AuthDeviceLoginStartRequest): Promise<AuthDeviceLoginChallenge>;
  pollDeviceLogin(input: AuthDeviceLoginPollRequest): Promise<AuthDeviceLoginPollResult>;
  completeDeviceLogin(input: AuthDeviceLoginAttemptRequest): Promise<AuthSession>;
  cancelDeviceLogin(input: AuthDeviceLoginAttemptRequest): Promise<void>;
  logout(): Promise<void>;
  logoutAll(): Promise<void>;
  getSavedAccounts(): Promise<SavedAccount[]>;
  switchAccount(userId: string): Promise<AuthSession>;
  removeAccount(userId: string): Promise<void>;
  fetchSubscription(input: SubscriptionFetchRequest): Promise<SubscriptionInfo>;
  fetchPersistentStorageLocations(input?: PersistentStorageLocationsFetchRequest): Promise<PersistentStorageLocationsResult>;
  resetPersistentStorage(input?: PersistentStorageResetRequest): Promise<PersistentStorageResetResult>;
  fetchGameAccountConnections(): Promise<GameAccountConnectionsResult>;
  linkGameAccount(input: GameAccountOperationRequest): Promise<GameAccountOperationResult>;
  unlinkGameAccount(input: GameAccountOperationRequest): Promise<GameAccountOperationResult>;
  resyncGameAccount(input: GameAccountOperationRequest): Promise<GameAccountOperationResult>;
  fetchMainGames(input: GamesFetchRequest): Promise<GameInfo[]>;
  fetchStorePanels(input: GamesFetchRequest): Promise<GamePanelResult[]>;
  fetchFeaturedGames(input: GamesFetchRequest): Promise<GameInfo[]>;
  fetchLibraryGames(input: GamesFetchRequest): Promise<GameInfo[]>;
  browseCatalog(input: CatalogBrowseRequest): Promise<CatalogBrowseResult>;
  fetchPublicGames(): Promise<GameInfo[]>;
  resolveLaunchAppId(input: ResolveLaunchIdRequest): Promise<string | null>;
  resolveStoreUrl(input: ResolveStoreUrlRequest): Promise<string | null>;
  markGameOwned(input: MarkGameOwnedRequest): Promise<MarkGameOwnedResult>;
  getPendingDirectLaunchRequest(): Promise<DirectLaunchRequest | null>;
  onDirectLaunchRequest(listener: (request: DirectLaunchRequest) => void): () => void;
  createSession(input: SessionCreateRequest): Promise<SessionInfo>;
  pollSession(input: SessionPollRequest): Promise<SessionInfo>;
  reportSessionAd(input: SessionAdReportRequest): Promise<SessionInfo>;
  stopSession(input: SessionStopRequest): Promise<void>;
  /** Get list of active sessions (status 2 or 3) */
  getActiveSessions(token?: string, streamingBaseUrl?: string): Promise<ActiveSessionInfo[]>;
  /** Claim/resume an existing session */
  claimSession(input: SessionClaimRequest): Promise<SessionInfo>;
  getNativeStreamerStatus(): Promise<NativeStreamerStatus>;
  getNativeCloudGsyncCapabilities(): Promise<NativeCloudGsyncCapabilities>;
  /** Show dialog asking user how to handle session conflict */
  showSessionConflictDialog(): Promise<SessionConflictChoice>;
  connectSignaling(input: SignalingConnectRequest): Promise<void>;
  disconnectSignaling(): Promise<void>;
  sendAnswer(input: SendAnswerRequest): Promise<void>;
  sendIceCandidate(input: IceCandidatePayload): Promise<void>;
  sendNativeInput(input: NativeInputPacket): void;
  setNativeInputPaused(paused: boolean): void;
  updateNativeRenderSurface(input: NativeRenderSurfaceUpdate): void;
  updateNativeShortcuts(shortcuts: NativeStreamerShortcutBindings): void;

  /** Update the native streamer receive bitrate limit mid-session (Kbps) */
  updateNativeBitrateLimit(maxBitrateKbps: number): void;
  /** Mute/unmute the native streamer microphone (WASAPI send path) mid-session. */
  setNativeMicrophoneEnabled(enabled: boolean): void;
  requestKeyframe(input: KeyframeRequest): Promise<void>;
  onSignalingEvent(listener: (event: MainToRendererSignalingEvent) => void): () => void;
  /** Listen for F11 fullscreen toggle from main process */
  onToggleFullscreen(listener: () => void): () => void;
  onExitFullscreen(listener: () => void): () => void;
  quitApp(): Promise<void>;
  getUpdaterState(): Promise<AppUpdaterState>;
  checkForUpdates(): Promise<AppUpdaterState>;
  downloadUpdate(): Promise<AppUpdaterState>;
  installUpdateAndRestart(): Promise<AppUpdaterState>;
  onUpdaterStateChanged(listener: (state: AppUpdaterState) => void): () => void;
  setFullscreen(v: boolean): Promise<void>;
  toggleFullscreen(): Promise<void>;
  togglePointerLock(): Promise<void>;
  /** Minimize the main window (custom frameless window controls) */
  minimizeWindow(): void;
  /** Toggle maximize/restore; resolves with the resulting maximized state */
  toggleMaximizeWindow(): Promise<boolean>;
  /** Get the current maximized state of the main window */
  getMaximizeWindowState(): Promise<boolean>;
  /** After a stream session ends, exit fullscreen and any stale maximized state */
  restoreWindowAfterSession(): Promise<void>;
  /** Close the main window */
  closeWindow(): void;
  /** Subscribe to maximize/restore changes from the main process */
  onMaximizeWindowStateChanged(listener: (maximized: boolean) => void): () => void;
  /** Notify main process that pointer lock state changed (active = true/false). */
  notifyPointerLockChange(active: boolean, suppressEscapeFullscreenGrace?: boolean): void;
  /** Tell main whether an active native session owns keyboard input through RawInput. */
  notifyNativeInputModeChange(active: boolean, rawInputOwnsEscape: boolean): void;
  /** Read plain text from the OS clipboard through Electron main process */
  readClipboardText(): Promise<string>;
  getSettings(): Promise<Settings>;
  setSetting<K extends keyof Settings>(key: K, value: Settings[K]): Promise<void>;
  /** Snapshot of the Chromium GPU process for accurate codec backend labels */
  getGpuInfo(): Promise<GpuBackendInfo>;
  resetSettings(): Promise<Settings>;
  selectNativeStreamerExecutable(): Promise<string | null>;
  getMicrophonePermission(): Promise<MicrophonePermissionResult>;
  /** Export logs in redacted format */
  exportLogs(format?: "text" | "json"): Promise<string>;
  /** Ping all regions and return latency results */
  pingRegions(regions: StreamRegion[]): Promise<PingResult[]>;

  /** Persist a PNG screenshot from a renderer-generated data URL */
  saveScreenshot(input: ScreenshotSaveRequest): Promise<ScreenshotEntry>;

  /**
   * Capture a screenshot from the native streamer's video chain (last
   * presented frame, PNG) and persist it to the gallery. Only valid while a
   * native streamer session is active.
   */
  captureNativeScreenshot(input: { gameTitle: string }): Promise<ScreenshotEntry>;

  /**
   * Start a native streamer recording (H.264 fragmented MP4). Chunks are
   * streamed to the main process, which appends them to the recording file
   * created by `beginRecording`.
   */
  startNativeRecording(recordingId: string): Promise<void>;

  /**
   * Finalize the native recording; resolves after every chunk was written.
   * Returns the base64 JPEG thumbnail of the first encoded frame, if any.
   */
  stopNativeRecording(): Promise<string | undefined>;

  /** Abort the native recording without finalizing the file. */
  abortNativeRecording(): Promise<void>;

  /**
   * Send a base64 message on a remote WebRTC data channel of the native
   * streamer (e.g. GFN `control_channel` clipboard responses).
   */
  sendNativeDataChannelMessage(label: string, payloadBase64: string): Promise<void>;

  /** List recent screenshots from the persistent screenshot directory */
  listScreenshots(): Promise<ScreenshotEntry[]>;

  /** Delete a screenshot from the persistent screenshot directory */
  deleteScreenshot(input: ScreenshotDeleteRequest): Promise<void>;

  /** Export a screenshot to a user-selected path */
  saveScreenshotAs(input: ScreenshotSaveAsRequest): Promise<ScreenshotSaveAsResult>;

  /** Listen for screenshot hotkey events from the main process (F11) */
  onTriggerScreenshot(listener: () => void): () => void;

  /** Listen for external Escape events forwarded by the main process */
  onExternalEscape(listener: () => void): () => void;

  /** Begin native raw-mouse capture + cursor confinement. Resolves false when
   *  the native addon is unavailable (renderer should fall back to pointer lock). */
  grabNativeMouse(): Promise<boolean>;

  /** Stop native raw-mouse capture and release the cursor. */
  releaseNativeMouse(): Promise<void>;

  /** Listen for raw mouse events (move/button/wheel) from the native addon. */
  onNativeMouseEvent(listener: (ev: NativeMouseEventPayload) => void): () => void;

  /** Open a trusted external URL in the OS default browser */
  openExternalUrl(url: string): Promise<void>;

  /** Begin a new recording session; returns a recordingId to use for subsequent calls */
  beginRecording(input: RecordingBeginRequest): Promise<RecordingBeginResult>;

  /** Stream a chunk of recorded video data to the main process */
  sendRecordingChunk(input: RecordingChunkRequest): Promise<void>;

  /** Finalise a recording; saves the video and optional thumbnail to disk */
  finishRecording(input: RecordingFinishRequest): Promise<RecordingEntry>;

  /** Abort an in-progress recording and remove the temporary file */
  abortRecording(input: RecordingAbortRequest): Promise<void>;

  /** List all saved recordings from the recordings directory */
  listRecordings(): Promise<RecordingEntry[]>;

  /** Delete a saved recording (and its thumbnail if present) */
  deleteRecording(input: RecordingDeleteRequest): Promise<void>;

  /** Reveal a saved recording in the system file manager */
  showRecordingInFolder(id: string): Promise<void>;

  /** List screenshot and recording media, optionally filtered by game title */
  listMediaByGame(input?: { gameTitle?: string }): Promise<MediaListingResult>;

  /** Resolve a thumbnail data URL for a media file path */
  getMediaThumbnail(input: { filePath: string }): Promise<string | null>;

  /** Reveal a media file path in the system file manager */
  showMediaInFolder(input: { filePath: string }): Promise<void>;

  /** Trusted file:// URL for in-app playback of a video under OpenNOW media root, or null */
  getMediaPlaybackUrl(input: { filePath: string }): Promise<string | null>;

  /** Delete a media file under the OpenNOW pictures root (recordings, screenshots, etc.) */
  deleteMediaFile(input: { filePath: string }): Promise<{ ok: boolean }>;

  /** Invalidate cached / sidecar thumbnails and regenerate (returns data URL when possible) */
  regenMediaThumbnail(input: { filePath: string }): Promise<{ ok: boolean; thumbnailDataUrl: string | null }>;

  deleteCache(): Promise<void>;

  /** Fetch current GFN queue wait times from the PrintedWaste API */
  fetchPrintedWasteQueue(): Promise<PrintedWasteQueueData>;
  /** Fetch PrintedWaste server mapping metadata (includes nuked status) */
  fetchPrintedWasteServerMapping(): Promise<PrintedWasteServerMapping>;
  getThanksData(): Promise<ThankYouDataResult>;
  provisionZortosCommunityProxy(): Promise<import("@shared/communityProxy").CommunityProxyProvisionResult>;
  /** Set Discord rich presence activity */
  setDiscordActivity(input: import("../discord").DiscordActivityUpdate): Promise<void>;
  /** Clear Discord rich presence activity */
  clearDiscordActivity(): Promise<void>;

  /** Fetch release highlights payload for a given version (defaults to current) */
  getReleaseHighlights(version?: string): Promise<ReleaseHighlightsPayload>;

  /** Mark the current version's highlights as acknowledged */
  ackReleaseHighlights(): Promise<void>;

  /** Subscribe to automatic release-highlights show events from main process */
  onReleaseHighlightsShow(listener: (payload: ReleaseHighlightsPayload) => void): () => void;

  /** Clear the GStreamer plugin registry cache (rebuilds in the background) */
  clearGStreamerCache(): Promise<{ cleared: boolean; path: string; rebuilding: boolean }>;

  /** Get current GStreamer registry scan status (cache presence + live scan state) */
  getGStreamerScanStatus(): Promise<{
    registryExists: boolean;
    registryPath: string;
    scanInProgress: boolean;
    lastStatus: string | null;
    lastReason: string | null;
  }>;

  /** Subscribe to GStreamer plugin scan status events from the main process */
  onGStreamerScanStatus(listener: (payload: { status: string; reason: string }) => void): () => void;
}
