package com.idroid.verticalslice

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities
import android.net.NetworkRequest
import android.os.Build
import android.os.IBinder
import android.os.SystemClock
import org.json.JSONObject
import java.io.BufferedInputStream
import java.io.BufferedReader
import java.io.BufferedWriter
import java.io.InputStreamReader
import java.io.OutputStreamWriter
import java.security.SecureRandom
import java.security.cert.X509Certificate
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicInteger
import javax.net.ssl.KeyManagerFactory
import javax.net.ssl.SSLContext
import javax.net.ssl.SSLServerSocket

class ContinuityService : Service() {
    private val executor = Executors.newCachedThreadPool()
    private val generation = AtomicInteger(0)
    private val isConnectingOutbound = java.util.concurrent.atomic.AtomicBoolean(false)
    @Volatile private var hasActiveSession = false
    @Volatile private var server: SSLServerSocket? = null
    @Volatile private var discovery: NsdDiscovery? = null
    @Volatile private var coreLink: NativeCoreLink? = null
    private var networkCallback: ConnectivityManager.NetworkCallback? = null
    private var persistentSessionId: Long = 0L

    override fun onCreate() {
        super.onCreate()
        persistentSessionId = loadOrMintSessionId()
        coreLink = NativeCoreLink.create(SystemClock.elapsedRealtime())
        registerNetworkCallback()
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        createChannel()
        if (Build.VERSION.SDK_INT >= 29) {
            startForeground(
                notificationId,
                notification(),
                ServiceInfo.FOREGROUND_SERVICE_TYPE_CONNECTED_DEVICE,
            )
        } else {
            startForeground(notificationId, notification())
        }
        if (server == null || server?.isClosed == true) {
            startTransport()
        }
        return START_STICKY
    }

    @Synchronized
    private fun startTransport() {
        val currentGen = generation.incrementAndGet()
        executor.execute {
            try {
                AndroidIdentityStore.ensureIdentity()
                val context = tlsContext()
                runCatching { server?.close() }
                val newServer = context.serverSocketFactory.createServerSocket(Signaling.defaultPort) as SSLServerSocket
                newServer.needClientAuth = true
                server = newServer

                discovery?.stop()
                val newDiscovery = NsdDiscovery(this)
                discovery = newDiscovery
                newDiscovery.startAdvertising(Signaling.defaultPort)
                newDiscovery.startBrowsing { host, port ->
                    if (isConnectingOutbound.compareAndSet(false, true)) {
                        executor.execute {
                            try {
                                val success = PairingTransport.reconnect(
                                    this@ContinuityService,
                                    host,
                                    port,
                                    coreLink,
                                    hasActiveSession,
                                )
                                if (success) {
                                    hasActiveSession = true
                                }
                            } catch (_: Throwable) {
                            } finally {
                                isConnectingOutbound.set(false)
                            }
                        }
                    }
                }

                while (generation.get() == currentGen && !newServer.isClosed) {
                    try {
                        val socket = newServer.accept() as javax.net.ssl.SSLSocket
                        executor.execute { handle(socket) }
                    } catch (e: Exception) {
                        if (generation.get() != currentGen || newServer.isClosed) break
                        SystemClock.sleep(200)
                    }
                }
            } catch (_: Throwable) {
                // If the listener failed to bind or initialize, schedule a retry if this generation is active
                if (generation.get() == currentGen) {
                    SystemClock.sleep(1_000)
                    if (generation.get() == currentGen) {
                        startTransport()
                    }
                }
            }
        }
    }

