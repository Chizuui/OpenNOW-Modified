import { useRef } from "react";
import type { JSX, RefObject } from "react";
import {
  Camera,
  ChevronLeft,
  ChevronRight,
  Circle,
  FolderOpen,
  Save,
  Square,
  Trash2,
  Video,
} from "lucide-react";
import type { RecordingEntry, ScreenshotEntry } from "@shared/gfn";
import { SettingRange } from "../../settings/SettingRange";
import { formatElapsed } from "../../../utils/timeFormat";
import { formatFileSize } from "../streamFormatters";
import { useMicMeter } from "../../../hooks/useMicMeter";

const RESOLUTION_OPTIONS = [
  { value: "720p", label: "720p" },
  { value: "1080p", label: "1080p" },
  { value: "1440p", label: "1440p" },
] as const;

interface StreamQuickMenuMediaPageProps {
  screenshotShortcut: string;
  screenshots: ScreenshotEntry[];
  isSavingScreenshot: boolean;
  screenshotApiAvailable: boolean;
  galleryError: string | null;
  galleryStripRef: RefObject<HTMLDivElement | null>;
  onCaptureScreenshot: () => void;
  onSelectScreenshot: (id: string) => void;
  onScrollGallery: (direction: "left" | "right") => void;
  recordingShortcut: string;
  recordings: RecordingEntry[];
  isRecording: boolean;
  recordingDurationMs: number;
  recordingError: string | null;
  recordingApiAvailable: boolean;
  usedMimeType: string | null;
  /** Recording strategy chosen at start (raw-track vs canvas-downscale). */
  usedStrategy: string | null;
  recordingBitrateMbps: number | null;
  recordingResolution: string;
  recordingFps: number;
  recordingMixMic: boolean;
  /**
   * Live microphone track (same one the app sends to the stream). Shown on a
   * level meter while an encoded recording is mixing the mic, so the user can
   * confirm the mic is actually being picked up by the recording path.
   */
  micTrack: MediaStreamTrack | null;
  /**
   * Native streamer mode: the recording happens in the native pipeline at the
   * stream's own resolution/FPS/bitrate — the web-mode encode settings below
   * do not apply, so the Resolution/FPS/Bitrate controls are hidden.
   */
  nativeRecording: boolean;
  /** Shown after a recording that dropped frames (encoder/queue could not keep up). */
  recordingDropNotice: string | null;
  /** Instant Replay buffer state (official capture parity). */
  instantReplayEnabled: boolean;
  instantReplaySeconds: number;
  /** True while a replay save is muxing — disables the save button. */
  isSavingInstantReplay: boolean;
  /** Display string of the Instant Replay save key (e.g. "Ctrl+0"). */
  instantReplaySaveShortcut: string;
  /** Sets the persisted Instant Replay setting (chips; the hotkey toggles). */
  onSetInstantReplay: (value: boolean) => void;
  /** Saves the buffered replay window; outcomes surface as capture toasts. */
  onSaveInstantReplay: () => void;
  onInstantReplaySecondsChange: (value: number) => void;
  onRecordingResolutionChange: (value: string) => void;
  onRecordingFpsChange: (value: number) => void;
  onRecordingBitrateMbpsChange: (value: number | null) => void;
  onRecordingMixMicChange: (value: boolean) => void;
  recCarouselRef: RefObject<HTMLDivElement | null>;
  onToggleRecording: () => void;
  onDeleteRecording: (filePath: string) => void;
  onScrollRecordings: (direction: "left" | "right") => void;
}

