package com.opencloudgaming.opennow.domain

import android.util.Log
import com.opencloudgaming.opennow.StreamSettings
import com.opencloudgaming.opennow.StreamPreset
import com.opencloudgaming.opennow.VideoCodec
import com.opencloudgaming.opennow.normalizeStreamResolutionForAspectAndPlan
import com.opencloudgaming.opennow.streamSettingsSessionSignature
import com.opencloudgaming.opennow.withCodecColorCompatibility
import com.opencloudgaming.opennow.withHdrAllowed
import com.opencloudgaming.opennow.withResolutionAllowed
import com.opencloudgaming.opennow.withFpsAllowed
import com.opencloudgaming.opennow.StreamResolutionPlan
import com.opencloudgaming.opennow.SubscriptionInfo

private const val TAG = "StreamUseCase"

/**
 * Use case handling stream settings operations.
 * Extracted from OpenNowViewModel for better separation of concerns.
 */
class StreamUseCase {

    /**
     * Apply a stream preset to the current settings.
     */
    fun applyPreset(
        settings: StreamSettings,
        preset: StreamPreset,
    ): StreamSettings {
        if (preset == StreamPreset.Custom) return settings
        return settings.applyingStreamPreset(preset)
    }

    /**
     * Normalize resolution for the given aspect ratio and subscription plan.
     */
    fun normalizeResolution(
        resolution: String,
        aspectRatio: String,
        subscriptionInfo: SubscriptionInfo?,
        fallbackMembershipTier: String?,
    ): String {
        return normalizeStreamResolutionForAspectAndPlan(
            resolution = resolution,
            aspectRatio = aspectRatio,
            subscriptionInfo = subscriptionInfo,
            fallbackMembershipTier = fallbackMembershipTier,
        )
    }

    /**
     * Ensure settings are compatible with codec and color combinations.
     */
    fun withCodecCompatibility(settings: StreamSettings): StreamSettings {
        return settings.withCodecColorCompatibility()
    }

    /**
     * Ensure HDR settings are allowed by the subscription plan.
     */
    fun withHdrAllowed(
        settings: StreamSettings,
        subscriptionInfo: SubscriptionInfo?,
        fallbackMembershipTier: String?,
    ): StreamSettings {
        return settings.withHdrAllowed(subscriptionInfo, fallbackMembershipTier)
    }

    /**
     * Ensure resolution is allowed by the subscription plan.
     */
    fun withResolutionAllowed(
        settings: StreamSettings,
        subscriptionInfo: SubscriptionInfo?,
        fallbackMembershipTier: String?,
    ): StreamSettings {
        return settings.withResolutionAllowed(subscriptionInfo, fallbackMembershipTier)
    }

    /**
     * Ensure FPS is allowed by the subscription plan.
     */
    fun withFpsAllowed(
        settings: StreamSettings,
        subscriptionInfo: SubscriptionInfo?,
        fallbackMembershipTier: String?,
    ): StreamSettings {
        return settings.withFpsAllowed(subscriptionInfo, fallbackMembershipTier)
    }

    /**
     * Generate a session signature for the given settings.
     */
    fun sessionSignature(settings: StreamSettings): String {
        return streamSettingsSessionSignature(settings)
    }

    /**
     * Get the maximum allowed FPS for the subscription plan.
     */
    fun maxAllowedFps(subscriptionInfo: SubscriptionInfo?, fallbackMembershipTier: String?): Int {
        return if (hasUltimatePlan(subscriptionInfo, fallbackMembershipTier)) 120 else 60
    }

    /**
     * Check if the user has an Ultimate streaming plan.
     */
    fun hasUltimatePlan(subscriptionInfo: SubscriptionInfo?, fallbackMembershipTier: String?): Boolean {
        return com.opencloudgaming.opennow.hasUltimateStreamingPlan(subscriptionInfo, fallbackMembershipTier)
    }

    /**
     * Check if the user has HDR streaming capability.
     */
    fun hasHdrCapability(subscriptionInfo: SubscriptionInfo?, fallbackMembershipTier: String?): Boolean {
        return com.opencloudgaming.opennow.hasHdrStreamingPlan(subscriptionInfo, fallbackMembershipTier)
    }

    companion object {
        private const val MAX_STANDARD_STREAM_FPS = 60
        private const val MAX_ULTIMATE_STREAM_FPS = 120
    }
}
