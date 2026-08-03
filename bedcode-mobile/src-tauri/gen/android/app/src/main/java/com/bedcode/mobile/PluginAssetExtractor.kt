package com.bedcode.mobile

import android.app.Activity
import android.util.Log
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.plugin.JSObject
import app.tauri.plugin.Invoke
import app.tauri.plugin.Plugin
import java.io.File
import java.io.FileOutputStream

/**
 * Tauri 插件 - 内置插件资源解压
 *
 * 将 APK assets/resources/plugins/ ** 解压到 filesDir/plugins/{plugin_id}/，
 * 写入 .bedcode-source 标记（内容 "apk-asset:{version}"，用于升级后按版本刷新）。
 * 由 Rust 端 android_plugins::init() 注册，启动时经 run_mobile_plugin 调用。
 */
@InvokeArg
internal class ExtractPluginsArgs {
    var appVersion: String = ""
}

class PluginAssetExtractor(private val activity: Activity) : Plugin(activity) {

    @Command
    fun extractBundledPlugins(invoke: Invoke) {
        val args = invoke.parseArgs(ExtractPluginsArgs::class.java)
        try {
            val count = extractAll(args.appVersion)
            val result = JSObject()
            result.put("success", true)
            result.put("count", count)
            invoke.resolve(result)
        } catch (e: Exception) {
            Log.e(TAG, "extractBundledPlugins failed", e)
            val result = JSObject()
            result.put("success", false)
            result.put("error", e.message ?: "Unknown error")
            invoke.resolve(result)
        }
    }

    /** 解压 assets/resources/plugins 下所有插件目录 */
    private fun extractAll(appVersion: String): Int {
        val pluginsRoot = "resources/plugins"
        val pluginIds = activity.assets.list(pluginsRoot) ?: return 0

        var extracted = 0
        for (id in pluginIds) {
            if (id.startsWith(".")) continue
            val assetDir = "$pluginsRoot/$id"
            val destDir = File(activity.filesDir, "plugins/$id")

            // 已解压且版本一致 → 跳过（应用升级换新内置插件时重新解压）
            val marker = File(destDir, MARKER_FILE)
            if (marker.exists() && marker.readText().trim() == "$SOURCE_APK_ASSET:$appVersion") continue

            if (destDir.exists()) destDir.deleteRecursively()
            if (copyAssetDir(assetDir, destDir)) {
                marker.parentFile?.mkdirs()
                marker.writeText("$SOURCE_APK_ASSET:$appVersion")
                extracted++
                Log.i(TAG, "Extracted bundled plugin: $id")
            }
        }
        return extracted
    }

    /** 递归复制 assets 目录到目标目录 */
    private fun copyAssetDir(assetDir: String, destDir: File): Boolean {
        val entries = activity.assets.list(assetDir) ?: return false
        if (!destDir.exists() && !destDir.mkdirs()) return false

        for (entry in entries) {
            val assetPath = "$assetDir/$entry"
            val destFile = File(destDir, entry)
            if (activity.assets.list(assetPath) != null) {
                // 子目录
                if (!copyAssetDir(assetPath, destFile)) return false
            } else {
                activity.assets.open(assetPath).use { input ->
                    FileOutputStream(destFile).use { output -> input.copyTo(output) }
                }
            }
        }
        return true
    }

    companion object {
        private const val TAG = "PluginAssetExtractor"
        const val MARKER_FILE = ".bedcode-source"
        const val SOURCE_APK_ASSET = "apk-asset"
    }
}
