package com.idroid.verticalslice

import java.net.InetAddress
import java.net.ServerSocket
import java.util.Base64
import java.util.UUID
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import javax.crypto.Mac
import javax.crypto.spec.SecretKeySpec

// Framework-free JVM entry point: the experiment has no test-framework
// dependency. Uses loopback sockets only, never an Android device. All pairing
// and payload fixtures are generated in memory and never printed or written.
object SignalingDiagnosticsTest {
    @JvmStatic
    fun main(args: Array<String>) {
        val cases = listOf(
            "authenticated_round_trip" to ::authenticatedRoundTrip,
            "tcp_connect_failure" to ::tcpConnectFailure,
            "malformed_response" to ::malformedResponse,
            "wrong_session" to ::wrongSession,
            "authentication_failure" to ::authenticationFailure,
            "malformed_authenticated_payload" to ::malformedAuthenticatedPayload,
            "oversized_response" to ::oversizedResponse,
            "oversized_offer" to ::oversizedOffer,
            "closed_response" to ::closedResponse,
            "response_timeout" to ::responseTimeout,
            "invalid_input" to ::invalidInput,
            "sdp_callback_errors_are_discarded" to ::sdpCallbackErrorsAreDiscarded,
            "diagnostic_format_is_timestamped_and_sanitized" to ::diagnosticFormatIsTimestampedAndSanitized,
            "diagnostic_state_tracks_current_stage" to ::diagnosticStateTracksCurrentStage,
        )
        for ((name, run) in cases) {
            try {
                run()
            } catch (_: Throwable) {
                // Even an unexpected failure must not print fixture material.
                throw AssertionError("FAIL: $name")
            }
            println("PASS: $name")
        }
        println("${cases.size} diagnostic/privacy tests passed")
    }

    private class Fixture {
        val pairing = UUID.randomUUID().toString()
        val session = UUID.randomUUID().toString()
        val offer = UUID.randomUUID().toString()
        val answer = UUID.randomUUID().toString()

        fun response(payload: String = encode(answer), session: String = this.session, key: String = pairing): String =
            "ANSWER\t$session\t$payload\t${authenticate(key, "ANSWER|$session|$payload")}\n"
    }

    private data class Exchange(val result: Result<String>, val statuses: List<DiagnosticState.Snapshot>)

    private val safeDiagnostic = Regex(
        "timestamp_ms=[0-9]+ diagnostic=[A-Z_]+ stage=[a-z_]+ status=(?:ok|failed|stopped)(?: failure=[A-Z_]+)?",
    )
    private val loopback = InetAddress.getLoopbackAddress()
    private val loopbackHost = checkNotNull(loopback.hostAddress)

    private fun exchange(fixture: Fixture, reply: (List<String>) -> String?): Exchange {
        val server = ServerSocket(0, 1, loopback)
        val worker = Executors.newSingleThreadExecutor()
        val serving = worker.submit {
            server.accept().use { socket ->
                socket.soTimeout = 2_000
                val fields = socket.getInputStream().bufferedReader(Charsets.UTF_8).readLine().split('\t')
                check(fields.size == 4 && fields[0] == "OFFER") { "Offer envelope mismatch" }
                check(fields[1] == fixture.session) { "Offer session mismatch" }
                check(fields[2] == encode(fixture.offer)) { "Offer payload mismatch" }
                check(fields[3] == authenticate(fixture.pairing, "OFFER|${fields[1]}|${fields[2]}")) { "Offer authentication mismatch" }
                reply(fields)?.let { response ->
                    socket.getOutputStream().write(response.toByteArray(Charsets.UTF_8))
                    socket.getOutputStream().flush()
                }
            }
        }
        return try {
            val statuses = mutableListOf<DiagnosticState.Snapshot>()
            val result = runCatching {
                Signaling.exchangeOffer(loopbackHost, server.localPort, fixture.session, fixture.pairing, fixture.offer, statuses::add)
            }
            serving.get(2, TimeUnit.SECONDS)
            check(statuses.all { safeDiagnostic.matches(DiagnosticState.format(it)) }) { "Unsafe status shape" }
            check(statuses.none {
                val line = DiagnosticState.format(it)
                fixture.pairing in line || fixture.offer in line || fixture.answer in line
            }) { "Fixture appeared in status" }
            Exchange(result, statuses)
        } finally {
            server.close()
            serving.cancel(true)
            worker.shutdownNow()
        }
    }

