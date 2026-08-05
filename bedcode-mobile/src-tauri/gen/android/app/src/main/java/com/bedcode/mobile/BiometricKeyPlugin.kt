package com.bedcode.mobile

import android.app.Activity
import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.security.keystore.KeyPermanentlyInvalidatedException
import android.util.Base64
import androidx.biometric.BiometricManager
import androidx.biometric.BiometricPrompt
import androidx.core.content.ContextCompat
import androidx.fragment.app.FragmentActivity
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.JSObject
import app.tauri.plugin.Invoke
import app.tauri.plugin.Plugin
import java.math.BigInteger
import java.security.KeyPairGenerator
import java.security.KeyStore
import java.security.Signature
import java.security.spec.ECGenParameterSpec
import java.util.concurrent.Executor

/**
 * Tauri 插件 - 生物认证密钥管理（Android Keystore）
 *
 * 私钥存 AndroidKeyStore，setUserAuthenticationRequired(true) 强制每次签名前
 * 必须通过生物认证（指纹/人脸），私钥永不出 Keystore。
 * 通过 Rust 端 api.register_android_plugin() 注册（见 android_plugins.rs）。
 */
@InvokeArg
internal class BiometricKeyArgs {
    var alias: String = ""
    var message: String = ""
}

@TauriPlugin
class BiometricKeyPlugin(private val activity: Activity) : Plugin(activity) {

    companion object {
        private const val KEYSTORE_PROVIDER = "AndroidKeyStore"
        private const val SCALAR_BYTES = 32
        private const val TAG = "BedCode-BiometricKey"
    }

    private val keystore: KeyStore by lazy {
        KeyStore.getInstance(KEYSTORE_PROVIDER).apply { load(null) }
    }

    /// 生成 P-256 密钥对（已存在则先删除重建，保证公钥可换绑）
    @Command
    fun generateKeyPair(invoke: Invoke) {
        val args = invoke.parseArgs(BiometricKeyArgs::class.java)
        val result = JSObject()
        try {
            if (keystore.containsAlias(args.alias)) {
                keystore.deleteEntry(args.alias)
            }
            val generator = KeyPairGenerator.getInstance(
                KeyProperties.KEY_ALGORITHM_EC, KEYSTORE_PROVIDER
            )
            val specBuilder = KeyGenParameterSpec.Builder(
                args.alias,
                KeyProperties.PURPOSE_SIGN
            )
                .setAlgorithmParameterSpec(ECGenParameterSpec("secp256r1"))
                .setDigests(KeyProperties.DIGEST_SHA256)
                .setKeySize(256)
                // 每次签名都必须重新生物认证（不设置 validity duration）
                .setUserAuthenticationRequired(true)
                .setInvalidatedByBiometricEnrollment(true)
            // API 30+ 明确限定仅生物特征（指纹/人脸），不含 PIN 等设备凭据。
            // 注意：Keystore 层只有 AUTH_BIOMETRIC_STRONG / AUTH_DEVICE_CREDENTIAL 两个选项；
            // 且按 Android CDD，仅强生物特征（Class 3）允许与 Keystore 集成做加密运算，
            // 弱生物特征（摄像头人脸等）无法解锁此类密钥，故不能用 weak 降级。
            if (android.os.Build.VERSION.SDK_INT >= android.os.Build.VERSION_CODES.R) {
                specBuilder.setUserAuthenticationParameters(0, KeyProperties.AUTH_BIOMETRIC_STRONG)
            }
            generator.initialize(specBuilder.build())
            val keyPair = generator.generateKeyPair()
            val publicKey = Base64.encodeToString(keyPair.public.encoded, Base64.NO_WRAP)
            result.put("success", true)
            result.put("publicKey", publicKey)
            invoke.resolve(result)
        } catch (e: Exception) {
            android.util.Log.e(TAG, "generateKeyPair failed: ${e.message}")
            result.put("success", false)
            result.put("error", e.message)
            invoke.resolve(result)
        }
    }

    /// 检查密钥是否存在
    @Command
    fun hasKey(invoke: Invoke) {
        val args = invoke.parseArgs(BiometricKeyArgs::class.java)
        val result = JSObject()
        try {
            result.put("hasKey", keystore.containsAlias(args.alias))
            invoke.resolve(result)
        } catch (e: Exception) {
            result.put("hasKey", false)
            invoke.resolve(result)
        }
    }

    /// 删除密钥（解绑时调用）
    @Command
    fun deleteKey(invoke: Invoke) {
        val args = invoke.parseArgs(BiometricKeyArgs::class.java)
        val result = JSObject()
        try {
            if (keystore.containsAlias(args.alias)) {
                keystore.deleteEntry(args.alias)
            }
            result.put("success", true)
            invoke.resolve(result)
        } catch (e: Exception) {
            result.put("success", false)
            result.put("error", e.message)
            invoke.resolve(result)
        }
    }

