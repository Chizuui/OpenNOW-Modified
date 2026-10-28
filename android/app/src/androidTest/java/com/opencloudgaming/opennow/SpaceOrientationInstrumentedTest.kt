package com.opencloudgaming.opennow

import android.content.pm.ActivityInfo
import android.content.res.Configuration
import android.os.SystemClock
import androidx.lifecycle.ViewModelProvider
import androidx.test.core.app.ActivityScenario
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Test

class SpaceOrientationInstrumentedTest {
    @Test fun realActivityLocksLandscapeAndRestoresItsOriginalOrientation() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val store = SettingsStore(context)
        val saved = store.settings.value
        runBlocking { assertTrue(store.restore(saved.copy(experimentalSpaceUi = false))) }
        try {
            ActivityScenario.launch(MainActivity::class.java).use { scenario ->
                var original = ActivityInfo.SCREEN_ORIENTATION_UNSPECIFIED
                scenario.onActivity { original = it.requestedOrientation }
                fun awaitCondition(check: (MainActivity) -> Boolean) {
                    val deadline = SystemClock.uptimeMillis() + 10_000
                    while (SystemClock.uptimeMillis() < deadline) {
                        var ready = false
                        scenario.onActivity { ready = check(it) }
                        if (ready) return
                        SystemClock.sleep(50)
                    }
                    fail("Activity orientation did not reach the requested state")
                }
                fun setSpace(enabled: Boolean) {
                    scenario.onActivity {
                        val model = ViewModelProvider(it)[OpenNowViewModel::class.java]
                        model.updateSettings(model.state.value.settings.copy(experimentalSpaceUi = enabled))
                    }
                }
                setSpace(true)
                awaitCondition { it.requestedOrientation == ActivityInfo.SCREEN_ORIENTATION_SENSOR_LANDSCAPE &&
                    it.resources.configuration.orientation == Configuration.ORIENTATION_LANDSCAPE }
                setSpace(false)
                awaitCondition { it.requestedOrientation == original }
            }
        } finally {
            runBlocking { assertTrue(store.restore(saved)) }
        }
    }
}
