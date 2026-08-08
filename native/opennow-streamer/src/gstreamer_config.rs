// Always compiled so present-policy unit tests run without the optional
// `gstreamer` feature; production callers live behind that feature.
#![allow(dead_code)]

pub(crate) const EXTERNAL_RENDERER_ENV: &str = "OPENNOW_NATIVE_EXTERNAL_RENDERER";
pub(crate) const RENDER_MODE_ENV: &str = "OPENNOW_NATIVE_RENDER_MODE";
pub(crate) const NATIVE_VIDEO_API_ENV: &str = "OPENNOW_NATIVE_VIDEO_API";
pub(crate) const NATIVE_VIDEO_BACKEND_ENV: &str = "OPENNOW_NATIVE_VIDEO_BACKEND";
pub(crate) const NATIVE_ZERO_COPY_ENV: &str = "OPENNOW_NATIVE_ZERO_COPY";
pub(crate) const NATIVE_PRESENT_MAX_FPS_ENV: &str = "OPENNOW_NATIVE_PRESENT_MAX_FPS";
pub(crate) const NATIVE_D3D_FULLSCREEN_ENV: &str = "OPENNOW_NATIVE_D3D_FULLSCREEN";
pub(crate) const AV1_DECODER_ENV: &str = "OPENNOW_NATIVE_AV1_DECODER";
pub(crate) const H265_DECODER_ENV: &str = "OPENNOW_NATIVE_H265_DECODER";
pub(crate) const PRESENT_LIMITER_AUTO_SENTINEL: u32 = u32::MAX;
pub(crate) const PRESENT_LIMITER_VRR_SENTINEL: u32 = u32::MAX - 1;
const VRR_REFRESH_HEADROOM_FPS: u32 = 3;

pub(crate) fn use_external_renderer_window() -> bool {
    std::env::var(EXTERNAL_RENDERER_ENV)
        .map(|value| {
            !matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "0" | "false" | "no" | "off"
            )
        })
        // Default to the internal child-surface renderer (single Electron window).
        .unwrap_or(false)
}

pub(crate) fn use_internal_renderer() -> bool {
    !use_external_renderer_window()
}

/// Render surface strategy for the native video sink.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NativeRenderMode {
    /// Floating fullscreen window owned by the streamer (legacy external).
    External,
    /// Separate top-level window sized/positioned to the stream rect and
    /// stacked directly below the Electron window (GFN-style, video behind
    /// a transparent UI shell).
    Stacked,
    /// Child surface embedded inside the Electron window (single window).
    Embedded,
}

/// Resolve the render mode, honouring `OPENNOW_NATIVE_RENDER_MODE` first and
/// falling back to the legacy `OPENNOW_NATIVE_EXTERNAL_RENDERER` boolean.
pub(crate) fn render_mode() -> NativeRenderMode {
    if let Ok(value) = std::env::var(RENDER_MODE_ENV) {
        match value.trim().to_ascii_lowercase().as_str() {
            "stacked" => return NativeRenderMode::Stacked,
            "embedded" | "internal" => return NativeRenderMode::Embedded,
            "external" | "floating" => return NativeRenderMode::External,
            _ => {}
        }
    }
    if use_external_renderer_window() {
        NativeRenderMode::External
    } else {
        NativeRenderMode::Embedded
    }
}

pub(crate) fn use_stacked_renderer() -> bool {
    render_mode() == NativeRenderMode::Stacked
}

pub(crate) fn requested_video_backend() -> String {
    std::env::var(NATIVE_VIDEO_BACKEND_ENV)
        .or_else(|_| std::env::var(NATIVE_VIDEO_API_ENV))
        .unwrap_or_else(|_| "auto".to_owned())
        .to_ascii_lowercase()
}

pub(crate) fn zero_copy_requested() -> bool {
    matches!(
        std::env::var(NATIVE_ZERO_COPY_ENV)
            .unwrap_or_else(|_| "auto".to_owned())
            .to_ascii_lowercase()
            .as_str(),
        "1" | "true" | "yes" | "forced"
    )
}

/// Decoder selection override for a codec family, resolved from
/// `OPENNOW_NATIVE_AV1_DECODER` / `OPENNOW_NATIVE_H265_DECODER`.
///
/// Some Windows D3D DXVA decoders are unreliable on specific GPUs (e.g.
/// Intel UHD AV1 hardware decode corrupts every frame), so the app exposes a
/// per-codec decoder override. `Software` maps to `dav1ddec` for AV1 and
/// `avdec_h265` for H265/HEVC.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CodecDecoderPreference {
    Auto,
    D3D12,
    D3D11,
    Software,
}