export function StreamQuickMenuMediaPage({
  screenshotShortcut,
  screenshots,
  isSavingScreenshot,
  screenshotApiAvailable,
  galleryError,
  galleryStripRef,
  onCaptureScreenshot,
  onSelectScreenshot,
  onScrollGallery,
  recordingShortcut,
  recordings,
  isRecording,
  recordingDurationMs,
  recordingError,
  recordingApiAvailable,
  usedMimeType,
  usedStrategy,
  recordingBitrateMbps,
  recordingResolution,
  recordingFps,
  recordingMixMic,
  micTrack,
  nativeRecording,
  recordingDropNotice,
  instantReplayEnabled,
  instantReplaySeconds,
  isSavingInstantReplay,
  instantReplaySaveShortcut,
  onSetInstantReplay,
  onSaveInstantReplay,
  onInstantReplaySecondsChange,
  onRecordingResolutionChange,
  onRecordingFpsChange,
  onRecordingBitrateMbpsChange,
  onRecordingMixMicChange,
  recCarouselRef,
  onToggleRecording,
  onDeleteRecording,
  onScrollRecordings,
}: StreamQuickMenuMediaPageProps): JSX.Element {
  // Live mic level meter: while an encoded bitstream recording is actively
  // mixing the microphone, show the real signal level so the user can confirm
  // the mic is being picked up (or not) by the recording path. Reuses the
  // same segmented RMS meter as the Controls page, but scoped to this page
  // and gated on the recording actually running with mic mix enabled.
  const micMeterRef = useRef<HTMLCanvasElement | null>(null);
  const micLevelMeterActive =
    isRecording && usedStrategy === "encoded-transform" && recordingMixMic;
  useMicMeter(micMeterRef, micTrack, micLevelMeterActive);
  return (
    <div className="sidebar-page" role="tabpanel">
      <section className="sidebar-section">
        <div className="sidebar-section-header">
          <span>Instant Replay</span>
          <span className="sidebar-section-sub">Save key: {instantReplaySaveShortcut}</span>
        </div>
        <div className="sidebar-row sidebar-row--column">
          <div className="sidebar-row-top">
            <span className="sidebar-label">Replay buffer</span>
            <span className="settings-value-badge">{instantReplayEnabled ? "On" : "Off"}</span>
          </div>
          <div className="sidebar-chip-row">
            <button
              type="button"
              className={`sidebar-chip${instantReplayEnabled ? " sidebar-chip--active" : ""}`}
              aria-pressed={instantReplayEnabled}
              onClick={() => onSetInstantReplay(true)}
            >
              <span>On</span>
            </button>
            <button
              type="button"
              className={`sidebar-chip${!instantReplayEnabled ? " sidebar-chip--active" : ""}`}
              aria-pressed={!instantReplayEnabled}
              onClick={() => onSetInstantReplay(false)}
            >
              <span>Off</span>
            </button>
          </div>
          <span className="sidebar-hint">
            {instantReplayEnabled
              ? `The last ${formatElapsed(instantReplaySeconds)} of gameplay is buffered and ready to save.`
              : "Off — turn it on to buffer recent gameplay for saving."}
          </span>
        </div>
        <div className="sidebar-row sidebar-row--column">
          <div className="sidebar-row-top">
            <span className="sidebar-label">Buffer length</span>
            <span className="settings-value-badge">{formatElapsed(instantReplaySeconds)}</span>
          </div>
          <SettingRange
            id="quick-menu-instant-replay-seconds"
            className="settings-slider"
            min={5}
            max={300}
            step={5}
            value={instantReplaySeconds}
            onPreview={onInstantReplaySecondsChange}
            onCommit={onInstantReplaySecondsChange}
          />
          <span className="sidebar-hint">
            How much recent gameplay is kept. Changes apply from the next session.
          </span>
        </div>
        <div className="sidebar-row sidebar-row--aligned">
          <span className="sidebar-label">Save replay</span>
          <button
            type="button"
            className="sidebar-button sidebar-screenshot-button"
            onClick={onSaveInstantReplay}
            disabled={isSavingInstantReplay || !instantReplayEnabled}
          >
            <Save size={14} />
            <span>{isSavingInstantReplay ? "Saving..." : "Save"}</span>
          </button>
        </div>
        {instantReplayEnabled && isRecording && !isSavingInstantReplay && (
          <span className="sidebar-hint">Stop the recording to save the replay.</span>
        )}
      </section>
      <div className="sidebar-separator" aria-hidden="true" />
      <section className="sidebar-section">
        <div className="sidebar-section-header">
          <span>Recording Settings</span>
          <span className="sidebar-section-sub">
            {nativeRecording ? "Records at stream quality" : "Applies to the next recording."}
          </span>
        </div>
        {nativeRecording ? (
          <div className="sidebar-row sidebar-row--column">
            <span className="sidebar-hint">
              Native mode records the stream at its full quality — resolution, frame rate, and
              bitrate follow the stream and cannot be changed here.
            </span>
          </div>
        ) : (
        <>
        <div className="sidebar-row sidebar-row--column">
          <div className="sidebar-row-top">
            <span className="sidebar-label">Resolution</span>
            <span className="settings-value-badge">{recordingResolution}</span>
          </div>
          <div className="sidebar-chip-row">
            {RESOLUTION_OPTIONS.map((option) => (
              <button
                key={option.value}
                type="button"
                className={`sidebar-chip${recordingResolution === option.value ? " sidebar-chip--active" : ""}`}
                aria-pressed={recordingResolution === option.value}
                onClick={() => onRecordingResolutionChange(option.value)}
              >
                <span>{option.label}</span>
              </button>
            ))}
          </div>
        </div>
        <div className="sidebar-row sidebar-row--column">
          <div className="sidebar-row-top">
            <span className="sidebar-label">Frame Rate</span>
            <span className="settings-value-badge">{recordingFps} FPS</span>
          </div>
          <div className="sidebar-chip-row">
            {[30, 60].map((fps) => (
              <button
                key={fps}
                type="button"
                className={`sidebar-chip${recordingFps === fps ? " sidebar-chip--active" : ""}`}
                aria-pressed={recordingFps === fps}
                onClick={() => onRecordingFpsChange(fps)}
              >
                <span>{fps}</span>
              </button>
            ))}
          </div>
        </div>
        <div className="sidebar-row sidebar-row--column">
          <div className="sidebar-row-top">
            <span className="sidebar-label">Bitrate</span>
            <span className="settings-value-badge">
              {recordingBitrateMbps === null ? "Auto" : `${recordingBitrateMbps} Mbps`}
            </span>
          </div>
          <div className="sidebar-chip-row">
            <button
              type="button"
              className={`sidebar-chip${recordingBitrateMbps === null ? " sidebar-chip--active" : ""}`}
              aria-pressed={recordingBitrateMbps === null}
              onClick={() => onRecordingBitrateMbpsChange(null)}
            >
              <span>Auto</span>
            </button>
            <button
              type="button"
              className={`sidebar-chip${recordingBitrateMbps !== null ? " sidebar-chip--active" : ""}`}
              aria-pressed={recordingBitrateMbps !== null}
              onClick={() => onRecordingBitrateMbpsChange(recordingBitrateMbps ?? 75)}
            >
              <span>Custom</span>
            </button>
          </div>
          <SettingRange
            id="quick-menu-recording-bitrate"
            className="settings-slider"
            min={5}
            max={200}
            step={5}
            value={recordingBitrateMbps ?? 75}
            disabled={recordingBitrateMbps === null}
            onPreview={onRecordingBitrateMbpsChange}
            onCommit={onRecordingBitrateMbpsChange}
          />
          <span className="sidebar-hint">
            Recordings are encoded on the CPU. Higher resolution and FPS raise encode load and
            can drop stream FPS on weaker machines.
          </span>
          <span className="sidebar-hint">
            Recordings are encoded on the CPU. Higher resolution and FPS raise encode load and
            can drop stream FPS on weaker machines.
          </span>
        </div>
        <div className="sidebar-row sidebar-row--column">
          <div className="sidebar-row-top">
            <span className="sidebar-label">Mix Mic</span>
            <span className="settings-value-badge">{recordingMixMic ? "On" : "Off"}</span>
          </div>
          <div className="sidebar-chip-row">
            <button
              type="button"
              className={`sidebar-chip${recordingMixMic ? " sidebar-chip--active" : ""}`}
              aria-pressed={recordingMixMic}
              onClick={() => onRecordingMixMicChange(true)}
            >
              <span>On</span>
            </button>
            <button
              type="button"
              className={`sidebar-chip${!recordingMixMic ? " sidebar-chip--active" : ""}`}
              aria-pressed={!recordingMixMic}
              onClick={() => onRecordingMixMicChange(false)}
            >
              <span>Off</span>
            </button>
          </div>
          <span className="sidebar-hint">
            Mixes your microphone into the recording's audio track. Only the audio is
            re-encoded — the video bitstream is captured as-is, so recording stays
            zero-cost. Requires an active mic.
          </span>
        </div>
        {micLevelMeterActive && (
          <div className="sidebar-row sidebar-row--column">
            <div className="sidebar-row-top">
              <span className="sidebar-label">Mic level</span>
              <span className="settings-value-badge">Live</span>
            </div>
            <canvas
              ref={micMeterRef}
              className="mic-meter-canvas"
              aria-label="Microphone level being mixed into the recording"
            />
            <span className="sidebar-hint">
              Live level of the mic being mixed into this recording — silence means the mic
              is not being picked up.
            </span>
          </div>
        )}
        </>
        )}
      </section>
      <div className="sidebar-separator" aria-hidden="true" />
      <section className="sidebar-section">
        <div className="sidebar-section-header">
          <span>Gallery</span>
          <span className="sidebar-section-sub">Screenshot key: {screenshotShortcut}</span>
        </div>
        <div className="sidebar-row sidebar-row--aligned">
          <span className="sidebar-label">Screenshots</span>
          <button
            type="button"
            className="sidebar-button sidebar-screenshot-button"
            onClick={onCaptureScreenshot}
            disabled={isSavingScreenshot || !screenshotApiAvailable}
          >
            <Camera size={14} />
            <span>{isSavingScreenshot ? "Capturing..." : "Capture"}</span>
          </button>
        </div>
        <div className="sidebar-gallery-row">
          <button
            type="button"
            className="sidebar-gallery-arrow"
            onClick={() => onScrollGallery("left")}
            aria-label="Scroll gallery left"
          >
            <ChevronLeft size={16} />
          </button>
          <div className="sidebar-gallery-strip" ref={galleryStripRef}>
            {screenshots.map((shot) => (
              <button
                key={shot.id}
                type="button"
                className="sidebar-gallery-item"
                onClick={() => onSelectScreenshot(shot.id)}
                title={new Date(shot.createdAtMs).toLocaleString()}
              >
                <img src={shot.thumbnailDataUrl} alt={`Screenshot ${shot.fileName}`} />
              </button>
            ))}
          </div>
          <button
            type="button"
            className="sidebar-gallery-arrow"
            onClick={() => onScrollGallery("right")}
            aria-label="Scroll gallery right"
          >
            <ChevronRight size={16} />
          </button>
        </div>
        {screenshots.length === 0 && (
          <span className="sidebar-hint">No screenshots yet. Press {screenshotShortcut} to capture one.</span>
        )}
        {galleryError && <span className="sidebar-hint sidebar-hint--error">{galleryError}</span>}
      </section>
      <div className="sidebar-separator" aria-hidden="true" />
      <section className="sidebar-section">
        <div className="sidebar-section-header">
          <span>Recordings</span>
          <span className="sidebar-section-sub">Record key: {recordingShortcut}</span>
        </div>
        {usedMimeType && (
          <span className="sidebar-hint sidebar-hint--codec">Codec: {usedMimeType}</span>
        )}
        {usedStrategy && (
          <span className="sidebar-hint sidebar-hint--codec">
            Mode:{" "}
            {usedStrategy === "encoded-transform"
              ? `encoded bitstream (no re-encode${recordingMixMic ? ", mic mix" : ""})`
              : usedStrategy === "raw-track"
                ? "raw track (stream res)"
                : "bounded canvas"}
          </span>
        )}
        <div className="sidebar-row sidebar-row--aligned">
          <span className="sidebar-label">
            {isRecording ? `Recording ${formatElapsed(Math.round(recordingDurationMs / 1000))}` : "Record"}
          </span>
          <button
            type="button"
            className="sidebar-button sidebar-screenshot-button"
            onClick={onToggleRecording}
            disabled={!recordingApiAvailable}
          >
            {isRecording ? <Square size={14} /> : <Circle size={14} />}
            <span>{isRecording ? "Stop" : "Start"}</span>
          </button>
        </div>
        {recordingError && (
          <span className="sidebar-hint sidebar-hint--error">{recordingError}</span>
        )}
        {recordingDropNotice && (
          <span className="sidebar-hint sidebar-hint--drop">{recordingDropNotice}</span>
        )}
        {recordings.length === 0 ? (
          <span className="sidebar-hint">No recordings yet. Press {recordingShortcut} to record.</span>
        ) : (
          <div className="sidebar-gallery-row">
            <button
              type="button"
              className="sidebar-gallery-arrow"
              onClick={() => onScrollRecordings("left")}
              aria-label="Scroll recordings left"
            >
              <ChevronLeft size={16} />
            </button>
            <div className="sidebar-rec-strip" ref={recCarouselRef}>
              {recordings.map((recording) => (
                <div key={recording.id} className="sidebar-rec-card">
                  {recording.thumbnailDataUrl ? (
                    <img
                      className="sidebar-rec-card-thumb"
                      src={recording.thumbnailDataUrl}
                      alt=""
                    />
                  ) : (
                    <div className="sidebar-rec-card-thumb sidebar-rec-card-thumb--placeholder">
                      <Video size={20} />
                    </div>
                  )}
                  <div className="sidebar-rec-card-meta">
                    <span className="sidebar-rec-card-title">{recording.gameTitle ?? "Untitled"}</span>
                    <span className="sidebar-rec-card-detail">
                      {formatElapsed(Math.round(recording.durationMs / 1000))} · {formatFileSize(recording.sizeBytes)}
                    </span>
                  </div>
                  <div className="sidebar-rec-card-actions">
                    <button
                      type="button"
                      className="sidebar-rec-card-action"
                      aria-label="Show in folder"
                      title="Show in folder"
                      onClick={() => { void window.openNow.showRecordingInFolder(recording.filePath); }}
                      disabled={typeof window.openNow?.showRecordingInFolder !== "function"}
                    >
                      <FolderOpen size={11} />
                    </button>
                    <button
                      type="button"
                      className="sidebar-rec-card-action sidebar-rec-card-action--danger"
                      aria-label="Delete recording"
                      title="Delete"
                      onClick={() => onDeleteRecording(recording.filePath)}
                    >
                      <Trash2 size={11} />
                    </button>
                  </div>
                </div>
              ))}
            </div>
            <button
              type="button"
              className="sidebar-gallery-arrow"
              onClick={() => onScrollRecordings("right")}
              aria-label="Scroll recordings right"
            >
              <ChevronRight size={16} />
            </button>
          </div>
        )}
      </section>
    </div>
  );
}
