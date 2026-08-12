package com.bedcode.mobile

import android.app.Activity
import android.content.Intent
import android.net.Uri
import android.os.Environment
import android.provider.DocumentsContract
import app.tauri.annotation.ActivityCallback
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
 * 为什么走 Tauri 的 Plugin.startActivityForResult() + @ActivityCallback，
 * 而不自持 ActivityResultLauncher（以下均按 androidx.activity 1.10.1 字节码
 * 验证，2026-08 重构时逐条核对）：
 *   - 带 LifecycleOwner 的 register(key, owner, ...) 重载要求调用时 lifecycle
 *     currentState < STARTED，否则抛
 *     "LifecycleOwner ... is attempting to register while current state is
 *     RESUMED. LifecycleOwners must call register before they are STARTED."
 *     （ActivityResultRegistry.register 字节码偏移 33-110）。
 *   - 无 lifecycle 的 register(key, contract, callback) 重载虽会绑 rc 令牌，
 *     但自持 launcher 的调用时机必然在 Activity RESUMED 之后（用户点按钮才
 *     发起），届时若用带 owner 重载必抛——即“在 RESUMED 的 Activity 上临时
 *     注册 launcher 发起选择”这条路径在新 androidx 下根本走不通。
 *  唯一合法注册点是 Activity 的 onCreate（STARTED 前）。Tauri 的
 *  PluginManager.onActivityCreate()（WryActivity.onCreate → Rust.onActivityCreate
 *  → PluginManager.onActivityCreate）正是在那里 registerForActivityResult 一次
 *  注册 startActivityForResultLauncher；Plugin.startActivityForResult() 经
 *  PluginHandle → PluginManager 转发到该 launcher（requestCode 由 PluginManager
 *  单例登记）。同仓库 SafTransferPlugin.saveToDocument 即用此模式并已验证可用。
 *
 *  已知残留失败模式（勿误以为已覆盖）：Activity 被系统销毁重建（内存压力回收、
 *  fontScale 变更等；manifest 已锁 portrait + configChanges 覆盖旋转/深浅色）时，
 *  lifecycle-owner 注册附带的观察者在 ON_DESTROY 调 unregister(key)（register$2$1
 *  字节码 161-171），重建后旧 key 已注销：
 *   - 重建后再 launch → "Attempting to launch an unregistered
 *     ActivityResultLauncher"，被下方 try/catch 捕获后 reject（用户可见失败、
 *     可重试）；
 *   - 选择器已打开、结果后到 → 结果落到新 registry 的 pendingResults 永久停放，
 *     invoke 永不落定（前端 promise 挂起，无兜底超时——前端调用方需自行超时）。
 *  本项目选择器均从用户点击触发、单一 picker 串行，正常路径不重建；残留风险
 *  仅限系统极端回收场景，故不做额外兜底，此处如实记录。
 */
@TauriPlugin
class SafPickerPlugin(private val activity: Activity) : Plugin(activity) {

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
            startActivityForResult(invoke, intent, "directoryResult")
        } catch (e: Exception) {
            android.util.Log.e(TAG, "pickDirectory launch failed: ${e.message}")
            invoke.reject("Failed to launch directory picker: ${e.message}")
        }
    }

    /// 目录树选择回调
    @ActivityCallback
    fun directoryResult(invoke: Invoke, result: androidx.activity.result.ActivityResult) {
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
                    android.util.Log.e(TAG, "directoryResult: RESULT_OK but no uri in data/clipData")
                    invoke.reject("No directory selected")
                    return
                }
                persistPermission(uri)
                invoke.resolve(
                    baseResult(uri).apply {
                        put("documentId", DocumentsContract.getTreeDocumentId(uri))
                    },
                )
                android.util.Log.i(TAG, "directoryResult: resolved uri=$uri")
            }
            Activity.RESULT_CANCELED -> invoke.resolve(JSObject().apply { put("cancelled", true) })
            else -> invoke.reject("Directory picker failed (resultCode=${result.resultCode})")
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
            startActivityForResult(invoke, intent, "fileResult")
        } catch (e: Exception) {
            android.util.Log.e(TAG, "pickFile launch failed: ${e.message}")
            invoke.reject("Failed to launch file picker: ${e.message}")
        }
    }

    /// 文件选择回调：优先 _data 列直读真实路径，否则回退交给 Rust 解析
    @ActivityCallback
    fun fileResult(invoke: Invoke, result: androidx.activity.result.ActivityResult) {
        android.util.Log.i(
            TAG,
            "fileResult: code=${result.resultCode} data=${result.data}",
        )
        when (result.resultCode) {
            Activity.RESULT_OK -> {
                val data = result.data
                // Intent.data 为 Java 平台类型，需显式标注 Uri? 才能经空检查智能转换
                val uri: Uri? = data?.data ?: data?.clipData?.getItemAt(0)?.uri
                if (uri == null) {
                    android.util.Log.e(TAG, "fileResult: RESULT_OK but no uri in data/clipData")
                    invoke.reject("No file selected")
                    return
                }
                persistPermission(uri)
                invoke.resolve(
                    baseResult(uri).apply {
                        put("documentId", DocumentsContract.getDocumentId(uri))
                        put("dataPath", queryDataPath(uri))
                    },
                )
            }
            Activity.RESULT_CANCELED -> invoke.resolve(JSObject().apply { put("cancelled", true) })
            else -> invoke.reject("File picker failed (resultCode=${result.resultCode})")
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