pub(crate) fn decoder_preference_from_value(value: &str, codec: &str) -> CodecDecoderPreference {
    let upper = codec.to_ascii_uppercase();
    let is_av1 = upper == "AV1";
    let is_h265 = upper == "H265" || upper == "HEVC";
    if !is_av1 && !is_h265 {
        return CodecDecoderPreference::Auto;
    }
    match value.trim().to_ascii_lowercase().as_str() {
        "auto" | "default" | "" => CodecDecoderPreference::Auto,
        "d3d12" | "hardware12" => CodecDecoderPreference::D3D12,
        "d3d11" | "hardware11" => CodecDecoderPreference::D3D11,
        "dav1d" | "avdec" | "software" | "sw" => CodecDecoderPreference::Software,
        _ => CodecDecoderPreference::Auto,
    }
}

pub(crate) fn av1_decoder_preference() -> CodecDecoderPreference {
    std::env::var(AV1_DECODER_ENV)
        .map(|value| decoder_preference_from_value(&value, "AV1"))
        .unwrap_or(CodecDecoderPreference::Auto)
}

pub(crate) fn h265_decoder_preference() -> CodecDecoderPreference {
    std::env::var(H265_DECODER_ENV)
        .map(|value| decoder_preference_from_value(&value, "H265"))
        .unwrap_or(CodecDecoderPreference::Auto)
}

pub(crate) fn resolve_present_max_fps(cloud_gsync_enabled: bool) -> u32 {
    if let Ok(value) = std::env::var(NATIVE_PRESENT_MAX_FPS_ENV) {
        let value = value.trim().to_ascii_lowercase();
        if value == "0" || value == "off" || value == "false" || value == "unlimited" {
            return 0;
        }
        if value == "auto" {
            return PRESENT_LIMITER_AUTO_SENTINEL;
        }
        if let Ok(fps) = value.parse::<u32>() {
            return fps;
        }
    }
    if cloud_gsync_enabled {
        PRESENT_LIMITER_VRR_SENTINEL
    } else {
        0
    }
}

pub(crate) fn automatic_present_max_fps(requested_fps: u32, display_hz: Option<u32>) -> u32 {
    display_hz
        .filter(|display_hz| *display_hz >= 30 && *display_hz < requested_fps)
        .unwrap_or(0)
}

pub(crate) fn vrr_present_max_fps(requested_fps: u32, display_hz: Option<u32>) -> u32 {
    display_hz
        .filter(|display_hz| *display_hz >= 30 && *display_hz <= requested_fps)
        .map(|display_hz| display_hz.saturating_sub(VRR_REFRESH_HEADROOM_FPS))
        .unwrap_or(0)
}

pub(crate) fn resolve_d3d_fullscreen_sink(cloud_gsync_enabled: bool) -> bool {
    if use_stacked_renderer() {
        // Stacked mode must never go exclusive fullscreen — the sink window
        // stays a regular top-level window stacked behind the Electron shell.
        return false;
    }
    resolve_d3d_fullscreen_sink_for(
        use_internal_renderer(),
        cloud_gsync_enabled,
        std::env::var(NATIVE_D3D_FULLSCREEN_ENV).ok(),
    )
}

