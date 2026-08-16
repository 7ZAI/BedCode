package com.bedcode.mobile

import android.content.Intent
import android.os.Bundle
import android.webkit.WebView
import androidx.activity.enableEdgeToEdge

class MainActivity : TauriActivity() {
    companion object {
        private const val TAG = "BedCode-MainActivity"
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        if (BuildConfig.DEBUG) {
            WebView.setWebContentsDebuggingEnabled(true)
        }

        // @TauriPlugin 注解的插件由 Tauri 构建系统自动注册，无需手动调用 PluginManager.register
        android.util.Log.d(TAG, "onCreate: enabling edge-to-edge mode")
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        android.util.Log.d(TAG, "onCreate: done")

        // 冷启动直达 action（通知 action 点击时进程已死）：处理意图后继续正常启动
        routeTransferBatchAction(intent)
    }

    // ==================== v2 批量传输请求通知 action 路由 ====================
    //
    // 通知「接受全部 / 拒绝全部」action 点击 → PendingIntent（本 Activity，
    // singleTop）→ 此处经 WebView evaluateJavascript 调宿主 Tauri 命令
    // plugin_filesrv_approve_transfer / plugin_filesrv_reject_transfer，
    // Rust 侧完成批状态迁移 + resolved 事件 + 跨端推送；同时取消该通知。

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        routeTransferBatchAction(intent)
    }

    /** 解析批量请求 action 意图并路由（非本 action 静默忽略） */
    private fun routeTransferBatchAction(intent: Intent?) {
        if (intent?.action != TaskNotificationManager.ACTION_TRANSFER_BATCH) return
        val action = intent.getStringExtra(TaskNotificationManager.EXTRA_BATCH_ACTION)
        val batchId = intent.getStringExtra(TaskNotificationManager.EXTRA_BATCH_ID)
        val pluginId = intent.getStringExtra(TaskNotificationManager.EXTRA_BATCH_PLUGIN_ID)
        if (action == null || batchId.isNullOrEmpty() || pluginId.isNullOrEmpty()) {
            android.util.Log.w(TAG, "transfer batch action intent missing extras")
            return
        }
        val command = when (action) {
            TaskNotificationManager.BATCH_ACTION_APPROVE -> "plugin_filesrv_approve_transfer"
            TaskNotificationManager.BATCH_ACTION_REJECT -> "plugin_filesrv_reject_transfer"
            else -> {
                android.util.Log.w(TAG, "unknown transfer batch action: $action")
                return
            }
        }
        android.util.Log.i(TAG, "routing transfer batch action: $command batch=$batchId")
        // 取消通知（应答已处理，批解决后的 cancel 为幂等兜底）
        TaskNotificationManager.getInstance(this).cancelTransferRequestNotification(batchId)

        // 路由回宿主命令：__TAURI_INTERNALS__.invoke 为 Tauri v2 Android WebView 全局桥
        val js = "window.__TAURI_INTERNALS__.invoke(" +
            "'$command', { pluginId: '" + jsEscape(pluginId) + "', batchId: '" + jsEscape(batchId) + "' })"
        runOnUiThread {
            try {
                val field = findDeclaredField(javaClass, "mWebView") ?: return@runOnUiThread
                field.isAccessible = true
                val webView = field.get(this) as? WebView ?: return@runOnUiThread
                webView.evaluateJavascript(js, null)
            } catch (e: Exception) {
                android.util.Log.w(TAG, "failed to route transfer batch action: ${e.message}")
            }
        }
    }

    /** JS 字符串转义（batchId/pluginId 进入单引号字符串）
     *
     * batchId 为对端可控输入：除反斜杠/单引号外，换行（\n/\r/\u2028/\u2029）
     * 未转义会导致 evaluateJavascript 拼接语法错误 → action 静默丢失。补全转义。 */
    private fun jsEscape(s: String): String {
        return s.replace("\\", "\\\\")
            .replace("'", "\\'")
            .replace("\n", "\\n")
            .replace("\r", "\\r")
            .replace("\u2028", "\\u2028")
            .replace("\u2029", "\\u2029")
    }

    // ==================== 后台保活：阻止 WebView 暂停 JS ====================
    //
    // WryActivity.onPause() 调用 mWebView.onPause()，暂停 WebView 的 JS 执行。
    // 切换应用时导致前端心跳、事件监听、重连逻辑全部冻结，WebSocket 连接中断。
    //
    // 策略：正常调用 super.onPause() 保持 Activity 生命周期完整，
    // 但在 super.onPause() 后立即调用 mWebView.onResume() 恢复 JS 执行。
    // 前台服务 + WakeLock 确保 CPU 不休眠，WebSocket IO 不中断。

    override fun onPause() {
        super.onPause()
        // WryActivity.onPause() 已暂停 WebView，立即恢复以保持 JS 运行
        resumeWebView()
        android.util.Log.d(TAG, "onPause: re-resumed WebView for background keep-alive")
    }

    override fun onResume() {
        super.onResume()
        // WebView 已被 WryActivity.onResume() 恢复，无需额外操作
    }

    /**
     * 通过反射获取 WryActivity 的 mWebView 并调用 onResume()
     *
     * WryActivity.mWebView 是 private lateinit，无法直接访问。
     * 反射是此场景下唯一的方式，因为 WryActivity 是 Tauri 自动生成的代码不能修改。
     */
    private fun resumeWebView() {
        try {
            // WryActivity 声明了 private lateinit var mWebView: RustWebView
            val field = findDeclaredField(javaClass, "mWebView") ?: return
            field.isAccessible = true
            val webView = field.get(this) as? WebView ?: return
            webView.onResume()
        } catch (e: Exception) {
            android.util.Log.w(TAG, "Failed to resume WebView via reflection: ${e.message}")
        }
    }

    /**
     * 递归查找类层次中声明的字段（包括父类 private 字段）
     */
    private fun findDeclaredField(clazz: Class<*>, name: String): java.lang.reflect.Field? {
        var current = clazz
        while (current != Any::class.java) {
            try {
                return current.getDeclaredField(name)
            } catch (_: NoSuchFieldException) {
                current = current.superclass ?: break
            }
        }
        return null
    }
}