    private fun expectFailure(result: Result<String>, expected: Signaling.FailureCode) {
        val failure = result.exceptionOrNull()
        check(failure is Signaling.Failure) { "Expected sanitized failure" }
        check(failure.code == expected) { "Wrong failure category" }
        check(failure.cause == null && failure.suppressed.isEmpty()) { "Raw exception retained" }
        check("diagnostic=SIGNALING_ERROR" in failure.diagnostic) { "Missing signaling error event" }
        check(safeDiagnostic.matches(failure.diagnostic) && failure.message == failure.diagnostic) { "Unsafe failure shape" }
        check(DiagnosticState.current()?.event == DiagnosticState.Event.SIGNALING_ERROR) { "Failure did not update diagnostic state" }
    }

    private fun authenticatedRoundTrip() {
        val fixture = Fixture()
        val exchange = exchange(fixture) { fixture.response() }
        check(exchange.result.getOrThrow() == fixture.answer) { "Answer mismatch" }
        check(exchange.statuses.last().event == DiagnosticState.Event.ANSWER_AUTHENTICATED) { "Missing authenticated status" }
    }

    private fun tcpConnectFailure() {
        val port = ServerSocket(0, 1, loopback).use { it.localPort }
        val fixture = Fixture()
        expectFailure(runCatching {
            Signaling.exchangeOffer(loopbackHost, port, fixture.session, fixture.pairing, fixture.offer) { }
        }, Signaling.FailureCode.TCP_CONNECTION_FAILED)
    }

    private fun malformedResponse() {
        expectFailure(exchange(Fixture()) { "UNEXPECTED\n" }.result, Signaling.FailureCode.MALFORMED_SIGNALING)
    }

    private fun wrongSession() {
        val fixture = Fixture()
        expectFailure(exchange(fixture) { fixture.response(session = UUID.randomUUID().toString()) }.result, Signaling.FailureCode.MALFORMED_SIGNALING)
    }

    private fun authenticationFailure() {
        val fixture = Fixture()
        expectFailure(exchange(fixture) { fixture.response(key = UUID.randomUUID().toString()) }.result, Signaling.FailureCode.AUTHENTICATION_FAILED)
    }

    private fun malformedAuthenticatedPayload() {
        val fixture = Fixture()
        val exchange = exchange(fixture) { fixture.response(payload = "!${UUID.randomUUID()}") }
        expectFailure(exchange.result, Signaling.FailureCode.MALFORMED_SIGNALING)
        check(exchange.statuses.none { it.event == DiagnosticState.Event.ANSWER_AUTHENTICATED }) { "Malformed payload reported authenticated" }
    }

    private fun oversizedResponse() {
        expectFailure(exchange(Fixture()) { "x".repeat(262_145) + "\n" }.result, Signaling.FailureCode.MALFORMED_SIGNALING)
    }

    private fun oversizedOffer() {
        val fixture = Fixture()
        val statuses = mutableListOf<DiagnosticState.Snapshot>()
        expectFailure(runCatching {
            Signaling.exchangeOffer(loopbackHost, Signaling.defaultPort, fixture.session, fixture.pairing, "x".repeat(262_144), statuses::add)
        }, Signaling.FailureCode.OFFER_TOO_LARGE)
        check(statuses.isEmpty()) { "Oversized offer attempted connection" }
    }

    private fun closedResponse() {
        expectFailure(exchange(Fixture()) { null }.result, Signaling.FailureCode.MALFORMED_SIGNALING)
    }

