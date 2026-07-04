package com.bedcode.mobile

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.Service
import android.content.Context
import android.content.Intent
import android.os.Build
import android.os.IBinder
import android.os.PowerManager
import androidx.core.app.NotificationCompat

/**
 * Android 前台服务
 *
 * 用于在后台保持 WebSocket 连接，防止系统杀死进程
 * 同时持有 PARTIAL_WAKE_LOCK 防止 CPU 休眠导致心跳中断
 */
class ForegroundService : Service() {
    companion object {
        private const val NOTIFICATION_ID = 1001
        private const val CHANNEL_ID = "bedcode_foreground"

        // WakeLock 引用，静态持有以确保跨 Service 实例复用
        private var wakeLock: PowerManager.WakeLock? = null

        /**
         * 启动前台服务
         */
        fun start(context: Context, title: String, content: String) {
            val intent = Intent(context, ForegroundService::class.java).apply {
                action = "START"
                putExtra("title", title)
                putExtra("content", content)
            }
            context.startForegroundService(intent)
        }

        /**
         * 停止前台服务
         */
        fun stop(context: Context) {
            context.stopService(Intent(context, ForegroundService::class.java))
        }

        /**
         * 更新通知内容
         */
        fun updateNotification(context: Context, title: String, content: String) {
            val intent = Intent(context, ForegroundService::class.java).apply {
                action = "UPDATE"
                putExtra("title", title)
                putExtra("content", content)
            }
            context.startService(intent)
        }
    }

    override fun onCreate() {
        super.onCreate()
        createNotificationChannel()
        acquireWakeLock()
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        when (intent?.action) {
            "START" -> {
                val title = intent.getStringExtra("title") ?: "BedCode"
                val content = intent.getStringExtra("content") ?: "后台运行中"
                startForeground(NOTIFICATION_ID, createNotification(title, content))
            }
            "UPDATE" -> {
                val title = intent.getStringExtra("title") ?: "BedCode"
                val content = intent.getStringExtra("content") ?: "后台运行中"
                updateNotificationInternal(title, content)
            }
        }
        return START_STICKY
    }

    override fun onDestroy() {
        releaseWakeLock()
        super.onDestroy()
    }

    override fun onBind(intent: Intent?): IBinder? = null

    /**
     * 获取 PARTIAL_WAKE_LOCK
     *
     * 保持 CPU 运行但允许屏幕关闭，确保 WebSocket 心跳和 IO 在息屏后继续工作
     */
    private fun acquireWakeLock() {
        if (wakeLock?.isHeld == true) return

        val powerManager = getSystemService(Context.POWER_SERVICE) as PowerManager
        wakeLock = powerManager.newWakeLock(
            PowerManager.PARTIAL_WAKE_LOCK,
            "BedCode:WebSocketKeepAlive"
        ).apply {
            acquire()
        }
        android.util.Log.d("ForegroundService", "WakeLock acquired")
    }

    /**
     * 释放 WakeLock
     */
    private fun releaseWakeLock() {
        wakeLock?.let {
            if (it.isHeld) {
                it.release()
                android.util.Log.d("ForegroundService", "WakeLock released")
            }
        }
        wakeLock = null
    }

    /**
     * 创建通知渠道 (Android 8.0+)
     */
    private fun createNotificationChannel() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            val channel = NotificationChannel(
                CHANNEL_ID,
                "BedCode 后台服务",
                NotificationManager.IMPORTANCE_LOW
            ).apply {
                description = "保持 WebSocket 连接"
                setShowBadge(false)
            }
            val manager = getSystemService(NotificationManager::class.java)
            manager.createNotificationChannel(channel)
        }
    }

    /**
     * 创建通知
     */
    private fun createNotification(title: String, content: String): Notification {
        return NotificationCompat.Builder(this, CHANNEL_ID)
            .setContentTitle(title)
            .setContentText(content)
            .setSmallIcon(R.drawable.ic_notification)
            .setOngoing(true)
            .setPriority(NotificationCompat.PRIORITY_LOW)
            .setCategory(NotificationCompat.CATEGORY_SERVICE)
            .build()
    }

    /**
     * 更新通知内容
     */
    private fun updateNotificationInternal(title: String, content: String) {
        val manager = getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
        manager.notify(NOTIFICATION_ID, createNotification(title, content))
    }
}
