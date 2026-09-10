package com.bedcode.mobile

import android.app.Activity
import android.content.ActivityNotFoundException
import android.content.Intent
import android.net.Uri
import android.os.Build
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
import java.io.FileOutputStream

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

        /// 私有下载目录内按名递归查找的最大深度（wire 相对路径层级极浅，深层防御）
        private const val MAX_PRIVATE_SEARCH_DEPTH = 8

        /// 现场镜像目录名（私有下载目录下；未授权「所有文件访问」时打开所在目录用）
        private const val REVEAL_DIR = "bedcode-reveal"

        /// 全部下载镜像目录名（设置页「打开下载目录」未授权时；增量复制本应用拥有的行）
        private const val MIRROR_ALL_DIR = "bedcode-downloads"
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

    /// 打开公共下载目录（设置页「下载目录」区打开按钮，核对文件是否落盘）
    ///
    /// 已授权「所有文件访问」→ 直接打开 primary:Download 文档树 URI（真实位置）；
    /// 未授权或授权后仍被拒（假授权/授权被回收/部分 ROM 树 URI 语义差异）→ 把
    /// 本应用拥有的 MediaStore 下载行（OWNER_PACKAGE_NAME 过滤）全部镜像到私有目录
    /// （content URI 自有行免权限可读，增量复制：同名同尺寸跳过、删除已不存在公共行的
    /// 旧镜像）后经 FileProvider 打开镜像目录——零权限、任意设备可用，用户可核对文件
    /// 是否落盘。镜像在后台线程执行避免大文件阻塞 UI；镜像可为空（无本应用下载记录时
    /// 打开空目录），不再依赖系统授权页（部分 ROM 不提供「所有文件访问」开关）。
    @Command
    fun openDownloadDir(invoke: Invoke) {
        val granted = Build.VERSION.SDK_INT < Build.VERSION_CODES.R ||
            Environment.isExternalStorageManager()
        if (granted) {
            try {
                startFolderView(primaryDownloadFolderUri())
                invoke.resolve(JSObject().apply { put("ok", true) })
                return
            } catch (e: ActivityNotFoundException) {
                android.util.Log.e(TAG, "openDownloadDir: no folder viewer found: ${e.message}")
                invoke.reject("openDownloadDir: no app can open this folder")
                return
            } catch (e: SecurityException) {
                // 授权后仍被拒（假授权/授权被回收/部分 ROM 树 URI 语义差异）：
                // 降级镜像路径（下方公共代码），不把用户堵在授权引导上
                android.util.Log.w(
                    TAG,
                    "openDownloadDir: public folder denied despite grant (${e.message}), falling back to mirror",
                )
            } catch (e: Exception) {
                android.util.Log.e(TAG, "openDownloadDir failed: ${e.message}")
                invoke.reject("openDownloadDir failed: ${e.message}")
                return
            }
        }
        // 未授权（或授权后被拒）：镜像本应用拥有的下载行到私有目录后打开。
        // 镜像可为空（无本应用下载记录时打开空目录，也是诚实视图）——不再依赖
        // 系统授权页（部分 ROM 不提供「所有文件访问」开关，授权引导是死路）。
        val items = queryOwnedMediaDownloads()
        android.util.Log.i(TAG, "openDownloadDir: mirroring ${items.size} owned download(s) to private dir")
        Thread {
            val dir = mirrorOwnedDownloadsToDir(items)
            activity.runOnUiThread {
                if (dir != null) {
                    startFolderView(
                        FileProvider.getUriForFile(
                            activity,
                            "${activity.packageName}.fileprovider",
                            dir,
                        ),
                    )
                    invoke.resolve(JSObject().apply { put("ok", true); put("mirror", true) })
                } else {
                    android.util.Log.w(TAG, "openDownloadDir: mirror dir unavailable")
                    invoke.reject("openDownloadDir: cannot create mirror directory")
                }
            }
        }.start()
    }

    /// 按文件名打开接收文件的所在目录（历史记录「打开所在文件夹」真机路径）

    /// 按文件名打开接收文件的所在目录（历史记录「打开所在文件夹」真机路径）
    ///
    /// 接收落点不在 wire 上（真实设备无路径字段，只有文件名），解析顺序：
    /// 1. MediaStore 公共下载按 displayName 命中最新一条 → 已授权「所有文件访问」时
    ///    打开 primary:Download 文档树目录（真实位置）；未授权或授权后仍被拒（假
    ///    授权/授权被回收/部分 ROM 树 URI 语义差异）→ 现场把文件镜像到私有目录
    ///    （content URI 自有行免权限可读）后经 FileProvider 打开镜像目录——零权限、
    ///    任意设备可用；镜像也失败才以 `needs_all_files_access` 固定前缀 reject
    ///    供前端引导跳系统设置；
    /// 2. 未命中（发布失败，文件仍留 app 私有下载目录）→ 私有目录按名查找
    ///    （顶层优先，子目录浅层递归兜底）+ FileProvider 暴露父目录。
    /// 两者均未命中 → reject（历史条目文件已被移动/删除）。
    /// 需 system:open 权限（前端 requireSystemOpenPermission 已校验）。
    /// 注：「导航选中文件」无通用 Android API（ExternalStorageProvider 树 URI
    /// 只能定位目录），仅在能到达目录后由用户按文件名查找。
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
                // 打开公共 Download 目录（ExternalStorageProvider 树 URI）需要「所有文件访问」。
                // 已授权 → 直接打开真实位置；未授权 / 授权后仍被拒（假授权、授权被回收、
                // 部分 ROM 树 URI 语义差异）→ 现场镜像到私有目录（content URI 自有行免权限
                // 可读）经 FileProvider 打开——零权限、任意设备可用，满足「只开目录」约定。
                // 镜像也失败才以 needs_all_files_access 引导（前端弹跳设置对话框）。
                val granted = Build.VERSION.SDK_INT < Build.VERSION_CODES.R ||
                    Environment.isExternalStorageManager()
                if (granted) {
                    try {
                        startFolderView(primaryDownloadFolderUri())
                        android.util.Log.i(TAG, "openFileLocationByName: MediaStore hit (name=$name), opening Download folder")
                        invoke.resolve(JSObject().apply { put("ok", true) })
                        return
                    } catch (e: SecurityException) {
                        android.util.Log.w(
                            TAG,
                            "openFileLocationByName: folder view denied despite grant (${e.message}), falling back to mirror (name=$name)",
                        )
                    } catch (e: ActivityNotFoundException) {
                        android.util.Log.e(TAG, "openFileLocationByName: no folder viewer found: ${e.message}")
                        invoke.reject("openFileLocationByName: no app can open this folder")
                        return
                    } catch (e: Exception) {
                        android.util.Log.e(TAG, "openFileLocationByName failed: ${e.message}")
                        invoke.reject("openFileLocationByName failed: ${e.message}")
                        return
                    }
                }
                // 镜像在后台线程执行：大文件（如几十 MB 的音频）在主线程同步复制会
                // 冻结 UI——系统选择弹窗入场动画叠加冻结即表现为页面抖动/白闪；
                // 复制完成后回主线程打开目录并回包。
                Thread {
                    val revealDir = mirrorToPrivateRevealDir(mediaUri, name) ?: findPrivateCopyFolder(name)
                    activity.runOnUiThread {
                        if (revealDir != null) {
                            android.util.Log.i(TAG, "openFileLocationByName: opening mirror folder for $name")
                            startFolderView(
                                FileProvider.getUriForFile(
                                    activity,
                                    "${activity.packageName}.fileprovider",
                                    revealDir,
                                ),
                            )
                            invoke.resolve(JSObject().apply { put("ok", true); put("mirror", true) })
                        } else {
                            android.util.Log.w(TAG, "openFileLocationByName: mirror failed, guiding user (name=$name)")
                            invoke.reject(
                                "needs_all_files_access: opening system Download folder requires All files access (MANAGE_EXTERNAL_STORAGE)",
                            )
                        }
                    }
                }.start()
                return
            }
            // 2. 私有下载目录按名查找 → FileProvider 暴露父目录
            //    （顶层优先，子目录递归兜底：接收 offer 的相对路径可能带子目录）
            val folder = findPrivateCopyFolder(name)
            if (folder != null) {
                android.util.Log.i(TAG, "openFileLocationByName: private copy folder hit: ${folder.absolutePath}")
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
            android.util.Log.w(TAG, "openFileLocationByName: file not found: $name")
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

    /// 公共 Download 目录 MediaStore 集合：与 writeMediaDownloads 写入同源
    ///
    /// 写入用 VOLUME_EXTERNAL_PRIMARY（content://media/external_primary/downloads），
    /// 此处必须同源查询——部分 ROM（实机：MIUI/澎湃平板）external 与
    /// external_primary 命名空间不互通，写入成功但 EXTERNAL_CONTENT_URI 查不到。
    private fun downloadsMediaCollection(): Uri =
        MediaStore.Downloads.getContentUri(MediaStore.VOLUME_EXTERNAL_PRIMARY)

    /// 按条件查公共下载最新一条（先 external_primary 与写入同源，miss 再兜底
    /// EXTERNAL_CONTENT_URI）：返回命中的集合与 _ID，URI 拼接以命中集合为准
    private fun queryMediaStoreDownload(selection: String, selectionArgs: Array<String>): Pair<Uri, Long>? {
        val projection = arrayOf(MediaStore.Downloads._ID)
        for (collection in listOf(downloadsMediaCollection(), MediaStore.Downloads.EXTERNAL_CONTENT_URI)) {
            activity.contentResolver.query(
                collection,
                projection,
                selection,
                selectionArgs,
                "${MediaStore.Downloads.DATE_ADDED} DESC",
            )?.use { cursor ->
                if (cursor.moveToFirst()) {
                    return collection to cursor.getLong(0)
                }
            }
        }
        return null
    }

    /// MediaStore Downloads 按 displayName 查最新一条的 content URI（openFile /
    /// openFileLocationByName 共用；未命中返回 null）。IS_PENDING=0 过滤保证
    /// 只命中已落位（非占位）行。
    ///
    /// 候选名容错：部分 ROM（实机：MIUI/澎湃平板）的 MediaProvider 在插入时会把
    /// 系统 MimeTypeMap 无映射的扩展名（.md/.json 等）安全加固为 `name.ext.txt`
    /// （实测 CONTEXT.md 落盘为 CONTEXT.md.txt），精确按 offer 名查询必然 miss。
    /// 因此在其后追加 `name + ".txt"` 候选，先精确后容错，命中即日志记录真名。
    private fun resolveMediaStoreDownloadUri(displayName: String): Uri? {
        if (displayName.isEmpty()) return null
        val candidates = buildList {
            add(displayName)
            // 后缀加固：请求名本身已 .txt 结尾时不追加，避免 `x.txt.txt`
            if (!displayName.endsWith(".txt", ignoreCase = true)) add("$displayName.txt")
        }
        for (name in candidates) {
            val selection =
                "${MediaStore.Downloads.DISPLAY_NAME} = ? AND ${MediaStore.Downloads.IS_PENDING} = 0"
            val (collection, id) = queryMediaStoreDownload(selection, arrayOf(name)) ?: continue
            if (name != displayName) {
                android.util.Log.i(
                    TAG,
                    "resolveMediaStoreDownloadUri: display_name='$name' (requested '$displayName', ROM renamed)",
                )
            }
            return Uri.withAppendedPath(collection, id.toString())
        }
        return null
    }

    /// 私有下载目录内按文件名定位所在目录（顶层优先 + 浅层递归兜底）
    ///
    /// 接收落盘目标 = download_dir.join(wire 相对路径)，wire 路径可能带子目录
    /// （历史实现只查顶层导致漏匹配 → file not found）；递归限制深度防异常深目录。
private fun findPrivateCopyFolder(displayName: String): File? {
        val dir = activity.getExternalFilesDir(Environment.DIRECTORY_DOWNLOADS) ?: return null
        fun search(base: File, depth: Int): File? {
            if (depth > MAX_PRIVATE_SEARCH_DEPTH) return null
            base.listFiles()?.let { entries ->
                for (f in entries) {
                    if (f.isFile && f.name == displayName) return f.parentFile ?: f
                }
                for (f in entries) {
                    if (f.isDirectory) {
                        search(f, depth + 1)?.let { return it }
                    }
                }
            }
            return null
        }
        return search(dir, 0)
    }

    /// 现场镜像：MediaStore content URI → 私有下载目录镜像目录（零权限）
    ///
    /// 未授予「所有文件访问」时，公共 Download 目录 URI 不可打开（真机实证
    /// SecurityException）。MediaStore 行为本应用插入，content URI 免权限可读——
    /// 据此把文件流拷贝到固定镜像目录（bedcode-reveal，复用前先清残留，保证目录
    /// 内只有本次目标文件），返回目录供 FileProvider 暴露（满足「只开目录」约定）。
    /// 拷贝失败（流不可读/目录创建失败）返回 null，调用方降级引导授权。
    private fun mirrorToPrivateRevealDir(uri: Uri, name: String): File? {
        return try {
            val downloads = activity.getExternalFilesDir(Environment.DIRECTORY_DOWNLOADS)
                ?: return null
            val dir = File(downloads, REVEAL_DIR)
            if (!dir.exists() && !dir.mkdirs()) return null
            // 复用固定目录：先清残留（上次打开所在目录的旧镜像），避免目录堆积
            dir.listFiles()?.forEach { it.delete() }
            val dest = File(dir, name)
            activity.contentResolver.openInputStream(uri)?.use { input ->
                FileOutputStream(dest).use { output -> input.copyTo(output) }
            } ?: return null
            dir
        } catch (e: Exception) {
            android.util.Log.w(TAG, "mirrorToPrivateRevealDir failed: ${e.message}")
            null
        }
    }

    /// 查询本应用拥有的公共下载行（OWNER_PACKAGE_NAME 过滤 + IS_PENDING=0）
    ///
    /// 返回 (content Uri, displayName, size) 三元组，按添加时间升序。
    /// OWNER_PACKAGE_NAME 为 API 29 新增列；查询失败（部分 ROM 不支持）返回空表。
    private fun queryOwnedMediaDownloads(): List<Triple<Uri, String, Long>> {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.Q) return emptyList()
        val out = mutableListOf<Triple<Uri, String, Long>>()
        return try {
            val collection = downloadsMediaCollection()
            activity.contentResolver.query(
                collection,
                arrayOf(
                    MediaStore.Downloads._ID,
                    MediaStore.Downloads.DISPLAY_NAME,
                    MediaStore.Downloads.SIZE,
                    MediaStore.MediaColumns.OWNER_PACKAGE_NAME,
                ),
                "${MediaStore.Downloads.OWNER_PACKAGE_NAME} = ? AND ${MediaStore.Downloads.IS_PENDING} = 0",
                arrayOf(activity.packageName),
                "${MediaStore.Downloads.DATE_ADDED} ASC",
            )?.use { c ->
                val nameIdx = c.getColumnIndex(MediaStore.Downloads.DISPLAY_NAME)
                val sizeIdx = c.getColumnIndex(MediaStore.Downloads.SIZE)
                while (c.moveToNext()) {
                    val id = c.getLong(c.getColumnIndexOrThrow(MediaStore.Downloads._ID))
                    val name = if (nameIdx >= 0) c.getString(nameIdx) ?: "" else ""
                    if (name.isEmpty()) continue
                    val size = if (sizeIdx >= 0 && !c.isNull(sizeIdx)) c.getLong(sizeIdx) else 0L
                    out.add(Triple(Uri.withAppendedPath(collection, id.toString()), name, size))
                }
            }
            out
        } catch (e: Exception) {
            android.util.Log.w(TAG, "queryOwnedMediaDownloads failed: ${e.message}")
            emptyList()
        }
    }

    /// 把本应用拥有的下载行增量镜像到私有目录（MIRROR_ALL_DIR）
    ///
    /// 增量语义：同名且同尺寸跳过（不重复拷贝）；镜像中不再对应公共行的旧文件删除
    /// （公共目录删除后镜像同步消失）。返回镜像目录，失败返回 null。
    private fun mirrorOwnedDownloadsToDir(items: List<Triple<Uri, String, Long>>): File? {
        return try {
            val downloads = activity.getExternalFilesDir(Environment.DIRECTORY_DOWNLOADS)
                ?: return null
            val dir = File(downloads, MIRROR_ALL_DIR)
            if (!dir.exists() && !dir.mkdirs()) return null
            // 1. 删除不再对应公共行的旧镜像
            val wanted = items.map { it.second }.toSet()
            dir.listFiles()?.forEach { f ->
                if (f.isFile && f.name !in wanted) f.delete()
            }
            // 2. 增量复制：同名且同尺寸跳过
            for ((uri, name, size) in items) {
                val dest = File(dir, name)
                if (dest.isFile && dest.length() == size) continue
                activity.contentResolver.openInputStream(uri)?.use { input ->
                    FileOutputStream(dest).use { output -> input.copyTo(output) }
                } ?: continue
            }
            dir
        } catch (e: Exception) {
            android.util.Log.w(TAG, "mirrorOwnedDownloadsToDir failed: ${e.message}")
            null
        }
    }

    /// primary 卷 Download 目录的文档树 URI（按名打开所在文件夹的目标目录）
    ///
    /// MediaStore 命中即文件位于公共 Download 目录；经 ExternalStorageProvider
    /// 文档树 URI 直接打开目录（Google Files / MIUI 文件均支持
    /// vnd.android.document/directory）。树 URI 只能定位目录、无法选中文件
    /// （Android 无通用「定位选中」API）。注：路径中的转义是 `primary%3ADownload`
    /// ——文档树 document id 的分隔符是冒号，必须 URL 编码，勿改成斜杠。
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
