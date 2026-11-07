package com.idroid.verticalslice

import android.content.Context
import android.os.SystemClock
import android.util.Base64
import org.json.JSONObject
import java.io.BufferedReader
import java.io.BufferedWriter
import java.io.InputStreamReader
import java.io.OutputStreamWriter
import java.net.InetSocketAddress
import java.net.Socket
import java.security.cert.CertificateException
import java.security.cert.X509Certificate
import javax.net.ssl.KeyManagerFactory
import javax.net.ssl.SSLContext
import javax.net.ssl.SSLSocket
import javax.net.ssl.TrustManager
import javax.net.ssl.X509TrustManager

internal data class PairingPayload(
    val version: Int,
    val macFingerprint: String,
    val token: String,
    val expiresAt: Long,
    val host: String,
    val port: Int,
) {
    companion object {
        fun parse(value: String): PairingPayload {
            val json = JSONObject(value)
            val hint = json.getString("serviceHint")
            val separator = hint.lastIndexOf(':')
            require(separator > 0) { "invalid service hint" }
            return PairingPayload(
                version = json.getInt("version"),
                macFingerprint = json.getString("macCertFingerprint"),
                token = json.getString("oneTimeToken"),
                expiresAt = json.getLong("expiresAt"),
                host = hint.substring(0, separator),
                port = hint.substring(separator + 1).toInt().also { require(it in 1..65535) },
            )
        }
    }
}

internal object PairingTransport {
    private const val connectTimeoutMs = 5_000
    private const val readTimeoutMs = 10_000
    private const val maxLineBytes = 64 * 1024

    fun pair(context: Context, payload: PairingPayload, name: String = "Android"): String {
        require(payload.version == 1) { "unsupported pairing version" }
        val identity = AndroidIdentityStore.ensureIdentity()
        val socket = tlsSocket(context, payload.host, payload.port, payload.macFingerprint)
        socket.use { ssl ->
            ssl.startHandshake()
            val macCertificate = (ssl.session.peerCertificates.firstOrNull() as? X509Certificate)
                ?: throw CertificateException("Mac certificate missing")
            val observed = AndroidIdentityStore.fingerprint(macCertificate.encoded)
            require(observed.equals(payload.macFingerprint, ignoreCase = true)) {
                "Mac certificate fingerprint mismatch"
            }
            val request = JSONObject()
                .put("type", "pair")
                .put("token", payload.token)
                .put("phoneFingerprint", AndroidIdentityStore.fingerprint(identity.encoded))
                .put("name", name)
                .put("certificate", Base64.encodeToString(identity.encoded, Base64.NO_WRAP))
            val response = exchange(ssl, request)
            require(response.startsWith("OK paired")) { response.removePrefix("ERROR ") }
            AndroidIdentityStore.saveMacPeer(
                context,
                observed,
                macCertificate.encoded,
                "Mac",
                payload.host,
                payload.port,
            )
            return observed
        }
    }

    fun reconnect(context: Context): Boolean {
        val host = AndroidIdentityStore.macHost(context) ?: return false
        return reconnect(context, host, AndroidIdentityStore.macPort(context))
    }

    fun reconnect(context: Context, host: String, port: Int): Boolean {
        val fingerprint = AndroidIdentityStore.macFingerprint(context) ?: return false
        val certificate = AndroidIdentityStore.macCertificateDer(context) ?: return false
        val socket = tlsSocket(context, host, port, fingerprint, certificate)
        val link = NativeCoreLink.create(SystemClock.elapsedRealtime())
        socket.use { ssl ->
            try {
                link?.beginConnecting(SystemClock.elapsedRealtime())
                ssl.startHandshake()
                val observed = (ssl.session.peerCertificates.firstOrNull() as? X509Certificate)
                    ?: return false
                require(AndroidIdentityStore.fingerprint(observed.encoded).equals(fingerprint, true))
                val request = JSONObject()
                    .put("type", "hello")
                    .put("fingerprint", AndroidIdentityStore.fingerprint(AndroidIdentityStore.certificateDer()))
                val ready = exchange(ssl, request).startsWith("OK ready")
                if (ready) {
                    link?.transportConnected(SystemClock.elapsedRealtime())
                    link?.handshakeReady(SystemClock.elapsedRealtime(), SystemClock.elapsedRealtime())
                }
                return ready
            } catch (error: Throwable) {
                runCatching { link?.connectionFailed(SystemClock.elapsedRealtime()) }
                throw error
            } finally {
                link?.close()
            }
        }
    }

    private fun exchange(socket: SSLSocket, request: JSONObject): String {
        val writer = BufferedWriter(OutputStreamWriter(socket.outputStream, Charsets.UTF_8))
        writer.write(request.toString())
        writer.write('\n'.code)
        writer.flush()
        val line = BufferedReader(InputStreamReader(socket.inputStream, Charsets.UTF_8)).readLine()
            ?: throw IllegalStateException("Mac closed TLS connection")
        require(line.toByteArray(Charsets.UTF_8).size <= maxLineBytes) { "response too large" }
        return line
    }

    private fun tlsSocket(
        context: Context,
        host: String?,
        port: Int,
        expectedFingerprint: String,
        expectedCertificate: ByteArray? = null,
    ): SSLSocket {
        val keyStore = AndroidIdentityStore.keyStore()
        val keyManagers = KeyManagerFactory.getInstance(KeyManagerFactory.getDefaultAlgorithm()).apply {
            init(keyStore, null)
        }.keyManagers
        val trust = PinnedTrustManager(expectedFingerprint, expectedCertificate)
        val sslContext = SSLContext.getInstance("TLSv1.3").apply {
            init(keyManagers, arrayOf<TrustManager>(trust), null)
        }
        val raw = Socket()
        requireNotNull(host) { "reconnect discovery endpoint is required" }
        raw.connect(InetSocketAddress(host, port), connectTimeoutMs)
        return (sslContext.socketFactory.createSocket(raw, host, port, true) as SSLSocket).apply {
            enabledProtocols = arrayOf("TLSv1.3")
            soTimeout = readTimeoutMs
            useClientMode = true
        }
    }
}

internal class PinnedTrustManager(
    private val expectedFingerprint: String,
    private val expectedCertificate: ByteArray?,
) : X509TrustManager {
    override fun checkServerTrusted(chain: Array<out X509Certificate>, authType: String) {
        require(chain.isNotEmpty()) { "server certificate missing" }
        val observed = AndroidIdentityStore.fingerprint(chain[0].encoded)
        if (!observed.equals(expectedFingerprint, ignoreCase = true)) {
            throw CertificateException("certificate fingerprint mismatch")
        }
        if (expectedCertificate != null && !expectedCertificate.contentEquals(chain[0].encoded)) {
            throw CertificateException("certificate changed")
        }
    }

    override fun checkClientTrusted(chain: Array<out X509Certificate>, authType: String) {
        require(chain.isNotEmpty()) { "client certificate missing" }
        val observed = AndroidIdentityStore.fingerprint(chain[0].encoded)
        if (!observed.equals(expectedFingerprint, ignoreCase = true)) {
            throw CertificateException("client certificate fingerprint mismatch")
        }
        if (expectedCertificate != null && !expectedCertificate.contentEquals(chain[0].encoded)) {
            throw CertificateException("client certificate changed")
        }
    }
    override fun getAcceptedIssuers(): Array<X509Certificate> = emptyArray()
}