    /// 设备是否支持生物认证密钥（硬件可用 + 已录入生物特征）
    ///
    /// 仅检查强生物特征（Class 3）：按 Android CDD，Keystore 加密运算只能由强生物特征解锁，
    /// 弱生物特征（摄像头人脸等）无法与 Keystore 集成。reason 为 BiometricManager 结果码，
    /// 供 UI 展示具体不支持原因。
    @Command
    fun isDeviceSupported(invoke: Invoke) {
        val result = JSObject()
        try {
            val biometricManager = BiometricManager.from(activity)
            val canAuth = biometricManager.canAuthenticate(BiometricManager.Authenticators.BIOMETRIC_STRONG)
            // 结果码：0=SUCCESS 1=HW_UNAVAILABLE 11=NONE_ENROLLED 12=NO_HARDWARE
            android.util.Log.d(TAG, "isDeviceSupported canAuthenticate=$canAuth")
            result.put("supported", canAuth == BiometricManager.BIOMETRIC_SUCCESS)
            result.put("reason", canAuth)
            invoke.resolve(result)
        } catch (e: Exception) {
            android.util.Log.e(TAG, "isDeviceSupported failed: ${e.message}")
            result.put("supported", false)
            result.put("reason", -1)
            invoke.resolve(result)
        }
    }

    /// 生物认证签名：弹系统指纹/人脸弹窗，认证通过后签名消息（hex 字符串的 UTF-8 字节）
    @Command
    fun sign(invoke: Invoke) {
        val args = invoke.parseArgs(BiometricKeyArgs::class.java)
        val result = JSObject()
        try {
            if (!keystore.containsAlias(args.alias)) {
                result.put("success", false)
                result.put("error", "Biometric key not generated")
                invoke.resolve(result)
                return
            }
            if (!(activity is FragmentActivity)) {
                result.put("success", false)
                result.put("error", "Activity is not FragmentActivity")
                invoke.resolve(result)
                return
            }

            val privateKeyEntry = keystore.getEntry(args.alias, null) as KeyStore.PrivateKeyEntry
            val signature = Signature.getInstance("SHA256withECDSA")
            signature.initSign(privateKeyEntry.privateKey)
            val cryptoObject = BiometricPrompt.CryptoObject(signature)

            val executor: Executor = ContextCompat.getMainExecutor(activity)
            val prompt = BiometricPrompt(
                activity as FragmentActivity, executor,
                object : BiometricPrompt.AuthenticationCallback() {
                    override fun onAuthenticationSucceeded(result: BiometricPrompt.AuthenticationResult) {
                        try {
                            val crypto = result.cryptoObject
                            val signer = crypto?.signature
                            if (signer == null) {
                                resultFail(invoke, "Biometric result missing signature crypto")
                                return
                            }
                            signer.update(args.message.toByteArray(Charsets.UTF_8))
                            val der = signer.sign()
                            val raw = derToRaw(der)
                            val response = JSObject()
                            response.put("success", true)
                            response.put("signature", Base64.encodeToString(raw, Base64.NO_WRAP))
                            invoke.resolve(response)
                        } catch (e: Exception) {
                            resultFail(invoke, "Sign failed: ${e.message}")
                        }
                    }

                    override fun onAuthenticationError(errorCode: Int, errString: CharSequence) {
                        resultFail(invoke, errString.toString())
                    }

                    override fun onAuthenticationFailed() {
                        // 生物特征不匹配：不 resolve，等待下一次尝试或取消
                    }
                }
            )

            val promptInfo = BiometricPrompt.PromptInfo.Builder()
                .setTitle("BedCode 生物认证")
                .setSubtitle("验证身份以解锁设备密钥")
                .setNegativeButtonText("取消")
                .build()
            prompt.authenticate(promptInfo, cryptoObject)
        } catch (e: KeyPermanentlyInvalidatedException) {
            resultFail(invoke, "Biometric key invalidated, please bind again")
        } catch (e: Exception) {
            resultFail(invoke, e.message)
        }
    }

    private fun resultFail(invoke: Invoke, message: String?) {
        val result = JSObject()
        result.put("success", false)
        result.put("error", message ?: "Unknown error")
        invoke.resolve(result)
    }

    /// ASN.1 DER → 原始 r||s（P-256 各 32 字节）
    ///
    /// DER 布局: 30 <总长> 02 <rlen> <r> 02 <slen> <s>。
    /// readDerInt 内部会跳过 tag 与长度字节；P-256 签名各段长度恒为单字节（总长 < 128），
    /// 因此 offset=2 后直接指向 r 的 tag，无需再跳长度。
    private fun derToRaw(der: ByteArray): ByteArray {
        var offset = 2 // 跳过 0x30 与总长度字节，此时指向 r 的 tag
        val r = readDerInt(der, offset)
        val s = readDerInt(der, r.second)

        val rFixed = toFixedLength(r.first, SCALAR_BYTES)
        val sFixed = toFixedLength(s.first, SCALAR_BYTES)
        return rFixed + sFixed
    }

    /// 读取 DER INTEGER（含 0x02 与长度），返回（去前导零后的值，下一个偏移）
    private fun readDerInt(der: ByteArray, offset: Int): Pair<ByteArray, Int> {
        if (der[offset].toInt() != 0x02) {
            throw IllegalArgumentException("Invalid DER: expected INTEGER")
        }
        val len = der[offset + 1].toInt() and 0xFF
        var value = der.copyOfRange(offset + 2, offset + 2 + len)
        var start = 0
        while (start < value.size - 1 && value[start].toInt() == 0) {
            start++
        }
        if (start > 0) {
            value = value.copyOfRange(start, value.size)
        }
        return Pair(value, offset + 2 + len)
    }

    /// 补齐/截断到定长标量字节
    private fun toFixedLength(value: ByteArray, length: Int): ByteArray {
        if (value.size == length) return value
        if (value.size < length) {
            val padded = ByteArray(length)
            System.arraycopy(value, 0, padded, length - value.size, value.size)
            return padded
        }
        // 超出（极罕见），取低 length 字节
        return value.copyOfRange(value.size - length, value.size)
    }
}
