package com.idroid.verticalslice.instagram

/**
 * Canonical Instagram encoding profiles.
 *
 * Every constant here maps directly to a parameter the Instagram Android
 * fast-path inspects before deciding whether to skip on-device re-encoding.
 * Deviating from these values forces a MediaCodec transcode pass, which
 * degrades quality on every Android SoC differently.
 *
 * Sources:
 *  - Meta engineering: rupload ingestion parameter constraints
 *  - Community A/B testing (Reddit r/editors, FFmpeg-micro, Becheloon)
 *  - Instagram internal MediaCodec fast-path heuristics (reverse-engineered)
 */
object InstagramExportProfile {

    // ---------------------------------------------------------------------------
    // Profile definitions
    // ---------------------------------------------------------------------------

    enum class Mode(
        val label: String,
        val widthPx: Int,
        val heightPx: Int,
        val frameRate: Int,
        /** Target bitrate in bits per second. */
        val videoBitrateBps: Int,
        /** VBV buffer size (burst headroom). Rule: 2 × maxBitrateBps. */
        val vbvBufferSizeBps: Int,
        /** Max instantaneous bitrate (burst cap). */
        val maxBitrateBps: Int,
        /** Audio bitrate in bits per second. */
        val audioBitrateBps: Int,
        val audioSampleRateHz: Int,
        /** H.264 IDR keyframe interval in frames (equals frameRate × 1 s). */
        val gopSizeFrames: Int,
        /** CRF for quality-based encode (lower = higher quality). */
        val crf: Int,
        /** Whether to run grain dithering pass to bust gradient banding. */
        val grainDitherEnabled: Boolean,
        /** True when the pipeline should scale UP to 4 K before export. */
        val is4kSupersampling: Boolean,
    ) {
        /**
         * Standard 1080p mode — targets Instagram's Android fast-path.
         *
         * The Android Instagram app skips its MediaCodec re-encode pass when it
         * detects a 1080×1920, CFR-30, H.264-High file within the bitrate window.
         * This profile is designed to match those constraints precisely.
         */
        REEL_1080P(
            label = "Reel 1080p (fast-path)",
            widthPx = 1080,
            heightPx = 1920,
            frameRate = 30,
            videoBitrateBps = 10_000_000,
            maxBitrateBps = 14_000_000,
            vbvBufferSizeBps = 20_000_000,
            audioBitrateBps = 128_000,
            audioSampleRateHz = 44_100,
            gopSizeFrames = 30,
            crf = 18,
            grainDitherEnabled = true,
            is4kSupersampling = false,
        ),

        /**
         * 4 K supersampling mode — deliberately exports at 3840×2160 so
         * Meta's server-side downsampler provides the final 1080p frame.
         *
         * When Instagram's ingest pipeline downsamples a 4 K source to 1080p,
         * the mathematical downscale acts as a Lanczos/bicubic supersampling
         * filter that recovers fine texture, hair detail, and edge sharpness
         * that would otherwise be lost in a native-1080p encode.
         *
         * Keep the bitrate between 30–38 Mbps. Above ~40 Mbps, Meta's ingest
         * emergency rate-limiter triggers an aggressive crush pass.
         */
        REEL_4K_SUPER(
            label = "Reel 4K supersampling",
            widthPx = 3840,
            heightPx = 2160,
            frameRate = 30,
            videoBitrateBps = 32_000_000,
            maxBitrateBps = 38_000_000,
            vbvBufferSizeBps = 60_000_000,
            audioBitrateBps = 128_000,
            audioSampleRateHz = 44_100,
            gopSizeFrames = 30,
            crf = 16,
            grainDitherEnabled = true,
            is4kSupersampling = true,
        ),

        /**
         * Square / portrait feed post — 4:5 aspect ratio.
         * This is the recommended format for feed posts (not Reels).
         */
        POST_1080P_4_5(
            label = "Post 1080×1350 (4:5)",
            widthPx = 1080,
            heightPx = 1350,
            frameRate = 30,
            videoBitrateBps = 10_000_000,
            maxBitrateBps = 14_000_000,
            vbvBufferSizeBps = 20_000_000,
            audioBitrateBps = 128_000,
            audioSampleRateHz = 44_100,
            gopSizeFrames = 30,
            crf = 18,
            grainDitherEnabled = true,
            is4kSupersampling = false,
        ),

        /**
         * Square feed post — 1:1 aspect ratio.
         */
        POST_1080P_SQUARE(
            label = "Post 1080×1080 (1:1)",
            widthPx = 1080,
            heightPx = 1080,
            frameRate = 30,
            videoBitrateBps = 10_000_000,
            maxBitrateBps = 14_000_000,
            vbvBufferSizeBps = 20_000_000,
            audioBitrateBps = 128_000,
            audioSampleRateHz = 44_100,
            gopSizeFrames = 30,
            crf = 18,
            grainDitherEnabled = true,
            is4kSupersampling = false,
        ),
    }

    // ---------------------------------------------------------------------------
    // Codec constants (must match MediaCodec MIME types exactly)
    // ---------------------------------------------------------------------------

    /** H.264 AVC encoder MIME — the only codec Instagram's fast-path accepts. */
    const val VIDEO_MIME = "video/avc"
    /** AAC-LC audio MIME. */
    const val AUDIO_MIME = "audio/mp4a-latm"
    /** AAC-LC profile constant for MediaCodecInfo. */
    const val AAC_PROFILE_LC = 2

    // ---------------------------------------------------------------------------
    // Container / mux constants
    // ---------------------------------------------------------------------------

    /**
     * The moov atom must be placed at the beginning of the MP4 container
     * (the "faststart" flag in FFmpeg).  Without this, Instagram's ingest
     * pipeline cannot begin streaming the file from its CDN fetch and may
     * time out or fall back to a lower-quality encode tier.
     */
    const val REQUIRE_MOOV_AT_START = true

    // ---------------------------------------------------------------------------
    // Closed-GOP parameter
    // ---------------------------------------------------------------------------

    /**
     * Disables scene-cut detection, forcing every GOP to have exactly
     * [Mode.gopSizeFrames] frames between IDR keyframes.
     * Instagram's ingest pipeline assumes predictable keyframe positions.
     */
    const val SCENE_CUT_THRESHOLD = 0

    // ---------------------------------------------------------------------------
    // Dither / grain constants
    // ---------------------------------------------------------------------------

    /**
     * Film grain intensity as a fraction of the full luma range [0, 1].
     * 0.012 ≈ 1.2 % — enough to bust gradient banding when the ingest
     * pipeline converts 10-bit color to 8-bit, without being visible to viewers.
     */
    const val GRAIN_STRENGTH = 0.012f

    // ---------------------------------------------------------------------------
    // Companion helpers
    // ---------------------------------------------------------------------------

    /**
     * Returns the [Mode] recommended for a given source aspect ratio.
     *
     * @param srcWidth  source video width in pixels
     * @param srcHeight source video height in pixels
     * @param prefer4K  true to request the 4 K supersampling tier when available
     */
    fun recommend(srcWidth: Int, srcHeight: Int, prefer4K: Boolean = false): Mode {
        val aspect = srcWidth.toFloat() / srcHeight.toFloat()
        return when {
            prefer4K && (aspect < 0.6f || aspect > 1.9f) -> Mode.REEL_4K_SUPER
            aspect < 0.95f && aspect > 0.75f -> Mode.POST_1080P_4_5
            aspect in 0.95f..1.05f -> Mode.POST_1080P_SQUARE
            else -> Mode.REEL_1080P
        }
    }
}
