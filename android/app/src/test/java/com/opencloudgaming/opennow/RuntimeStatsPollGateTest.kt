package com.opencloudgaming.opennow

import org.junit.Assert.*
import org.junit.Test
import java.util.concurrent.CountDownLatch
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit

class RuntimeStatsPollGateTest {
    @Test fun slowCallbackKeepsOnlyOneRequestInFlight() {
        val gate = RuntimeStatsPollGate()
        val request = checkNotNull(gate.begin())
        repeat(20) { assertNull(gate.begin()) }
        assertTrue(gate.isCurrent(request))
        assertTrue(gate.finish(request))
        assertNotNull(gate.begin())
    }

    @Test fun failedEnqueueOrNativeCallCanReleaseTheRequest() {
        val gate = RuntimeStatsPollGate()
        val rejected = checkNotNull(gate.begin())
        assertTrue(gate.finish(rejected))
        val next = checkNotNull(gate.begin())
        assertFalse(gate.finish(rejected))
        assertTrue(gate.isCurrent(next))
    }

    @Test fun oldCallbackAfterReconnectCannotReleaseTheNewRequest() {
        val gate = RuntimeStatsPollGate()
        val old = checkNotNull(gate.begin())
        gate.reset()
        val current = checkNotNull(gate.begin())
        assertFalse(gate.isCurrent(old))
        assertFalse(gate.finish(old))
        assertNull(gate.begin())
        assertTrue(gate.finish(current))
        assertNotNull(gate.begin())
    }

    @Test fun simultaneousRequestsHaveOnlyOneOwner() {
        val gate = RuntimeStatsPollGate()
        val ready = CountDownLatch(4)
        val start = CountDownLatch(1)
        val workers = Executors.newFixedThreadPool(4)
        try {
            val results = (0 until 4).map {
                workers.submit<RuntimeStatsPollGate.Request?> {
                    ready.countDown()
                    check(start.await(5, TimeUnit.SECONDS))
                    gate.begin()
                }
            }
            assertTrue(ready.await(5, TimeUnit.SECONDS))
            start.countDown()
            val winners = results.mapNotNull { it.get(5, TimeUnit.SECONDS) }
            assertEquals(1, winners.size)
            assertTrue(gate.finish(winners.single()))
        } finally {
            start.countDown()
            workers.shutdownNow()
        }
    }
}
