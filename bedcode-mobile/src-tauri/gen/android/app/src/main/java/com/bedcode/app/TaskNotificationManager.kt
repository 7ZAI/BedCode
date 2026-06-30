package com.bedcode.app

import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.os.Build
import androidx.core.app.NotificationCompat

/**
 * 任务通知管理器
 *
 * 管理每个会话的独立通知卡片，支持更新和取消
 * 与 ForegroundService 的通知（ID=1001）独立，使用不同渠道和 ID 范围
 */
class TaskNotificationManager(private val context: Context) {

    companion object {
        const val CHANNEL_ID = "bedcode_task"
        const val CHANNEL_NAME = "任务状态通知"
        const val NOTIFICATION_ID_BASE = 2000
        const val NOTIFICATION_ID_RANGE = 1000

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
        createChannel()
    }

    /**
     * 创建 HIGH importance 通知渠道
     *
     * Android 8.0+ 必须创建渠道才能发通知
     * HIGH importance: 弹出 heads-up、声音、震动
     */
    private fun createChannel() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            val existing = notificationManager.getNotificationChannel(CHANNEL_ID)
            if (existing != null) return

            val channel = NotificationChannel(
                CHANNEL_ID,
                CHANNEL_NAME,
                NotificationManager.IMPORTANCE_HIGH
            ).apply {
                description = "任务状态变更提醒"
                enableVibration(true)
                enableLights(true)
                setShowBadge(true)
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
     * 显示或更新任务通知
     *
     * @param sessionId 会话 ID（用于生成 notification ID）
     * @param sessionName 会话名称（通知标题）
     * @param status 任务状态
     * @param reason 状态原因（可选）
     * @param vibrate 是否震动
     * @param sound 是否声音
     */
    fun showTaskNotification(
        sessionId: String,
        sessionName: String,
        status: String,
        reason: String?,
        vibrate: Boolean,
        sound: Boolean
    ) {
        val notificationId = getNotificationId(sessionId)
        val contentText = buildContentText(status, reason)

        val builder = NotificationCompat.Builder(context, CHANNEL_ID)
            .setContentTitle(sessionName)
            .setContentText(contentText)
            .setSmallIcon(R.drawable.ic_notification)
            .setOngoing(false)
            .setAutoCancel(false)
            // 非提醒时 setOnlyAlertOnce 避免重复提醒音
            .setOnlyAlertOnce(!vibrate && !sound)
            .setCategory(NotificationCompat.CATEGORY_STATUS)
            .setStyle(NotificationCompat.BigTextStyle().bigText(contentText))

        // 震动/声音控制：非提醒时静默
        if (!vibrate && !sound) {
            builder.setPriority(NotificationCompat.PRIORITY_LOW)
            builder.setSilent(true)
        } else {
            builder.setPriority(NotificationCompat.PRIORITY_HIGH)
        }

        // 点击通知跳转到应用
        val intent = context.packageManager.getLaunchIntentForPackage(context.packageName)
        intent?.let {
            it.flags = Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP
            val pendingIntent = PendingIntent.getActivity(
                context,
                notificationId,
                it,
                PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
            )
            builder.setContentIntent(pendingIntent)
        }

        notificationManager.notify(notificationId, builder.build())
    }

    /**
     * 取消指定会话的通知
     */
    fun cancelTaskNotification(sessionId: String) {
        notificationManager.cancel(getNotificationId(sessionId))
    }

    /**
     * 取消所有任务通知
     *
     * 遍历可能存在的 ID 范围进行取消，确保无遗漏
     */
    fun cancelAllTaskNotifications() {
        for (i in NOTIFICATION_ID_BASE until NOTIFICATION_ID_BASE + NOTIFICATION_ID_RANGE) {
            notificationManager.cancel(i)
        }
    }

    /**
     * 构建通知内容文本
     */
    private fun buildContentText(status: String, reason: String?): String {
        return when (status) {
            "idle" -> "空闲"
            "in_progress" -> "执行中..."
            "asking" -> "等待输入"
            "completed" -> "任务完成"
            "interrupted" -> if (reason != null) "任务中断: $reason" else "任务中断"
            else -> status
        }
    }
}
