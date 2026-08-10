package com.bedcode.mobile

import android.app.Activity
import android.util.Log
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.JSObject
import app.tauri.plugin.Invoke
import app.tauri.plugin.Plugin
import java.io.File
import java.io.FileOutputStream
import java.io.IOException

/**
 * Tauri 插件 - 内置插件资源解压
 *
 * 将 APK assets/resources/plugins/mobile/ ** 解压到 app_data_dir/plugins/{plugin_id}/
 * （即 Context.dataDir/plugins/，与 Rust `app_data_dir()` 一致，勿用 filesDir），
 * 写入 .bedcode-source 标记（内容 "apk-asset:{version}"，用于升级后按版本刷新）。
 * 由 Rust 端 android_plugins::init() 注册，启动时经 run_mobile_plugin 调用。
 *
 * 注意：assets 中内置插件带平台层级（resources/plugins/mobile/{plugin_id}/，
 * 由 tauri.conf.json bundle resources 保留相对路径所致），解压时必须以
 * mobile 子目录为根逐个展开，使 dataDir/plugins/{plugin_id}/plugin.json
 * 与 Rust 端 PluginLoader::load_all 的一级目录扫描契约一致。
 */
@InvokeArg
internal class ExtractPluginsArgs {
    var appVersion: String = ""
}

