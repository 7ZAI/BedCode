package com.bedcode.mobile

import android.app.Activity
import android.os.Build
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
import java.util.zip.ZipFile

/**
 * OCR 模型惰性解压（PP-OCRv4 ONNX → dataDir/ocr_models/）
 *
 * 将 APK assets/resources/ocr_models/ 下的 .onnx 文件解压到 app_data_dir/ocr_models/
 * （即 Context.dataDir/ocr_models/，与 Rust `app_data_dir()` 一致，勿用 filesDir），
 * 写入 .bedcode-source 标记（内容 "apk-asset:{version}"，升级后按版本刷新）。
 * 惰性触发：Rust 侧 models_present() 为 false 时经 plugin_ocr_restore_models 调用。
 *
 * 由 Rust 端 android_plugins/ocr_models.rs 注册（Builder 名 "ocr-model-extractor"）。
 * gen/android 重建恢复清单：本文件须恢复（参见 AGENTS.md「Android」节）。
 */
@InvokeArg
internal class ExtractOcrModelsArgs {
    var appVersion: String = ""
}

@TauriPlugin
class OcrModelExtractorPlugin(private val activity: Activity) : Plugin(activity) {

    @Command
    fun extractOcrModels(invoke: Invoke) {
        val args = invoke.parseArgs(ExtractOcrModelsArgs::class.java)
        try {
            val count = extractAll(args.appVersion)
            val result = JSObject()
            result.put("success", true)
            result.put("count", count)
            invoke.resolve(result)
        } catch (e: Exception) {
            Log.e(TAG, "extractOcrModels failed", e)
            val result = JSObject()
            result.put("success", false)
            result.put("error", e.message ?: "Unknown error")
            invoke.resolve(result)
        }
    }

    /** nativeLibraryDir（onnxruntime .so 所在目录，供 Rust 侧 dlopen 路径探测） */
    @Command
    fun getNativeLibraryDir(invoke: Invoke) {
        val result = JSObject()
        result.put("dir", activity.applicationInfo.nativeLibraryDir)
        invoke.resolve(result)
    }

    /**
     * 确保 libonnxruntime.so 在文件系统上可 dlopen：从 APK 提取到 dataDir（幂等）。
     *
     * 现代 Android（targetSdk 31+，64 位 app）默认 extractNativeLibs=false，APK 内 .so
     * 为 uncompressed（Stored），安装时**不会**解压到 nativeLibraryDir，而 Rust 侧的
     * ort::init_from 走原生 dlopen，必须拿到真实文件路径。故从 APK（sourceDir 的 ZipFile）
     * 提取一份到 dataDir 根目录，与 OCR 模型解压同思路；幂等：目标存在且大小一致则跳过。
     * 返回最终可用路径（所有 ABI 均无该条目时回退 nativeLibraryDir 路径）。
     */
    @Command
    fun ensureNativeLib(invoke: Invoke) {
        val result = JSObject()
        try {
            result.put("path", extractNativeLib())
        } catch (e: Exception) {
            Log.e(TAG, "ensureNativeLib failed", e)
            result.put("error", e.message ?: "Unknown error")
        }
        invoke.resolve(result)
    }

    /** 从 APK 提取 lib/<abi>/<NATIVE_LIB_NAME> 到 dataDir；幂等跳过，返回最终路径 */
    private fun extractNativeLib(): String {
        val dest = File(activity.dataDir, NATIVE_LIB_NAME)
        val apkPath = activity.applicationInfo.sourceDir
        ZipFile(apkPath).use { zip ->
            for (abi in Build.SUPPORTED_ABIS) {
                val entry = zip.getEntry("lib/$abi/$NATIVE_LIB_NAME") ?: continue
                if (dest.isFile && dest.length() == entry.size) return dest.absolutePath
                zip.getInputStream(entry).use { ins ->
                    FileOutputStream(dest).use { out -> ins.copyTo(out) }
                }
                Log.i(TAG, "Extracted native lib $NATIVE_LIB_NAME ($abi) -> ${dest.absolutePath}")
                return dest.absolutePath
            }
        }
        return File(activity.applicationInfo.nativeLibraryDir, NATIVE_LIB_NAME).absolutePath
    }

    /**
     * 解压三模型；幂等：版本标记匹配且全部 .onnx 在目标目录时跳过。
     * 惰性语义在 Rust 侧（models_present 才调用），此处再做文件级校验兜底。
     */
    private fun extractAll(appVersion: String): Int {
        val modelsRoot = "resources/ocr_models"
        val modelNames = (activity.assets.list(modelsRoot) ?: return 0)
            .filter { it.endsWith(".onnx") }
            .toSet()
        if (modelNames.isEmpty()) return 0

        val destDir = File(activity.dataDir, MODELS_DIR_NAME)
        val marker = File(destDir, MARKER_FILE)
        val markerOk =
            marker.exists() && marker.readText().trim() == "$SOURCE_APK_ASSET:$appVersion"
        if (markerOk && modelNames.all { File(destDir, it).isFile }) return 0

        // 全量重解压：先删旧产物，避免遗留已下架模型文件
        if (destDir.exists()) destDir.deleteRecursively()
        if (!destDir.mkdirs()) throw IOException("cannot create $destDir")

        var extracted = 0
        for (name in modelNames) {
            val assetPath = "$modelsRoot/$name"
            activity.assets.open(assetPath).use { ins ->
                FileOutputStream(File(destDir, name)).use { out -> ins.copyTo(out) }
            }
            extracted++
            Log.i(TAG, "Extracted OCR model: $name")
        }
        marker.writeText("$SOURCE_APK_ASSET:$appVersion")
        return extracted
    }

    companion object {
        private const val TAG = "OcrModelExtractorPlugin"
        const val MARKER_FILE = ".bedcode-source"
        const val SOURCE_APK_ASSET = "apk-asset"

        /** 模型数据目录名（与 Rust ocr::models::MODELS_DIR_NAME 一致，位于 app_data_dir 下） */
        const val MODELS_DIR_NAME = "ocr_models"

        /** onnxruntime 动态库文件名（与 Rust ocr::engine::onnxruntime_so_path 一致） */
        const val NATIVE_LIB_NAME = "libonnxruntime.so"
    }
}
