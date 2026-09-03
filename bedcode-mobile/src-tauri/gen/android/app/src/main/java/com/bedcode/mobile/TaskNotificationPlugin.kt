package com.bedcode.mobile

import android.Manifest
import android.app.Activity
import android.media.AudioAttributes
import android.media.Ringtone
import android.media.RingtoneManager
import android.os.Build
import android.os.VibrationEffect
import android.os.Vibrator
import android.os.VibratorManager
import android.content.Context
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

@InvokeArg
internal class ShowIntentAskNotificationArgs {
    var intentId: String = ""
    var title: String = ""
    var body: String = ""
    var acceptLabel: String = ""
    var rejectLabel: String = ""
}

@InvokeArg
internal class ShowPullNoticeArgs {
    var intentId: String = ""
    var title: String = ""
    var body: String = ""
}

@InvokeArg
internal class CancelIntentNotificationArgs {
    var intentId: String = ""
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
        /** 设置页预览震动的时长（毫秒），与通知渠道默认震动体感接近 */
        private const val PREVIEW_VIBRATE_MS = 300L
    }

    /** 当前预览提示音实例：重复触发时先停掉上一次，避免叠音 */
    private var previewRingtone: Ringtone? = null

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

    /**
     * 预览震动一次（设置页开启「震动反馈」时触发）
     *
     * 直接走 Vibrator 服务，不经过通知渠道，保证无论通知权限/渠道如何配置都能给出反馈
     */
    @Command
    fun testVibrate(invoke: Invoke) {
        try {
            val vibrator = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
                val manager = activity.getSystemService(Context.VIBRATOR_MANAGER_SERVICE) as VibratorManager
                manager.defaultVibrator
            } else {
                @Suppress("DEPRECATION")
                activity.getSystemService(Context.VIBRATOR_SERVICE) as Vibrator
            }
            vibrator.vibrate(
                VibrationEffect.createOneShot(PREVIEW_VIBRATE_MS, VibrationEffect.DEFAULT_AMPLITUDE)
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

    /**
     * 预览提示音一次（设置页开启「任务完成提示音」时触发）
     *
     * 播放系统默认通知音（USAGE_NOTIFICATION 流），与实际通知提示音同源；
     * 重复触发先停掉上一次播放，避免叠音
     */
    @Command
    fun testSound(invoke: Invoke) {
        try {
            previewRingtone?.stop()
            previewRingtone = null

            val uri = RingtoneManager.getDefaultUri(RingtoneManager.TYPE_NOTIFICATION)
            val attributes = AudioAttributes.Builder()
                .setUsage(AudioAttributes.USAGE_NOTIFICATION)
                .setContentType(AudioAttributes.CONTENT_TYPE_SONIFICATION)
                .build()
            val ringtone = RingtoneManager.getRingtone(activity, uri)
            ringtone.audioAttributes = attributes
            ringtone.play()
            previewRingtone = ringtone

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

    /**
     * 显示 push 审批通知（v2.1 后台/锁屏，带接受/拒绝 action）
     *
     * 由宿主 Rust（file_service/notify.rs）在 intent push 且 App 后台时调用；
     * action 点击经 PendingIntent(kind=intent) → MainActivity →
     * `plugin_filesrv_respond_intent`（accepted → 手机执行下载）。
     */
    @Command
    fun showIntentAskNotification(invoke: Invoke) {
        val args = invoke.parseArgs(ShowIntentAskNotificationArgs::class.java)

        try {
            manager.showIntentAskNotification(
                intentId = args.intentId,
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

    /** 显示 pull 信息性通知（v2.1 桌面拉取本机文件，无 action 按钮） */
    @Command
    fun showPullNotice(invoke: Invoke) {
        val args = invoke.parseArgs(ShowPullNoticeArgs::class.java)

        try {
            manager.showPullNotice(args.intentId, args.title, args.body)
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

    /** 取消 intent 审批通知（应答后由宿主 Rust 调用） */
    @Command
    fun cancelIntentNotification(invoke: Invoke) {
        val args = invoke.parseArgs(CancelIntentNotificationArgs::class.java)

        try {
            manager.cancelIntentNotification(args.intentId)
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
