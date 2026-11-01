package id.almena.keystore

import android.app.Activity
import android.content.pm.PackageManager
import android.os.Build
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyPermanentlyInvalidatedException
import android.security.keystore.KeyProperties
import android.util.Base64
import androidx.biometric.BiometricManager
import androidx.biometric.BiometricManager.Authenticators.BIOMETRIC_STRONG
import androidx.biometric.BiometricPrompt
import androidx.core.content.ContextCompat
import androidx.fragment.app.FragmentActivity
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import java.security.KeyStore
import java.security.ProviderException
import javax.crypto.AEADBadTagException
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/** What crosses from Rust: bytes, as standard base64. */
@InvokeArg
class Bytes {
    lateinit var data: String
}

/** Bytes, and the words `BiometricPrompt` shows in the interface's language. */
@InvokeArg
class Prompted {
    lateinit var data: String
    var title: String = ""
    var subtitle: String = ""
    var cancel: String = ""
}

/**
 * Seals and opens the vault's record under an AES-256-GCM key that lives in the
 * Android Keystore and never leaves it: StrongBox where the phone has one, its
 * TEE otherwise. Called from Rust only (`almena-keystore`, `vault::store`).
 *
 * Sealed is the 12-byte nonce, then the ciphertext and its tag, authenticated
 * over [AAD] — so a sealed record is only ever opened as one.
 *
 * **Biometrics are a second key, [BIOMETRIC_ALIAS]**, that does nothing until
 * `BiometricPrompt` has recognised a strong biometric for that one operation
 * (`vault::presence::android`), and that the system invalidates when another
 * fingerprint or face is enrolled. Rejections carry a code the Rust side reads:
 * `cancelled`, `changed`, `unavailable`, `failed`.
 *
 * `BiometricPrompt` needs a `FragmentActivity`. Tauri's `MainActivity` is one —
 * `TauriActivity` → `WryActivity` → `AppCompatActivity` — in code `tauri android
 * init` writes again, so nothing here asks `gen/android` to change; an activity
 * that were not one answers `unavailable`.
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

    @Command
    fun biometricAvailable(invoke: Invoke) {
        invoke.resolve(answer(biometricReady()))
    }

    /** A new biometric key, and the data key sealed under it once recognised. */
    @Command
    fun biometricArm(invoke: Invoke) {
        val args = invoke.parseArgs(Prompted::class.java)
        if (!biometricReady()) {
            invoke.reject("biometric: unavailable", UNAVAILABLE)
            return
        }
        try {
            val plain = Base64.decode(args.data, Base64.NO_WRAP)
            // A new key every time it is turned on: whatever an old one sealed
            // is replaced by what this one does.
            forgetBiometric()
            val cipher = Cipher.getInstance(TRANSFORMATION)
            cipher.init(Cipher.ENCRYPT_MODE, generate(BIOMETRIC_ALIAS, ::biometricSpec))
            authenticate(invoke, args, cipher) { unlocked ->
                unlocked.updateAAD(BIOMETRIC_AAD)
                val sealed = unlocked.iv + unlocked.doFinal(plain)
                invoke.resolve(bytes(sealed))
            }
        } catch (failure: Exception) {
            forgetBiometric()
            invoke.reject("biometric_arm: ${failure.javaClass.simpleName}", FAILED)
        }
    }

    /** The data key, once recognised. */
    @Command
    fun biometricOpen(invoke: Invoke) {
        val args = invoke.parseArgs(Prompted::class.java)
        try {
            val sealed = Base64.decode(args.data, Base64.NO_WRAP)
            if (sealed.size <= NONCE_BYTES) {
                invoke.reject("biometric_open: short", CHANGED)
                return
            }
            val cipher = decrypting(sealed) ?: run {
                invoke.reject("biometric_open: changed", CHANGED)
                return
            }
            authenticate(invoke, args, cipher) { unlocked ->
                unlocked.updateAAD(BIOMETRIC_AAD)
                try {
                    invoke.resolve(
                        bytes(unlocked.doFinal(sealed, NONCE_BYTES, sealed.size - NONCE_BYTES)),
                    )
                } catch (mismatch: AEADBadTagException) {
                    // A copy some other key sealed: nothing this one opens.
                    invoke.reject("biometric_open: mismatch", CHANGED)
                }
            }
        } catch (failure: Exception) {
            invoke.reject("biometric_open: ${failure.javaClass.simpleName}", FAILED)
        }
    }

    /**
     * Whether the copy can no longer be opened. The Keystore answers when the
     * cipher is set up, before any prompt: an invalidated key throws there.
     */
    @Command
    fun biometricChanged(invoke: Invoke) {
        val changed = try {
            val sealed = Base64.decode(invoke.parseArgs(Bytes::class.java).data, Base64.NO_WRAP)
            sealed.size <= NONCE_BYTES || decrypting(sealed) == null
        } catch (failure: Exception) {
            // Not knowing is not a change: the prompt will say.
            false
        }
        invoke.resolve(answer(changed))
    }

    @Command
    fun biometricForget(invoke: Invoke) {
        try {
            forgetBiometric()
            invoke.resolve()
        } catch (failure: Exception) {
            invoke.reject("biometric_forget: ${failure.javaClass.simpleName}", FAILED)
        }
    }

    private fun biometricReady(): Boolean =
        activity is FragmentActivity &&
            BiometricManager.from(activity).canAuthenticate(BIOMETRIC_STRONG) ==
            BiometricManager.BIOMETRIC_SUCCESS

    /** A cipher set up to open `sealed`, or null when its key is gone or invalidated. */
    private fun decrypting(sealed: ByteArray): Cipher? {
        val key = key(BIOMETRIC_ALIAS) ?: return null
        val cipher = Cipher.getInstance(TRANSFORMATION)
        return try {
            cipher.init(Cipher.DECRYPT_MODE, key, GCMParameterSpec(TAG_BITS, sealed, 0, NONCE_BYTES))
            cipher
        } catch (invalidated: KeyPermanentlyInvalidatedException) {
            null
        }
    }

    /**
     * Shows `BiometricPrompt` for `cipher`, strong biometrics only, with the
     * negative button the interface named ("Use PIN"), and hands `then` the
     * cipher the recognition unlocked. Every way it ends resolves `invoke`.
     */
    private fun authenticate(
        invoke: Invoke,
        args: Prompted,
        cipher: Cipher,
        then: (Cipher) -> Unit,
    ) {
        val host = activity as? FragmentActivity ?: run {
            invoke.reject("biometric: no fragment activity", UNAVAILABLE)
            return
        }
        if (args.title.isBlank() || args.cancel.isBlank()) {
            invoke.reject("biometric: no words", FAILED)
            return
        }
        host.runOnUiThread {
            val callback = object : BiometricPrompt.AuthenticationCallback() {
                override fun onAuthenticationSucceeded(result: BiometricPrompt.AuthenticationResult) {
                    val unlocked = result.cryptoObject?.cipher ?: run {
                        invoke.reject("biometric: no cipher", FAILED)
                        return
                    }
                    try {
                        then(unlocked)
                    } catch (failure: Exception) {
                        invoke.reject("biometric: ${failure.javaClass.simpleName}", FAILED)
                    }
                }

                override fun onAuthenticationError(code: Int, message: CharSequence) {
                    val reason = when (code) {
                        BiometricPrompt.ERROR_NEGATIVE_BUTTON,
                        BiometricPrompt.ERROR_USER_CANCELED,
                        BiometricPrompt.ERROR_CANCELED,
                        -> CANCELLED
                        BiometricPrompt.ERROR_NO_BIOMETRICS,
                        BiometricPrompt.ERROR_HW_NOT_PRESENT,
                        BiometricPrompt.ERROR_HW_UNAVAILABLE,
                        -> UNAVAILABLE
                        else -> FAILED
                    }
                    invoke.reject("biometric: $code", reason)
                }

                // A finger not recognised leaves the prompt up for another try;
                // only how it ends is an answer.
            }
            val info = BiometricPrompt.PromptInfo.Builder()
                .setTitle(args.title)
                .apply { if (args.subtitle.isNotBlank()) setSubtitle(args.subtitle) }
                .setNegativeButtonText(args.cancel)
                .setAllowedAuthenticators(BIOMETRIC_STRONG)
                .setConfirmationRequired(false)
                .build()
            BiometricPrompt(host, ContextCompat.getMainExecutor(host), callback)
                .authenticate(info, BiometricPrompt.CryptoObject(cipher))
        }
    }

    private fun forgetBiometric() {
        val store = KeyStore.getInstance(PROVIDER).apply { load(null) }
        if (store.containsAlias(BIOMETRIC_ALIAS)) store.deleteEntry(BIOMETRIC_ALIAS)
    }

    private fun answer(value: Boolean): JSObject {
        val answer = JSObject()
        answer.put("answer", value)
        return answer
    }

    private fun bytes(data: ByteArray): JSObject {
        val answer = JSObject()
        answer.put("data", Base64.encodeToString(data, Base64.NO_WRAP))
        return answer
    }

    private fun key(alias: String = ALIAS): SecretKey? {
        val store = KeyStore.getInstance(PROVIDER).apply { load(null) }
        return store.getKey(alias, null) as? SecretKey
    }

    private fun generate(): SecretKey = generate(ALIAS) {}

    /** Made inside the Keystore, in StrongBox when there is one. */
    private fun generate(alias: String, configure: (KeyGenParameterSpec.Builder) -> Unit): SecretKey {
        val strongBox = Build.VERSION.SDK_INT >= Build.VERSION_CODES.P &&
            activity.packageManager.hasSystemFeature(PackageManager.FEATURE_STRONGBOX_KEYSTORE)
        return try {
            generate(alias, strongBox, configure)
        } catch (failure: ProviderException) {
            // StrongBoxUnavailableException is a ProviderException; it is
            // named this way because the class does not exist before API 28.
            if (strongBox) generate(alias, false, configure) else throw failure
        }
    }

    private fun generate(
        alias: String,
        strongBox: Boolean,
        configure: (KeyGenParameterSpec.Builder) -> Unit,
    ): SecretKey {
        val spec = KeyGenParameterSpec.Builder(
            alias,
            KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT,
        )
            .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
            .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
            .setKeySize(256)
            .setRandomizedEncryptionRequired(true)
        if (strongBox && Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
            spec.setIsStrongBoxBacked(true)
        }
        configure(spec)
        val generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, PROVIDER)
        generator.init(spec.build())
        return generator.generateKey()
    }

    /**
     * Strong biometrics, for each operation on its own, and gone the moment
     * somebody else's fingerprint or face could be added. Before API 30 the
     * same is said with a validity of -1: authentication for every use, which
     * only a `CryptoObject` prompt gives.
     */
    private fun biometricSpec(spec: KeyGenParameterSpec.Builder) {
        spec.setUserAuthenticationRequired(true)
        spec.setInvalidatedByBiometricEnrollment(true)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            spec.setUserAuthenticationParameters(0, KeyProperties.AUTH_BIOMETRIC_STRONG)
        } else {
            @Suppress("DEPRECATION")
            spec.setUserAuthenticationValidityDurationSeconds(-1)
        }
    }

    private companion object {
        const val PROVIDER = "AndroidKeyStore"
        const val ALIAS = "almena-vault"
        const val BIOMETRIC_ALIAS = "almena-vault-biometric"
        const val TRANSFORMATION = "AES/GCM/NoPadding"
        const val NONCE_BYTES = 12
        const val TAG_BITS = 128
        val AAD = "almena-vault/keystore".toByteArray()
        val BIOMETRIC_AAD = "almena-vault/biometric".toByteArray()

        const val CANCELLED = "cancelled"
        const val CHANGED = "changed"
        const val UNAVAILABLE = "unavailable"
        const val FAILED = "failed"
    }
}
