import { useCallback, useEffect, useRef, useState } from "react";
import type { RefObject } from "react";
import type { RecordingEntry } from "@shared/gfn";
import { fitThumbnailSize, selectRecordingMimeType } from "../components/stream/streamRuntimeHelpers";

// Record at ≤720p / 60fps via canvas downscale to keep MediaRecorder encode cost
// low (it shares the main thread with the WebRTC decoder). GFN uses a GPU encoder
// off-pipeline; this is the closest the web client can get without new tooling.
const RECORD_CAP_WIDTH = 1280;
const RECORD_CAP_HEIGHT = 720;
const RECORD_CAP_FPS = 30;

interface UseStreamRecorderOptions {
  videoRef: RefObject<HTMLVideoElement | null>;
  audioRef: RefObject<HTMLAudioElement | null>;
  gameTitle: string;
  micTrack: MediaStreamTrack | null;
  recordingBitrateMbps: number | null;
}

export function useStreamRecorder({
  videoRef,
  audioRef,
  gameTitle,
  micTrack,
  recordingBitrateMbps,
}: UseStreamRecorderOptions) {
  const [isRecording, setIsRecording] = useState(false);
  const [recordings, setRecordings] = useState<RecordingEntry[]>([]);
  const [recordingDurationMs, setRecordingDurationMs] = useState(0);
  const [recordingError, setRecordingError] = useState<string | null>(null);
  const [usedMimeType, setUsedMimeType] = useState<string | null>(null);
  const mediaRecorderRef = useRef<MediaRecorder | null>(null);
  const recordingIdRef = useRef<string | null>(null);
  const recordingStartTimeRef = useRef(0);
  const recordingTimerRef = useRef<number | undefined>(undefined);
  const recordingFrameCallbackRef = useRef<number | undefined>(undefined);
  const recordingVideoTrackRef = useRef<MediaStreamTrack | null>(null);
  const thumbnailDataUrlRef = useRef<string | null>(null);
  const recCarouselRef = useRef<HTMLDivElement | null>(null);
  const audioCtxRef = useRef<AudioContext | null>(null);

  // The canvas capture track runs its own internal rasterize timer at the
  // capture rate even when drawImage is never called, so it must be stopped
  // explicitly or it keeps burning main-thread CPU after recording ends.
  const stopRecordingVideoTrack = (): void => {
    const track = recordingVideoTrackRef.current;
    if (track && track.readyState === "live") {
      track.stop();
    }
    recordingVideoTrackRef.current = null;
  };
  const recordingApiAvailable =
    typeof window.openNow?.beginRecording === "function" &&
    typeof window.openNow?.sendRecordingChunk === "function" &&
    typeof window.openNow?.finishRecording === "function" &&
    typeof window.openNow?.abortRecording === "function" &&
    typeof window.openNow?.listRecordings === "function" &&
    typeof window.openNow?.deleteRecording === "function";

  const refreshRecordings = useCallback(async () => {
    setRecordingError(null);
    if (!recordingApiAvailable) return;
    try {
      const items = await window.openNow.listRecordings();
      setRecordings(items);
    } catch (error) {
      console.error("[StreamView] Failed to load recordings:", error);
      setRecordingError("Unable to load recordings.");
    }
  }, [recordingApiAvailable]);

  const deleteRecording = useCallback(async (id: string) => {
    setRecordingError(null);
    if (!recordingApiAvailable) return;
    try {
      await window.openNow.deleteRecording({ id });
      setRecordings((prev) => prev.filter((recording) => recording.id !== id));
    } catch (error) {
      console.error("[StreamView] Failed to delete recording:", error);
      setRecordingError("Unable to delete recording.");
    }
  }, [recordingApiAvailable]);

  const scrollRecordings = useCallback((direction: "left" | "right") => {
    const strip = recCarouselRef.current;
    if (!strip) return;
    strip.scrollBy({ left: direction === "left" ? -200 : 200, behavior: "smooth" });
  }, []);

  const toggleRecording = useCallback(async () => {
    setRecordingError(null);

    if (isRecording) {
      mediaRecorderRef.current?.stop();
      return;
    }

    if (!recordingApiAvailable) {
      setRecordingError("Recording API unavailable. Restart OpenNOW to enable recording.");
      return;
    }

    const video = videoRef.current;
    if (!video || !video.srcObject) {
      setRecordingError("Stream is not ready for recording yet.");
      return;
    }

    const stream = video.srcObject as MediaStream;
    const mimeType = selectRecordingMimeType((candidate) => MediaRecorder.isTypeSupported(candidate));
    setUsedMimeType(mimeType);

    const audioCtx = new AudioContext();
    audioCtxRef.current = audioCtx;
    const audioDest = audioCtx.createMediaStreamDestination();

    const audioElement = audioRef.current;
    const gameAudioStream = audioElement?.srcObject instanceof MediaStream ? audioElement.srcObject : null;
    if (gameAudioStream && gameAudioStream.getAudioTracks().length > 0) {
      audioCtx.createMediaStreamSource(gameAudioStream).connect(audioDest);
    }

    if (micTrack && micTrack.readyState === "live") {
      const micStream = new MediaStream([micTrack]);
      audioCtx.createMediaStreamSource(micStream).connect(audioDest);
    }

    // Record via a canvas downscale (720p@60) instead of re-encoding the raw
    // 1080p60 stream track. MediaRecorder encodes on the same main thread as the
    // WebRTC decoder, so full-res re-encode starves the CPU and makes the stream
    // stutter — and in severe cases drops ICE back to the home screen. Capping
    // pixels keeps encode cost low (GFN-native uses a GPU H.264 encoder off the
    // frame pipeline; the web client can't, so this is the closest parity).
    let recordVideoTrack: MediaStreamTrack | null = null;
    if (video.videoWidth > 0 && video.videoHeight > 0) {
      const scale = Math.min(
        1,
        RECORD_CAP_WIDTH / video.videoWidth,
        RECORD_CAP_HEIGHT / video.videoHeight,
      );
      const capWidth = Math.max(1, Math.round(video.videoWidth * scale));
      const capHeight = Math.max(1, Math.round(video.videoHeight * scale));
      const cap = document.createElement("canvas");
      cap.width = capWidth;
      cap.height = capHeight;
      const capCtx = cap.getContext("2d");
      const capStream = cap.captureStream(RECORD_CAP_FPS);
      recordVideoTrack = capStream.getVideoTracks()[0];
      if (recordVideoTrack) {
        recordVideoTrack.contentHint = "detail";
        recordingVideoTrackRef.current = recordVideoTrack;
      }
      // Draw only when a NEW video frame is presented and cap it to the
      // recording frame rate. The old loop re-queued requestAnimationFrame
      // unconditionally, so on 120/144 fps streams the main thread ran a
      // synchronous full-frame drawImage per display refresh — starving the
      // WebRTC decoder/compositor and dropping stream FPS.
      let lastDrawMs = 0;
      const drawLatestFrame = (now: number): void => {
        recordingFrameCallbackRef.current = video.requestVideoFrameCallback(drawLatestFrame);
        if (!capCtx || video.videoWidth === 0 || video.readyState < 2) {
          return;
        }
        if (now - lastDrawMs >= 1000 / RECORD_CAP_FPS) {
          lastDrawMs = now;
          capCtx.drawImage(video, 0, 0, capWidth, capHeight);
        }
      };
      recordingFrameCallbackRef.current = video.requestVideoFrameCallback(drawLatestFrame);
    }
    const videoTracksForRecord = recordVideoTrack ? [recordVideoTrack] : stream.getVideoTracks();

    const composed = new MediaStream([
      ...videoTracksForRecord,
      ...audioDest.stream.getAudioTracks(),
    ]);

    let recordingId: string;
    try {
      const result = await window.openNow.beginRecording({ mimeType });
      recordingId = result.recordingId;
    } catch (error) {
      console.error("[StreamView] Failed to begin recording:", error);
      const frameVideo = videoRef.current;
      if (recordingFrameCallbackRef.current !== undefined && frameVideo) {
        frameVideo.cancelVideoFrameCallback?.(recordingFrameCallbackRef.current);
      }
      recordingFrameCallbackRef.current = undefined;
      stopRecordingVideoTrack();
      audioCtx.close().catch(() => undefined);
      audioCtxRef.current = null;
      setRecordingError("Could not start recording.");
      return;
    }

    recordingIdRef.current = recordingId;
    thumbnailDataUrlRef.current = null;
    recordingStartTimeRef.current = Date.now();
    setRecordingDurationMs(0);
    setIsRecording(true);

    recordingTimerRef.current = window.setInterval(() => {
      setRecordingDurationMs(Date.now() - recordingStartTimeRef.current);
    }, 500);

    let isFirstChunk = true;
    const recorderOptions: MediaRecorderOptions = { mimeType };
    if (recordingBitrateMbps !== null) {
      // 720p30 never benefits from more than ~12 Mbps; capping avoids burning
      // main-thread MediaRecorder encode budget on a bitrate the picture can't
      // use (a major source of stream FPS drops during recording). Auto mode
      // (null) is intentionally left untouched: Chromium picks a conservative
      // resolution-based default (~2.5 Mbps for 720p) that's already below the
      // cap, so overriding it would only add encode load.
      recorderOptions.videoBitsPerSecond =
        Math.max(1, Math.min(12, Math.round(recordingBitrateMbps))) * 1_000_000;
    }
    const recorder = new MediaRecorder(composed, recorderOptions);

    recorder.ondataavailable = (event: BlobEvent) => {
      if (!event.data || event.data.size === 0) return;

      if (isFirstChunk) {
        isFirstChunk = false;
        const currentVideo = videoRef.current;
        if (currentVideo && currentVideo.videoWidth > 0 && currentVideo.videoHeight > 0) {
          const { width, height } = fitThumbnailSize(
            currentVideo.videoWidth,
            currentVideo.videoHeight,
          );
          const canvas = document.createElement("canvas");
          canvas.width = width;
          canvas.height = height;
          const context = canvas.getContext("2d");
          if (context) {
            context.drawImage(currentVideo, 0, 0, width, height);
            thumbnailDataUrlRef.current = canvas.toDataURL("image/jpeg", 0.72);
          }
        }
      }

      void event.data.arrayBuffer().then((buffer) => {
        const id = recordingIdRef.current;
        if (!id) return;
        window.openNow.sendRecordingChunk({ recordingId: id, chunk: buffer }).catch((error: unknown) => {
          console.error("[StreamView] Failed to send recording chunk:", error);
        });
      });
    };

    recorder.onstop = () => {
      window.clearInterval(recordingTimerRef.current);
      recordingTimerRef.current = undefined;
      const frameVideo = videoRef.current;
      if (recordingFrameCallbackRef.current !== undefined && frameVideo) {
        frameVideo.cancelVideoFrameCallback?.(recordingFrameCallbackRef.current);
      }
      recordingFrameCallbackRef.current = undefined;
      stopRecordingVideoTrack();
      audioCtxRef.current?.close().catch(() => undefined);
      audioCtxRef.current = null;
      const id = recordingIdRef.current;
      recordingIdRef.current = null;
      setIsRecording(false);

      if (!id) return;

      const durationMs = Date.now() - recordingStartTimeRef.current;
      void window.openNow
        .finishRecording({
          recordingId: id,
          durationMs,
          gameTitle,
          thumbnailDataUrl: thumbnailDataUrlRef.current ?? undefined,
        })
        .then((entry) => {
          setRecordings((prev) => [entry, ...prev].slice(0, 20));
          thumbnailDataUrlRef.current = null;
        })
        .catch((error: unknown) => {
          console.error("[StreamView] Failed to finish recording:", error);
          setRecordingError("Recording could not be saved.");
        });
    };

    recorder.onerror = () => {
      window.clearInterval(recordingTimerRef.current);
      recordingTimerRef.current = undefined;
      const frameVideo = videoRef.current;
      if (recordingFrameCallbackRef.current !== undefined && frameVideo) {
        frameVideo.cancelVideoFrameCallback?.(recordingFrameCallbackRef.current);
      }
      recordingFrameCallbackRef.current = undefined;
      stopRecordingVideoTrack();
      audioCtxRef.current?.close().catch(() => undefined);
      audioCtxRef.current = null;
      const id = recordingIdRef.current;
      recordingIdRef.current = null;
      setIsRecording(false);
      thumbnailDataUrlRef.current = null;
      if (id) {
        window.openNow.abortRecording({ recordingId: id }).catch(() => undefined);
      }
      setRecordingError("Recording encountered an error.");
    };

    mediaRecorderRef.current = recorder;
    recorder.start(5000);
  }, [
    audioRef,
    gameTitle,
    isRecording,
    micTrack,
    recordingApiAvailable,
    recordingBitrateMbps,
    videoRef,
  ]);

  useEffect(() => {
    return () => {
      window.clearInterval(recordingTimerRef.current);
      const frameVideo = videoRef.current;
      if (recordingFrameCallbackRef.current !== undefined && frameVideo) {
        frameVideo.cancelVideoFrameCallback?.(recordingFrameCallbackRef.current);
      }
      recordingFrameCallbackRef.current = undefined;
      stopRecordingVideoTrack();
      const recorder = mediaRecorderRef.current;
      const id = recordingIdRef.current;
      if (recorder && recorder.state !== "inactive") {
        recorder.stop();
      }
      if (id) {
        window.openNow.abortRecording({ recordingId: id }).catch(() => undefined);
        recordingIdRef.current = null;
      }
      audioCtxRef.current?.close().catch(() => undefined);
      audioCtxRef.current = null;
    };
  }, []);

  return {
    isRecording,
    recordings,
    recordingDurationMs,
    recordingError,
    usedMimeType,
    recCarouselRef,
    recordingApiAvailable,
    refreshRecordings,
    deleteRecording,
    scrollRecordings,
    toggleRecording,
  };
}
