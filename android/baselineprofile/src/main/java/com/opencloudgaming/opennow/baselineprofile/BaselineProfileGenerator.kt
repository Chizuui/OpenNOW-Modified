package com.opencloudgaming.opennow.baselineprofile

import androidx.benchmark.macro.junit4.BaselineProfileRule
import androidx.test.uiautomator.By
import androidx.test.uiautomator.Direction
import androidx.test.uiautomator.Until
import org.junit.Rule
import org.junit.Test

/**
 * Generates a Baseline Profile for the critical startup + catalog-scroll path.
 *
 * Run on a rooted emulator or Gradle Managed Device:
 *   ./gradlew :app:generateBaselineProfile
 * which writes app/src/main/generated/baselineProfiles/baseline-prof.txt, packaged
 * into the release APK and installed at first run by androidx.profileinstaller.
 */
class BaselineProfileGenerator {
    @get:Rule val rule = BaselineProfileRule()

    @Test
    fun generate() = rule.collect(packageName = "com.opencloudgaming.opennow") {
        pressHome()
        startActivityAndWait()

        // Let the first frame settle, then exercise the catalog grid — the hottest
        // scrolling surface — so its composition/layout code gets pre-compiled.
        device.waitForIdle()
        val content = device.findObject(By.scrollable(true))
        if (content != null) {
            content.setGestureMargin(device.displayWidth / 5)
            repeat(3) {
                content.scroll(Direction.DOWN, 0.8f)
                device.waitForIdle()
            }
            content.scroll(Direction.UP, 0.8f)
        }
        device.wait(Until.hasObject(By.scrollable(true)), 2_000)
    }
}
