package com.opencloudgaming.opennow

import android.content.Context
import android.net.wifi.WifiManager
import android.os.Build

/** Owned by NativeStreamClient for both transports; low latency requires foreground/screen-on. */
internal class StreamWifiPerformanceLock(context: Context) {
    private val wifi = context.applicationContext.getSystemService(WifiManager::class.java)
    private var lock: WifiManager.WifiLock? = null

    @Suppress("DEPRECATION")
    fun acquire() {
        if (lock != null) return
        runCatching {
            val mode = if (Build.VERSION.SDK_INT >= 29) WifiManager.WIFI_MODE_FULL_LOW_LATENCY
                else WifiManager.WIFI_MODE_FULL_HIGH_PERF
            val acquired = wifi?.createWifiLock(mode, "OpenNOW:stream") ?: return
            acquired.setReferenceCounted(false)
            acquired.acquire()
            lock = acquired
            NativeInputDiagnostics.add("stream Wi-Fi performance lock acquired mode=$mode")
        }.onFailure {
            NativeInputDiagnostics.add("stream Wi-Fi performance lock unavailable: ${it.javaClass.simpleName}")
        }
    }

    fun release() {
        val acquired = lock ?: return
        lock = null
        runCatching { if (acquired.isHeld) acquired.release() }
            .onFailure { NativeInputDiagnostics.add("stream Wi-Fi lock release failed: ${it.javaClass.simpleName}") }
    }
}
