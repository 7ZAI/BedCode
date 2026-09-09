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

    /// 打开文件所在目录（历史记录「打开所在文件夹」）
    ///
    /// 目标目录 = path 的父目录，经 FileProvider 暴露后 ACTION_VIEW：
    /// 首选 resource/folder（Google Files 等主流文件管理器支持打开目录 URI），
    /// 无查看器时回退 vnd.android.document/directory 再试一次。
    @Command
    fun openFileLocation(invoke: Invoke) {
        val args = invoke.parseArgs(OpenFileLocationArgs::class.java)
        if (args.path.isEmpty()) {
            invoke.reject("openFileLocation: path is required")
            return
        }
        try {
            val dir = File(args.path).parentFile ?: File(args.path)
            if (!dir.exists() || !dir.isDirectory) {
                invoke.reject("openFileLocation: directory not found: ${dir.absolutePath}")
                return
            }
            val uri = FileProvider.getUriForFile(
                activity,
                "${activity.packageName}.fileprovider",
                dir,
            )
            startFolderView(uri)
            invoke.resolve(JSObject().apply { put("ok", true) })
        } catch (e: ActivityNotFoundException) {
            android.util.Log.e(TAG, "openFileLocation: no folder viewer found: ${e.message}")
            invoke.reject("openFileLocation: no app can open this folder")
        } catch (e: Exception) {
            android.util.Log.e(TAG, "openFileLocation failed: ${e.message}")
            invoke.reject("openFileLocation failed: ${e.message}")
        }
    }

    /// 按文件名打开接收文件的所在目录（历史记录「打开所在文件夹」真机路径）
    ///
    /// 接收落点不在 wire 上（真实设备无路径字段，只有文件名），解析顺序：
    /// 1. MediaStore 公共下载按 displayName 命中最新一条 → 打开
    ///    primary:Download 文档树目录（ExternalStorageProvider）；
    ///    未授权「所有文件访问」时目录 URI 无权限（SecurityException）→
    ///    降级打开文件本身（MediaStore 自有插入行，content URI 可访问）；
    /// 2. 未命中（发布失败，文件仍留 app 私有下载目录）→ 私有目录按名查找 +
    ///    FileProvider 暴露父目录。
    /// 两者均未命中 → reject（历史条目文件已被移动/删除）。
    /// 需 system:open 权限（前端 requireSystemOpenPermission 已校验）。
    @Command
    fun openFileLocationByName(invoke: Invoke) {
        val args = invoke.parseArgs(OpenFileByNameArgs::class.java)
        if (args.displayName.isEmpty()) {
            invoke.reject("openFileLocationByName: displayName is required")
            return
        }
        try {
            val name = args.displayName
            // 1. MediaStore 公共下载按名命中 → 目标位于公共 Download 目录
            val mediaUri = resolveMediaStoreDownloadUri(name)
            if (mediaUri != null) {
                try {
                    startFolderView(primaryDownloadFolderUri())
                } catch (e: SecurityException) {
                    // 未授权「所有文件访问」（ExternalStorageProvider 目录 URI 对应用
                    // 无权限，真机实证 SecurityException）：降级打开文件本身——
                    // MediaStore 行为应用自有插入，content URI 免权限可访问
                    android.util.Log.w(
                        TAG,
                        "folder view denied (${e.message}), opening file instead",
                    )
                    openMediaStoreFile(mediaUri, name)
                }
                invoke.resolve(JSObject().apply { put("ok", true) })
                return
            }
            // 2. 私有下载目录按名查找 → FileProvider 暴露父目录
            val dir = activity.getExternalFilesDir(Environment.DIRECTORY_DOWNLOADS)
            val file = dir?.let { File(it, name) }?.takeIf { it.exists() }
            if (file != null) {
                val folder = file.parentFile ?: file
                startFolderView(
                    FileProvider.getUriForFile(
                        activity,
                        "${activity.packageName}.fileprovider",
                        folder,
                    ),
                )
                invoke.resolve(JSObject().apply { put("ok", true) })
                return
            }
            invoke.reject("openFileLocationByName: file not found: $name")
        } catch (e: ActivityNotFoundException) {
            android.util.Log.e(TAG, "openFileLocationByName: no folder viewer found: ${e.message}")
            invoke.reject("openFileLocationByName: no app can open this folder")
        } catch (e: Exception) {
            android.util.Log.e(TAG, "openFileLocationByName failed: ${e.message}")
            invoke.reject("openFileLocationByName failed: ${e.message}")
        }
    }

    /// 启动目录查看 Intent：vnd.android.document/directory 优先，
    /// ActivityNotFoundException 时回退 resource/folder。
    ///
    /// 顺序依据（2026-08-15 实测）：不少设备（含 MIUI）没有应用注册
    /// resource/folder（或仅网盘类 app 注册，选择器体验差），而
    /// Google Files（documentsui）普遍注册 vnd.android.document/directory
    /// 且 isDefault=true —— 先发它可直接打开文件管理器、不弹选择器。
    /// resource/folder 作为回退（部分设备只有文件管理器注册它）。
    private fun startFolderView(uri: Uri) {
        val flags = Intent.FLAG_GRANT_READ_URI_PERMISSION
        try {
            val intent = Intent(Intent.ACTION_VIEW).apply {
                setDataAndType(uri, "vnd.android.document/directory")
                addFlags(flags)
            }
            activity.startActivity(intent)
        } catch (e: ActivityNotFoundException) {
            val fallback = Intent(Intent.ACTION_VIEW).apply {
                setDataAndType(uri, "resource/folder")
                addFlags(flags)
            }
            activity.startActivity(fallback)
        }
    }

    /// 打开 MediaStore 文件本身（自有插入行，content URI 免权限）
    ///
    /// 未授权「所有文件访问」时的所在文件夹降级路径：ExternalStorageProvider
    /// 目录 URI 无权限，退而打开文件——用户至少能查看/分享下载结果。
    private fun openMediaStoreFile(uri: Uri, displayName: String) {
        val mime = activity.contentResolver.getType(uri)
            ?: MimeTypeMap.getSingleton().getMimeTypeFromExtension(
                displayName.substringAfterLast('.', "").lowercase(),
            )
            ?: "*/*"
        val intent = Intent(Intent.ACTION_VIEW).apply {
            setDataAndType(uri, mime)
            addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
        }
        activity.startActivity(intent)
    }

    /// 解析可分享的 content URI：MediaStore 公共下载（按名查最新）→ FileProvider
    private fun resolveContentUri(path: String, displayName: String): Uri? {
        resolveMediaStoreDownloadUri(displayName)?.let { return it }
        // MediaStore 未命中：FileProvider 暴露本地路径（app 私有外部目录）
        val file = File(path)
        if (!file.exists()) return null
        return FileProvider.getUriForFile(
            activity,
            "${activity.packageName}.fileprovider",
            file,
        )
    }

    /// MediaStore Downloads 按 displayName 查最新一条的 content URI（openFile /
    /// openFileLocationByName 共用；未命中返回 null）
    private fun resolveMediaStoreDownloadUri(displayName: String): Uri? {
        if (displayName.isEmpty()) return null
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
        return null
    }

    /// primary 卷 Download 目录的文档树 URI（按名打开所在文件夹的目标目录）
    ///
    /// MediaStore 命中即文件位于公共 Download 目录；经 ExternalStorageProvider
    /// 文档树 URI 直接打开（Google Files / MIUI 文件均支持 vnd.android.document/directory）。
    private fun primaryDownloadFolderUri(): Uri =
        Uri.parse("content://com.android.externalstorage.documents/document/primary%3ADownload")
}

@InvokeArg
internal class OpenFileArgs {
    var path: String = ""
    var displayName: String = ""
}

@InvokeArg
internal class OpenFileByNameArgs {
    var displayName: String = ""
}

@InvokeArg
internal class OpenFileLocationArgs {
    var path: String = ""
}
