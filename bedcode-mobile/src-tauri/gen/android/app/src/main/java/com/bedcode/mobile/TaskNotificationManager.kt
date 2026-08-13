package com.bedcode.mobile

import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.os.Build
import androidx.core.app.NotificationCompat

/**
 * 任务/连接通知管理器
 *
 * 管理每个会话的独立通知卡片，支持更新和取消。
 * 与 ForegroundService 的通知（ID=1001）独立，使用不同渠道和 ID 范围。
 *
 * 声音/震动必须在渠道级别分开控制（Android 8+ 通知级 `setSound(null)` 会回退到渠道默认声音，
 * 无法静音单条通知），因此按开关组合使用三个渠道：
 * - [CHANNEL_ID]：IMPORTANCE_HIGH，声音 + 震动（含 heads-up 弹出）
 * - [CHANNEL_ID_VIBRATE]：IMPORTANCE_HIGH 但渠道无声，仅震动
 * - [CHANNEL_ID_SILENT]：IMPORTANCE_LOW，完全静默
 *
 * "仅响铃不震动"组合用有声渠道 + 通知级空震动 pattern 覆盖渠道默认震动。
 */
class TaskNotificationManager(private val context: Context) {

    companion object {
        /** 声音 + 震动渠道 */
        const val CHANNEL_ID = "bedcode_task"
        const val CHANNEL_NAME = "任务状态通知"
        /** 仅震动渠道（HIGH 重要性保证 heads-up，渠道配置无声） */
        const val CHANNEL_ID_VIBRATE = "bedcode_task_vibrate"
        const val CHANNEL_NAME_VIBRATE = "任务状态通知（仅震动）"
        /** 静默渠道 */
        const val CHANNEL_ID_SILENT = "bedcode_task_silent"
        const val CHANNEL_NAME_SILENT = "任务状态通知（静默）"

        const val NOTIFICATION_ID_BASE = 2000
        const val NOTIFICATION_ID_RANGE = 1000
        /** 批量传输请求通知 ID 基数（v2；action 应答后/批解决后取消） */
        const val NOTIFICATION_ID_BATCH_BASE = 4000
        const val NOTIFICATION_ID_BATCH_RANGE = 500
        /** 连接状态通知固定 ID */
        const val CONNECTION_NOTIFICATION_ID = 3001
        /** 插件自主通知固定 ID（host_notify，不与会话绑定） */
        const val PLUGIN_NOTIFICATION_ID = 3002

        /** 批量请求通知 action 的 Intent action（MainActivity 路由识别） */
        const val ACTION_TRANSFER_BATCH = "com.bedcode.mobile.action.TRANSFER_BATCH"
        /** Intent extra：应答动作（approve / reject） */
        const val EXTRA_BATCH_ACTION = "batch_action"
        /** Intent extra：批 ID */
        const val EXTRA_BATCH_ID = "batch_id"
        /** Intent extra：请求方插件 ID（路由回插件命令用） */
        const val EXTRA_BATCH_PLUGIN_ID = "batch_plugin_id"
        /** 应答动作取值：接受全部 */
        const val BATCH_ACTION_APPROVE = "approve"
        /** 应答动作取值：拒绝全部 */
        const val BATCH_ACTION_REJECT = "reject"

        @Volatile
        private var instance: TaskNotificationManager? = null

        fun getInstance(context: Context): TaskNotificationManager {
            return instance ?: synchronized(this) {
                instance ?: TaskNotificationManager(context.applicationContext).also { instance = it }
            }
        }
    }

    private val notificationManager =
        context.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager

    init {
        createChannels()
    }

    /**
     * 创建通知渠道
     *
     * 已存在则跳过：渠道创建后用户可在系统设置中调整声音/震动，应用不应覆盖用户选择
     */
    private fun createChannels() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return

        if (notificationManager.getNotificationChannel(CHANNEL_ID) == null) {
            val channel = NotificationChannel(
                CHANNEL_ID,
                CHANNEL_NAME,
                NotificationManager.IMPORTANCE_HIGH
            ).apply {
                description = "任务状态变更提醒（声音+震动）"
                enableVibration(true)
                enableLights(true)
                setShowBadge(true)
            }
            notificationManager.createNotificationChannel(channel)
        }

        if (notificationManager.getNotificationChannel(CHANNEL_ID_VIBRATE) == null) {
            val channel = NotificationChannel(
                CHANNEL_ID_VIBRATE,
                CHANNEL_NAME_VIBRATE,
                NotificationManager.IMPORTANCE_HIGH
            ).apply {
                description = "任务状态变更提醒（仅震动，无声音）"
                // 渠道级静音：声音置 null 后渠道不播放提示音，震动由渠道默认 pattern 提供
                setSound(null, null)
                enableVibration(true)
                enableLights(true)
                setShowBadge(true)
            }
            notificationManager.createNotificationChannel(channel)
        }

