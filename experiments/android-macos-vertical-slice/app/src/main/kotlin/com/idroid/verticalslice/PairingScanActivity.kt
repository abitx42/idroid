package com.idroid.verticalslice

import android.Manifest
import android.content.pm.PackageManager
import android.os.Bundle
import android.widget.FrameLayout
import android.widget.TextView
import androidx.camera.core.CameraSelector
import androidx.camera.core.ImageAnalysis
import androidx.camera.core.Preview
import androidx.camera.lifecycle.ProcessCameraProvider
import androidx.camera.view.PreviewView
import androidx.activity.ComponentActivity
import androidx.core.app.ActivityCompat
import androidx.core.content.ContextCompat
import com.google.mlkit.vision.barcode.BarcodeScanning
import com.google.mlkit.vision.barcode.BarcodeScannerOptions
import com.google.mlkit.vision.barcode.common.Barcode
import com.google.mlkit.vision.common.InputImage
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicBoolean

class PairingScanActivity : ComponentActivity() {
    private val executor = Executors.newSingleThreadExecutor()
    private val completed = AtomicBoolean(false)
    private lateinit var status: TextView
    private lateinit var previewView: PreviewView

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val preview = PreviewView(this)
        previewView = preview
        status = TextView(this).apply {
            text = "Point the camera at the Mac pairing QR"
            setTextColor(android.graphics.Color.WHITE)
            setBackgroundColor(0x99000000.toInt())
            setPadding(24, 24, 24, 24)
        }
        setContentView(FrameLayout(this).apply {
            addView(preview, FrameLayout.LayoutParams(-1, -1))
            addView(status, FrameLayout.LayoutParams(-1, -2).apply {
                gravity = android.view.Gravity.BOTTOM
            })
        })
        if (ContextCompat.checkSelfPermission(this, Manifest.permission.CAMERA) == PackageManager.PERMISSION_GRANTED) {
            startCamera(preview)
        } else {
            ActivityCompat.requestPermissions(this, arrayOf(Manifest.permission.CAMERA), requestCamera)
        }
    }

    override fun onRequestPermissionsResult(
        requestCode: Int,
        permissions: Array<out String>,
        grantResults: IntArray,
    ) {
        super.onRequestPermissionsResult(requestCode, permissions, grantResults)
        if (requestCode == requestCamera && grantResults.firstOrNull() == PackageManager.PERMISSION_GRANTED) {
            startCamera(previewView)
        } else {
            status.text = "Camera permission is required to pair"
        }
    }

    private fun startCamera(preview: PreviewView) {
        val provider = ProcessCameraProvider.getInstance(this)
        provider.addListener({
            val cameraProvider = provider.get()
            val imageAnalysis = ImageAnalysis.Builder()
                .setBackpressureStrategy(ImageAnalysis.STRATEGY_KEEP_ONLY_LATEST)
                .build()
            val options = BarcodeScannerOptions.Builder()
                .setBarcodeFormats(Barcode.FORMAT_QR_CODE)
                .build()
            val scanner = BarcodeScanning.getClient(options)
            imageAnalysis.setAnalyzer(executor) { imageProxy ->
                val image = imageProxy.image
                if (image == null || completed.get()) {
                    imageProxy.close()
                    return@setAnalyzer
                }
                scanner.process(
                    InputImage.fromMediaImage(image, imageProxy.imageInfo.rotationDegrees),
                ).addOnSuccessListener { codes ->
                    val value = codes.firstNotNullOfOrNull { it.rawValue }
                    if (value != null && completed.compareAndSet(false, true)) {
                        setResult(RESULT_OK, intent.putExtra(resultPayload, value))
                        finish()
                    }
                }.addOnCompleteListener { imageProxy.close() }
            }
            cameraProvider.unbindAll()
            cameraProvider.bindToLifecycle(
                this,
                CameraSelector.DEFAULT_BACK_CAMERA,
                Preview.Builder().build().also { it.setSurfaceProvider(preview.surfaceProvider) },
                imageAnalysis,
            )
        }, ContextCompat.getMainExecutor(this))
    }

    override fun onDestroy() {
        executor.shutdownNow()
        super.onDestroy()
    }

    companion object {
        const val resultPayload = "pairing_payload"
        private const val requestCamera = 701
    }
}
