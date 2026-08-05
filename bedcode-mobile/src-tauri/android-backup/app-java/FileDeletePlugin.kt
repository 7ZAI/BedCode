package com.bedcode.mobile

import android.app.Activity
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.JSObject
import app.tauri.plugin.Invoke
import app.tauri.plugin.Plugin
import java.io.File

/**
 * Tauri 插件 - 删除文件
 *
 * 宿主删除文件能力（WASM 插件 HostFs::fs_delete 的 Android 实现）。
 * 删除前做 fs_auth 授权校验的是 Rust 宿主侧，Kotlin 侧只执行删除；
 * 文件不存在视为成功（幂等，与桌面端 host_fs_delete 语义一致）。
 *
 * 由 Rust 端 android_plugins.rs 注册（FileDeletePlugin）。
 * gen/android 重建恢复清单：本文件须恢复（参见 AGENTS.md「Android」节）。
 */
@InvokeArg
internal class FileDeleteArgs {
    var path: String = ""
}

@TauriPlugin
class FileDeletePlugin(private val activity: Activity) : Plugin(activity) {

    companion object {
        private const val TAG = "BedCode-FileDelete"
    }

    /// 删除指定路径文件（不存在也返回 ok，幂等）
    @Command
    fun deleteFile(invoke: Invoke) {
        val args = invoke.parseArgs(FileDeleteArgs::class.java)
        val result = JSObject()
        try {
            val file = File(args.path)
            val deleted = !file.exists() || file.delete()
            result.put("ok", deleted)
            if (!deleted) {
                android.util.Log.e(TAG, "delete failed: ${args.path}")
                result.put("error", "delete failed: ${args.path}")
            }
            invoke.resolve(result)
        } catch (e: Exception) {
            android.util.Log.e(TAG, "deleteFile failed: ${e.message}")
            result.put("ok", false)
            result.put("error", e.message)
            invoke.resolve(result)
        }
    }
}