/// Pure policy for exclusive D3D fullscreen present.
///
/// Internal (child HWND) always stays windowed — exclusive fullscreen fights
/// Electron parenting. External may enable it for Cloud G-Sync/VRR, or via
/// `OPENNOW_NATIVE_D3D_FULLSCREEN`.
pub(crate) fn resolve_d3d_fullscreen_sink_for(
    internal_renderer: bool,
    cloud_gsync_enabled: bool,
    env_override: Option<String>,
) -> bool {
    if internal_renderer {
        return false;
    }

    if let Some(value) = env_override {
        let value = value.trim().to_ascii_lowercase();
        if matches!(value.as_str(), "1" | "on" | "true" | "yes") {
            return true;
        }
        if matches!(value.as_str(), "0" | "off" | "false" | "no") {
            return false;
        }
    }

    cloud_gsync_enabled
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automatic_present_limiter_uses_display_refresh_below_requested_fps() {
        assert_eq!(automatic_present_max_fps(240, Some(165)), 165);
        assert_eq!(automatic_present_max_fps(240, Some(240)), 0);
        assert_eq!(automatic_present_max_fps(240, Some(1)), 0);
        assert_eq!(automatic_present_max_fps(240, None), 0);
    }

    #[test]
    fn default_present_policy_is_uncapped_without_vrr() {
        assert_eq!(resolve_present_max_fps(false), 0);
        assert_eq!(
            resolve_present_max_fps(true),
            PRESENT_LIMITER_VRR_SENTINEL
        );
    }

    #[test]
    fn vrr_present_limiter_stays_below_refresh_ceiling() {
        assert_eq!(vrr_present_max_fps(240, Some(165)), 162);
        assert_eq!(vrr_present_max_fps(165, Some(165)), 162);
        assert_eq!(vrr_present_max_fps(120, Some(165)), 0);
        assert_eq!(vrr_present_max_fps(240, None), 0);
    }

    #[test]
    fn internal_renderer_never_enables_exclusive_d3d_fullscreen() {
        assert!(!resolve_d3d_fullscreen_sink_for(true, true, None));
        assert!(!resolve_d3d_fullscreen_sink_for(
            true,
            true,
            Some("1".to_owned())
        ));
        assert!(!resolve_d3d_fullscreen_sink_for(
            true,
            false,
            Some("on".to_owned())
        ));
    }

    #[test]
    fn external_renderer_follows_cloud_gsync_and_env_for_d3d_fullscreen() {
        assert!(resolve_d3d_fullscreen_sink_for(false, true, None));
        assert!(!resolve_d3d_fullscreen_sink_for(false, false, None));
        assert!(resolve_d3d_fullscreen_sink_for(
            false,
            false,
            Some("1".to_owned())
        ));
        assert!(!resolve_d3d_fullscreen_sink_for(
            false,
            true,
            Some("0".to_owned())
        ));
    }

    #[test]
    fn decoder_preference_parses_values_per_codec() {
        assert_eq!(
            decoder_preference_from_value("dav1d", "AV1"),
            CodecDecoderPreference::Software
        );
        assert_eq!(
            decoder_preference_from_value("software", "AV1"),
            CodecDecoderPreference::Software
        );
        assert_eq!(
            decoder_preference_from_value("avdec", "H265"),
            CodecDecoderPreference::Software
        );
        assert_eq!(
            decoder_preference_from_value("software", "HEVC"),
            CodecDecoderPreference::Software
        );
        assert_eq!(
            decoder_preference_from_value("d3d12", "AV1"),
            CodecDecoderPreference::D3D12
        );
        assert_eq!(
            decoder_preference_from_value("d3d11", "H265"),
            CodecDecoderPreference::D3D11
        );
        assert_eq!(
            decoder_preference_from_value("auto", "AV1"),
            CodecDecoderPreference::Auto
        );
        assert_eq!(
            decoder_preference_from_value("garbage", "AV1"),
            CodecDecoderPreference::Auto
        );
        // The override only applies to AV1/H265 codec families.
        assert_eq!(
            decoder_preference_from_value("dav1d", "H264"),
            CodecDecoderPreference::Auto
        );
    }

    #[test]
    fn decoder_preference_env_honours_codec_specific_vars() {
        unsafe {
            std::env::set_var(AV1_DECODER_ENV, "dav1d");
            std::env::set_var(H265_DECODER_ENV, "software");
        }
        assert_eq!(av1_decoder_preference(), CodecDecoderPreference::Software);
        assert_eq!(h265_decoder_preference(), CodecDecoderPreference::Software);
        unsafe {
            std::env::remove_var(AV1_DECODER_ENV);
            std::env::remove_var(H265_DECODER_ENV);
        }
        assert_eq!(av1_decoder_preference(), CodecDecoderPreference::Auto);
        assert_eq!(h265_decoder_preference(), CodecDecoderPreference::Auto);
    }

    #[test]
    fn render_mode_prefers_explicit_env_and_falls_back_to_legacy_boolean() {
        // Explicit stacked mode wins even if the legacy boolean says external.
        unsafe {
            std::env::set_var(RENDER_MODE_ENV, "stacked");
            std::env::set_var(EXTERNAL_RENDERER_ENV, "1");
        }
        assert_eq!(render_mode(), NativeRenderMode::Stacked);
        assert!(use_stacked_renderer());

        unsafe {
            std::env::set_var(RENDER_MODE_ENV, "embedded");
        }
        assert_eq!(render_mode(), NativeRenderMode::Embedded);
        assert!(!use_stacked_renderer());

        // Legacy fallback: external flag on => External.
        unsafe {
            std::env::remove_var(RENDER_MODE_ENV);
            std::env::set_var(EXTERNAL_RENDERER_ENV, "1");
        }
        assert_eq!(render_mode(), NativeRenderMode::External);

        // Legacy fallback: external flag off => Embedded.
        unsafe {
            std::env::set_var(EXTERNAL_RENDERER_ENV, "0");
        }
        assert_eq!(render_mode(), NativeRenderMode::Embedded);

        unsafe {
            std::env::remove_var(RENDER_MODE_ENV);
            std::env::remove_var(EXTERNAL_RENDERER_ENV);
        }
    }
}
