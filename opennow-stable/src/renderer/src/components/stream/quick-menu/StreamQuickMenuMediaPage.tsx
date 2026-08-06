import type { JSX, RefObject } from "react";
import {
  Camera,
  ChevronLeft,
  ChevronRight,
  Circle,
  FolderOpen,
  Square,
  Trash2,
  Video,
} from "lucide-react";
import type { RecordingEntry, ScreenshotEntry } from "@shared/gfn";
import { SettingRange } from "../../settings/SettingRange";
import { formatElapsed } from "../../../utils/timeFormat";
import { formatFileSize } from "../streamFormatters";

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
  recordingBitrateMbps: number | null;
  recordingResolution: string;
  recordingFps: number;
  onRecordingResolutionChange: (value: string) => void;
  onRecordingFpsChange: (value: number) => void;
  onRecordingBitrateMbpsChange: (value: number | null) => void;
  recCarouselRef: RefObject<HTMLDivElement | null>;
  onToggleRecording: () => void;
  onDeleteRecording: (id: string) => void;
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
  recordingBitrateMbps,
  recordingResolution,
  recordingFps,
  onRecordingResolutionChange,
  onRecordingFpsChange,
  onRecordingBitrateMbpsChange,
  recCarouselRef,
  onToggleRecording,
  onDeleteRecording,
  onScrollRecordings,
}: StreamQuickMenuMediaPageProps): JSX.Element {
  return (
    <div className="sidebar-page" role="tabpanel">
      <section className="sidebar-section">
        <div className="sidebar-section-header">
          <span>Recording Settings</span>
          <span className="sidebar-section-sub">Applies to the next recording.</span>
        </div>
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
        </div>
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
                <img src={shot.dataUrl} alt={`Screenshot ${shot.fileName}`} />
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
                      onClick={() => { void window.openNow.showRecordingInFolder(recording.id); }}
                      disabled={typeof window.openNow?.showRecordingInFolder !== "function"}
                    >
                      <FolderOpen size={11} />
                    </button>
                    <button
                      type="button"
                      className="sidebar-rec-card-action sidebar-rec-card-action--danger"
                      aria-label="Delete recording"
                      title="Delete"
                      onClick={() => onDeleteRecording(recording.id)}
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
