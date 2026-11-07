package com.idroid.verticalslice

import android.content.Context
import android.net.nsd.NsdManager
import android.net.nsd.NsdServiceInfo
import android.os.Build

internal class NsdDiscovery(context: Context) {
    private val appContext = context.applicationContext
    private val manager = context.getSystemService(NsdManager::class.java)
    private val serviceType = "_acontinuity._tcp."
    private var registration: NsdManager.RegistrationListener? = null
    private var discovery: NsdManager.DiscoveryListener? = null
    private var multicastLock: android.net.wifi.WifiManager.MulticastLock? = null

    fun startAdvertising(port: Int, onRegistered: (String) -> Unit = {}) {
        val info = NsdServiceInfo().apply {
            serviceName = "iDroid Android"
            serviceType = this@NsdDiscovery.serviceType
            this.port = port
            if (Build.VERSION.SDK_INT >= 21) {
                setAttribute("v", "1")
                setAttribute("role", "android")
            }
        }
        registration = object : NsdManager.RegistrationListener {
            override fun onServiceRegistered(serviceInfo: NsdServiceInfo) {
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
                if (serviceInfo.serviceName == "iDroid Android") return
                manager.resolveService(serviceInfo, object : NsdManager.ResolveListener {
                    override fun onServiceResolved(resolved: NsdServiceInfo) {
                        onCandidate(resolved.host.hostAddress ?: return, resolved.port)
                    }

                    override fun onResolveFailed(serviceInfo: NsdServiceInfo, errorCode: Int) = Unit
                })
            }

            override fun onServiceLost(serviceInfo: NsdServiceInfo) = Unit
        }
        manager.discoverServices(serviceType, NsdManager.PROTOCOL_DNS_SD, discovery)
    }

    fun stop() {
        registration?.let { runCatching { manager.unregisterService(it) } }
        discovery?.let { runCatching { manager.stopServiceDiscovery(it) } }
        registration = null
        discovery = null
        multicastLock?.let { if (it.isHeld) it.release() }
        multicastLock = null
    }
}
