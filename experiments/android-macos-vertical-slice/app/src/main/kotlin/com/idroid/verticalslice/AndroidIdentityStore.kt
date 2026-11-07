package com.idroid.verticalslice

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import java.math.BigInteger
import java.security.KeyPairGenerator
import java.security.KeyStore
import java.security.MessageDigest
import java.security.cert.X509Certificate
import java.util.Date
import javax.security.auth.x500.X500Principal

internal object AndroidIdentityStore {
    private const val keyAlias = "idroid-continuity-identity-v1"
    private const val preferences = "continuity-trust"
    private const val macFingerprintKey = "mac_fingerprint"
    private const val macCertificateKey = "mac_certificate_der"
    private const val macNameKey = "mac_name"
    private const val macHostKey = "mac_host"
    private const val macPortKey = "mac_port"

    fun ensureIdentity(): X509Certificate {
        val store = keyStore()
        if (!store.containsAlias(keyAlias)) {
            val generator = KeyPairGenerator.getInstance(
                KeyProperties.KEY_ALGORITHM_EC,
                "AndroidKeyStore",
            )
            val now = Date()
            generator.initialize(
                KeyGenParameterSpec.Builder(
                    keyAlias,
                    KeyProperties.PURPOSE_SIGN or KeyProperties.PURPOSE_AGREE_KEY,
                )
                    .setAlgorithmParameterSpec(java.security.spec.ECGenParameterSpec("secp256r1"))
                    .setDigests(KeyProperties.DIGEST_SHA256)
                    .setCertificateSubject(X500Principal("CN=iDroid Android"))
                    .setCertificateSerialNumber(BigInteger.ONE)
                    .setCertificateNotBefore(now)
                    .setCertificateNotAfter(Date(now.time + 10L * 365 * 24 * 60 * 60 * 1000))
                    .build(),
            )
            generator.generateKeyPair()
        }
        return store.getCertificate(keyAlias) as X509Certificate
    }

    fun certificateDer(): ByteArray = ensureIdentity().encoded

    fun fingerprint(): String = fingerprint(ensureIdentity().encoded)

    fun fingerprint(certificateDer: ByteArray): String =
        MessageDigest.getInstance("SHA-256")
            .digest(certificateDer)
            .joinToString("") { "%02x".format(it) }

    fun keyAlias(): String = keyAlias

    fun saveMacPeer(
        context: Context,
        fingerprint: String,
        certificateDer: ByteArray,
        name: String,
        host: String,
        port: Int,
    ) {
        context.getSharedPreferences(preferences, Context.MODE_PRIVATE)
            .edit()
            .putString(macFingerprintKey, fingerprint)
            .putString(macCertificateKey, android.util.Base64.encodeToString(certificateDer, android.util.Base64.NO_WRAP))
            .putString(macNameKey, name)
            .putString(macHostKey, host)
            .putInt(macPortKey, port)
            .apply()
    }

    fun macFingerprint(context: Context): String? = context
        .getSharedPreferences(preferences, Context.MODE_PRIVATE)
        .getString(macFingerprintKey, null)

    fun macCertificateDer(context: Context): ByteArray? = context
        .getSharedPreferences(preferences, Context.MODE_PRIVATE)
        .getString(macCertificateKey, null)
        ?.let { android.util.Base64.decode(it, android.util.Base64.DEFAULT) }

    fun macName(context: Context): String = context
        .getSharedPreferences(preferences, Context.MODE_PRIVATE)
        .getString(macNameKey, "Mac") ?: "Mac"

    fun macHost(context: Context): String? = context
        .getSharedPreferences(preferences, Context.MODE_PRIVATE)
        .getString(macHostKey, null)

    fun macPort(context: Context): Int = context
        .getSharedPreferences(preferences, Context.MODE_PRIVATE)
        .getInt(macPortKey, Signaling.defaultPort)

    fun forgetMac(context: Context) {
        context.getSharedPreferences(preferences, Context.MODE_PRIVATE)
            .edit()
            .remove(macFingerprintKey)
            .remove(macCertificateKey)
            .remove(macNameKey)
            .remove(macHostKey)
            .remove(macPortKey)
            .apply()
    }

    fun keyStore(): KeyStore = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
}
