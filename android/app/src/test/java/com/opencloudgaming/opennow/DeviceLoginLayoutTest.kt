package com.opencloudgaming.opennow

import android.content.res.Configuration
import androidx.compose.ui.unit.dp
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class DeviceLoginLayoutTest {
    @Test
    fun usesSideBySideLayoutWhenHandheldIsLandscape() {
        assertTrue(
            shouldUseSideBySideDeviceLoginLayout(
                orientation = Configuration.ORIENTATION_LANDSCAPE,
                preferLandscapeLayout = false,
                availableWidth = 640.dp,
            ),
        )
    }

    @Test
    fun keepsPortraitDeviceLoginStacked() {
        assertFalse(
            shouldUseSideBySideDeviceLoginLayout(
                orientation = Configuration.ORIENTATION_PORTRAIT,
                preferLandscapeLayout = false,
                availableWidth = 640.dp,
            ),
        )
    }

    @Test
    fun keepsCrampedLandscapeDeviceLoginStacked() {
        assertFalse(
            shouldUseSideBySideDeviceLoginLayout(
                orientation = Configuration.ORIENTATION_LANDSCAPE,
                preferLandscapeLayout = false,
                availableWidth = 480.dp,
            ),
        )
    }

    @Test
    fun honorsExplicitLandscapePreference() {
        assertTrue(
            shouldUseSideBySideDeviceLoginLayout(
                orientation = Configuration.ORIENTATION_PORTRAIT,
                preferLandscapeLayout = true,
                availableWidth = 320.dp,
            ),
        )
    }
}
