package com.idroid.verticalslice.instagram

import android.media.MediaCodecInfo
import android.media.MediaCodecList
import android.media.MediaFormat
import android.os.Build

/**
 * Inspects the device's hardware MediaCodec capabilities at run time and
 * decides whether a given [InstagramExportProfile.Mode] can be encoded without
 * the software-fallback path.
 *
 * Hardware H.264 encoders vary wildly across Android SoCs:
 *   - Snapdragon (Qualcomm Hexagon DSP)
 *   - Exynos (Samsung MFC)
 *   - Dimensity / Helio (MediaTek Venc)
 *   - Tensor (Google ISP)
 *
 * When a hardware encoder does not support the requested profile / level /
 * bitrate, the system silently falls back to the software OMX or C2 AVC
 * encoder, which is slower and sometimes produces lower-quality output.
 * This inspector surfaces that information before the encode begins.
 */
object DeviceCapabilityInspector {

    // -------------------------------------------------------------------------
    // Public result type
    // -------------------------------------------------------------------------

    data class Result(
        /** True when a hardware-accelerated H.264 encoder was found. */
        val hardwareH264Available: Boolean,
        /** Name of the best encoder found (hardware preferred). */
        val encoderName: String,
        /** Whether the encoder is marked as hardware-accelerated by the OS. */
        val isHardwareAccelerated: Boolean,
        /** Max supported resolution (width × height) for H.264. */
        val maxResolutionWidth: Int,
        /** Max supported resolution height. */
        val maxResolutionHeight: Int,
        /** Maximum supported bitrate in bps. */
        val maxBitrateKbps: Int,
        /** Maximum supported frame rate. */
        val maxFrameRate: Int,
        /** True when the requested profile can be natively encoded on-device. */
        val profileSupported: Boolean,
        /** Human-readable summary for display / logging. */
        val summary: String,
    )

    // -------------------------------------------------------------------------
    // Inspection entry point
    // -------------------------------------------------------------------------

    /**
     * Inspects the device codec stack for the given export [mode] and returns
     * a [Result] describing hardware capability and profile compatibility.
     */
    fun inspect(mode: InstagramExportProfile.Mode): Result {
        val codecList = MediaCodecList(MediaCodecList.ALL_CODECS)
        val format = mediaFormatFor(mode)
        val encoderName = codecList.findEncoderForFormat(format) ?: return noEncoder(mode)

        val info = codecList.codecInfos.firstOrNull { it.name == encoderName }
            ?: return noEncoder(mode)

        val caps = info.getCapabilitiesForType(InstagramExportProfile.VIDEO_MIME)
        val videoCaps = caps.videoCapabilities
        val hardwareAccelerated = if (Build.VERSION.SDK_INT >= 29) {
            info.isHardwareAccelerated
        } else {
            !encoderName.contains("sw", ignoreCase = true) &&
                !encoderName.contains("omx.google", ignoreCase = true)
        }

        val maxWidth = videoCaps.supportedWidths.upper
        val maxHeight = videoCaps.supportedHeights.upper
        val maxKbps = videoCaps.bitrateRange.upper / 1000
        val maxFps = videoCaps.supportedFrameRates.upper.toInt()

        val profileSupported = caps.profileLevels.any { pl ->
            pl.profile == MediaCodecInfo.CodecProfileLevel.AVCProfileHigh &&
                pl.level >= MediaCodecInfo.CodecProfileLevel.AVCLevel41
        }

        val summary = buildSummary(
            mode = mode,
            encoderName = encoderName,
            hardwareAccelerated = hardwareAccelerated,
            maxWidth = maxWidth,
            maxHeight = maxHeight,
            maxKbps = maxKbps,
            maxFps = maxFps,
            profileSupported = profileSupported,
        )

        return Result(
            hardwareH264Available = hardwareAccelerated,
            encoderName = encoderName,
            isHardwareAccelerated = hardwareAccelerated,
            maxResolutionWidth = maxWidth,
            maxResolutionHeight = maxHeight,
            maxBitrateKbps = maxKbps,
            maxFrameRate = maxFps,
            profileSupported = profileSupported,
            summary = summary,
        )
    }

    // -------------------------------------------------------------------------
    // Private helpers
    // -------------------------------------------------------------------------

    private fun mediaFormatFor(mode: InstagramExportProfile.Mode): MediaFormat =
        MediaFormat.createVideoFormat(
            InstagramExportProfile.VIDEO_MIME,
            mode.widthPx,
            mode.heightPx,
        ).apply {
            setInteger(MediaFormat.KEY_BIT_RATE, mode.videoBitrateBps)
            setInteger(MediaFormat.KEY_FRAME_RATE, mode.frameRate)
            setInteger(MediaFormat.KEY_COLOR_FORMAT,
                MediaCodecInfo.CodecCapabilities.COLOR_FormatSurface)
            setInteger(MediaFormat.KEY_I_FRAME_INTERVAL, 1)
            setInteger(MediaFormat.KEY_PROFILE,
                MediaCodecInfo.CodecProfileLevel.AVCProfileHigh)
            setInteger(MediaFormat.KEY_LEVEL,
                MediaCodecInfo.CodecProfileLevel.AVCLevel41)
        }

    private fun noEncoder(mode: InstagramExportProfile.Mode) = Result(
        hardwareH264Available = false,
        encoderName = "none",
        isHardwareAccelerated = false,
        maxResolutionWidth = 0,
        maxResolutionHeight = 0,
        maxBitrateKbps = 0,
        maxFrameRate = 0,
        profileSupported = false,
        summary = "No H.264 encoder found for profile ${mode.label}.",
    )

    private fun buildSummary(
        mode: InstagramExportProfile.Mode,
        encoderName: String,
        hardwareAccelerated: Boolean,
        maxWidth: Int,
        maxHeight: Int,
        maxKbps: Int,
        maxFps: Int,
        profileSupported: Boolean,
    ): String = buildString {
        appendLine("═══ Device Codec Inspection ═══")
        appendLine("Profile:         ${mode.label}")
        appendLine("Encoder:         $encoderName")
        appendLine("Hardware accel:  ${if (hardwareAccelerated) "✓ YES" else "✗ NO (software fallback)"}")
        appendLine("Max resolution:  ${maxWidth}×${maxHeight}")
        appendLine("Max bitrate:     ${maxKbps} kbps")
        appendLine("Max frame rate:  ${maxFps} fps")
        appendLine("H264 High L4.1:  ${if (profileSupported) "✓ supported" else "✗ not supported"}")
        appendLine("────────────────────────────")
        if (maxWidth < mode.widthPx || maxHeight < mode.heightPx) {
            appendLine("⚠ RESOLUTION beyond codec max — will trigger software fallback")
        } else if (maxKbps * 1000 < mode.videoBitrateBps) {
            appendLine("⚠ BITRATE beyond codec max — quality will be capped")
        } else if (!profileSupported) {
            appendLine("⚠ H264 High L4.1 not supported — Baseline/Main profile used")
        } else {
            appendLine("✓ This device can fast-path the ${mode.label} profile")
        }
    }
}
