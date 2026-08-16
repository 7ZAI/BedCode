package com.bedcode.mobile

import android.app.Activity
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.os.Environment
import android.provider.Settings
import app.tauri.annotation.Command
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.JSObject
import app.tauri.plugin.Invoke
import app.tauri.plugin.Plugin

/**
 * Tauri 插件 - 所有文件访问权限引导（MANAGE_EXTERNAL_STORAGE）
 *
 * Android 11+ 分区存储下，非媒体集合的顶层自定义目录（如存储根目录下的
 * 自定义文件夹）经真实路径 read_dir 会被 FUSE 过滤为空，宿主文件服务
 * 因此看不到内容。该权限无运行时弹窗申请机制，只能跳转系统设置页手动
 * 开启——本插件提供一键跳转（部分厂商 ROM 无该页面时兜底应用详情页）。
 *
 * 授权状态检查在 Rust 宿主侧（server.rs needs_all_files_access + notice
 * 链路）与前端按钮处配合使用；Kotlin 侧只负责状态查询与 Intent 跳转。
 *
 * 由 Rust 端 android_plugins.rs 注册（AllFilesAccessPlugin）。
 * gen/android 重建恢复清单：本文件须恢复（参见 AGENTS.md「Android」节）。
 */
@TauriPlugin
class AllFilesAccessPlugin(private val activity: Activity) : Plugin(activity) {

    companion object {
        private const val TAG = "BedCode-AllFilesAccess"
    }

    /**
     * 查询「所有文件访问权限」状态；未授权时跳转系统授权页（或兜底应用详情页）。
     *
     * 返回 { granted: bool, jumped: bool }：granted 为跳转前的授权状态，
     * jumped 标记本次是否发起了跳转（已授权时为 false）。
     */
    @Command
    fun openAllFilesAccessSettings(invoke: Invoke) {
        val result = JSObject()
        try {
            val granted = Build.VERSION.SDK_INT >= Build.VERSION_CODES.R
                && Environment.isExternalStorageManager()
            result.put("granted", granted)
            if (!granted) {
                result.put("jumped", jumpToSettings())
            } else {
                result.put("jumped", false)
            }
            invoke.resolve(result)
        } catch (e: Exception) {
            android.util.Log.e(TAG, "openAllFilesAccessSettings failed: ${e.message}")
            result.put("granted", false)
            result.put("jumped", false)
            result.put("error", e.message)
            invoke.resolve(result)
        }
    }

    /** 跳转系统授权页；无该页面（部分厂商 ROM）时兜底应用详情页 */
    private fun jumpToSettings(): Boolean {
        val grantIntent = Intent(Settings.ACTION_MANAGE_ALL_FILES_ACCESS_PERMISSION)
            .setData(Uri.parse("package:${activity.packageName}"))
        if (grantIntent.resolveActivity(activity.packageManager) != null) {
            activity.startActivity(grantIntent)
            return true
        }
        val detailIntent = Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS)
            .setData(Uri.parse("package:${activity.packageName}"))
        if (detailIntent.resolveActivity(activity.packageManager) != null) {
            activity.startActivity(detailIntent)
            return true
        }
        return false
    }
}
