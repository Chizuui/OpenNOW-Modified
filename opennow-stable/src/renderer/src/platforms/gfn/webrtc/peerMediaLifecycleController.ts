interface PeerMediaLifecycleDependencies {
  videoElement: HTMLVideoElement;
  audioElement: HTMLAudioElement;
  onRenderFrame: () => void;
  log: (message: string) => void;
}

export class PeerMediaLifecycleController {
  private readonly videoStream = new MediaStream();
  private readonly audioStream = new MediaStream();
  private outputVolume = 1;
  private videoPlaybackRecoveryCleanup: (() => void) | null = null;

  constructor(private readonly dependencies: PeerMediaLifecycleDependencies) {
    dependencies.videoElement.srcObject = this.videoStream;
    dependencies.audioElement.srcObject = this.audioStream;
    // Keep the audio element muted until a track arrives; attachTrack() plays
    // it directly. Playing the WebRTC audio track through the media element
    // keeps libwebrtc's A/V sync clock intact — the previous AudioContext
    // re-route decoupled audio from the video pipeline, causing drift and
    // underrun stutter that read as "lag" even with a healthy network.
    dependencies.audioElement.muted = true;
    dependencies.audioElement.volume = this.outputVolume;
  }

  getVideoTrack(): MediaStreamTrack | null {
    return this.videoStream.getVideoTracks()[0] ?? null;
  }

  attachTrack(track: MediaStreamTrack): void {
    if (track.kind === "video") {
      this.replaceTrackInStream(this.videoStream, track);
      const video = this.dependencies.videoElement;
      this.videoPlaybackRecoveryCleanup?.();
      const recoverPlayback = (): void => {
        // Chromium can leave a MediaStream video element paused after an
        // occlusion/focus transition even though inbound-rtp and decoded-frame
        // counters continue advancing. Re-assert playback only when the page
        // is visible/focused; this is safe on Windows, Linux, and macOS and
        // does not alter codec or decoder selection.
        if (document.visibilityState === "hidden" || !video.paused) {
          return;
        }
        void video.play().catch(() => {
          // Autoplay policy may reject until the next user gesture; the next
          // focus/visibility event will retry without interrupting the stream.
        });
      };
      const onVisibilityChange = (): void => {
        if (document.visibilityState === "visible") recoverPlayback();
      };
      const onFocus = (): void => recoverPlayback();
      const onPageShow = (): void => recoverPlayback();
      document.addEventListener("visibilitychange", onVisibilityChange);
      window.addEventListener("focus", onFocus);
      window.addEventListener("pageshow", onPageShow);
      this.videoPlaybackRecoveryCleanup = () => {
        document.removeEventListener("visibilitychange", onVisibilityChange);
        window.removeEventListener("focus", onFocus);
        window.removeEventListener("pageshow", onPageShow);
      };
      const frameCallback = () => {
        this.dependencies.onRenderFrame();
        if (this.videoStream.active) {
          video.requestVideoFrameCallback(frameCallback);
        }
      };
      video.requestVideoFrameCallback(frameCallback);

      this.dependencies.log(
        `Video element before play: paused=${video.paused}, readyState=${video.readyState}, size=${video.videoWidth}x${video.videoHeight}`,
      );
      video
        .play()
        .then(() => {
          this.dependencies.log("Video element playback started");
        })
        .catch((playError) => {
          this.dependencies.log(`Video play() failed: ${String(playError)}`);
        });
      window.setTimeout(() => {
        this.dependencies.log(
          `Video element post-play: paused=${video.paused}, readyState=${video.readyState}, size=${video.videoWidth}x${video.videoHeight}`,
        );
      }, 1500);

      track.onunmute = () => {
        this.dependencies.log("Video track unmuted");
      };
      track.onmute = () => {
        this.dependencies.log("Warning: video track muted by sender");
      };
      track.onended = () => {
        this.dependencies.log("Warning: video track ended");
      };
      this.dependencies.log("Video track attached");
      return;
    }

    if (track.kind === "audio") {
      this.replaceTrackInStream(this.audioStream, track);
      this.dependencies.audioElement.volume = this.outputVolume;
      this.startAudioElementPlayback();
    }
  }

  private startAudioElementPlayback(): void {
    const audio = this.dependencies.audioElement;
    const unlock = (): void => {
      audio.muted = false;
      document.removeEventListener("pointerdown", unlock);
      document.removeEventListener("keydown", unlock);
    };
    audio.muted = false;
    audio
      .play()
      .then(() => {
        this.dependencies.log("Audio track attached (element playback)");
      })
      .catch((playError) => {
        // Unmuted autoplay can be rejected before the first user activation.
        // Fall back to muted playback (always allowed) and unmute on the next
        // user gesture so audio is never silently lost.
        this.dependencies.log(
          `Audio autoplay blocked; will unmute on next gesture: ${String(playError)}`,
        );
        audio.muted = true;
        audio.play().catch(() => {});
        document.addEventListener("pointerdown", unlock);
        document.addEventListener("keydown", unlock);
      });
  }

  setOutputVolume(volume: number): void {
    this.outputVolume = Math.max(
      0,
      Math.min(1, Number.isFinite(volume) ? volume : 1),
    );
    this.dependencies.audioElement.volume = this.outputVolume;
  }

  reset(): void {
    this.videoPlaybackRecoveryCleanup?.();
    this.videoPlaybackRecoveryCleanup = null;
    this.cleanupAudioRouting();
    this.clearTracks();
  }

  cleanupAudio(): void {
    this.cleanupAudioRouting();
  }

  clearTracks(): void {
    this.videoPlaybackRecoveryCleanup?.();
    this.videoPlaybackRecoveryCleanup = null;
    for (const track of this.videoStream.getTracks()) {
      this.videoStream.removeTrack(track);
    }
    for (const track of this.audioStream.getTracks()) {
      this.audioStream.removeTrack(track);
    }
  }

  private replaceTrackInStream(
    stream: MediaStream,
    track: MediaStreamTrack,
  ): void {
    const existingTracks = track.kind === "video"
      ? stream.getVideoTracks()
      : stream.getAudioTracks();
    for (const existingTrack of existingTracks) {
      stream.removeTrack(existingTrack);
    }
    stream.addTrack(track);
  }

  private cleanupAudioRouting(): void {
    this.dependencies.audioElement.pause();
    this.dependencies.audioElement.muted = true;
  }
}
