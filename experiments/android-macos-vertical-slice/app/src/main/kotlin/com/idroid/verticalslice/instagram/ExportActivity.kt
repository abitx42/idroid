package com.idroid.verticalslice.instagram

import android.app.Activity
import android.content.Intent
import android.net.Uri
import android.os.Bundle
import android.os.Environment
import android.view.View
import android.widget.*
import androidx.documentfile.provider.DocumentFile
import com.idroid.verticalslice.instagram.InstagramExportProfile.Mode
import java.io.File
import java.text.SimpleDateFormat
import java.util.*
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicBoolean

/**
 * Self-contained Activity that lets the user pick a gallery video, select an
 * Instagram export profile, inspect device codec capabilities, and run the
 * transcode / remux pipeline.
 *
 * # UI structure (programmatic, no XML)
 *
 * ```
 * [iDroid — Instagram Export Optimizer]
 * ─────────────────────────────────────
 * [Pick Video]
 *  source: <filename>
 *
 * Export profile:
 * (•) Reel 1080p (fast-path)
 * ( ) Reel 4K supersampling
 * ( ) Post 1080×1350 (4:5)
 * ( ) Post 1080×1080 (1:1)
 *
 * [Inspect device codecs]
 * <device capability summary>
 *
 * [Export for Instagram]
 * [────────────── 0 % ──────────────]
 *
 * <status text>
 * ```
 *
 * # 4 K mode note shown in UI
 *
 * When the user selects the 4 K supersampling profile, an inline banner
 * explains the strategy: Instagram's ingest server downsamples 4 K → 1080p
 * using a Lanczos filter, which preserves texture and edge detail better than
 * uploading a native-1080p file.
 */
class ExportActivity : Activity() {

    companion object {
        private const val REQUEST_PICK_VIDEO = 71
        private const val TAG = "iDroid.ExportActivity"
    }

    private val executor = Executors.newSingleThreadExecutor()
    private val cancelled = AtomicBoolean(false)

    private lateinit var pickBtn: Button
    private lateinit var sourceLabel: TextView
    private lateinit var radioGroup: RadioGroup
    private lateinit var radio1080p: RadioButton
    private lateinit var radio4k: RadioButton
    private lateinit var radioPost45: RadioButton
    private lateinit var radioPostSq: RadioButton
    private lateinit var fourKBanner: TextView
    private lateinit var inspectBtn: Button
    private lateinit var deviceInfo: TextView
    private lateinit var exportBtn: Button
    private lateinit var progress: ProgressBar
    private lateinit var statusLabel: TextView

    private var selectedUri: Uri? = null

