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
}