@TauriPlugin
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

    /**
     * 插件解压目标根目录
     *
     * 必须与 Rust 端 `app_data_dir()` 一致 —— Tauri 在 Android 上将其解析为
     * `Context.getDataDir()`（即 /data/user/0/<pkg>，而非 filesDir）。
     * Rust 的 PluginLoader 扫描 `app_data_dir/plugins`，此处若写到 filesDir
     * 会导致解压成功但扫描为 0。
     */
    private val pluginsBaseDir: File
        get() = File(activity.dataDir, PLUGIN_BASE_DIR)

    /** 解压 assets/resources/plugins/mobile 下所有插件目录 */
    private fun extractAll(appVersion: String): Int {
        // 清理旧版本误布局（filesDir/plugins 下的历史产物，见 cleanupLegacyLayout）
        cleanupLegacyLayout()

        // assets 中内置插件带平台层级 mobile/（与 Rust dev_copy_plugins 的
        // resources/plugins/mobile 及构建产物布局一致），此处展开到 {plugin_id}
        val pluginsRoot = "resources/plugins/$PLATFORM_DIR"
        val pluginIds = (activity.assets.list(pluginsRoot) ?: return 0)
            .filter { !it.startsWith(".") }
            .toSet()

        // 清理 APK 中已移除的插件（暂停开发/从 bundle 移除）：保留带 apk-asset
        // 标记但不在当前 assets 列表中的残留目录会继续被宿主 load_all 扫描加载，
        // 必须删除，否则已下架插件在升级后依然出现在应用里
        cleanupRemovedPlugins(pluginIds)

        var extracted = 0
        for (id in pluginIds) {
            val assetDir = "$pluginsRoot/$id"
            val destDir = File(pluginsBaseDir, id)

            // 已解压、版本一致且 manifest 是真实文件 → 跳过（升级后按版本刷新）。
            // manifest 必须是文件：历史 bug 曾把 plugin.json 解压成目录，
            // 仅凭标记跳过会让损坏产物永远无法自愈。
            // debug 构建（tauri:android:dev）：插件重构建而应用版本未变时标记仍匹配，
            // 会一直加载旧产物 —— debug 下始终重新解压保证 dev 看到最新构建。
            val marker = File(destDir, MARKER_FILE)
            val manifestOk = File(destDir, PLUGIN_MANIFEST_FILE).isFile
            if (!BuildConfig.DEBUG &&
                manifestOk &&
                marker.exists() &&
                marker.readText().trim() == "$SOURCE_APK_ASSET:$appVersion"
            ) continue

            if (destDir.exists()) destDir.deleteRecursively()
            if (copyAssetDir(assetDir, destDir) && File(destDir, PLUGIN_MANIFEST_FILE).isFile) {
                marker.parentFile?.mkdirs()
                marker.writeText("$SOURCE_APK_ASSET:$appVersion")
                extracted++
                Log.i(TAG, "Extracted bundled plugin: $id -> $destDir")
            } else {
                Log.e(TAG, "Extract bundled plugin failed or incomplete: $id")
            }
        }
        return extracted
    }

    /**
     * 清理旧版本遗留的插件目录
     *
     * 历史版本曾把插件解压到 `filesDir/plugins`（含 `mobile/` 误布局与按 id 展开
     * 两种形态），与当前约定的 `dataDir/plugins` 不一致且不再被任何代码读写。
     * 仅当目录内存在 apk-asset 来源标记（即本解压器历史产物）时才删除，
     * 避免误删其他来源数据。
     */
    private fun cleanupLegacyLayout() {
        val legacyBase = File(activity.filesDir, PLUGIN_BASE_DIR)
        if (!legacyBase.isDirectory) return

        // 收集带 apk-asset 标记的插件目录（含早期 plugins/mobile 误布局整体）
        val legacyDirs = mutableListOf<File>()
        val mobileDir = File(legacyBase, PLATFORM_DIR)
        if (File(mobileDir, MARKER_FILE).let { it.exists() && it.readText().trim().startsWith("$SOURCE_APK_ASSET:") }) {
            legacyDirs.add(mobileDir)
        }
        legacyBase.listFiles()?.forEach { dir ->
            if (dir.isDirectory && dir.name != PLATFORM_DIR) {
                val marker = File(dir, MARKER_FILE)
                if (marker.exists() && marker.readText().trim().startsWith("$SOURCE_APK_ASSET:")) {
                    legacyDirs.add(dir)
                }
            }
        }

        legacyDirs.forEach { dir ->
            if (dir.deleteRecursively()) {
                Log.i(TAG, "Removed legacy plugin dir: $dir")
            }
        }
        // 若 filesDir/plugins 已空则一并移除
        if (legacyBase.isDirectory && (legacyBase.listFiles()?.isEmpty() != false)) {
            legacyBase.delete()
        }
    }

    /**
     * 清理 APK 中已移除的内置插件（暂停开发/从 bundle 剔除）
     *
     * 仅删除带 apk-asset 来源标记且不在当前 assets 列表中的目录，
     * 保留 file-install / remote-download 来源的用户安装插件。
     * 无标记（旧版本安装 / 手动放入）或标记读取失败时无法确认内置来源，
     * 一律保守保留并告警——误删用户插件不可恢复。
     */
    private fun cleanupRemovedPlugins(activeIds: Set<String>) {
        val base = pluginsBaseDir
        if (!base.isDirectory) return
        base.listFiles()?.forEach { dir ->
            if (!dir.isDirectory || dir.name in activeIds) return@forEach
            val marker = File(dir, MARKER_FILE)
            val isApkAsset = try {
                if (!marker.exists()) {
                    // 无来源标记：可能是标记机制引入前安装的插件，无法确认内置
                    // 来源，保守保留（仅清理标记正向确认为 apk-asset 的目录）
                    Log.w(TAG, "Plugin without source marker kept: ${dir.name}")
                    false
                } else {
                    marker.readText().trim().startsWith("$SOURCE_APK_ASSET:")
                }
            } catch (e: Exception) {
                // 标记损坏/不可读：无法确认来源，保守保留
                Log.w(TAG, "Plugin marker unreadable, kept: ${dir.name}", e)
                false
            }
            if (isApkAsset && dir.deleteRecursively()) {
                Log.i(TAG, "Removed bundled plugin no longer in APK: ${dir.name}")
            }
        }
    }

    /**
     * 递归复制 assets 目录到目标目录
     *
     * 文件/目录判定用 `assets.open()` 是否成功，而非 `assets.list()` 的返回值：
     * 部分 Android 版本对文件调用 list() 会返回空数组（非 null），
     * 导致文件被误判为目录、plugin.json 被创建成空目录。
     */
    private fun copyAssetDir(assetDir: String, destDir: File): Boolean {
        val entries = activity.assets.list(assetDir) ?: return false
        if (!destDir.exists() && !destDir.mkdirs()) return false

        for (entry in entries) {
            if (entry.isEmpty()) continue
            val assetPath = "$assetDir/$entry"
            val destFile = File(destDir, entry)

            // 先按文件尝试打开；失败则视为子目录递归
            val input = try {
                activity.assets.open(assetPath)
            } catch (e: IOException) {
                null
            }

            if (input != null) {
                input.use { ins ->
                    FileOutputStream(destFile).use { output -> ins.copyTo(output) }
                }
            } else {
                if (!copyAssetDir(assetPath, destFile)) return false
            }
        }
        return true
    }

    companion object {
        private const val TAG = "PluginAssetExtractor"
        const val MARKER_FILE = ".bedcode-source"
        const val SOURCE_APK_ASSET = "apk-asset"

        /** 插件数据目录名（与 Rust PLUGIN_DATA_DIR 一致，位于 app_data_dir 下） */
        private const val PLUGIN_BASE_DIR = "plugins"

        /** 插件 manifest 文件名（与 Rust PLUGIN_MANIFEST_FILE 一致） */
        private const val PLUGIN_MANIFEST_FILE = "plugin.json"

        /** assets 中内置插件的平台子目录（resources/plugins/mobile/{id}） */
        private const val PLATFORM_DIR = "mobile"
    }
}
