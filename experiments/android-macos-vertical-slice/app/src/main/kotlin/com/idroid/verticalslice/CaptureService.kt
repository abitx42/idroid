package com.idroid.verticalslice

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.Service
import android.content.Intent
import android.content.pm.ServiceInfo
import android.media.projection.MediaProjection
import android.os.Build
import android.os.IBinder
import android.util.Log
import org.webrtc.DefaultVideoDecoderFactory
import org.webrtc.DefaultVideoEncoderFactory
import org.webrtc.EglBase
import org.webrtc.Logging
import org.webrtc.MediaConstraints
import org.webrtc.PeerConnection
import org.webrtc.PeerConnectionFactory
import org.webrtc.ScreenCapturerAndroid
import org.webrtc.SessionDescription
import org.webrtc.SurfaceTextureHelper
import org.webrtc.VideoSource
import java.util.UUID
import java.util.concurrent.Executors
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean

class CaptureService : Service() {
    private val executor = Executors.newSingleThreadExecutor()
    // Keep ICE wait and signaling independent from capture/PeerConnection setup
    // callbacks.  A WebRTC callback may arrive while the setup worker is still
    // unwinding; using a separate worker prevents signaling from being starved
    // behind that callback chain.
    private val signalingExecutor = Executors.newSingleThreadExecutor()
    private val iceGatheringComplete = CountDownLatch(1)
    private val iceCompleteReported = AtomicBoolean(false)
    private val stopping = AtomicBoolean(false)
    private var capturer: ScreenCapturerAndroid? = null
    private var factory: PeerConnectionFactory? = null
    private var peer: PeerConnection? = null
    private var egl: EglBase? = null
    private var textureHelper: SurfaceTextureHelper? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (intent == null) {
            fail(DiagnosticState.Stage.SERVICE_START, DiagnosticState.Failure.SERVICE_START_FAILED)
            return START_NOT_STICKY
        }
        try {
            if (Build.VERSION.SDK_INT >= 29) {
                startForeground(notificationId, notification(), ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PROJECTION)
            } else {
                startForeground(notificationId, notification())
            }
            report(DiagnosticState.Event.CAPTURE_SERVICE_STARTED, DiagnosticState.Stage.SERVICE_START)
            executor.execute { startCapture(intent) }
        } catch (_: Throwable) {
            intent.removeExtra(extraPairingCode)
            fail(DiagnosticState.Stage.SERVICE_START, DiagnosticState.Failure.SERVICE_START_FAILED)
        }
        return START_NOT_STICKY
    }

    private fun startCapture(intent: Intent) {
        var failureStage = DiagnosticState.Stage.SERVICE_START
        var failure = DiagnosticState.Failure.SERVICE_START_FAILED
        try {
            if (stopping.get()) return
            val host = intent.getStringExtra(extraHost).orEmpty()
            val port = intent.getIntExtra(extraPort, Signaling.defaultPort)
            val pairing = intent.getStringExtra(extraPairingCode).orEmpty()
            intent.removeExtra(extraPairingCode)
            val resultCode = intent.getIntExtra(extraResultCode, android.app.Activity.RESULT_CANCELED)
            val resultData = projectionConsentData(intent)
            if (resultData == null || resultCode != android.app.Activity.RESULT_OK) {
                fail(DiagnosticState.Stage.SCREEN_CAPTURE, DiagnosticState.Failure.SCREEN_CAPTURE_FAILED)
                return
            }

            failureStage = DiagnosticState.Stage.SCREEN_CAPTURE
            failure = DiagnosticState.Failure.SCREEN_CAPTURE_FAILED
            PeerConnectionFactory.initialize(
                PeerConnectionFactory.InitializationOptions.builder(this)
                    .setEnableInternalTracer(false)
                    // Drop library log strings, which can include SDP/errors.
                    .setInjectableLogger({ _, _, _ -> }, Logging.Severity.LS_NONE)
                    .createInitializationOptions()
            )
            egl = EglBase.create()
            textureHelper = SurfaceTextureHelper.create("idroid-capture", egl!!.eglBaseContext)
            val encoderFactory = DefaultVideoEncoderFactory(egl!!.eglBaseContext, true, true)
            val decoderFactory = DefaultVideoDecoderFactory(egl!!.eglBaseContext)
            factory = PeerConnectionFactory.builder()
                .setVideoEncoderFactory(encoderFactory)
                .setVideoDecoderFactory(decoderFactory)
                .createPeerConnectionFactory()

            val source: VideoSource = factory!!.createVideoSource(false)
            val screenCapturer = ScreenCapturerAndroid(resultData, object : MediaProjection.Callback() {
                override fun onStop() {
                    if (stopping.compareAndSet(false, true)) {
                        report(
                            event = DiagnosticState.Event.CAPTURE_STOPPED_BY_ANDROID,
                            stage = DiagnosticState.Stage.SCREEN_CAPTURE,
                            result = DiagnosticState.Result.STOPPED,
                        )
                        stopSelf()
                    }
                }
            })
            capturer = screenCapturer
            screenCapturer.initialize(textureHelper, this, source.capturerObserver)
            screenCapturer.startCapture(640, 360, 30)
            report(DiagnosticState.Event.SCREEN_CAPTURE_STARTED, DiagnosticState.Stage.SCREEN_CAPTURE)

            failureStage = DiagnosticState.Stage.SDP
            failure = DiagnosticState.Failure.SDP_OFFER_FAILED
            val video = factory!!.createVideoTrack("screen-track", source)
            val config = PeerConnection.RTCConfiguration(emptyList())
            config.sdpSemantics = PeerConnection.SdpSemantics.UNIFIED_PLAN
            val createdPeer = factory?.createPeerConnection(config, observer())
            if (createdPeer == null) {
                fail(DiagnosticState.Stage.SDP, DiagnosticState.Failure.SDP_OFFER_FAILED)
                return
            }
            peer = createdPeer
            createdPeer.addTrack(video, listOf("screen-stream"))
            peer!!.createOffer(object : SdpObserverAdapter({
                fail(DiagnosticState.Stage.SDP, DiagnosticState.Failure.SDP_OFFER_FAILED)
            }) {
                override fun onCreateSuccess(description: SessionDescription) {
                    if (stopping.get()) return
                    report(DiagnosticState.Event.SDP_OFFER_CREATED, DiagnosticState.Stage.SDP)
                    try {
                        peer!!.setLocalDescription(object : SdpObserverAdapter({
                            fail(DiagnosticState.Stage.SDP, DiagnosticState.Failure.SDP_LOCAL_DESCRIPTION_FAILED)
                        }) {
                            override fun onSetSuccess() {
                                if (stopping.get()) return
                                report(DiagnosticState.Event.LOCAL_DESCRIPTION_SET, DiagnosticState.Stage.SDP)
                                waitForIceAndSignal(host, port, pairing, description)
                            }
                        }, description)
                    } catch (_: Throwable) {
                        fail(DiagnosticState.Stage.SDP, DiagnosticState.Failure.SDP_LOCAL_DESCRIPTION_FAILED)
                    }
                }
            }, MediaConstraints())
        } catch (_: Throwable) {
            fail(failureStage, failure)
        } finally {
            intent.removeExtra(extraPairingCode)
        }
    }

    private fun waitForIceAndSignal(host: String, port: Int, pairing: String, offer: SessionDescription) {
        // The observer records ICE completion. The bounded implementation uses
        // the gathered SDP and does not silently fall back to a public relay.
        if (stopping.get()) return
        try {
            signalingExecutor.execute {
                var failureStage = DiagnosticState.Stage.SIGNALING
                var failure = DiagnosticState.Failure.SIGNALING_FAILED
                try {
                    if (stopping.get()) return@execute
                    if (!iceGatheringComplete.await(5, TimeUnit.SECONDS)) {
                        fail(DiagnosticState.Stage.ICE, DiagnosticState.Failure.ICE_GATHERING_TIMEOUT)
                        return@execute
                    }
                    if (stopping.get()) return@execute
                    val gatheredOffer = peer?.localDescription ?: offer
                    val answer = Signaling.exchangeOffer(host, port, UUID.randomUUID().toString(), pairing, gatheredOffer.description) { snapshot ->
                        if (!stopping.get()) Log.i(tag, DiagnosticState.format(snapshot))
                    }
                    if (stopping.get()) return@execute
                    failureStage = DiagnosticState.Stage.SDP_REMOTE
                    failure = DiagnosticState.Failure.SDP_REMOTE_DESCRIPTION_FAILED
                    peer!!.setRemoteDescription(object : SdpObserverAdapter({
                        fail(DiagnosticState.Stage.SDP_REMOTE, DiagnosticState.Failure.SDP_REMOTE_DESCRIPTION_FAILED)
                    }) {
                        override fun onSetSuccess() {
                            if (!stopping.get()) report(DiagnosticState.Event.REMOTE_DESCRIPTION_SET, DiagnosticState.Stage.SDP_REMOTE)
                        }
                    }, SessionDescription(SessionDescription.Type.ANSWER, answer))
                } catch (safe: Signaling.Failure) {
                    fail(safe)
                } catch (_: Throwable) {
                    fail(failureStage, failure)
                }
            }
        } catch (_: Throwable) {
            fail(DiagnosticState.Stage.SIGNALING, DiagnosticState.Failure.SIGNALING_FAILED)
        }
    }

    // Only app-owned enum values reach these helpers. Never pass a throwable,
    // callback error string, pairing code, HMAC, or SDP to the diagnostic path.
    private fun fail(stage: DiagnosticState.Stage, failure: DiagnosticState.Failure) {
        if (stopping.compareAndSet(false, true)) {
            report(DiagnosticState.Event.SIGNALING_ERROR, stage, failure)
            stopSelf()
        }
    }

    private fun fail(failure: Signaling.Failure) {
        fail(failure.code.diagnosticStage, failure.code.diagnosticFailure)
    }

    private fun report(
        event: DiagnosticState.Event,
        stage: DiagnosticState.Stage,
        failure: DiagnosticState.Failure? = null,
        result: DiagnosticState.Result = if (failure == null) DiagnosticState.Result.OK else DiagnosticState.Result.FAILED,
    ) {
        val snapshot = DiagnosticState.record(event, stage, failure, result)
        if (failure == null) {
            Log.i(tag, DiagnosticState.format(snapshot))
        } else {
            Log.e(tag, DiagnosticState.format(snapshot))
        }
    }

    private fun observer(): PeerConnection.Observer = object : PeerConnection.Observer {
        override fun onIceGatheringChange(state: PeerConnection.IceGatheringState?) {
            if (state == PeerConnection.IceGatheringState.COMPLETE) {
                iceGatheringComplete.countDown()
                if (!stopping.get() && iceCompleteReported.compareAndSet(false, true)) {
                    report(DiagnosticState.Event.ICE_GATHERING_COMPLETE, DiagnosticState.Stage.ICE)
                }
            }
        }

        override fun onSignalingChange(state: PeerConnection.SignalingState?) { }

        override fun onIceConnectionChange(state: PeerConnection.IceConnectionState?) {
            if (state == PeerConnection.IceConnectionState.FAILED) {
                fail(DiagnosticState.Stage.ICE, DiagnosticState.Failure.ICE_CONNECTION_FAILED)
            }
        }

        override fun onConnectionChange(state: PeerConnection.PeerConnectionState?) {
            if (state == PeerConnection.PeerConnectionState.FAILED) {
                fail(DiagnosticState.Stage.SIGNALING, DiagnosticState.Failure.PEER_CONNECTION_FAILED)
            }
        }
        override fun onIceCandidate(candidate: org.webrtc.IceCandidate?) { }
        override fun onTrack(transceiver: org.webrtc.RtpTransceiver?) { }
        override fun onAddTrack(receiver: org.webrtc.RtpReceiver?, streams: Array<out org.webrtc.MediaStream>?) { }
        override fun onDataChannel(channel: org.webrtc.DataChannel?) { }
        override fun onIceCandidatesRemoved(candidates: Array<out org.webrtc.IceCandidate>?) { }
        override fun onIceConnectionReceivingChange(receiving: Boolean) { }
        override fun onRenegotiationNeeded() { }
        override fun onRemoveStream(stream: org.webrtc.MediaStream?) { }
        override fun onAddStream(stream: org.webrtc.MediaStream?) { }
    }

    override fun onDestroy() {
        stopping.set(true)
        cleanup { capturer?.stopCapture() }
        cleanup { capturer?.dispose() }
        cleanup { peer?.dispose() }
        cleanup { factory?.dispose() }
        cleanup { textureHelper?.dispose() }
        cleanup { egl?.release() }
        executor.shutdownNow()
        signalingExecutor.shutdownNow()
        report(DiagnosticState.Event.CAPTURE_SERVICE_DESTROYED, DiagnosticState.Stage.CLEANUP)
        super.onDestroy()
    }

    private inline fun cleanup(block: () -> Unit) {
        try { block() } catch (_: Throwable) {
            report(DiagnosticState.Event.SIGNALING_ERROR, DiagnosticState.Stage.SESSION, DiagnosticState.Failure.SIGNALING_FAILED)
        }
    }

    override fun onBind(intent: Intent?): IBinder? = null

    @Suppress("DEPRECATION")
    private fun projectionConsentData(intent: Intent): Intent? =
        if (Build.VERSION.SDK_INT >= 33) {
            intent.getParcelableExtra(extraResultData, Intent::class.java)
        } else {
            intent.getParcelableExtra(extraResultData)
        }

    private fun notification(): Notification {
        val manager = getSystemService(NotificationManager::class.java)
        if (Build.VERSION.SDK_INT >= 26) manager.createNotificationChannel(NotificationChannel(channel, "Screen capture", NotificationManager.IMPORTANCE_LOW))
        return Notification.Builder(this, channel).setContentTitle("iDroid capture experiment").setContentText("Screen capture is active").setSmallIcon(android.R.drawable.ic_menu_view).setOngoing(true).build()
    }

    companion object {
        private const val tag = "iDroidCapture"
        private const val channel = "idroid-capture"
        private const val notificationId = 8675
        const val extraResultCode = "resultCode"
        const val extraResultData = "resultData"
        const val extraHost = "host"
        const val extraPort = "port"
        const val extraPairingCode = "pairingCode"
    }
}

internal open class SdpObserverAdapter(private val onFailure: () -> Unit) : org.webrtc.SdpObserver {
    override fun onCreateSuccess(description: SessionDescription) {}
    override fun onSetSuccess() {}
    override fun onCreateFailure(error: String?) { onFailure() }
    override fun onSetFailure(error: String?) { onFailure() }
}
