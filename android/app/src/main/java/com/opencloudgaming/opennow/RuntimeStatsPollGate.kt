package com.opencloudgaming.opennow

import java.util.concurrent.atomic.AtomicReference

/** Holds one request through its asynchronous callback, including the queued JNI operation. */
internal class RuntimeStatsPollGate {
    internal class Request
    private val pending = AtomicReference<Request?>()

    fun begin(): Request? {
        if (pending.get() != null) return null
        val request = Request()
        return request.takeIf { pending.compareAndSet(null, it) }
    }

    fun isCurrent(request: Request): Boolean = pending.get() === request

    fun finish(request: Request): Boolean = pending.compareAndSet(request, null)

    /** A callback from the closed peer cannot release a newer peer's request. */
    fun reset() { pending.set(null) }
}
