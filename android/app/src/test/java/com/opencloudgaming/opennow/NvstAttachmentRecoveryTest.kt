package com.opencloudgaming.opennow

import org.junit.Assert.*
import org.junit.Test

class NvstAttachmentRecoveryTest {
    @Test fun rejectedOrMissingAttachmentRequestsAResumeClaim() {
        assertTrue(nvstAttachmentRequiresResume("nvst-attachment-rejected: OPTIONS failed: 400 OK"))
        assertTrue(nvstAttachmentRequiresResume("nvst-connect-failed: Could not open RTSPS control channel: HTTP 404"))
        for (error in listOf(
            "nvst-forbidden: OPTIONS failed: 403 Forbidden",
            "nvst-rtsp-failed: SETUP failed: 400 Bad Request",
            "nvst-rtsp-timeout: RTSPS OPTIONS timed out",
            "NVST media timed out; cloud session retained",
        )) assertFalse(error, nvstAttachmentRequiresResume(error))
    }

    @Test fun timeoutsUseLocalRetriesBeforeOneBoundedResume() {
        val gate = NvstAttachmentResumeGate()
        for (error in listOf(
            "nvst-rtsp-timeout: RTSPS ANNOUNCE timed out",
            "NVST media timed out; cloud session retained",
        )) {
            assertFalse(nvstAttachmentRequiresResume(error))
            assertTrue(nvstAttachmentRequiresResume(error, retriesExhausted = true))
        }
        assertTrue(gate.tryAcquire("running-session"))
        assertFalse(gate.tryAcquire("running-session"))
    }

    @Test fun exhaustingRetriesDoesNotClaimForPolicyOrServiceFailures() {
        for (error in listOf(
            "nvst-forbidden: Could not open RTSPS control channel: HTTP 403",
            "nvst-connect-failed: Could not open RTSPS control channel: HTTP 401",
            "nvst-service-unavailable: HTTP 503",
            "nvst-rtsp-failed: SETUP failed: 400 Bad Request",
            "invalid-rtsps-endpoint: RTSPS endpoint has no host",
        )) assertFalse(error, nvstAttachmentRequiresResume(error, retriesExhausted = true))
    }

    @Test fun repeatedRejectedAttachmentCannotKeepReclaimingTheSameCloudSession() {
        val gate = NvstAttachmentResumeGate()
        assertTrue(gate.tryAcquire("running-session"))
        // The gate belongs to the ViewModel; every new native client consults this same budget.
        repeat(4) { assertFalse(gate.tryAcquire("running-session")) }
        assertTrue(gate.tryAcquire("new-session"))
        assertFalse(gate.tryAcquire("new-session"))
    }

    @Test fun stoppingTheSessionClearsTheResumeBudget() {
        val gate = NvstAttachmentResumeGate()
        assertTrue(gate.tryAcquire("running-session"))
        gate.reset()
        assertTrue(gate.tryAcquire("running-session"))
        assertFalse(gate.tryAcquire(""))
    }
}
