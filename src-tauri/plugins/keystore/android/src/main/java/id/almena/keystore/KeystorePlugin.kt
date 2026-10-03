package id.almena.keystore

import android.app.Activity
import android.content.pm.PackageManager
import android.os.Build
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import java.security.KeyStore
import java.security.ProviderException
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/** What crosses from Rust: bytes, as standard base64. */
@InvokeArg
class Bytes {
    lateinit var data: String
}

/**
 * Seals and opens the vault's record under an AES-256-GCM key that lives in the
 * Android Keystore and never leaves it: StrongBox where the phone has one, its
 * TEE otherwise. Called from Rust only (`almena-keystore`, `vault::store`).
 *
 * Sealed is the 12-byte nonce, then the ciphertext and its tag, authenticated
 * over [AAD] — so a sealed record is only ever opened as one.
 */
@TauriPlugin
class KeystorePlugin(private val activity: Activity) : Plugin(activity) {
    @Command
    fun seal(invoke: Invoke) {
        try {
            val plain = Base64.decode(invoke.parseArgs(Bytes::class.java).data, Base64.NO_WRAP)
            val cipher = Cipher.getInstance(TRANSFORMATION)
            cipher.init(Cipher.ENCRYPT_MODE, key() ?: generate())
            cipher.updateAAD(AAD)
            val sealed = cipher.iv + cipher.doFinal(plain)
            invoke.resolve(bytes(sealed))
        } catch (failure: Exception) {
            invoke.reject("keystore_seal: ${failure.javaClass.simpleName}")
        }
    }

    @Command
    fun open(invoke: Invoke) {
        try {
            val sealed = Base64.decode(invoke.parseArgs(Bytes::class.java).data, Base64.NO_WRAP)
            if (sealed.size <= NONCE_BYTES) {
                invoke.reject("keystore_open: short")
                return
            }
            // No key is not a reason to make one: what was sealed under the
            // old one can only be opened by it.
            val key = key() ?: run {
                invoke.reject("keystore_open: no key")
                return
            }
            val cipher = Cipher.getInstance(TRANSFORMATION)
            cipher.init(
                Cipher.DECRYPT_MODE,
                key,
                GCMParameterSpec(TAG_BITS, sealed, 0, NONCE_BYTES),
            )
            cipher.updateAAD(AAD)
            val plain = cipher.doFinal(sealed, NONCE_BYTES, sealed.size - NONCE_BYTES)
            invoke.resolve(bytes(plain))
        } catch (failure: Exception) {
            invoke.reject("keystore_open: ${failure.javaClass.simpleName}")
        }
    }

    private fun bytes(data: ByteArray): JSObject {
        val answer = JSObject()
        answer.put("data", Base64.encodeToString(data, Base64.NO_WRAP))
        return answer
    }

    private fun key(): SecretKey? {
        val store = KeyStore.getInstance(PROVIDER).apply { load(null) }
        return store.getKey(ALIAS, null) as? SecretKey
    }

    /** Made inside the Keystore, in StrongBox when there is one. */
    private fun generate(): SecretKey {
        val strongBox = Build.VERSION.SDK_INT >= Build.VERSION_CODES.P &&
            activity.packageManager.hasSystemFeature(PackageManager.FEATURE_STRONGBOX_KEYSTORE)
        return try {
            generate(strongBox)
        } catch (failure: ProviderException) {
            // StrongBoxUnavailableException is a ProviderException; it is
            // named this way because the class does not exist before API 28.
            if (strongBox) generate(false) else throw failure
        }
    }

    private fun generate(strongBox: Boolean): SecretKey {
        val spec = KeyGenParameterSpec.Builder(
            ALIAS,
            KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT,
        )
            .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
            .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
            .setKeySize(256)
            .setRandomizedEncryptionRequired(true)
        if (strongBox && Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
            spec.setIsStrongBoxBacked(true)
        }
        val generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, PROVIDER)
        generator.init(spec.build())
        return generator.generateKey()
    }

    private companion object {
        const val PROVIDER = "AndroidKeyStore"
        const val ALIAS = "almena-vault"
        const val TRANSFORMATION = "AES/GCM/NoPadding"
        const val NONCE_BYTES = 12
        const val TAG_BITS = 128
        val AAD = "almena-vault/keystore".toByteArray()
    }
}
