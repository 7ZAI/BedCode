package com.bedcode.mobile

import android.app.Activity
import android.os.Build
import android.provider.Settings
import app.tauri.annotation.Command
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.JSObject
import app.tauri.plugin.Invoke
import app.tauri.plugin.Plugin

/**
 * Tauri 插件 - 获取系统设备信息
 *
 * 返回用户设置的设备名称（Settings.Global "device_name"，即系统设置中的设备名，
 * 如小米手机的 "xiaomi k30"；API 25+ 可读，无权限要求）、机型、厂商与 OS 版本。
 * Settings.Global 取不到时回退 Build.MODEL（如 "Redmi K30"）。
 *
 * 由 Rust 端 android_plugins.rs 注册（DeviceInfoPlugin）。
 * gen/android 重建恢复清单：本文件须恢复（参见 AGENTS.md「Android」节）。
 */
@TauriPlugin
class DeviceInfoPlugin(private val activity: Activity) : Plugin(activity) {

    companion object {
        private const val TAG = "BedCode-DeviceInfo"
    }

    /// 获取设备信息
    @Command
    fun getDeviceInfo(invoke: Invoke) {
        val result = JSObject()
        try {
            // API 25+ 支持读取用户设置的设备名称；早期版本回退机型
            val userDeviceName = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.N_MR1) {
                Settings.Global.getString(activity.contentResolver, Settings.Global.DEVICE_NAME)
            } else {
                null
            }
            result.put("deviceName", userDeviceName ?: Build.MODEL)
            result.put("model", Build.MODEL)
            result.put("manufacturer", Build.MANUFACTURER)
            result.put("osVersion", Build.VERSION.RELEASE)
            result.put("sdkInt", Build.VERSION.SDK_INT)
            result.put("success", true)
            invoke.resolve(result)
        } catch (e: Exception) {
            android.util.Log.e(TAG, "getDeviceInfo failed: ${e.message}")
            result.put("success", false)
            result.put("error", e.message)
            invoke.resolve(result)
        }
    }
}
