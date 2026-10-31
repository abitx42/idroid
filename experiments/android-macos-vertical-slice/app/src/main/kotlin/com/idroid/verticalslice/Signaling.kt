package com.idroid.verticalslice

import java.io.BufferedInputStream
import java.io.BufferedWriter
import java.io.ByteArrayOutputStream
import java.io.OutputStreamWriter
import java.io.InputStream
import java.net.InetSocketAddress
import java.net.Socket
import java.net.SocketTimeoutException
import java.nio.charset.StandardCharsets
import java.security.MessageDigest
import java.util.Base64
import java.util.concurrent.CopyOnWriteArraySet
import javax.crypto.Mac
import javax.crypto.spec.SecretKeySpec

/**
 * Process-local, UI-independent diagnostic state.
 *
 * Every value that can reach a log line or the Activity is an enum value from
 * this closed vocabulary.  In particular, this type has no field for an
 * exception, endpoint, pairing code, HMAC, or SDP.  The timestamp is the only
 * value supplied by the caller's clock.
 */
internal object DiagnosticState {
    enum class Event {
        START_BUTTON_PRESSED,
        PROJECTION_CONSENT_GRANTED,
        CAPTURE_SERVICE_STARTED,
        CAPTURE_SERVICE_DESTROYED,
        SCREEN_CAPTURE_STARTED,
        SCREEN_FRAME_CAPTURED,
        SDP_OFFER_CREATED,
        ICE_GATHERING_COMPLETE,
        ICE_GATHERING_STARTED,
        TCP_CONNECTING,
        TCP_CONNECTED,
        PEER_CONNECTION_CONNECTED,
        SIGNALING_ERROR,
        USER_STOP_REQUESTED,
        CAPTURE_STOPPED_BY_ANDROID,
        LOCAL_DESCRIPTION_SET,
        REMOTE_DESCRIPTION_SET,
        OFFER_SENT,
        ANSWER_AUTHENTICATED,
    }

    enum class Stage(val token: String, val label: String) {
        IDLE("idle", "idle"),
        SESSION("session", "session"),
        PROJECTION_CONSENT("projection_consent", "projection consent"),
        SERVICE_START("service_start", "capture service startup"),
        CLEANUP("cleanup", "cleanup"),
        SCREEN_CAPTURE("screen_capture", "screen capture"),
        SDP("sdp", "SDP"),
        SDP_REMOTE("sdp_remote", "remote SDP"),
        ICE("ice", "ICE gathering"),
        TCP("tcp", "TCP connection"),
        SIGNALING("signaling", "signaling"),
    }

    enum class Result(val token: String) {
        OK("ok"),
        FAILED("failed"),
        STOPPED("stopped"),
    }

    enum class Failure {
        NOTIFICATION_PERMISSION_REQUIRED,
        NOTIFICATION_PERMISSION_DENIED,
        PROJECTION_CONSENT_REQUEST_FAILED,
        CAPTURE_CONSENT_DENIED,
        SERVICE_START_FAILED,
        SCREEN_CAPTURE_FAILED,
        SDP_OFFER_FAILED,
        SDP_LOCAL_DESCRIPTION_FAILED,
        SDP_REMOTE_DESCRIPTION_FAILED,
        ICE_GATHERING_TIMEOUT,
        ICE_CONNECTION_FAILED,
        PEER_CONNECTION_FAILED,
        INVALID_CONFIGURATION,
        SESSION_ALREADY_ACTIVE,
        H264_NOT_SUPPORTED,
        SCREEN_FRAME_TIMEOUT,
        SDP_OFFER_TIMEOUT,
        SDP_LOCAL_DESCRIPTION_TIMEOUT,
        SDP_REMOTE_DESCRIPTION_TIMEOUT,
        PEER_CONNECTION_TIMEOUT,
        TCP_CONNECTION_FAILED,
        TCP_CONNECT_TIMEOUT,
        SIGNALING_WRITE_FAILED,
        ANSWER_READ_FAILED,
        ANSWER_READ_TIMEOUT,
        MALFORMED_SIGNALING,
        AUTHENTICATION_FAILED,
        INVALID_SIGNALING_INPUT,
        OFFER_TOO_LARGE,
        SIGNALING_FAILED,
    }

    data class Snapshot(
        val timestampMs: Long,
        val event: Event,
        val stage: Stage,
        val result: Result,
        val failure: Failure?,
    )

    private val listeners = CopyOnWriteArraySet<(Snapshot) -> Unit>()

    @Volatile
    private var latest: Snapshot? = null

    fun record(
        event: Event,
        stage: Stage,
        failure: Failure? = null,
        result: Result = if (failure == null) Result.OK else Result.FAILED,
        timestampMs: Long = System.currentTimeMillis(),
    ): Snapshot {
        val snapshot = Snapshot(timestampMs, event, stage, result, failure)
        latest = snapshot
        listeners.forEach { listener ->
            try {
                listener(snapshot)
            } catch (_: Throwable) {
                // A UI observer must never change capture/signaling behavior.
            }
        }
        return snapshot
    }

    fun current(): Snapshot? = latest

