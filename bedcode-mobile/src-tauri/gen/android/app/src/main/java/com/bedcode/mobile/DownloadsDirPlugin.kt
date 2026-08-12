package com.bedcode.mobile

import android.app.Activity
import android.content.ActivityNotFoundException
import android.content.Intent
import android.net.Uri
import android.os.Environment
import android.provider.MediaStore
import android.webkit.MimeTypeMap
import androidx.core.content.FileProvider
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.JSObject
import app.tauri.plugin.Invoke
import app.tauri.plugin.Plugin
import java.io.File

/**
 * Tauri 插件 - 获取 Android 外部私有下载目录
 *
 * 返回 Context.getExternalFilesDir(Environment.DIRECTORY_DOWNLOADS) 的绝对路径。
 * 该目录位于 /storage/emulated/0/Android/data/com.bedcode.mobile/files/Download，
 * 免存储权限、多数文件管理器可见。外部存储不可用时回退到 null（宿主侧兜底处理）。
 *
 * 由 Rust 端 android_plugins.rs 注册（DownloadsDirPlugin）。
 * gen/android 重建恢复清单：本文件须恢复（参见 AGENTS.md「Android」节）。
 */
@TauriPlugin
class DownloadsDirPlugin(private val activity: Activity) : Plugin(activity) {

    companion object {
        private const val TAG = "BedCode-DownloadsDir"
    }

    /// 获取外部私有下载目录绝对路径
    @Command
    fun getDownloadsDir(invoke: Invoke) {
        val result = JSObject()
        try {
            val dir = activity.getExternalFilesDir(Environment.DIRECTORY_DOWNLOADS)
            if (dir != null) {
                result.put("path", dir.absolutePath)
            } else {
                // 外部存储不可用（如 USB 大容量存储模式）
                result.put("path", "")
            }
            invoke.resolve(result)
        } catch (e: Exception) {
            android.util.Log.e(TAG, "getDownloadsDir failed: ${e.message}")
            result.put("path", "")
            result.put("error", e.message)
            invoke.resolve(result)
        }
    }

    /// 打开已下载文件（传输完成「查看本地文件」）
    ///
    /// 解析顺序：
    /// 1. MediaStore 公共下载按 displayName 查最新一条（下载完成经
    ///    write_media_downloads 发布），命中 → ACTION_VIEW content URI；
    /// 2. 未命中（发布失败/上传方向源文件等）→ FileProvider 暴露本地路径
    ///    （external-path 覆盖 /storage/emulated/0/Android/data/...）。
    /// 授权读权限给目标应用；无可用查看器时返回错误文案。
    @Command
    fun openFile(invoke: Invoke) {
        val args = invoke.parseArgs(OpenFileArgs::class.java)
        if (args.path.isEmpty()) {
            invoke.reject("openFile: path is required")
            return
        }
        try {
            val uri = resolveContentUri(args.path, args.displayName)
            if (uri == null) {
                invoke.reject("openFile: file not found (MediaStore or FileProvider)")
                return
            }
            val mime = activity.contentResolver.getType(uri)
                ?: MimeTypeMap.getSingleton().getMimeTypeFromExtension(
                    args.path.substringAfterLast('.', "").lowercase(),
                )
                ?: "*/*"
            val intent = Intent(Intent.ACTION_VIEW).apply {
                setDataAndType(uri, mime)
                addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
            }
            activity.startActivity(intent)
            invoke.resolve(JSObject().apply { put("ok", true) })
        } catch (e: ActivityNotFoundException) {
            android.util.Log.e(TAG, "openFile: no viewer found: ${e.message}")
            invoke.reject("openFile: no app can open this file type")
        } catch (e: Exception) {
            android.util.Log.e(TAG, "openFile failed: ${e.message}")
            invoke.reject("openFile failed: ${e.message}")
        }
    }

    /// 解析可分享的 content URI：MediaStore 公共下载（按名查最新）→ FileProvider
    private fun resolveContentUri(path: String, displayName: String): Uri? {
        if (displayName.isNotEmpty()) {
            val projection = arrayOf(MediaStore.Downloads._ID)
            val selection = "${MediaStore.Downloads.DISPLAY_NAME} = ?"
            val selectionArgs = arrayOf(displayName)
            activity.contentResolver.query(
                MediaStore.Downloads.EXTERNAL_CONTENT_URI,
                projection,
                selection,
                selectionArgs,
                "${MediaStore.Downloads.DATE_ADDED} DESC",
            )?.use { cursor ->
                if (cursor.moveToFirst()) {
                    val id = cursor.getLong(0)
                    return Uri.withAppendedPath(MediaStore.Downloads.EXTERNAL_CONTENT_URI, id.toString())
                }
            }
        }
        // MediaStore 未命中：FileProvider 暴露本地路径（app 私有外部目录）
        val file = File(path)
        if (!file.exists()) return null
        return FileProvider.getUriForFile(
            activity,
            "${activity.packageName}.fileprovider",
            file,
        )
    }
}

@InvokeArg
internal class OpenFileArgs {
    var path: String = ""
    var displayName: String = ""
}