    private fun responseTimeout() {
        expectFailure(exchange(Fixture()) {
            Thread.sleep(11_000)
            null
        }.result, Signaling.FailureCode.ANSWER_READ_TIMEOUT)
    }

    private fun invalidInput() {
        val fixture = Fixture()
        expectFailure(runCatching {
            Signaling.exchangeOffer(loopbackHost, 0, fixture.session, fixture.pairing, fixture.offer) { }
        }, Signaling.FailureCode.INVALID_SIGNALING_INPUT)
        expectFailure(runCatching {
            Signaling.exchangeOffer("", Signaling.defaultPort, fixture.session, fixture.pairing, fixture.offer) { }
        }, Signaling.FailureCode.INVALID_SIGNALING_INPUT)
        expectFailure(runCatching {
            Signaling.exchangeOffer(loopbackHost, Signaling.defaultPort, fixture.session, fixture.pairing.take(7), fixture.offer) { }
        }, Signaling.FailureCode.INVALID_SIGNALING_INPUT)
    }

    private fun sdpCallbackErrorsAreDiscarded() {
        var failures = 0
        val observer = SdpObserverAdapter { failures++ }
        observer.onCreateFailure(UUID.randomUUID().toString())
        observer.onSetFailure(UUID.randomUUID().toString())
        observer.onCreateFailure(null)
        observer.onSetFailure(null)
        check(failures == 4) { "Missing SDP failure callback" }
    }

    private fun diagnosticFormatIsTimestampedAndSanitized() {
        val secret = UUID.randomUUID().toString()
        val snapshot = DiagnosticState.Snapshot(
            timestampMs = 1_735_689_600_123,
            event = DiagnosticState.Event.SIGNALING_ERROR,
            stage = DiagnosticState.Stage.TCP,
            result = DiagnosticState.Result.FAILED,
            failure = DiagnosticState.Failure.AUTHENTICATION_FAILED,
        )
        val line = DiagnosticState.format(snapshot)
        check(line == "timestamp_ms=1735689600123 diagnostic=SIGNALING_ERROR stage=tcp status=failed failure=AUTHENTICATION_FAILED") {
            "Unexpected diagnostic format"
        }
        check(safeDiagnostic.matches(line)) { "Diagnostic is not closed vocabulary" }
        check(secret !in line && secret !in DiagnosticState.uiText(snapshot)) { "Secret-like fixture leaked" }
        check("Stage: TCP connection" in DiagnosticState.uiText(snapshot)) { "UI stage missing" }
        check("Failure: AUTHENTICATION_FAILED" in DiagnosticState.uiText(snapshot)) { "UI failure missing" }
    }

    private fun diagnosticStateTracksCurrentStage() {
        var observed: DiagnosticState.Snapshot? = null
        val listener: (DiagnosticState.Snapshot) -> Unit = { observed = it }
        DiagnosticState.addListener(listener)
        try {
            val snapshot = DiagnosticState.record(
                event = DiagnosticState.Event.SCREEN_CAPTURE_STARTED,
                stage = DiagnosticState.Stage.SCREEN_CAPTURE,
                timestampMs = 1234,
            )
            check(DiagnosticState.current() == snapshot) { "Current diagnostic stage was not retained" }
            check(observed == snapshot) { "Diagnostic listener did not receive stage" }
            check(snapshot.failure == null && snapshot.result == DiagnosticState.Result.OK) { "Success stage became a failure" }
        } finally {
            DiagnosticState.removeListener(listener)
        }
    }

    private fun encode(value: String): String = Base64.getUrlEncoder().withoutPadding().encodeToString(value.toByteArray(Charsets.UTF_8))

    private fun authenticate(key: String, value: String): String {
        val mac = Mac.getInstance("HmacSHA256")
        mac.init(SecretKeySpec(key.toByteArray(Charsets.UTF_8), "HmacSHA256"))
        return mac.doFinal(value.toByteArray(Charsets.UTF_8)).joinToString("") { "%02x".format(it) }
    }
}
