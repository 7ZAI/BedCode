package com.bedcode.app

import android.app.Activity
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.Plugin
import app.tauri.plugin.JSObject

/**
 * Tauri 插件 - 任务通知桥接
 *
 * 提供前端调用 Android 通知管理器的接口
 * @TauriPlugin 注解由 Tauri 构建系统自动注册，无需手动调用 PluginManager.register
 */
@InvokeArg
internal class TaskNotificationArgs {
    var sessionId: String = ""
    var sessionName: String = ""
    var taskStatus: String = ""
    var taskReason: String? = null
    var vibrate: Boolean = false
    var sound: Boolean = false
}

@InvokeArg
internal class CancelTaskNotificationArgs {
    var sessionId: String = ""
}

@TauriPlugin
class TaskNotificationPlugin(private val activity: Activity) : Plugin(activity) {

    private val manager by lazy { TaskNotificationManager.getInstance(activity) }

    @Command
    fun showTaskNotification(invoke: Invoke) {
        val args = invoke.parseArgs(TaskNotificationArgs::class.java)

        try {
            manager.showTaskNotification(
                sessionId = args.sessionId,
                sessionName = args.sessionName,
                status = args.taskStatus,
                reason = args.taskReason,
                vibrate = args.vibrate,
                sound = args.sound
            )
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
    fun cancelTaskNotification(invoke: Invoke) {
        val args = invoke.parseArgs(CancelTaskNotificationArgs::class.java)

        try {
            manager.cancelTaskNotification(args.sessionId)
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
    fun cancelAllTaskNotifications(invoke: Invoke) {
        try {
            manager.cancelAllTaskNotifications()
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