    private fun handle(socket: javax.net.ssl.SSLSocket) {
        socket.use { ssl ->
            try {
                coreLink?.beginConnecting(SystemClock.elapsedRealtime())
                ssl.startHandshake()
                val peer = ssl.session.peerCertificates.firstOrNull() as? X509Certificate ?: run {
                    coreLink?.connectionFailed(SystemClock.elapsedRealtime())
                    return
                }
                val expected = AndroidIdentityStore.macFingerprint(this) ?: run {
                    coreLink?.connectionFailed(SystemClock.elapsedRealtime())
                    return
                }
                if (!AndroidIdentityStore.fingerprint(peer.encoded).equals(expected, true)) {
                    coreLink?.connectionFailed(SystemClock.elapsedRealtime())
                    return
                }
                val input = BufferedInputStream(ssl.inputStream)
                input.mark(10)
                val header = ByteArray(10)
                val readHeader = input.read(header)
                input.reset()

                val isBinaryHello = readHeader >= 10 &&
                    header[0] == 'I'.code.toByte() &&
                    header[1] == 'D'.code.toByte() &&
                    header[2] == 'R'.code.toByte() &&
                    header[3] == '0'.code.toByte() &&
                    header[4] == 2.toByte() &&
                    header[5] == 17.toByte()

                if (isBinaryHello) {
                    val fullHeader = ByteArray(10)
                    var readHeaderBytes: Int = 0
                    while (readHeaderBytes < 10) {
                        val count = input.read(fullHeader, readHeaderBytes, 10 - readHeaderBytes)
                        if (count < 0) break
                        readHeaderBytes += count
                    }
                    val payloadLen = (fullHeader[6].toInt() and 0xFF) or
                        ((fullHeader[7].toInt() and 0xFF) shl 8) or
                        ((fullHeader[8].toInt() and 0xFF) shl 16) or
                        ((fullHeader[9].toInt() and 0xFF) shl 24)
                    val payload = ByteArray(payloadLen)
                    var readPayloadBytes: Int = 0
                    while (readPayloadBytes < payloadLen) {
                        val count = input.read(payload, readPayloadBytes, payloadLen - readPayloadBytes)
                        if (count < 0) break
                        readPayloadBytes += count
                    }
                    if (payloadLen < 36) {
                        coreLink?.connectionFailed(SystemClock.elapsedRealtime())
                        return
                    }
                    val observed = payload.sliceArray(4 until 36).joinToString("") { "%02x".format(it) }
                    if (!observed.equals(expected, ignoreCase = true)) {
                        coreLink?.connectionFailed(SystemClock.elapsedRealtime())
                        return
                    }
                    coreLink?.transportConnected(SystemClock.elapsedRealtime())
                    coreLink?.handshakeReady(SystemClock.elapsedRealtime(), hasActiveSession, persistentSessionId)
                    hasActiveSession = true
                    // Send binary HelloAck frame (golden vector: 494452300212020000000200) followed by newline for backward compatibility
                    val helloAck = byteArrayOf(
                        0x49, 0x44, 0x52, 0x30, // IDR0
                        0x02, // version = 2
                        0x12, // kind = 18 (ControlHelloAck)
                        0x02, 0x00, 0x00, 0x00, // length = 2
                        0x02, 0x00 // selected_version = 2
                    )
                    ssl.outputStream.write(helloAck)
                    ssl.outputStream.write("OK ready\n".toByteArray(Charsets.UTF_8))
                    ssl.outputStream.flush()
                } else {
                    val reader = BufferedReader(InputStreamReader(input, Charsets.UTF_8))
                    val line = reader.readLine() ?: run {
                        coreLink?.connectionFailed(SystemClock.elapsedRealtime())
                        return
                    }
                    val request = JSONObject(line)
                    if (request.optString("type") != "hello" ||
                        request.optString("fingerprint") != expected) {
                        coreLink?.connectionFailed(SystemClock.elapsedRealtime())
                        return
                    }
                    coreLink?.transportConnected(SystemClock.elapsedRealtime())
                    coreLink?.handshakeReady(SystemClock.elapsedRealtime(), hasActiveSession, persistentSessionId)
                    hasActiveSession = true
                    val writer = BufferedWriter(OutputStreamWriter(ssl.outputStream, Charsets.UTF_8))
                    writer.write("OK ready\n")
                    writer.flush()
                }
            } catch (_: Throwable) {
                coreLink?.connectionFailed(SystemClock.elapsedRealtime())
            }
        }
    }

    private fun registerNetworkCallback() {
        val cm = getSystemService(Context.CONNECTIVITY_SERVICE) as? ConnectivityManager ?: return
        val callback = object : ConnectivityManager.NetworkCallback() {
            override fun onAvailable(network: Network) {
                coreLink?.networkChanged(SystemClock.elapsedRealtime())
                startTransport()
            }

            override fun onLost(network: Network) {
                coreLink?.networkChanged(SystemClock.elapsedRealtime())
                startTransport()
            }
        }
        networkCallback = callback
        try {
            if (Build.VERSION.SDK_INT >= 24) {
                cm.registerDefaultNetworkCallback(callback)
            } else {
                val request = NetworkRequest.Builder()
                    .addCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET)
                    .build()
                cm.registerNetworkCallback(request, callback)
            }
        } catch (_: Throwable) {
            // Best effort network monitoring
        }
    }

    private fun loadOrMintSessionId(): Long {
        val prefs = getSharedPreferences("idroid_service", Context.MODE_PRIVATE)
        val existing = prefs.getLong("session_id", 0L)
        if (existing > 0L) return existing
        val minted = (SecureRandom().nextLong() and Long.MAX_VALUE).coerceAtLeast(1L)
        prefs.edit().putLong("session_id", minted).apply()
        return minted
    }

    private fun tlsContext(): SSLContext {
        val keyStore = AndroidIdentityStore.keyStore()
        val keyManagers = KeyManagerFactory.getInstance(KeyManagerFactory.getDefaultAlgorithm()).apply {
            init(keyStore, null)
        }.keyManagers
        val trust = PinnedTrustManager(
            AndroidIdentityStore.macFingerprint(this) ?: "",
            AndroidIdentityStore.macCertificateDer(this),
        )
        return SSLContext.getInstance("TLSv1.3").apply {
            init(keyManagers, arrayOf(trust), null)
        }
    }

    private fun createChannel() {
        if (Build.VERSION.SDK_INT < 26) return
        getSystemService(NotificationManager::class.java)?.createNotificationChannel(
            NotificationChannel(channel, "iDroid continuity", NotificationManager.IMPORTANCE_LOW),
        )
    }

    private fun notification(): Notification = if (Build.VERSION.SDK_INT >= 26) {
        Notification.Builder(this, channel)
            .setContentTitle("iDroid continuity")
            .setContentText("Link discovery is active")
            .setSmallIcon(android.R.drawable.stat_sys_data_bluetooth)
            .setOngoing(true)
            .build()
    } else {
        Notification.Builder(this)
            .setContentTitle("iDroid continuity")
            .setContentText("Link discovery is active")
            .setSmallIcon(android.R.drawable.stat_sys_data_bluetooth)
            .setOngoing(true)
            .build()
    }

    override fun onDestroy() {
        generation.incrementAndGet()
        hasActiveSession = false
        isConnectingOutbound.set(false)
        networkCallback?.let { callback ->
            val cm = getSystemService(Context.CONNECTIVITY_SERVICE) as? ConnectivityManager
            runCatching { cm?.unregisterNetworkCallback(callback) }
        }
        networkCallback = null
        discovery?.stop()
        discovery = null
        runCatching { server?.close() }
        server = null
        coreLink?.close()
        coreLink = null
        executor.shutdownNow()
        super.onDestroy()
    }

    override fun onBind(intent: Intent?): IBinder? = null

    companion object {
        private const val channel = "idroid-continuity"
        private const val notificationId = 810
    }
}
