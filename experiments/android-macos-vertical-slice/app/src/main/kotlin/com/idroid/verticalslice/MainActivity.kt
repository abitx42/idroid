package com.idroid.verticalslice

import android.Manifest
import android.app.Activity
import android.content.Intent
import android.content.pm.PackageManager
import android.media.projection.MediaProjectionManager
import android.os.Build
import android.os.Bundle
import android.text.InputType
import android.text.method.PasswordTransformationMethod
import android.util.Log
import android.view.View
import android.view.inputmethod.EditorInfo
import android.widget.Button
import android.widget.EditText
import android.widget.LinearLayout
import android.widget.TextView
import android.widget.Toast

class MainActivity : Activity() {
    private lateinit var host: EditText
    private lateinit var port: EditText
    private lateinit var pairingCode: EditText
    private lateinit var status: TextView
    private lateinit var startButton: Button

    private val diagnosticListener: (DiagnosticState.Snapshot) -> Unit = { snapshot ->
        runOnUiThread {
            if (::status.isInitialized) status.text = DiagnosticState.uiText(snapshot)
            if (::startButton.isInitialized && (
                    snapshot.result == DiagnosticState.Result.FAILED ||
                        snapshot.event == DiagnosticState.Event.USER_STOP_REQUESTED ||
                        snapshot.event == DiagnosticState.Event.CAPTURE_STOPPED_BY_ANDROID ||
                        snapshot.event == DiagnosticState.Event.CAPTURE_SERVICE_DESTROYED
                    )) {
                startButton.isEnabled = true
            }
        }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        host = field("Mac IP address")
        port = field("Signaling port", Signaling.defaultPort.toString()).apply {
            inputType = InputType.TYPE_CLASS_NUMBER
        }
        pairingCode = field("Experimental pairing code").apply {
            inputType = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_PASSWORD or InputType.TYPE_TEXT_FLAG_NO_SUGGESTIONS
            transformationMethod = object : PasswordTransformationMethod() {
                // Do not honor Android's optional last-character reveal.
                override fun onTextChanged(s: CharSequence, start: Int, before: Int, count: Int) { }
            }
            isSaveEnabled = false
            isSaveFromParentEnabled = false
            importantForAutofill = View.IMPORTANT_FOR_AUTOFILL_NO_EXCLUDE_DESCENDANTS
            if (Build.VERSION.SDK_INT >= 30) {
                importantForContentCapture = View.IMPORTANT_FOR_CONTENT_CAPTURE_NO_EXCLUDE_DESCENDANTS
            }
            imeOptions = imeOptions or EditorInfo.IME_FLAG_NO_PERSONALIZED_LEARNING
        }
        status = TextView(this).apply {
            text = DiagnosticState.current()?.let(DiagnosticState::uiText) ?: "Stage: idle\nDiagnostic: NONE\nStatus: idle\nFailure: NONE"
            setPadding(0, 24, 0, 24)
        }
        DiagnosticState.addListener(diagnosticListener)
        startButton = Button(this).apply {
            text = "Consent and start capture"
            setOnClickListener {
                isEnabled = false
                requestProjectionConsent()
            }
        }
        val stop = Button(this).apply {
            text = "Stop capture"
            setOnClickListener {
                pairingCode.text.clear()
                report(
                    event = DiagnosticState.Event.USER_STOP_REQUESTED,
                    stage = DiagnosticState.Stage.SESSION,
                    result = DiagnosticState.Result.STOPPED,
                )
                stopService(Intent(this@MainActivity, CaptureService::class.java))
                startButton.isEnabled = true
            }
        }
        setContentView(LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(32, 48, 32, 32)
            addView(TextView(this@MainActivity).apply {
                text = "iDroid experimental Android-to-Mac video slice"
                textSize = 20f
            })
            addView(host); addView(port); addView(pairingCode)
            addView(startButton); addView(stop); addView(status)
        })
    }

    private fun field(hint: String, value: String = ""): EditText = EditText(this).apply {
        this.hint = hint
        setText(value)
        inputType = android.text.InputType.TYPE_CLASS_TEXT
    }

    private fun requestProjectionConsent() {
        report(DiagnosticState.Event.START_BUTTON_PRESSED, DiagnosticState.Stage.SESSION)
        if (!validateConfiguration()) return
        if (Build.VERSION.SDK_INT >= 33 && checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED) {
            report(
                event = DiagnosticState.Event.SIGNALING_ERROR,
                stage = DiagnosticState.Stage.PROJECTION_CONSENT,
                failure = DiagnosticState.Failure.NOTIFICATION_PERMISSION_REQUIRED,
            )
            requestPermissions(arrayOf(Manifest.permission.POST_NOTIFICATIONS), requestNotifications)
            return
        }
        launchProjectionConsent()
    }

    private fun validateConfiguration(): Boolean {
        val address = host.text.toString().trim()
        val portValue = port.text.toString().toIntOrNull()
        val pairing = pairingCode.text.toString()
        val pairingBytes = pairing.toByteArray(Charsets.UTF_8)
        val validPairing = pairingBytes.size in 8..256 && pairing.all { it.code in 0x21..0x7e }
        if (!isPrivateIpv4(address) || portValue !in 1..65535 || !validPairing) {
            startButton.isEnabled = true
            report(
                event = DiagnosticState.Event.SIGNALING_ERROR,
                stage = DiagnosticState.Stage.SESSION,
                failure = DiagnosticState.Failure.INVALID_CONFIGURATION,
            )
            return false
        }
        return true
    }

    private fun isPrivateIpv4(value: String): Boolean {
        val octets = value.split('.')
        if (octets.size != 4) return false
        val numbers = octets.map { it.toIntOrNull() ?: return false }
        if (numbers.any { it !in 0..255 }) return false
        return numbers[0] == 10 ||
            (numbers[0] == 172 && numbers[1] in 16..31) ||
            (numbers[0] == 192 && numbers[1] == 168)
    }

    private fun launchProjectionConsent() {
        try {
            val manager = getSystemService(MediaProjectionManager::class.java)
            startActivityForResult(manager.createScreenCaptureIntent(), requestCapture)
        } catch (_: Throwable) {
            report(
                event = DiagnosticState.Event.SIGNALING_ERROR,
                stage = DiagnosticState.Stage.PROJECTION_CONSENT,
                failure = DiagnosticState.Failure.PROJECTION_CONSENT_REQUEST_FAILED,
            )
        }
    }

    override fun onRequestPermissionsResult(requestCode: Int, permissions: Array<out String>, grantResults: IntArray) {
        super.onRequestPermissionsResult(requestCode, permissions, grantResults)
        if (requestCode != requestNotifications) return
        if (grantResults.firstOrNull() == PackageManager.PERMISSION_GRANTED) {
            launchProjectionConsent()
        } else {
            pairingCode.text.clear()
            report(
                event = DiagnosticState.Event.SIGNALING_ERROR,
                stage = DiagnosticState.Stage.PROJECTION_CONSENT,
                failure = DiagnosticState.Failure.NOTIFICATION_PERMISSION_DENIED,
            )
        }
    }

    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        super.onActivityResult(requestCode, resultCode, data)
        if (requestCode != requestCapture) return
        if (resultCode != RESULT_OK || data == null) {
            pairingCode.text.clear()
            startButton.isEnabled = true
            report(
                event = DiagnosticState.Event.SIGNALING_ERROR,
                stage = DiagnosticState.Stage.PROJECTION_CONSENT,
                failure = DiagnosticState.Failure.CAPTURE_CONSENT_DENIED,
            )
            return
        }
        report(DiagnosticState.Event.PROJECTION_CONSENT_GRANTED, DiagnosticState.Stage.PROJECTION_CONSENT)
        val intent = Intent(this, CaptureService::class.java).apply {
            putExtra(CaptureService.extraResultCode, resultCode)
            putExtra(CaptureService.extraResultData, data)
            putExtra(CaptureService.extraHost, host.text.toString().trim())
            putExtra(CaptureService.extraPort, port.text.toString().toIntOrNull() ?: Signaling.defaultPort)
            putExtra(CaptureService.extraPairingCode, pairingCode.text.toString())
        }
        pairingCode.text.clear()
        try {
            startForegroundService(intent)
            Toast.makeText(this, "Projection consent granted", Toast.LENGTH_SHORT).show()
        } catch (_: Exception) {
            startButton.isEnabled = true
            report(
                event = DiagnosticState.Event.SIGNALING_ERROR,
                stage = DiagnosticState.Stage.SERVICE_START,
                failure = DiagnosticState.Failure.SERVICE_START_FAILED,
            )
        } finally {
            intent.removeExtra(CaptureService.extraPairingCode)
        }
    }

    override fun onDestroy() {
        DiagnosticState.removeListener(diagnosticListener)
        if (::pairingCode.isInitialized) pairingCode.text.clear()
        super.onDestroy()
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

    companion object {
        private const val tag = "iDroidCapture"
        private const val requestCapture = 41
        private const val requestNotifications = 42
    }
}