        if (notificationManager.getNotificationChannel(CHANNEL_ID_SILENT) == null) {
            val channel = NotificationChannel(
                CHANNEL_ID_SILENT,
                CHANNEL_NAME_SILENT,
                NotificationManager.IMPORTANCE_LOW
            ).apply {
                description = "后台静默通知（无声音无震动）"
                enableVibration(false)
                setShowBadge(false)
            }
            notificationManager.createNotificationChannel(channel)
        }
    }

    /**
     * 根据 sessionId 生成唯一 notification ID
     *
     * 基数 2000 避免与前台服务通知 ID（1001）冲突
     */
    fun getNotificationId(sessionId: String): Int {
        return NOTIFICATION_ID_BASE + Math.abs(sessionId.hashCode()) % NOTIFICATION_ID_RANGE
    }

    /**
     * 显示或更新会话任务通知
     *
     * @param sessionId 会话 ID（用于生成 notification ID）
     * @param title 通知标题（会话名）
     * @param body 通知正文（状态文本，由前端 i18n 构建）
     * @param vibrate 是否震动
     * @param sound 是否播放提示音
     */
    fun showTaskNotification(
        sessionId: String,
        title: String,
        body: String,
        vibrate: Boolean,
        sound: Boolean
    ) {
        notify(getNotificationId(sessionId), title, body, vibrate, sound)
    }

    /**
     * 显示连接状态通知（固定 ID [CONNECTION_NOTIFICATION_ID]，新通知覆盖旧通知）
     */
    fun showConnectionNotification(
        title: String,
        body: String,
        vibrate: Boolean,
        sound: Boolean
    ) {
        notify(CONNECTION_NOTIFICATION_ID, title, body, vibrate, sound)
    }

    /**
     * 显示插件自主通知（host_notify）
     *
     * 插件系统发起的通用通知，不与会话绑定，固定 ID [PLUGIN_NOTIFICATION_ID]，
     * 使用默认渠道（声音+震动），不参与设置页开关控制
     */
    fun showPluginNotification(title: String, body: String) {
        notify(PLUGIN_NOTIFICATION_ID, title, body, vibrate = true, sound = true)
    }

    /**
     * 显示批量传输请求通知（v2 后台/锁屏应答）
     *
     * 带「接受全部 / 拒绝全部」两个 action 按钮；点击经 PendingIntent 路由到
     * MainActivity，由 WebView evaluateJavascript 调宿主命令
     * plugin_filesrv_approve_transfer / plugin_filesrv_reject_transfer。
     * 通知 ID 按 batchId 稳定映射，批解决后按同 ID 取消。
     *
     * @param batchId 批 ID（通知 ID 映射 + Intent extra）
     * @param pluginId 请求方插件 ID（命令路由用）
     * @param title 通知标题（宿主按语言偏好构建）
     * @param body 通知正文（宿主构建）
     * @param acceptLabel 「接受全部」按钮文案
     * @param rejectLabel 「拒绝全部」按钮文案
     */
    fun showTransferRequestNotification(
        batchId: String,
        pluginId: String,
        title: String,
        body: String,
        acceptLabel: String,
        rejectLabel: String
    ) {
        val id = getBatchNotificationId(batchId)
        val builder = NotificationCompat.Builder(context, CHANNEL_ID)
            .setContentTitle(title)
            .setContentText(body)
            .setSmallIcon(R.drawable.ic_notification)
            .setOngoing(false)
            .setAutoCancel(false)
            .setOnlyAlertOnce(true)
            .setCategory(NotificationCompat.CATEGORY_STATUS)
            .setStyle(NotificationCompat.BigTextStyle().bigText(body))
            .setPriority(NotificationCompat.PRIORITY_HIGH)

        // 默认点击 = 打开应用（保留）；action 点击 = 应答路由
        val openIntent = context.packageManager.getLaunchIntentForPackage(context.packageName)
        openIntent?.let {
            it.flags = Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP
            builder.setContentIntent(
                PendingIntent.getActivity(
                    context,
                    id,
                    it,
                    PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
                )
            )
        }

        // 接受全部：点击 → MainActivity → approve 命令（requestCode 区分 action，防 PendingIntent 复用串线）
        builder.addAction(
            0,
            acceptLabel,
            actionPendingIntent(batchId, pluginId, BATCH_ACTION_APPROVE, id)
        )
        // 拒绝全部
        builder.addAction(
            0,
            rejectLabel,
            actionPendingIntent(batchId, pluginId, BATCH_ACTION_REJECT, id)
        )

        notificationManager.notify(id, builder.build())
    }

    /** 构造 action 点击 PendingIntent（Activity 路由，requestCode 含 action 区分） */
    private fun actionPendingIntent(
        batchId: String,
        pluginId: String,
        action: String,
        baseCode: Int
    ): PendingIntent {
        val intent = Intent(context, MainActivity::class.java).apply {
            // this.action 显式限定：参数名 action 遮蔽了 Intent.action 属性
            this.action = ACTION_TRANSFER_BATCH
            flags = Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP
            putExtra(EXTRA_BATCH_ACTION, action)
            putExtra(EXTRA_BATCH_ID, batchId)
            putExtra(EXTRA_BATCH_PLUGIN_ID, pluginId)
        }
        // requestCode 掺入 action 与批 ID 哈希：同批 approve/reject 两个 PendingIntent 互不覆盖
        val requestCode = baseCode * 2 + if (action == BATCH_ACTION_APPROVE) 0 else 1
        return PendingIntent.getActivity(
            context,
            requestCode,
            intent,
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
        )
    }

    /** 取消批量传输请求通知（批已解决 / action 已应答后） */
    fun cancelTransferRequestNotification(batchId: String) {
        notificationManager.cancel(getBatchNotificationId(batchId))
    }

    /** 批量请求通知 ID（基数 4000，与任务/连接通知隔离）
     *
     * 用 `and 0x7fffffff` 代替 Math.abs：hashCode() == Int.MIN_VALUE 时 abs 仍为负，
     * 会产出负数通知 ID（NotificationManager 行为未定义）。仍存在哈希碰撞覆盖的
     * 低概率（同批并发两通知），接受（批量请求通知生命周期短，cancel 幂等兜底）。 */
    fun getBatchNotificationId(batchId: String): Int {
        return NOTIFICATION_ID_BATCH_BASE +
            (batchId.hashCode() and 0x7fffffff) % NOTIFICATION_ID_BATCH_RANGE
    }

    /**
     * 构建并发送通知，震动/声音按开关分别应用
     */
    private fun notify(id: Int, title: String, body: String, vibrate: Boolean, sound: Boolean) {
        // 按开关组合选择渠道：全静默用 LOW 渠道；仅震动用无声渠道；其余用有声渠道
        val channelId = when {
            !vibrate && !sound -> CHANNEL_ID_SILENT
            vibrate && !sound -> CHANNEL_ID_VIBRATE
            else -> CHANNEL_ID
        }

        val builder = NotificationCompat.Builder(context, channelId)
            .setContentTitle(title)
            .setContentText(body)
            .setSmallIcon(R.drawable.ic_notification)
            .setOngoing(false)
            .setAutoCancel(false)
            // 提醒通知每次状态更新都触发提示；静默通知避免无谓提醒
            .setOnlyAlertOnce(!vibrate && !sound)
            .setCategory(NotificationCompat.CATEGORY_STATUS)
            .setStyle(NotificationCompat.BigTextStyle().bigText(body))
            .setPriority(
                if (vibrate || sound) NotificationCompat.PRIORITY_HIGH
                else NotificationCompat.PRIORITY_LOW
            )

        // 仅响铃不震动：空震动 pattern 覆盖渠道默认震动（null pattern 会回退渠道设置）
        if (!vibrate && sound) {
            builder.setVibrate(longArrayOf())
        }

        // 点击通知跳转到应用
        val intent = context.packageManager.getLaunchIntentForPackage(context.packageName)
        intent?.let {
            it.flags = Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP
            val pendingIntent = PendingIntent.getActivity(
                context,
                id,
                it,
                PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
            )
            builder.setContentIntent(pendingIntent)
        }

        notificationManager.notify(id, builder.build())
    }

    /**
     * 取消指定会话的通知
     */
    fun cancelTaskNotification(sessionId: String) {
        notificationManager.cancel(getNotificationId(sessionId))
    }

    /**
     * 取消连接状态通知
     */
    fun cancelConnectionNotification() {
        notificationManager.cancel(CONNECTION_NOTIFICATION_ID)
    }

    /**
     * 取消所有任务/连接通知
     *
     * 遍历可能存在的 ID 范围进行取消，确保无遗漏
     */
    fun cancelAllTaskNotifications() {
        for (i in NOTIFICATION_ID_BASE until NOTIFICATION_ID_BASE + NOTIFICATION_ID_RANGE) {
            notificationManager.cancel(i)
        }
        notificationManager.cancel(CONNECTION_NOTIFICATION_ID)
    }
}
