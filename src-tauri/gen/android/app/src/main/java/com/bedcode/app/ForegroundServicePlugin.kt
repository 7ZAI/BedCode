package com.bedcode.app

import android.app.Activity
import app.tauri.annotation.Command
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.JSObject
import app.tauri.plugin.Invoke
import app.tauri.plugin.Plugin

/**
 * Tauri 插件 - 前台服务桥接
 *
 * 提供 JavaScript 调用 Android 前台服务的接口
 */
@TauriPlugin
class ForegroundServicePlugin(private val activity: Activity) : Plugin(activity) {

    @Command
    fun startForegroundService(invoke: Invoke) {
        val title = invoke.getString("title") ?: "BedCode"
        val content = invoke.getString("content") ?: "后台运行中"

        try {
            ForegroundService.start(activity, title, content)
            val result = JSObject()
            result.put("success", true)
            invoke.resolve(result)
        } catch (e: Exception) {
            val result = JSObject()
            result.put("success", false)
            result.put("error", e.message)
            invoke.resolve(result)
        }
    }

    @Command
    fun stopForegroundService(invoke: Invoke) {
        try {
            ForegroundService.stop(activity)
            val result = JSObject()
            result.put("success", true)
            invoke.resolve(result)
        } catch (e: Exception) {
            val result = JSObject()
            result.put("success", false)
            result.put("error", e.message)
            invoke.resolve(result)
        }
    }

    @Command
    fun updateForegroundNotification(invoke: Invoke) {
        val title = invoke.getString("title") ?: "BedCode"
        val content = invoke.getString("content") ?: "后台运行中"

        try {
            ForegroundService.updateNotification(activity, title, content)
            val result = JSObject()
            result.put("success", true)
            invoke.resolve(result)
        } catch (e: Exception) {
            val result = JSObject()
            result.put("success", false)
            result.put("error", e.message)
            invoke.resolve(result)
        }
    }
}