    // -------------------------------------------------------------------------
    // Lifecycle
    // -------------------------------------------------------------------------

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        buildUi()
    }

    override fun onDestroy() {
        cancelled.set(true)
        executor.shutdownNow()
        super.onDestroy()
    }

    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        super.onActivityResult(requestCode, resultCode, data)
        if (requestCode != REQUEST_PICK_VIDEO || resultCode != RESULT_OK) return
        val uri = data?.data ?: return
        selectedUri = uri
        val doc = DocumentFile.fromSingleUri(this, uri)
        sourceLabel.text = "Source: ${doc?.name ?: uri.lastPathSegment}"
        statusLabel.text = "Video selected. Inspect codecs or export directly."
    }

    // -------------------------------------------------------------------------
    // UI construction
    // -------------------------------------------------------------------------

    private fun buildUi() {
        val root = ScrollView(this)
        val container = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(32, 48, 32, 48)
        }
        root.addView(container)

        // Title
        container.addView(tv("iDroid — Instagram Export Optimizer", 20f, isBold = true))
        container.addView(divider())

        // Pick video
        pickBtn = button("📂  Pick gallery video") { openPicker() }
        container.addView(pickBtn)
        sourceLabel = tv("No video selected", 14f)
        container.addView(sourceLabel)
        container.addView(spacer(16))

        // Profile selection
        container.addView(tv("Export profile:", 15f, isBold = true))
        container.addView(spacer(8))

        radioGroup = RadioGroup(this).apply { orientation = RadioGroup.VERTICAL }
        radio1080p = radioBtn("Reel 1080p — fast-path (best for most devices)")
        radio4k = radioBtn("Reel 4K supersampling  ⬆  let Instagram scale down (sharpest)")
        radioPost45 = radioBtn("Post 1080×1350  (4:5 portrait feed)")
        radioPostSq = radioBtn("Post 1080×1080  (1:1 square feed)")
        radioGroup.addView(radio1080p)
        radioGroup.addView(radio4k)
        radioGroup.addView(radioPost45)
        radioGroup.addView(radioPostSq)
        radio1080p.isChecked = true
        container.addView(radioGroup)

        // 4 K explanation banner (hidden until 4 K mode selected)
        fourKBanner = tv(
            "⬆ 4K Supersampling: exports at 3840×2160 @ 32 Mbps.\n" +
                "Instagram's ingest server downsamples to 1080p using Lanczos filtering,\n" +
                "recovering texture, hair, and edge detail lost in native-1080p encodes.\n" +
                "Upload size is ~3× larger but playback quality is noticeably sharper.",
            13f,
        ).apply {
            setBackgroundColor(0x22_00_FF_00.toInt())
            setPadding(16, 16, 16, 16)
            visibility = View.GONE
        }
        container.addView(fourKBanner)
        container.addView(spacer(8))

        radioGroup.setOnCheckedChangeListener { _, id ->
            fourKBanner.visibility = if (id == radio4k.id) View.VISIBLE else View.GONE
        }

        // Device inspection
        container.addView(divider())
        inspectBtn = button("🔍  Inspect device codec capabilities") { runInspect() }
        container.addView(inspectBtn)
        deviceInfo = tv("", 12f).apply { typeface = android.graphics.Typeface.MONOSPACE }
        container.addView(deviceInfo)
        container.addView(spacer(8))

        // Export controls
        container.addView(divider())
        exportBtn = button("🚀  Export for Instagram") { runExport() }
        container.addView(exportBtn)
        container.addView(spacer(8))

        progress = ProgressBar(this, null, android.R.attr.progressBarStyleHorizontal).apply {
            max = 100
            progress = 0
        }
        container.addView(progress)
        container.addView(spacer(8))

        statusLabel = tv("Select a video to begin.", 13f)
        container.addView(statusLabel)

        setContentView(root)
    }

    // -------------------------------------------------------------------------
    // Actions
    // -------------------------------------------------------------------------

    private fun openPicker() {
        val intent = Intent(Intent.ACTION_PICK).apply {
            type = "video/*"
        }
        startActivityForResult(intent, REQUEST_PICK_VIDEO)
    }

    private fun runInspect() {
        val mode = selectedMode()
        inspectBtn.isEnabled = false
        deviceInfo.text = "Inspecting…"
        executor.execute {
            val result = DeviceCapabilityInspector.inspect(mode)
            runOnUiThread {
                deviceInfo.text = result.summary
                inspectBtn.isEnabled = true
            }
        }
    }

    private fun runExport() {
        val uri = selectedUri ?: run {
            toast("Please pick a video first.")
            return
        }
        val mode = selectedMode()

        exportBtn.isEnabled = false
        cancelled.set(false)
        progress.progress = 0
        statusLabel.text = "Preparing export…"

        val outputFile = newOutputFile(mode)

        executor.execute {
            try {
                val exporter = VideoExporter(this)
                val result = exporter.export(
                    sourceUri = uri,
                    outputFile = outputFile,
                    mode = mode,
                    cancelled = cancelled,
                ) { fraction, stage ->
                    runOnUiThread {
                        progress.progress = (fraction * 100).toInt()
                        statusLabel.text = "[$stage]  ${(fraction * 100).toInt()} %"
                    }
                }
                runOnUiThread { onExportDone(result) }
            } catch (e: Exception) {
                android.util.Log.e(TAG, "Export failed", e)
                runOnUiThread {
                    statusLabel.text = "Export failed: ${e.message}"
                    exportBtn.isEnabled = true
                    progress.progress = 0
                }
            }
        }
    }

    private fun onExportDone(result: VideoExporter.ExportResult) {
        val method = if (result.didRemux) "remux (fast-path)" else "full transcode"
        val secs = result.durationMs / 1000
        statusLabel.text = buildString {
            appendLine("✅ Export complete (${secs}s, $method)")
            appendLine("Profile: ${result.mode.label}")
            appendLine("Output: ${result.outputFile.name}")
            appendLine("Path: ${result.outputFile.absolutePath}")
            appendLine()
            appendLine("Device codec:")
            appendLine("  Encoder: ${result.deviceResult.encoderName}")
            appendLine("  Hardware: ${if (result.deviceResult.isHardwareAccelerated) "YES ✓" else "NO (sw)"}")
            appendLine()
            appendLine("Now share ${result.outputFile.name} via Instagram. 🚀")
        }
        exportBtn.isEnabled = true
        progress.progress = 100

        // Scan file into MediaStore so gallery apps and sharing sheets see it
        sendBroadcast(Intent(Intent.ACTION_MEDIA_SCANNER_SCAN_FILE).apply {
            data = Uri.fromFile(result.outputFile)
        })
    }

    // -------------------------------------------------------------------------
    // Helpers
    // -------------------------------------------------------------------------

    private fun selectedMode(): Mode = when (radioGroup.checkedRadioButtonId) {
        radio4k.id -> Mode.REEL_4K_SUPER
        radioPost45.id -> Mode.POST_1080P_4_5
        radioPostSq.id -> Mode.POST_1080P_SQUARE
        else -> Mode.REEL_1080P
    }

    private fun newOutputFile(mode: Mode): File {
        val dir = getExternalFilesDir(Environment.DIRECTORY_MOVIES)
            ?: filesDir
        val ts = SimpleDateFormat("yyyyMMdd_HHmmss", Locale.US).format(Date())
        val tag = when (mode) {
            Mode.REEL_4K_SUPER -> "4k_super"
            Mode.POST_1080P_4_5 -> "post_45"
            Mode.POST_1080P_SQUARE -> "post_sq"
            else -> "reel_1080p"
        }
        return File(dir, "idroid_ig_${tag}_$ts.mp4")
    }

    private fun tv(text: String, size: Float, isBold: Boolean = false): TextView =
        TextView(this).apply {
            this.text = text
            textSize = size
            if (isBold) setTypeface(null, android.graphics.Typeface.BOLD)
            setPadding(0, 4, 0, 4)
        }

    private fun button(label: String, onClick: () -> Unit): Button =
        Button(this).apply {
            text = label
            setOnClickListener { onClick() }
            layoutParams = LinearLayout.LayoutParams(
                LinearLayout.LayoutParams.MATCH_PARENT,
                LinearLayout.LayoutParams.WRAP_CONTENT,
            ).apply { setMargins(0, 8, 0, 8) }
        }

    private fun radioBtn(label: String): RadioButton =
        RadioButton(this).apply { text = label; setPadding(0, 6, 0, 6) }

    private fun divider(): View = View(this).apply {
        setBackgroundColor(0x33_AA_AA_AA.toInt())
        layoutParams = LinearLayout.LayoutParams(
            LinearLayout.LayoutParams.MATCH_PARENT, 2,
        ).apply { setMargins(0, 16, 0, 16) }
    }

    private fun spacer(dp: Int): View = View(this).apply {
        layoutParams = LinearLayout.LayoutParams(
            LinearLayout.LayoutParams.MATCH_PARENT,
            (dp * resources.displayMetrics.density).toInt(),
        )
    }

    private fun toast(msg: String) =
        Toast.makeText(this, msg, Toast.LENGTH_SHORT).show()
}
