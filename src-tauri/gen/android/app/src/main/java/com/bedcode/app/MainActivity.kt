package com.bedcode.app

import android.os.Bundle
import android.webkit.WebView
import androidx.activity.enableEdgeToEdge

class MainActivity : TauriActivity() {
    companion object {
        private const val TAG = "BedCode-MainActivity"
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        // 启用 WebView 远程调试（仅 debug 构建）
        if (BuildConfig.DEBUG) {
            WebView.setWebContentsDebuggingEnabled(true)
        }

        android.util.Log.d(TAG, "onCreate: enabling edge-to-edge mode")
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        android.util.Log.d(TAG, "onCreate: done")
    }
}
