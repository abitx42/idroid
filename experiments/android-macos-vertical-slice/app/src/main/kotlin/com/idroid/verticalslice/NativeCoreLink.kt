package com.idroid.verticalslice

/** Optional JNI bridge used by the real Android build when the Rust ABI is supplied. */
internal class NativeCoreLink private constructor(private val handle: Long) : AutoCloseable {
    fun beginConnecting(nowMs: Long) = requireResult(nativeBeginConnecting(handle, nowMs))
    fun transportConnected(nowMs: Long) = requireResult(nativeTransportConnected(handle, nowMs))
    fun connectionFailed(nowMs: Long) = requireResult(nativeConnectionFailed(handle, nowMs))
    fun networkChanged(nowMs: Long) = requireResult(nativeNetworkChanged(handle, nowMs))
    fun handshakeReady(nowMs: Long, sessionId: Long) =
        requireResult(nativeHandshakeReady(handle, nowMs, sessionId))

    override fun close() {
        nativeFree(handle)
    }

    private fun requireResult(value: Int) {
        check(value == 0) { "core link operation failed: $value" }
    }

    companion object {
        private var loaded = false

        fun create(nowMs: Long): NativeCoreLink? {
            if (!loaded) {
                loaded = runCatching { System.loadLibrary("idroid_core_jni") }.isSuccess
            }
            if (!loaded) return null
            val handle = nativeNew(nowMs)
            return if (handle == 0L) null else NativeCoreLink(handle)
        }

        private external fun nativeNew(nowMs: Long): Long
        private external fun nativeFree(handle: Long)
        private external fun nativeBeginConnecting(handle: Long, nowMs: Long): Int
        private external fun nativeTransportConnected(handle: Long, nowMs: Long): Int
        private external fun nativeConnectionFailed(handle: Long, nowMs: Long): Int
        private external fun nativeNetworkChanged(handle: Long, nowMs: Long): Int
        private external fun nativeHandshakeReady(
            handle: Long,
            nowMs: Long,
            sessionId: Long,
        ): Int
    }
}
