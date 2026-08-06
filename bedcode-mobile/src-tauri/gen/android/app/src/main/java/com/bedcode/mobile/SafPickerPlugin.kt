package com.bedcode.mobile

import android.app.Activity
import android.content.Intent
import android.net.Uri
import android.os.Environment
import android.provider.DocumentsContract
import androidx.activity.result.ActivityResult
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
            startActivityForResult(invoke, intent, "directoryResult")
        } catch (e: Exception) {
            android.util.Log.e(TAG, "pickDirectory launch failed: ${e.message}")
            invoke.reject("Failed to launch directory picker: ${e.message}")
        }
    }

    /// 目录树选择回调
    @ActivityCallback
    fun directoryResult(invoke: Invoke, result: ActivityResult) {
        when (result.resultCode) {
            Activity.RESULT_OK -> {
                val data = result.data
                // Intent.data 为 Java 平台类型，需显式标注 Uri? 才能经空检查智能转换
                val uri: Uri? = data?.data
                if (uri == null) {
                    invoke.reject("No directory selected")
                    return
                }
                persistPermission(uri)
                invoke.resolve(
                    baseResult(uri).apply {
                        put("documentId", DocumentsContract.getTreeDocumentId(uri))
                    },
                )
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
    fun fileResult(invoke: Invoke, result: ActivityResult) {
        when (result.resultCode) {
            Activity.RESULT_OK -> {
                val data = result.data
                // Intent.data 为 Java 平台类型，需显式标注 Uri? 才能经空检查智能转换
                val uri: Uri? = data?.data
                if (uri == null) {
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
