package com.idroid.verticalslice

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.Service
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import android.os.SystemClock
import org.json.JSONObject
import java.io.BufferedReader
import java.io.BufferedWriter
import java.io.InputStreamReader
import java.io.OutputStreamWriter
import java.net.ServerSocket
import java.security.cert.X509Certificate
import java.util.concurrent.Executors
import javax.net.ssl.KeyManagerFactory
import javax.net.ssl.SSLContext
import javax.net.ssl.SSLServerSocket

class ContinuityService : Service() {
    private val executor = Executors.newCachedThreadPool()
    private var server: SSLServerSocket? = null
    private var discovery: NsdDiscovery? = null
    private var coreLink: NativeCoreLink? = null
    private val sessionId = SystemClock.elapsedRealtime()

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
        if (server == null) startTransport()
        return START_STICKY
    }

    private fun startTransport() {
        executor.execute {
            try {
                AndroidIdentityStore.ensureIdentity()
                coreLink = NativeCoreLink.create(SystemClock.elapsedRealtime())
                val context = tlsContext()
                server = context.serverSocketFactory.createServerSocket(Signaling.defaultPort) as SSLServerSocket
                server?.needClientAuth = true
                discovery = NsdDiscovery(this)
                discovery?.startAdvertising(Signaling.defaultPort)
                discovery?.startBrowsing { host, port ->
                    executor.execute { runCatching { PairingTransport.reconnect(this, host, port) } }
                }
                while (!server!!.isClosed) {
                    val socket = server!!.accept() as javax.net.ssl.SSLSocket
                    executor.execute { handle(socket) }
                }
            } catch (_: Throwable) {
                // The foreground service remains alive; the next start or
                // network-change callback retries the listener.
            }
        }
    }

    private fun handle(socket: javax.net.ssl.SSLSocket) {
        socket.use { ssl ->
            try {
                ssl.startHandshake()
                val peer = ssl.session.peerCertificates.firstOrNull() as? X509Certificate ?: return
                val expected = AndroidIdentityStore.macFingerprint(this) ?: return
                if (!AndroidIdentityStore.fingerprint(peer.encoded).equals(expected, true)) return
                val reader = BufferedReader(InputStreamReader(ssl.inputStream, Charsets.UTF_8))
                val line = reader.readLine() ?: return
                val request = JSONObject(line)
                if (request.optString("type") != "hello" ||
                    request.optString("fingerprint") != expected) return
                coreLink?.networkChanged(SystemClock.elapsedRealtime())
                coreLink?.beginConnecting(SystemClock.elapsedRealtime())
                coreLink?.transportConnected(SystemClock.elapsedRealtime())
                coreLink?.handshakeReady(SystemClock.elapsedRealtime(), sessionId)
                val writer = BufferedWriter(OutputStreamWriter(ssl.outputStream, Charsets.UTF_8))
                writer.write("OK ready\n")
                writer.flush()
            } catch (_: Throwable) {
                // Keep diagnostics closed-vocabulary and do not leak TLS data.
            }
        }
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
        getSystemService(NotificationManager::class.java).createNotificationChannel(
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
        discovery?.stop()
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
