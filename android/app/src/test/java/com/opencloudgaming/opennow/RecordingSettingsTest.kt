package com.opencloudgaming.opennow

import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import org.junit.Assert.*
import org.junit.Test

class RecordingSettingsTest {
    @Test fun existingSettingsKeepPersonalInputsOff() {
        val restored = Json.decodeFromString<StreamSettings>("{}")
        assertFalse(restored.recordingMicrophoneEnabled)
        assertFalse(restored.recordingCameraEnabled)
    }

    @Test fun optionsPersistIndependentlyAndSurvivePresetChanges() {
        for (mic in listOf(false, true)) for (camera in listOf(false, true)) {
            val original = StreamSettings(recordingMicrophoneEnabled = mic, recordingCameraEnabled = camera)
            val restored = Json.decodeFromString<StreamSettings>(Json.encodeToString(original))
            val preset = StreamSettings().withUserStreamOptionsFrom(restored)
            assertEquals(mic, preset.recordingMicrophoneEnabled)
            assertEquals(camera, preset.recordingCameraEnabled)
            assertEquals(MicrophoneMode.Disabled, preset.microphoneMode)
        }
    }
}
