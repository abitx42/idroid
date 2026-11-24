package com.idroid.verticalslice

import android.content.Context
import android.net.nsd.NsdManager
import android.net.nsd.NsdServiceInfo
import android.os.Build
import android.os.SystemClock
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.Executors
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.atomic.AtomicBoolean

internal class NsdDiscovery(context: Context) {
    private val appContext = context.applicationContext
    private val manager = context.getSystemService(NsdManager::class.java)
    private val serviceType = "_acontinuity._tcp."
    private var registration: NsdManager.RegistrationListener? = null
    private var discovery: NsdManager.DiscoveryListener? = null
    private var multicastLock: android.net.wifi.WifiManager.MulticastLock? = null
    private val resolveExecutor = Executors.newSingleThreadExecutor()
    private val recentPeers = ConcurrentHashMap<String, Long>()
    private val isResolving = AtomicBoolean(false)
    private val resolveQueue = LinkedBlockingQueue<NsdServiceInfo>()
    @Volatile private var myServiceName: String? = "iDroid Android"

    fun startAdvertising(port: Int, onRegistered: (String) -> Unit = {}) {
        val info = NsdServiceInfo().apply {
            serviceName = "iDroid Android"
            serviceType = this@NsdDiscovery.serviceType
            this.port = port
            setAttribute("v", "1")
            setAttribute("role", "android")
        }
        registration = object : NsdManager.RegistrationListener {
            override fun onServiceRegistered(serviceInfo: NsdServiceInfo) {
                myServiceName = serviceInfo.serviceName
                onRegistered(serviceInfo.serviceName)
            }

            override fun onRegistrationFailed(serviceInfo: NsdServiceInfo, errorCode: Int) = Unit
            override fun onServiceUnregistered(serviceInfo: NsdServiceInfo) = Unit
            override fun onUnregistrationFailed(serviceInfo: NsdServiceInfo, errorCode: Int) = Unit
        }
        manager.registerService(info, NsdManager.PROTOCOL_DNS_SD, registration)
    }

    fun startBrowsing(onCandidate: (host: String, port: Int) -> Unit) {
        val wifi = appContext.getSystemService(Context.WIFI_SERVICE) as android.net.wifi.WifiManager
        multicastLock = wifi.createMulticastLock("idroid-nsd").apply {
            setReferenceCounted(false)
            acquire()
        }
        discovery = object : NsdManager.DiscoveryListener {
            override fun onDiscoveryStarted(serviceType: String) = Unit
            override fun onDiscoveryStopped(serviceType: String) = Unit
            override fun onStartDiscoveryFailed(serviceType: String, errorCode: Int) = Unit
            override fun onStopDiscoveryFailed(serviceType: String, errorCode: Int) = Unit
            override fun onServiceFound(serviceInfo: NsdServiceInfo) {
                if (serviceInfo.serviceType != this@NsdDiscovery.serviceType) return
                if (serviceInfo.serviceName == myServiceName) return

                if (Build.VERSION.SDK_INT >= 34) {
                    resolveModern(serviceInfo, onCandidate)
                } else {
                    enqueueResolve(serviceInfo, onCandidate)
                }
            }

            override fun onServiceLost(serviceInfo: NsdServiceInfo) = Unit
        }
        manager.discoverServices(serviceType, NsdManager.PROTOCOL_DNS_SD, discovery)
    }

    private fun resolveModern(
        serviceInfo: NsdServiceInfo,
        onCandidate: (host: String, port: Int) -> Unit,
    ) {
        if (Build.VERSION.SDK_INT < 34) return
        val callback = object : NsdManager.ServiceInfoCallback {
            override fun onServiceUpdated(resolved: NsdServiceInfo) {
                handleResolved(resolved, onCandidate)
                runCatching { manager.unregisterServiceInfoCallback(this) }
            }

            override fun onServiceLost() = Unit
            override fun onServiceInfoCallbackRegistrationFailed(errorCode: Int) = Unit
            override fun onServiceInfoCallbackUnregistered() = Unit
        }
        try {
            manager.registerServiceInfoCallback(serviceInfo, resolveExecutor, callback)
        } catch (_: Throwable) {
            enqueueResolve(serviceInfo, onCandidate)
        }
    }

    private fun enqueueResolve(
        serviceInfo: NsdServiceInfo,
        onCandidate: (host: String, port: Int) -> Unit,
    ) {
        resolveQueue.offer(serviceInfo)
        processNextResolve(onCandidate)
    }

    private fun processNextResolve(onCandidate: (host: String, port: Int) -> Unit) {
        if (!isResolving.compareAndSet(false, true)) return
        resolveExecutor.execute {
            val next = resolveQueue.poll()
            if (next == null) {
                isResolving.set(false)
                return@execute
            }
            try {
                manager.resolveService(next, object : NsdManager.ResolveListener {
                    override fun onServiceResolved(resolved: NsdServiceInfo) {
                        try {
                            handleResolved(resolved, onCandidate)
                        } finally {
                            isResolving.set(false)
                            processNextResolve(onCandidate)
                        }
                    }

                    override fun onResolveFailed(serviceInfo: NsdServiceInfo, errorCode: Int) {
                        isResolving.set(false)
                        processNextResolve(onCandidate)
                    }
                })
            } catch (_: Throwable) {
                isResolving.set(false)
                processNextResolve(onCandidate)
            }
        }
    }

    private fun handleResolved(
        resolved: NsdServiceInfo,
        onCandidate: (host: String, port: Int) -> Unit,
    ) {
        val role = resolved.attributes["role"]?.let { String(it, Charsets.UTF_8) }
        if (role != null && role != "mac") return

        val host = resolved.host?.hostAddress ?: return
        val port = resolved.port
        val key = "$host:$port"
        val now = SystemClock.elapsedRealtime()
        val lastSeen = recentPeers[key] ?: 0L
        if (now - lastSeen < 1_000L) return
        recentPeers[key] = now
        onCandidate(host, port)
    }

    fun stop() {
        registration?.let { runCatching { manager.unregisterService(it) } }
        discovery?.let { runCatching { manager.stopServiceDiscovery(it) } }
        registration = null
        discovery = null
        resolveQueue.clear()
        resolveExecutor.shutdownNow()
        multicastLock?.let { if (it.isHeld) it.release() }
        multicastLock = null
    }
}
