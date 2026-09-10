package com.bedcode.mobile

import android.app.Activity
import android.content.Intent
import android.net.Uri
import android.os.Environment
import android.provider.DocumentsContract
import androidx.activity.ComponentActivity
import androidx.activity.result.ActivityResult
import androidx.activity.result.ActivityResultLauncher
import androidx.activity.result.contract.ActivityResultContracts
import app.tauri.annotation.Command
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.JSObject
import app.tauri.plugin.Invoke
import app.tauri.plugin.Plugin

/**
 * Tauri 插件 - SAF 文件/目录选择器
 *
 * 用 Android Storage Access Framework 弹系统选择器（免任何存储权限）：
 * - pickDirectory：ACTION_OPEN_DOCUMENT_TREE，选共享目录（挂载根）
 * - pickFile：ACTION_OPEN_DOCUMENT，选本地文件（上传用）
 *
 * 选择后调用 takePersistableUriPermission 持久化授权（重启仍有效），
 * 并把 SAF Uri 拆成 (authority, documentId, primaryDir) 返回；
 * 真实路径解析（externalstorage/downloads provider → /storage/...）由 Rust 侧
 * saf_path 模块完成（可单测），Kotlin 侧只负责选择器与 Uri 元数据提取。
 *
 * 文件选择额外尝试 _data 列直读真实路径（Downloads/Media 等 provider 有效），
 * 命中则优先返回，Rust 侧解析失败时以此兜底。
 *
 * 由 Rust 端 android_plugins.rs 注册（SafPickerPlugin）。
 * gen/android 重建恢复清单：本文件须恢复（参见 AGENTS.md「Android」节）。
 *
 * 为什么走 ActivityResultRegistry 无 owner 动态注册，而不用 Tauri 的
 * Plugin.startActivityForResult() + @ActivityCallback（2026-09 真机实证修复）：
 *   - PluginManager.onActivityCreate 只在静态 activity 字段为空时注册一次
 *     launcher（guard 防重复注册）；Activity 被系统销毁重建（内存压力回收、
 *     fontScale 变更等）后 onActivityCreate 因 guard 直接 return，而旧 Activity
 *     销毁时 lifecycle-owner 注册已 unregister 对应 key——Plugin 持有的
 *     static launcher 自此失效，任何 startActivityForResult 都抛
 *     "Attempting to launch an unregistered ActivityResultLauncher"。
 *     真机（release 8-26/9-6/9-7）pickDirectory 反复命中，上传 pickFile 同源。
 *   - 本实现改用 activity.activityResultRegistry.register(key, contract, callback)
 *     无 lifecycle-owner 重载：每次调用注册独立 key、回调内 unregister，不依赖
 *     PluginManager 的注册时机；Activity 重建后 registry 随新实例重建，旧 key
 *     自然失效，下次调用重新注册即可用。该重载不检查 lifecycle state，可在
 *     Activity RESUMED 后调用（与带 owner 重载不同，无 "register before STARTED"
 *     限制）。
 *   - 同一时刻仅一个选择器在途（系统选择器模态），key 用纳秒时间戳保证唯一。
 */
@TauriPlugin
class SafPickerPlugin(private val activity: Activity) : Plugin(activity) {

    // 警告：构造参数必须精确声明为 android.app.Activity，禁止用 AppCompatActivity 等子类。
    // Rust 侧 register_android_plugin 以 JNI 精确签名 "(Landroid/app/Activity;)V" 查找构造器；
    // 声明为子类后 saf-picker 类上无该签名，ART 会解析命中父类 Plugin.<init>(Activity) 并
    // 以父类构造器创建实例——子类构造器体从不执行，activity 字段保持 null，
    // pickDirectory 在 activity.activityResultRegistry 处 NPE（真机实证 2026-09-09）。
    // activityResultRegistry 在 androidx Activity（ComponentActivity）上，经转换访问。

    companion object {
        private const val TAG = "BedCode-SafPicker"
        private const val FLAG_READ_WRITE =
            Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION
    }

    // ==================== 目录选择（共享根） ====================

    /// 弹系统目录树选择器
    @Command
    fun pickDirectory(invoke: Invoke) {
        try {
            val intent = Intent(Intent.ACTION_OPEN_DOCUMENT_TREE)
            intent.addFlags(
                Intent.FLAG_GRANT_READ_URI_PERMISSION or
                    Intent.FLAG_GRANT_WRITE_URI_PERMISSION or
                    Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION or
                    Intent.FLAG_GRANT_PREFIX_URI_PERMISSION,
            )
            android.util.Log.i(TAG, "pickDirectory: launching ACTION_OPEN_DOCUMENT_TREE")
            launchPicker(intent) { result ->
                android.util.Log.i(
                    TAG,
                    "directoryResult: code=${result.resultCode} data=${result.data} clip=${result.data?.clipData}",
                )
                when (result.resultCode) {
                    Activity.RESULT_OK -> {
                        val data = result.data
                        // Intent.data 为 Java 平台类型，需显式标注 Uri? 才能经空检查智能转换
                        val uri: Uri? = data?.data ?: data?.clipData?.getItemAt(0)?.uri
                        if (uri == null) {
                            android.util.Log.e(
                                TAG,
                                "directoryResult: RESULT_OK but no uri in data/clipData",
                            )
                            invoke.reject("No directory selected")
                            return@launchPicker
                        }
                        persistPermission(uri)
                        invoke.resolve(
                            baseResult(uri).apply {
                                put("documentId", DocumentsContract.getTreeDocumentId(uri))
                            },
                        )
                        android.util.Log.i(TAG, "directoryResult: resolved uri=$uri")
                    }
                    Activity.RESULT_CANCELED ->
                        invoke.resolve(JSObject().apply { put("cancelled", true) })
                    else -> invoke.reject(
                        "Directory picker failed (resultCode=${result.resultCode})",
                    )
                }
            }
        } catch (e: Exception) {
            android.util.Log.e(TAG, "pickDirectory launch failed: ${e.message}")
            invoke.reject("Failed to launch directory picker: ${e.message}")
        }
    }

