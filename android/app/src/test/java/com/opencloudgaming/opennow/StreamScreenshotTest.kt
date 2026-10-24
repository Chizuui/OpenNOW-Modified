package com.opencloudgaming.opennow

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class StreamScreenshotTest {
    @Test fun standardSurfacesRetainTheirResolution() {
        for (size in listOf(1280 to 720, 1920 to 1080, 3840 to 2160, 1080 to 2400)) {
            assertEquals(size, screenshotSize(size.first, size.second))
        }
    }

    @Test fun oversizedSurfacesStayWithinBitmapBudgetWithoutChangingAspectRatio() {
        for ((width, height) in listOf(7680 to 4320, 4320 to 7680, 8192 to 8192)) {
            val (outWidth, outHeight) = screenshotSize(width, height)
            assertTrue(outWidth.toLong() * outHeight <= 8_388_608L)
            assertEquals(width.toDouble() / height, outWidth.toDouble() / outHeight, 0.001)
        }
    }

    @Test(expected = IllegalArgumentException::class)
    fun unavailableSurfaceCannotAllocateBitmap() {
        screenshotSize(0, 1080)
    }
}
