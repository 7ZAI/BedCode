package com.bedcode.mobile

import android.app.Activity
import android.os.Environment
import app.tauri.annotation.Command
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.JSObject
import app.tauri.plugin.Invoke
import app.tauri.plugin.Plugin

/**
 * Tauri 插件 - 获取 Android 外部私有下载目录
 *
 * 返回 Context.getExternalFilesDir(Environment.DIRECTORY_DOWNLOADS) 的绝对路径。
 * 该目录位于 /storage/emulated/0/Android/data/com.bedcode.mobile/files/Download，
 * 免存储权限、多数文件管理器可见。外部存储不可用时回退到 null（宿主侧兜底处理）。
 *
 * 由 Rust 端 android_plugins.rs 注册（DownloadsDirPlugin）。
 * gen/android 重建恢复清单：本文件须恢复（参见 AGENTS.md「Android」节）。
 */
@TauriPlugin
class DownloadsDirPlugin(private val activity: Activity) : Plugin(activity) {

    companion object {
        private const val TAG = "BedCode-DownloadsDir"
    }

    /// 获取外部私有下载目录绝对路径
    @Command
    fun getDownloadsDir(invoke: Invoke) {
        val result = JSObject()
        try {
            val dir = activity.getExternalFilesDir(Environment.DIRECTORY_DOWNLOADS)
            if (dir != null) {
                result.put("path", dir.absolutePath)
            } else {
                // 外部存储不可用（如 USB 大容量存储模式）
                result.put("path", "")
            }
            invoke.resolve(result)
        } catch (e: Exception) {
            android.util.Log.e(TAG, "getDownloadsDir failed: ${e.message}")
            result.put("path", "")
            result.put("error", e.message)
            invoke.resolve(result)
        }
    }
}
