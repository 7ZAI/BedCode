package com.bedcode.mobile

import android.app.Activity
import androidx.core.view.WindowCompat
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin

/**
 * Tauri 插件 - 系统状态栏/导航栏图标外观同步
 *
 * 为什么需要：App 启用 edge-to-edge 后（MainActivity.enableEdgeToEdge），
 * 内容绘制在透明系统栏之后，系统栏图标（时间/日期/电量等）由系统按
 * 「设备系统」深浅色用 SystemBarStyle.auto() 决定外观。而 BedCode 有
 * 自己独立的主题设置（light/dark/system，存于 settings.json，前端
 * html.dark 类切换），两者不一致时——设备浅色 + App 夜间模式——深色
 * 背景上仍是深色图标，状态栏时间日期完全不可读。
 *
 * 本插件把前端主题变化经 Rust 桥（android_plugins/status_bar_style.rs）
 * 同步到系统栏：App 深色 → 浅色图标（isAppearanceLight*=false）；
 * App 浅色 → 深色图标（isAppearanceLight*=true）。
 *
 * 由 Rust 端 android_plugins.rs 注册（Builder 名 "status-bar-style"）。
 * gen/android 重建恢复清单：本文件须恢复（参见 AGENTS.md「Android」节）。
 */
@InvokeArg
internal class SetStatusBarStyleArgs {
    var dark: Boolean = false
}

@TauriPlugin
class StatusBarStylePlugin(private val activity: Activity) : Plugin(activity) {

    companion object {
        private const val TAG = "BedCode-StatusBarStyle"
    }

    /**
     * 同步系统栏图标外观（幂等：重复调用仅重设相同值）
     *
     * @param dark App 当前是否为深色主题
     */
    @Command
    fun setStyle(invoke: Invoke) {
        val args = invoke.parseArgs(SetStatusBarStyleArgs::class.java)
        try {
            val controller =
                WindowCompat.getInsetsController(activity.window, activity.window.decorView)
            // 深色背景配浅色图标；浅色背景配深色图标
            controller.isAppearanceLightStatusBars = !args.dark
            controller.isAppearanceLightNavigationBars = !args.dark
            android.util.Log.i(TAG, "system bar appearance synced: dark=${args.dark}")
            invoke.resolve(JSObject().apply { put("ok", true) })
        } catch (e: Exception) {
            android.util.Log.e(TAG, "failed to sync system bar appearance: ${e.message}")
            invoke.reject("Failed to sync system bar appearance: ${e.message}")
        }
    }
}