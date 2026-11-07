#include <jni.h>
#include <cstdint>

#include "idroid_core_ffi.h"

extern "C" JNIEXPORT jlong JNICALL
Java_com_idroid_verticalslice_NativeCoreLink_00024Companion_nativeNew(
    JNIEnv *, jclass, jlong now_ms) {
    return reinterpret_cast<jlong>(idroid_link_new(static_cast<uint64_t>(now_ms)));
}

extern "C" JNIEXPORT void JNICALL
Java_com_idroid_verticalslice_NativeCoreLink_00024Companion_nativeFree(
    JNIEnv *, jclass, jlong handle) {
    idroid_link_free(reinterpret_cast<IdroidLink *>(handle));
}

extern "C" JNIEXPORT jint JNICALL
Java_com_idroid_verticalslice_NativeCoreLink_00024Companion_nativeBeginConnecting(
    JNIEnv *, jclass, jlong handle, jlong now_ms) {
    return idroid_link_begin_connecting(
        reinterpret_cast<IdroidLink *>(handle), static_cast<uint64_t>(now_ms));
}

extern "C" JNIEXPORT jint JNICALL
Java_com_idroid_verticalslice_NativeCoreLink_00024Companion_nativeTransportConnected(
    JNIEnv *, jclass, jlong handle, jlong now_ms) {
    return idroid_link_transport_connected(
        reinterpret_cast<IdroidLink *>(handle), static_cast<uint64_t>(now_ms));
}

extern "C" JNIEXPORT jint JNICALL
Java_com_idroid_verticalslice_NativeCoreLink_00024Companion_nativeConnectionFailed(
    JNIEnv *, jclass, jlong handle, jlong now_ms) {
    return idroid_link_connection_failed(
        reinterpret_cast<IdroidLink *>(handle), static_cast<uint64_t>(now_ms));
}

extern "C" JNIEXPORT jint JNICALL
Java_com_idroid_verticalslice_NativeCoreLink_00024Companion_nativeNetworkChanged(
    JNIEnv *, jclass, jlong handle, jlong now_ms) {
    return idroid_link_network_changed(
        reinterpret_cast<IdroidLink *>(handle), static_cast<uint64_t>(now_ms));
}

extern "C" JNIEXPORT jint JNICALL
Java_com_idroid_verticalslice_NativeCoreLink_00024Companion_nativeHandshakeReady(
    JNIEnv *, jclass, jlong handle, jlong now_ms, jlong session_id) {
    return idroid_link_handshake_ready(
        reinterpret_cast<IdroidLink *>(handle), static_cast<uint64_t>(now_ms), true,
        static_cast<uint64_t>(session_id));
}
