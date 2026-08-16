package com.bedcode.app

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
