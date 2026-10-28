package com.opencloudgaming.opennow

import org.junit.Assert.*
import org.junit.Test

class SpaceUiEnablementTest {
    private class Preferences {
        var confirmed = false
        var enabled = false
        var confirmationWrites = 0

        fun gate() = SpaceUiEnablement(
            hasConfirmed = { confirmed },
            rememberConfirmation = { confirmed = true; confirmationWrites++ },
            setEnabled = { enabled = it },
        )
    }

    @Test fun firstEnableWaitsForConfirmationAndCancellationKeepsTheWarningForNextTime() {
        val preferences = Preferences()
        val gate = preferences.gate()
        gate.request(true)
        assertTrue(gate.confirmationPending.value)
        assertFalse(preferences.enabled)
        assertFalse(preferences.confirmed)

        gate.cancel()
        assertFalse(gate.confirmationPending.value)
        assertFalse(preferences.enabled)
        assertFalse(preferences.confirmed)

        val reopened = preferences.gate()
        reopened.request(true)
        assertTrue(reopened.confirmationPending.value)
        assertFalse(preferences.enabled)
    }

    @Test fun confirmationEnablesAndFutureEnablesUseTheRememberedChoice() {
        val preferences = Preferences()
        val gate = preferences.gate()
        gate.request(true)
        gate.confirm()
        assertTrue(preferences.enabled)
        assertTrue(preferences.confirmed)
        assertFalse(gate.confirmationPending.value)

        gate.request(false)
        assertFalse(preferences.enabled)
        val reopened = preferences.gate()
        reopened.request(true)
        assertTrue(preferences.enabled)
        assertFalse(reopened.confirmationPending.value)
        assertEquals(1, preferences.confirmationWrites)
    }

    @Test fun disablingAlwaysWorksAndInvalidatesAPendingConfirmation() {
        val preferences = Preferences().apply { enabled = true }
        val gate = preferences.gate()
        gate.request(false)
        assertFalse(preferences.enabled)
        assertFalse(gate.confirmationPending.value)

        gate.request(true)
        gate.request(false)
        gate.confirm()
        assertFalse(preferences.enabled)
        assertFalse(preferences.confirmed)
        assertFalse(gate.confirmationPending.value)
    }

    @Test fun staleOrRepeatedConfirmationDoesNotEnableOrWriteAgain() {
        val preferences = Preferences()
        val gate = preferences.gate()
        gate.confirm()
        assertFalse(preferences.enabled)
        gate.request(true)
        gate.cancel()
        gate.confirm()
        assertFalse(preferences.enabled)
        assertEquals(0, preferences.confirmationWrites)

        gate.request(true)
        gate.confirm()
        gate.confirm()
        assertEquals(1, preferences.confirmationWrites)
    }
}