    fun addListener(listener: (Snapshot) -> Unit) {
        listeners.add(listener)
        latest?.let {
            try {
                listener(it)
            } catch (_: Throwable) {
                // Keep observer failures out of the diagnostic surface.
            }
        }
    }

    fun removeListener(listener: (Snapshot) -> Unit) {
        listeners.remove(listener)
    }

    /** Stable, machine-readable output containing only closed-vocabulary values. */
    fun format(snapshot: Snapshot): String = buildString {
        append("timestamp_ms=").append(snapshot.timestampMs)
        append(" diagnostic=").append(snapshot.event.name)
        append(" stage=").append(snapshot.stage.token)
        append(" status=").append(snapshot.result.token)
        snapshot.failure?.let { append(" failure=").append(it.name) }
    }

    /** Stable UI text containing only closed-vocabulary values. */
    fun uiText(snapshot: Snapshot): String = buildString {
        append("Stage: ").append(snapshot.stage.label)
        append("\nDiagnostic: ").append(snapshot.event.name)
        append("\nStatus: ").append(snapshot.result.token)
        append("\nFailure: ").append(snapshot.failure?.name ?: "NONE")
        append("\nUpdated: ").append(snapshot.timestampMs)
    }
}

internal object Signaling {
    private const val maxLineBytes = 262_144
    const val defaultPort = 18765

    enum class FailureCode(val stage: String, val category: String) {
        INVALID_SIGNALING_INPUT("signaling_input", "invalid"),
        OFFER_PREPARATION_FAILED("signaling_prepare", "failed"),
        OFFER_TOO_LARGE("signaling_prepare", "too_large"),
        TCP_CONNECTION_FAILED("tcp_connect", "failed"),
        TCP_CONNECT_TIMEOUT("tcp_connect", "timeout"),
        SIGNALING_WRITE_FAILED("signaling_send", "failed"),
        ANSWER_READ_FAILED("signaling_response", "io_failed"),
        ANSWER_READ_TIMEOUT("signaling_response", "timeout"),
        MALFORMED_SIGNALING("signaling_response", "malformed"),
        AUTHENTICATION_FAILED("signaling_auth", "failed");

        val diagnosticStage: DiagnosticState.Stage
            get() = when (this) {
                INVALID_SIGNALING_INPUT,
                SIGNALING_WRITE_FAILED,
                ANSWER_READ_FAILED,
                ANSWER_READ_TIMEOUT,
                MALFORMED_SIGNALING,
                AUTHENTICATION_FAILED,
                -> DiagnosticState.Stage.SIGNALING
                OFFER_PREPARATION_FAILED,
                OFFER_TOO_LARGE,
                -> DiagnosticState.Stage.SDP
                TCP_CONNECTION_FAILED,
                TCP_CONNECT_TIMEOUT,
                -> DiagnosticState.Stage.TCP
            }

        val diagnosticFailure: DiagnosticState.Failure
            get() = when (this) {
                INVALID_SIGNALING_INPUT -> DiagnosticState.Failure.INVALID_SIGNALING_INPUT
                OFFER_PREPARATION_FAILED -> DiagnosticState.Failure.SDP_OFFER_FAILED
                OFFER_TOO_LARGE -> DiagnosticState.Failure.OFFER_TOO_LARGE
                TCP_CONNECTION_FAILED -> DiagnosticState.Failure.TCP_CONNECTION_FAILED
                TCP_CONNECT_TIMEOUT -> DiagnosticState.Failure.TCP_CONNECT_TIMEOUT
                SIGNALING_WRITE_FAILED -> DiagnosticState.Failure.SIGNALING_WRITE_FAILED
                ANSWER_READ_FAILED -> DiagnosticState.Failure.ANSWER_READ_FAILED
                ANSWER_READ_TIMEOUT -> DiagnosticState.Failure.ANSWER_READ_TIMEOUT
                MALFORMED_SIGNALING -> DiagnosticState.Failure.MALFORMED_SIGNALING
                AUTHENTICATION_FAILED -> DiagnosticState.Failure.AUTHENTICATION_FAILED
            }
    }

    // Do not retain the original exception/cause: it can contain endpoint,
    // authenticated-envelope, or library-controlled text.
    class Failure(
        val code: FailureCode,
        timestampMs: Long = System.currentTimeMillis(),
    ) : Exception() {
        val diagnostic: String = DiagnosticState.format(
            DiagnosticState.Snapshot(
                timestampMs = timestampMs,
                event = DiagnosticState.Event.SIGNALING_ERROR,
                stage = code.diagnosticStage,
                result = DiagnosticState.Result.FAILED,
                failure = code.diagnosticFailure,
            ),
        )
        override val message: String get() = diagnostic
    }

