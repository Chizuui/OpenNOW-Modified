package com.opencloudgaming.opennow

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow

/** One first-enable gate for settings and the profile menu; cancellation never changes preferences. */
internal class SpaceUiEnablement(
    private val hasConfirmed: () -> Boolean,
    private val rememberConfirmation: () -> Unit,
    private val setEnabled: (Boolean) -> Unit,
) {
    private val pending = MutableStateFlow(false)
    val confirmationPending = pending.asStateFlow()

    fun request(enabled: Boolean) {
        if (enabled && !hasConfirmed()) {
            pending.value = true
        } else {
            pending.value = false
            setEnabled(enabled)
        }
    }

    fun confirm() {
        if (!pending.value) return
        rememberConfirmation()
        pending.value = false
        setEnabled(true)
    }

    fun cancel() {
        pending.value = false
    }
}
