package com.opencloudgaming.opennow.domain

import android.app.Application
import android.util.Log
import com.opencloudgaming.opennow.AppSettings
import com.opencloudgaming.opennow.SettingsStore
import com.opencloudgaming.opennow.StreamPreset
import kotlinx.coroutines.flow.StateFlow

private const val TAG = "SettingsUseCase"

/**
 * Use case handling settings operations.
 * Extracted from OpenNowViewModel for better separation of concerns.
 */
class SettingsUseCase(
    private val application: Application,
    private val settingsStore: SettingsStore,
) {
    /**
     * Get the current settings as a StateFlow.
     */
    val settings: StateFlow<AppSettings> = settingsStore.settings

    /**
     * Update settings with a transformation function.
     */
    fun update(transform: (AppSettings) -> AppSettings) {
        settingsStore.update(transform)
    }

    /**
     * Update specific stream settings.
     */
    fun updateStream(transform: (com.opencloudgaming.opennow.StreamSettings) -> com.opencloudgaming.opennow.StreamSettings) {
        settingsStore.update { current ->
            current.copy(stream = transform(current.stream))
        }
    }

    /**
     * Apply a stream preset.
     */
    fun applyStreamPreset(preset: StreamPreset) {
        settingsStore.update { current ->
            current.copy(
                streamPreset = preset,
                stream = current.stream.applyingStreamPreset(preset),
            )
        }
    }

    /**
     * Reset settings to defaults.
     */
    fun resetToDefaults() {
        settingsStore.update { AppSettings() }
    }

    /**
     * Get the current settings value.
     */
    fun currentValue(): AppSettings {
        return settingsStore.settings.value
    }

    /**
     * Update UI accent color.
     */
    fun updateAccent(accent: com.opencloudgaming.opennow.UiAccent) {
        settingsStore.update { it.copy(uiAccent = accent) }
    }

    /**
     * Toggle dynamic color.
     */
    fun toggleDynamicColor() {
        settingsStore.update { it.copy(dynamicColor = !it.dynamicColor) }
    }

    /**
     * Toggle nerd mode.
     */
    fun toggleNerdMode() {
        settingsStore.update { it.copy(nerdMode = !it.nerdMode) }
    }

    /**
     * Toggle analytics opt-out.
     */
    fun setAnalyticsOptOut(optOut: Boolean) {
        settingsStore.update {
            it.copy(
                analyticsOptOut = optOut,
                analyticsConsentAsked = true,
            )
        }
    }

    /**
     * Update touch settings.
     */
    fun updateTouchSettings(transform: (com.opencloudgaming.opennow.AndroidTouchSettings) -> com.opencloudgaming.opennow.AndroidTouchSettings) {
        settingsStore.update { current ->
            current.copy(androidTouch = transform(current.androidTouch))
        }
    }
}