    fun exchangeOffer(
        host: String,
        port: Int,
        sessionId: String,
        pairingCode: String,
        offerSdp: String,
        onStatus: (DiagnosticState.Snapshot) -> Unit,
    ): String {
        if (!isNumericIpv4(host) || port !in 1..65535 || !validPairingCode(pairingCode)) {
            val failure = Failure(FailureCode.INVALID_SIGNALING_INPUT)
            recordFailure(failure)
            throw failure
        }
        var failureCode = FailureCode.OFFER_PREPARATION_FAILED
        try {
            val payload = encode(offerSdp)
            val auth = hmac(pairingCode, "OFFER|$sessionId|$payload")
            val message = "OFFER\t$sessionId\t$payload\t$auth\n"
            if (message.toByteArray(StandardCharsets.UTF_8).size > maxLineBytes) {
                throw Failure(FailureCode.OFFER_TOO_LARGE)
            }
            Socket().use { socket ->
                failureCode = FailureCode.TCP_CONNECTION_FAILED
                emitStatus(DiagnosticState.Event.TCP_CONNECTING, DiagnosticState.Stage.TCP, onStatus)
                socket.connect(InetSocketAddress(host, port), 5_000)
                emitStatus(DiagnosticState.Event.TCP_CONNECTED, DiagnosticState.Stage.TCP, onStatus)
                failureCode = FailureCode.SIGNALING_WRITE_FAILED
                socket.soTimeout = 10_000
                val input = BufferedInputStream(socket.getInputStream())
                val writer = BufferedWriter(OutputStreamWriter(socket.getOutputStream(), StandardCharsets.UTF_8))
                writer.write(message)
                writer.flush()
                emitStatus(DiagnosticState.Event.OFFER_SENT, DiagnosticState.Stage.SIGNALING, onStatus)
                failureCode = FailureCode.ANSWER_READ_FAILED
                val line = readLineBounded(input)
                failureCode = FailureCode.MALFORMED_SIGNALING
                require(line.isNotEmpty()) { "Mac closed signaling connection" }
                val fields = line.split('\t')
                require(fields.size == 4 && fields[0] == "ANSWER") { "Unexpected signaling response" }
                require(fields[1] == sessionId) { "Signaling session ID mismatch" }
                failureCode = FailureCode.AUTHENTICATION_FAILED
                require(MessageDigest.isEqual(
                    fields[3].toByteArray(StandardCharsets.US_ASCII),
                    hmac(pairingCode, "ANSWER|$sessionId|${fields[2]}").toByteArray(StandardCharsets.US_ASCII),
                )) {
                    "Signaling authentication failed"
                }
                failureCode = FailureCode.MALFORMED_SIGNALING
                val answer = decode(fields[2])
                emitStatus(DiagnosticState.Event.ANSWER_AUTHENTICATED, DiagnosticState.Stage.SIGNALING, onStatus)
                return answer
            }
        } catch (safe: Failure) {
            // Socket.use may have attached a close exception as suppressed.
            val failure = Failure(safe.code)
            recordFailure(failure)
            throw failure
        } catch (error: Exception) {
            val code = when {
                error is SocketTimeoutException && failureCode == FailureCode.TCP_CONNECTION_FAILED -> FailureCode.TCP_CONNECT_TIMEOUT
                error is SocketTimeoutException && failureCode == FailureCode.ANSWER_READ_FAILED -> FailureCode.ANSWER_READ_TIMEOUT
                else -> failureCode
            }
            val failure = Failure(code)
            recordFailure(failure)
            throw failure
        }
    }

    private fun encode(value: String): String =
        Base64.getUrlEncoder().withoutPadding().encodeToString(value.toByteArray(StandardCharsets.UTF_8))

    private fun emitStatus(
        event: DiagnosticState.Event,
        stage: DiagnosticState.Stage,
        onStatus: (DiagnosticState.Snapshot) -> Unit,
    ) {
        onStatus(DiagnosticState.record(event, stage))
    }

    private fun recordFailure(failure: Failure) {
        DiagnosticState.record(
            event = DiagnosticState.Event.SIGNALING_ERROR,
            stage = failure.code.diagnosticStage,
            failure = failure.code.diagnosticFailure,
        )
    }

    private fun decode(value: String): String =
        String(Base64.getUrlDecoder().decode(value), StandardCharsets.UTF_8)

    private fun readLineBounded(input: InputStream): String {
        val bytes = ByteArrayOutputStream()
        while (true) {
            val next = input.read()
            if (next == -1) break
            if (next == '\n'.code) break
            if (bytes.size() >= maxLineBytes) throw Failure(FailureCode.MALFORMED_SIGNALING)
            bytes.write(next)
        }
        return String(bytes.toByteArray(), StandardCharsets.UTF_8).removeSuffix("\r")
    }

    private fun hmac(secret: String, value: String): String {
        val mac = Mac.getInstance("HmacSHA256")
        mac.init(SecretKeySpec(secret.toByteArray(StandardCharsets.UTF_8), "HmacSHA256"))
        return mac.doFinal(value.toByteArray(StandardCharsets.UTF_8)).joinToString("") { "%02x".format(it) }
    }

    private fun validPairingCode(value: String): Boolean =
        value.toByteArray(StandardCharsets.UTF_8).size in 8..256 &&
            value.all { it.code in 0x21..0x7e }

    private fun isNumericIpv4(value: String): Boolean {
        val octets = value.split('.')
        if (octets.size != 4) return false
        val numbers = octets.map { it.toIntOrNull() ?: return false }
        if (numbers.any { it !in 0..255 }) return false
        return true
    }
}
