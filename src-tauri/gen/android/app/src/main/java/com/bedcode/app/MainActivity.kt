package com.bedcode.app

import android.os.Bundle
import android.webkit.WebView
import androidx.activity.enableEdgeToEdge

class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    // 启用 WebView 远程调试（仅 debug 构建）
    WebView.setWebContentsDebuggingEnabled(true)

    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
  }
}
