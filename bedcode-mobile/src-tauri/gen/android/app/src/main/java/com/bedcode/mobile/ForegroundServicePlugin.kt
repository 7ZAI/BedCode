package com.bedcode.mobile

import android.app.Activity
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.JSObject
import app.tauri.plugin.Invoke
import app.tauri.plugin.Plugin

/**
 * Tauri 插件 - 前台服务桥接
 *
 * 提供 JavaScript 调用 Android 前台服务的接口
 * 通过 Rust 端 api.register_android_plugin() 注册到 PluginManager
 */
@InvokeArg
internal class ForegroundServiceArgs {
    var title: String = "BedCode"
    var content: String = "后台运行中"
}

@TauriPlugin
class ForegroundServicePlugin(private val activity: Activity) : Plugin(activity) {

    @Command
    fun startForegroundService(invoke: Invoke) {
        val args = invoke.parseArgs(ForegroundServiceArgs::class.java)

        try {
            ForegroundService.start(activity, args.title, args.content)
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
        val args = invoke.parseArgs(ForegroundServiceArgs::class.java)

        try {
            ForegroundService.updateNotification(activity, args.title, args.content)
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