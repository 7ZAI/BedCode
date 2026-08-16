package com.bedcode.mobile

import android.Manifest
import android.app.Activity
import android.os.Build
import androidx.core.app.NotificationManagerCompat
import app.tauri.PermissionState
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.Permission
import app.tauri.annotation.PermissionCallback
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.Plugin
import app.tauri.plugin.JSObject

/**
 * Tauri 插件 - 任务/连接/插件通知桥接
 *
 * 提供前端调用 Android 通知管理器的接口，支持震动/声音分开控制，
 * 并承担 Android 13+ 通知权限（POST_NOTIFICATIONS）的检查与请求。
 * 通过 Rust 端 api.register_android_plugin() 注册到 PluginManager。
 */
@InvokeArg
internal class ShowTaskNotificationArgs {
    var sessionId: String = ""
    var title: String = ""
    var body: String = ""
    var vibrate: Boolean = false
    var sound: Boolean = false
}

@InvokeArg
internal class ShowConnectionNotificationArgs {
    var title: String = ""
    var body: String = ""
    var vibrate: Boolean = false
    var sound: Boolean = false
}

@InvokeArg
internal class ShowPluginNotificationArgs {
    var title: String = ""
    var body: String = ""
}

@InvokeArg
internal class ShowTransferRequestNotificationArgs {
    var batchId: String = ""
    var pluginId: String = ""
    var title: String = ""
    var body: String = ""
    var acceptLabel: String = ""
    var rejectLabel: String = ""
}

@InvokeArg
internal class CancelTransferRequestNotificationArgs {
    var batchId: String = ""
}

@InvokeArg
internal class CancelTaskNotificationArgs {
    var sessionId: String = ""
}

@TauriPlugin(
    permissions = [
        Permission(strings = [Manifest.permission.POST_NOTIFICATIONS], alias = "permissionState")
    ]
)
class TaskNotificationPlugin(private val activity: Activity) : Plugin(activity) {

    companion object {
        /** 与 @TauriPlugin 声明的权限 alias 保持一致 */
        private const val LOCAL_NOTIFICATIONS = "permissionState"
    }

    private val manager by lazy { TaskNotificationManager.getInstance(activity) }

    /**
     * 通知权限是否已授予（Android 13+ 需 POST_NOTIFICATIONS，且系统通知总开关开启）
     */
    private fun isPermissionGranted(): Boolean {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) return true
        return getPermissionState(LOCAL_NOTIFICATIONS) == PermissionState.GRANTED
            && NotificationManagerCompat.from(activity).areNotificationsEnabled()
    }

    /**
     * 检查通知权限
     */
    @Command
    fun checkNotificationPermission(invoke: Invoke) {
        val result = JSObject()
        result.put("granted", isPermissionGranted())
        invoke.resolve(result)
    }

    /**
     * 请求通知权限（未授权时弹系统授权框，结果异步返回）
     */
    @Command
    fun requestNotificationPermission(invoke: Invoke) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU
            || getPermissionState(LOCAL_NOTIFICATIONS) == PermissionState.GRANTED
        ) {
            val result = JSObject()
            result.put("granted", true)
            invoke.resolve(result)
            return
        }
        requestPermissionForAlias(LOCAL_NOTIFICATIONS, invoke, "permissionsCallback")
    }

    /**
     * 权限请求回调：系统授权框关闭后返回最新权限状态
     */
    @PermissionCallback
    fun permissionsCallback(invoke: Invoke) {
        val result = JSObject()
        result.put("granted", isPermissionGranted())
        invoke.resolve(result)
    }

    @Command
    fun showTaskNotification(invoke: Invoke) {
        val args = invoke.parseArgs(ShowTaskNotificationArgs::class.java)

        try {
            manager.showTaskNotification(
                sessionId = args.sessionId,
                title = args.title,
                body = args.body,
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
    fun showConnectionNotification(invoke: Invoke) {
        val args = invoke.parseArgs(ShowConnectionNotificationArgs::class.java)

        try {
            manager.showConnectionNotification(
                title = args.title,
                body = args.body,
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
    fun showPluginNotification(invoke: Invoke) {
        val args = invoke.parseArgs(ShowPluginNotificationArgs::class.java)

        try {
            manager.showPluginNotification(args.title, args.body)
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

    /**
     * 显示批量传输请求通知（v2 后台/锁屏应答，带接受全部/拒绝全部 action）
     *
     * 由宿主 Rust（file_service/notify.rs）在批 pending 且 App 后台时调用；
     * action 点击经 PendingIntent → MainActivity → 宿主命令路由回 Rust。
     */
    @Command
    fun showTransferRequestNotification(invoke: Invoke) {
        val args = invoke.parseArgs(ShowTransferRequestNotificationArgs::class.java)

        try {
            manager.showTransferRequestNotification(
                batchId = args.batchId,
                pluginId = args.pluginId,
                title = args.title,
                body = args.body,
                acceptLabel = args.acceptLabel,
                rejectLabel = args.rejectLabel
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

    /** 取消批量传输请求通知（批已解决后由宿主 Rust 调用） */
    @Command
    fun cancelTransferRequestNotification(invoke: Invoke) {
        val args = invoke.parseArgs(CancelTransferRequestNotificationArgs::class.java)

        try {
            manager.cancelTransferRequestNotification(args.batchId)
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
    fun cancelConnectionNotification(invoke: Invoke) {
        try {
            manager.cancelConnectionNotification()
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