    // ==================== 文件选择（上传） ====================

    /// 弹系统文件选择器
    @Command
    fun pickFile(invoke: Invoke) {
        try {
            val intent = Intent(Intent.ACTION_OPEN_DOCUMENT)
            intent.addCategory(Intent.CATEGORY_OPENABLE)
            intent.type = "*/*"
            launchPicker(intent) { result ->
                android.util.Log.i(TAG, "fileResult: code=${result.resultCode} data=${result.data}")
                when (result.resultCode) {
                    Activity.RESULT_OK -> {
                        val data = result.data
                        // Intent.data 为 Java 平台类型，需显式标注 Uri? 才能经空检查智能转换
                        val uri: Uri? = data?.data ?: data?.clipData?.getItemAt(0)?.uri
                        if (uri == null) {
                            android.util.Log.e(
                                TAG,
                                "fileResult: RESULT_OK but no uri in data/clipData",
                            )
                            invoke.reject("No file selected")
                            return@launchPicker
                        }
                        persistPermission(uri)
                        invoke.resolve(
                            baseResult(uri).apply {
                                put("documentId", DocumentsContract.getDocumentId(uri))
                                put("dataPath", queryDataPath(uri))
                            },
                        )
                    }
                    Activity.RESULT_CANCELED ->
                        invoke.resolve(JSObject().apply { put("cancelled", true) })
                    else -> invoke.reject("File picker failed (resultCode=${result.resultCode})")
                }
            }
        } catch (e: Exception) {
            android.util.Log.e(TAG, "pickFile launch failed: ${e.message}")
            invoke.reject("Failed to launch file picker: ${e.message}")
        }
    }

    // ==================== 选择器基础设施 ====================

    /// 动态注册一次性 ActivityResultLauncher（无 lifecycle-owner 重载）。
    ///
    /// 每次调用以纳秒时间戳生成独立 key 注册，回调后立即 unregister；
    /// 选择器模态单实例，不会并发。launch 抛异常时兜底 unregister 防 key 泄漏。
    private fun launchPicker(intent: Intent, onResult: (ActivityResult) -> Unit) {
        val registry = (activity as ComponentActivity).activityResultRegistry
        val key = "saf-picker-${System.nanoTime()}"
        lateinit var launcher: ActivityResultLauncher<Intent>
        launcher = registry.register(
            key,
            ActivityResultContracts.StartActivityForResult(),
        ) { result ->
            launcher.unregister()
            onResult(result)
        }
        try {
            launcher.launch(intent)
        } catch (e: Exception) {
            launcher.unregister()
            throw e
        }
    }

    // ==================== 工具 ====================

    /// 公共元数据：uri / authority / 显示名 / 主存储根（供 Rust 端路径解析）
    private fun baseResult(uri: Uri): JSObject {
        val displayName = try {
            activity.contentResolver.query(
                uri,
                arrayOf(android.provider.OpenableColumns.DISPLAY_NAME),
                null,
                null,
                null,
            )?.use { c ->
                if (c.moveToFirst()) {
                    val idx = c.getColumnIndex(android.provider.OpenableColumns.DISPLAY_NAME)
                    if (idx >= 0) c.getString(idx) ?: "" else ""
                } else {
                    ""
                }
            } ?: ""
        } catch (e: Exception) {
            ""
        }
        return JSObject().apply {
            put("uri", uri.toString())
            put("authority", uri.authority ?: "")
            put("displayName", displayName)
            put(
                "primaryDir",
                Environment.getExternalStorageDirectory().absolutePath,
            )
        }
    }

    /// 持久化 SAF 授权（provider 不支持持久化时静默降级为单次授权）
    private fun persistPermission(uri: Uri) {
        try {
            activity.contentResolver.takePersistableUriPermission(uri, FLAG_READ_WRITE)
        } catch (e: SecurityException) {
            android.util.Log.w(TAG, "takePersistableUriPermission unavailable: ${e.message}")
        }
    }

    /// 查询 _data 列直读真实路径（Downloads/Media provider 有效；不可用返回空串）
    private fun queryDataPath(uri: Uri): String {
        return try {
            activity.contentResolver.query(uri, arrayOf("_data"), null, null, null)?.use { c ->
                if (c.moveToFirst()) {
                    val idx = c.getColumnIndex("_data")
                    if (idx >= 0) c.getString(idx) ?: "" else ""
                } else {
                    ""
                }
            } ?: ""
        } catch (e: Exception) {
            ""
        }
    }
}
