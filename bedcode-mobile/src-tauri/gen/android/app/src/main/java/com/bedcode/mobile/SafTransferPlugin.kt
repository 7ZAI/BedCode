package com.bedcode.mobile

import android.app.Activity
import android.content.ContentValues
import android.content.Intent
import android.net.Uri
import android.os.Build

import android.os.ParcelFileDescriptor
import android.provider.DocumentsContract
import android.provider.MediaStore
import android.provider.OpenableColumns
import android.system.Os
import android.system.OsConstants
import android.util.Base64
import android.webkit.MimeTypeMap
import androidx.activity.result.ActivityResult
import app.tauri.annotation.ActivityCallback
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.JSArray
import app.tauri.plugin.JSObject
import app.tauri.plugin.Invoke
import app.tauri.plugin.Plugin
import java.io.File
import java.io.FileInputStream
import java.io.FileOutputStream
import java.io.IOException
import java.io.InputStream
import java.io.OutputStream
import java.util.UUID
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicLong

/**
 * Tauri 插件 - SAF 存储传输后端（SafIo trait 的 Kotlin 实现）
 *
 * 为移动端文件传输提供 SAF（Storage Access Framework）存储访问，
 * 全部经 ContentResolver / DocumentsContract 流转，零存储权限依赖
 * （与 All Files 权限无关，见 docs/adr/0009）。
 *
 * 能力（对应宿主 Rust SafIo trait）：
 * - listTreeChildren：DocumentsContract.buildChildDocumentsUriUsingTree
 *   列目录树子条目（name/isDir/size/mime/uri/documentId）
 * - safToCache：中转复制（Relay Copy）——SAF 源 → app 私有 cache 的顺序流
 *   复制（512KB 缓冲、OpenableColumns.SIZE 预检、未知大小按流处理），
 *   后台线程执行，经 copyProgress 轮询进度、cancelCopy 取消
 * - safOpen/safRead/safSeek/safClose/safSeekable：上传 SAF 流直传（M3）——
 *   句柄式流读取（ParcelFileDescriptor 维护流位置），消除 v1 中转复制的
 *   双倍 IO；句柄表以 uri 为 key，任务内重连复用 fd 顺序续读，pipe 流
 *   （getStatSize()==-1 不可 seek）跨任务由宿主报 not-seekable-resume
 *   触发全量重传（spec M3 续传策略）
 * - saveToDocument：「保存到…」（M3）——ACTION_CREATE_DOCUMENT 单文件
 *   对话框（用户选位置）→ ContentResolver 流拷贝（写完即达）
 * - checkAuthorized：persistedUriPermissions 检测树授权是否仍有效
 *
 * 残留清理：取消/失败立即删除半成品；宿主在 file-transfer 插件激活时调用
 * cleanupStaleCopies 清扫 staging 目录（「插件激活时扫描清理 cache 残留」），
 * 上传完成后由插件侧删除 cache 副本（见 spec「复制桥语义」）。
 *
 * 由 Rust 端 android_plugins.rs 注册（SafTransferPlugin）。
 * gen/android 重建恢复清单：本文件须恢复（参见 AGENTS.md「Android」节）。
 */
@InvokeArg
internal class ListTreeArgs {
    var treeUri: String = ""
    var documentId: String = ""
}

@InvokeArg
internal class SafToCacheArgs {
    var uri: String = ""
    var destName: String = ""
}

@InvokeArg
internal class CopyArgs {
    var copyId: String = ""
}

@InvokeArg
internal class CheckAuthorizedArgs {
    var treeUri: String = ""
}

@InvokeArg
internal class WriteMediaDownloadsArgs {
    var srcPath: String = ""
    var displayName: String = ""
    var mimeType: String = ""
}

@InvokeArg
internal class SafOpenArgs {
    var uri: String = ""
    var mode: String = ""
    var offset: Long = 0
}

@InvokeArg
internal class SafReadArgs {
    var handleId: String = ""
    var len: Int = 0
}

@InvokeArg
internal class SafSeekArgs {
    var handleId: String = ""
    var offset: Long = 0
}

@InvokeArg
internal class SafStreamHandleArgs {
    var handleId: String = ""
}

@InvokeArg
internal class SafUriArgs {
    var uri: String = ""
}

@InvokeArg
internal class SaveToDocumentArgs {
    var srcPath: String = ""
    var suggestedName: String = ""
    var mimeType: String = ""
}

/** 一次 SAF 流直传的活跃句柄（safOpen 创建，safRead/safSeek 复用，safClose 释放）
 *
 * 句柄表以 uri 为 key：任务内断线重连（fd 保持）重复 safOpen 同一 uri 时
 * 直接复用既有句柄，流位置延续上次中断点（顺序续读，不重读）——pipe 流
 * 不可 seek 时的任务内续传语义（spec M3）。
 *
 * **必须强引用持有 pfd**：ParcelFileDescriptor 实现了 Closeable，若 safOpen
 * 返回后 pfd 局部变量失去引用，GC 会在某个时机 finalize 并 close 底层 fd，
 * 后续 safRead 的 input.read() 即报 EBADF (Bad file descriptor)。此前仅把
 * pfd.fileDescriptor / FileInputStream(pfd.fileDescriptor) 存进句柄，
 * pfd 本身可被 GC，导致上传中途随机失败。修复：句柄强持有 pfd，
 * closeStream 先关 input 再关 pfd（二者等价关底层 fd，双保险）。
 */
internal class StreamHandle(
    val handleId: String,
    val uri: Uri,
    val input: FileInputStream,
    val fd: java.io.FileDescriptor,
    val pfd: ParcelFileDescriptor,
    val seekable: Boolean,
    /** 文件总大小（statSize；pipe 流/未知为 0），safOpen 一并回报供进度条 */
    val size: Long,
) {
    /** 最近使用时间（safOpen 时清扫超时句柄，防泄漏） */
    @Volatile
    var lastUsed: Long = System.currentTimeMillis()
}

/** 一次中转复制的运行状态（后台线程写，轮询线程读，全部原子/易失字段） */
internal class CopyHandle(
    /** OpenableColumns.SIZE 预检值；未知大小（流式 provider）为 -1 */
    val total: Long,
    val destPath: String,
) {
    /** 已复制字节数 */
    val done = AtomicLong(0)
    /** 取消标记（cancelCopy 置位，复制循环每轮检查） */
    val cancelled = AtomicBoolean(false)
    /** 复制是否已结束（成功/失败/取消三者其一） */
    @Volatile
    var finished: Boolean = false
    /** 失败原因（仅失败时非空） */
    @Volatile
    var error: String? = null
}

@TauriPlugin
class SafTransferPlugin(private val activity: Activity) : Plugin(activity) {

    companion object {
        private const val TAG = "BedCode-SafTransfer"

        /** 顺序流复制缓冲（512KB） */
        private const val COPY_BUFFER_BYTES = 512 * 1024

        /** 单次 safRead 最大读取字节（512KB，base64 后约 700KB，跨桥可控） */
        private const val MAX_READ_BYTES = 512 * 1024

        /** 复制 staging 子目录名（app 私有 cache 下，系统可清理） */
        private const val STAGING_DIR = "bedcode_uploads"
    }

    /** 活跃复制句柄表（copyId → 句柄；跨命令轮询/取消共享） */
    private val copies = ConcurrentHashMap<String, CopyHandle>()

    /** 活跃流直传句柄表（uri → 句柄；safOpen 复用/清扫，任务内重连保 fd） */
    private val streams = ConcurrentHashMap<String, StreamHandle>()

    /** 流句柄表上限（超限时按最近使用时间淘汰最旧，防泄漏） */
    private val MAX_STREAMS = 16

    /** 流句柄空闲超时（safOpen 时清扫，防任务终态后 fd 泄漏） */
    private val STREAM_IDLE_TIMEOUT_MS = 10 * 60 * 1000L

    // ==================== 目录树遍历 ====================

    /// 列出 treeUri 下 documentId 目录的直接子条目
    @Command
    fun listTreeChildren(invoke: Invoke) {
        val args = invoke.parseArgs(ListTreeArgs::class.java)
        if (args.treeUri.isEmpty() || args.documentId.isEmpty()) {
            invoke.reject("listTreeChildren: treeUri and documentId are required")
            return
        }
        try {
            val treeUri = Uri.parse(args.treeUri)
            val childrenUri =
                DocumentsContract.buildChildDocumentsUriUsingTree(treeUri, args.documentId)
            val entries = JSArray()
            activity.contentResolver.query(
                childrenUri,
                null,
                null,
                null,
                null,
            )?.use { c ->
                val nameIdx = c.getColumnIndex(OpenableColumns.DISPLAY_NAME)
                val mimeIdx = c.getColumnIndex(DocumentsContract.Document.COLUMN_MIME_TYPE)
                val sizeIdx = c.getColumnIndex(OpenableColumns.SIZE)
                val docIdIdx = c.getColumnIndex(DocumentsContract.Document.COLUMN_DOCUMENT_ID)
                while (c.moveToNext()) {
                    val docId = if (docIdIdx >= 0) c.getString(docIdIdx) ?: "" else ""
                    val mime = if (mimeIdx >= 0) c.getString(mimeIdx) ?: "" else ""
                    val isDir = mime == DocumentsContract.Document.MIME_TYPE_DIR
                    val size = if (sizeIdx >= 0 && !isDir) c.getLong(sizeIdx) else 0L
                    entries.put(
                        JSObject().apply {
                            put("name", if (nameIdx >= 0) c.getString(nameIdx) ?: "" else "")
                            put("isDir", isDir)
                            put("size", size)
                            put("mime", mime)
                            put("uri", DocumentsContract.buildDocumentUriUsingTree(treeUri, docId).toString())
                            put("documentId", docId)
                        },
                    )
                }
            }
            invoke.resolve(JSObject().apply { put("entries", entries) })
        } catch (e: Exception) {
            // 授权被回收/目录被删时查询抛 SecurityException / IllegalArgumentException，
            // 原样透传供前端标记「共享目录失效」
            android.util.Log.e(TAG, "listTreeChildren failed: ${e.message}")
            invoke.reject("listTreeChildren failed: ${e.message}")
        }
    }

    // ==================== 中转复制（Relay Copy） ====================

    /// 启动中转复制：SAF 源 → app 私有 cache，立即返回 {copyId, destPath}
    ///
    /// 复制在后台线程顺序流执行（不可断点续传，见 spec「复制桥语义」）；
    /// 进度经 copyProgress 轮询，取消经 cancelCopy。
    @Command
    fun safToCache(invoke: Invoke) {
        val args = invoke.parseArgs(SafToCacheArgs::class.java)
        if (args.uri.isEmpty() || args.destName.isEmpty()) {
            invoke.reject("safToCache: uri and destName are required")
            return
        }
        // 纵深防御：destName 来自前端 entry.name（SAF 文档名不含路径分隔符，
        // 天然安全），但本命令对所有 fileservice 权限插件开放——含路径分隔符
        // （含绝对路径）、父目录跳转（..）的名称可逃逸 staging 目录，一律拒绝；
        // 分隔符已拒绝后 ".." 精确匹配即覆盖剩余逃逸向量
        if (args.destName.contains('/') || args.destName.contains('\\') || args.destName == "..") {
            invoke.reject("safToCache: destName must be a plain file name without path separators")
            return
        }
        try {
            val uri = Uri.parse(args.uri)
            // 大小预检：OpenableColumns.SIZE（云盘等流式 provider 可能 -1，按未知处理）
            var total = -1L
            activity.contentResolver.query(
                uri,
                arrayOf(OpenableColumns.SIZE),
                null,
                null,
                null,
            )?.use { c ->
                if (c.moveToFirst()) {
                    val idx = c.getColumnIndex(OpenableColumns.SIZE)
                    if (idx >= 0 && !c.isNull(idx)) total = c.getLong(idx)
                }
            }

            val staging = stagingDir()
            // 清扫已终态句柄（终态结果已可被轮询读取后不再需要，表大小有界）
            sweepFinishedCopies()
            val destFile = uniqueDestFile(staging, args.destName)
            val copyId = UUID.randomUUID().toString()
            val handle = CopyHandle(total, destFile.absolutePath)
            copies[copyId] = handle

            Thread {
                runCopy(copyId, uri, destFile, handle)
            }.start()

            invoke.resolve(
                JSObject().apply {
                    put("copyId", copyId)
                    put("destPath", destFile.absolutePath)
                },
            )
        } catch (e: Exception) {
            android.util.Log.e(TAG, "safToCache failed: ${e.message}")
            invoke.reject("safToCache failed: ${e.message}")
        }
    }

    /// 轮询中转复制进度
    @Command
    fun copyProgress(invoke: Invoke) {
        val args = invoke.parseArgs(CopyArgs::class.java)
        val handle = copies[args.copyId]
        if (handle == null) {
            invoke.reject("copyProgress: unknown copyId ${args.copyId}")
            return
        }
        invoke.resolve(
            JSObject().apply {
                put("copyId", args.copyId)
                put("done", handle.done.get())
                put("total", handle.total)
                put("finished", handle.finished)
                put("cancelled", handle.cancelled.get())
                put("error", handle.error)
                put("destPath", handle.destPath)
            },
        )
    }

    /// 取消中转复制（复制线程删除半成品后结束）
    @Command
    fun cancelCopy(invoke: Invoke) {
        val args = invoke.parseArgs(CopyArgs::class.java)
        val handle = copies[args.copyId]
        if (handle == null) {
            invoke.reject("cancelCopy: unknown copyId ${args.copyId}")
            return
        }
        handle.cancelled.set(true)
        invoke.resolve(JSObject().apply { put("ok", true) })
    }

    /// 清扫中转复制残留：删除 staging 目录中不属于活跃复制的文件
    ///
    /// 「插件激活时扫描清理 cache 残留」（spec「复制桥语义」）：file-transfer
    /// 插件激活与设置/上传页加载时调用。活跃复制（含已终态未清扫句柄）的
    /// 目标文件跳过删除，防止误伤进行中的复制。
    @Command
    fun cleanupStaleCopies(invoke: Invoke) {
        try {
            val staging = stagingDir()
            val active = copies.values.map { it.destPath }.toSet()
            var removed = 0
            staging.listFiles()?.forEach { f ->
                if (f.isFile && f.absolutePath !in active && f.delete()) removed++
            }
            android.util.Log.i(TAG, "cleanupStaleCopies removed $removed file(s)")
            invoke.resolve(JSObject().apply { put("removed", removed) })
        } catch (e: Exception) {
            android.util.Log.e(TAG, "cleanupStaleCopies failed: ${e.message}")
            invoke.reject("cleanupStaleCopies failed: ${e.message}")
        }
    }

    // ==================== MediaStore 落点（M2 接收方向） ====================

    /// 写入 MediaStore.Downloads 公共下载目录（API 29+ 零权限，系统文件管理器可见）
    ///
    /// 接收方向（移动端下载 + 桌面端推送）的统一落点：srcPath 为 app 私有
    /// 下载目录中的最终文件，本命令将其流拷贝到公共 Download 目录（用户可见）。
    /// API < 29 不支持 MediaStore.Downloads（RELATIVE_PATH 为 API 29 新增），
    /// 一律 reject —— Rust 侧据此回退私有目录（硬约束：不新增权限、运行时判断）。
    @Command
    fun writeMediaDownloads(invoke: Invoke) {
        val args = invoke.parseArgs(WriteMediaDownloadsArgs::class.java)
        if (args.srcPath.isEmpty() || args.displayName.isEmpty()) {
            invoke.reject("writeMediaDownloads: srcPath and displayName are required")
            return
        }
        // 纵深防御：displayName 将作为 MediaStore DISPLAY_NAME（文件名），
        // 含路径分隔符的名称可逃逸 Download 相对路径，一律拒绝
        if (args.displayName.contains('/') || args.displayName.contains('\\')) {
            invoke.reject("writeMediaDownloads: displayName must be a plain file name")
            return
        }
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.Q) {
            invoke.reject("writeMediaDownloads: MediaStore.Downloads requires API 29+")
            return
        }
        try {
            val src = File(args.srcPath)
            if (!src.isFile) {
                invoke.reject("writeMediaDownloads: src not found: ${args.srcPath}")
                return
            }
            val mime = if (args.mimeType.isNotEmpty()) {
                args.mimeType
            } else {
                // 按扩展名推断（推断失败退化为通用二进制类型）
                val ext = args.displayName.substringAfterLast('.', "")
                MimeTypeMap.getSingleton().getMimeTypeFromExtension(ext.lowercase())
                    ?: "application/octet-stream"
            }

            // IS_PENDING 原子可见：插入占位（其他应用不可见）→ 流拷贝 →
            // 清除 pending 落位（拷贝失败删除占位行，公共目录无半成品）
            val collection = MediaStore.Downloads.getContentUri(MediaStore.VOLUME_EXTERNAL_PRIMARY)

            // 同名即拒（US19 语义）：公共 Download 目录已有同名 → 拒绝，避免系统
            // 自动改名副本；宿主据此返回 duplicate-name（不回退私有目录覆盖）
            val dupExists = activity.contentResolver.query(
                collection,
                arrayOf(MediaStore.Downloads._ID),
                "${MediaStore.Downloads.DISPLAY_NAME}=?",
                arrayOf(args.displayName),
                null,
            )?.use { c -> c.count > 0 } ?: false
            if (dupExists) {
                invoke.reject("duplicate-name: ${args.displayName} already exists in system Downloads")
                return
            }

            // IS_PENDING 原子可见：插入占位（其他应用不可见）→ 流拷贝 →
            // 清除 pending 落位（拷贝失败删除占位行，公共目录无半成品）
            val values = ContentValues().apply {
                put(MediaStore.Downloads.DISPLAY_NAME, args.displayName)
                put(MediaStore.Downloads.MIME_TYPE, mime)
                put(MediaStore.Downloads.RELATIVE_PATH, "Download")
                put(MediaStore.Downloads.IS_PENDING, 1)
            }
            val uri = activity.contentResolver.insert(collection, values)
                ?: throw IOException("MediaStore insert returned null")
            try {
                activity.contentResolver.openOutputStream(uri, "w")?.use { output ->
                    FileInputStream(src).use { input ->
                        val buffer = ByteArray(COPY_BUFFER_BYTES)
                        while (true) {
                            val n = input.read(buffer)
                            if (n < 0) break
                            output.write(buffer, 0, n)
                        }
                    }
                } ?: throw IOException("openOutputStream returned null for $uri")

                // 落位：清除 pending，系统文件管理器立即可见
                val done = ContentValues().apply { put(MediaStore.Downloads.IS_PENDING, 0) }
                activity.contentResolver.update(uri, done, null, null)
                android.util.Log.i(TAG, "writeMediaDownloads ok: ${args.displayName}")
                invoke.resolve(JSObject().apply { put("ok", true) })
            } catch (e: Exception) {
                // 拷贝失败：删除占位行（公共目录不残留半成品），错误透传
                try {
                    activity.contentResolver.delete(uri, null, null)
                } catch (cleanup: Exception) {
                    android.util.Log.w(TAG, "writeMediaDownloads cleanup failed: ${cleanup.message}")
                }
                throw e
            }
        } catch (e: Exception) {
            android.util.Log.e(TAG, "writeMediaDownloads failed: ${e.message}")
            invoke.reject("writeMediaDownloads failed: ${e.message}")
        }
    }

    // ==================== 上传 SAF 流直传（M3） ====================

    /// 打开 SAF 源为可流读句柄（句柄表以 uri 为 key，任务内重连复用）
    ///
    /// offset 语义（spec M3 续传策略）：
    /// - 无活跃句柄且可 seek（getStatSize()!=-1）→ 打开后直接 seek 到 offset
    ///   （真续传）
    /// - 无活跃句柄且不可 seek（pipe 流）且 offset>0 → 只能从头打开，返回
    ///   effectiveOffset=0，宿主发现与请求 offset 不一致时回报
    ///   not-seekable-resume，插件重建 session 全量重传
    /// - 已有活跃句柄（任务内断线重连）→ 复用 fd 顺序续读，不重开不重读；
    ///   offset=0（显式全量重传）时强制关闭重开
    @Command
    fun safOpen(invoke: Invoke) {
        val args = invoke.parseArgs(SafOpenArgs::class.java)
        if (args.uri.isEmpty() || args.mode.isEmpty()) {
            invoke.reject("safOpen: uri and mode are required")
            return
        }
        try {
            sweepStaleStreams()
            val uri = Uri.parse(args.uri)
            val existing = streams[args.uri]
            if (existing != null) {
                if (args.offset == 0L) {
                    // 显式从头：关闭旧 fd 重开（重试/重建 session 语义）；
                    // 立即移出表，后续 openFileDescriptor 失败也不残留已关句柄
                    streams.remove(args.uri)
                    closeStream(existing)
                } else {
                    // 任务内续读：复用 fd，effectiveOffset = 当前流位置
                    val pos = try {
                        Os.lseek(existing.fd, 0, OsConstants.SEEK_CUR)
                    } catch (e: Exception) {
                        android.util.Log.w(TAG, "safOpen: lseek(cur) failed: ${e.message}")
                        0L
                    }
                    existing.lastUsed = System.currentTimeMillis()
                    invoke.resolve(
                        JSObject().apply {
                            put("handleId", existing.handleId)
                            put("effectiveOffset", pos)
                            put("seekable", existing.seekable)
                            put("size", existing.size)
                        },
                    )
                    return
                }
            }

            // 打开新 fd：文件流（可 seek）或 pipe 流（不可 seek，getStatSize()==-1）
            val pfd = activity.contentResolver.openFileDescriptor(uri, args.mode)
                ?: throw IllegalStateException("openFileDescriptor returned null for ${args.uri}")
            val input = FileInputStream(pfd.fileDescriptor)
            val seekable = pfd.statSize != -1L
            var effectiveOffset = 0L
            if (seekable && args.offset > 0) {
                // 真续传：直接 seek 到断点
                val seeked = try {
                    Os.lseek(pfd.fileDescriptor, args.offset, OsConstants.SEEK_SET)
                } catch (e: Exception) {
                    android.util.Log.w(TAG, "safOpen: seek failed: ${e.message}")
                    -1L
                }
                if (seeked >= 0) {
                    effectiveOffset = args.offset
                } else {
                    // seek 失败（provider 声明可 seek 但实际拒绝）：按 pipe 流处理，
                    // 从头读并回报 effectiveOffset=0，由宿主决定是否全量重传
                    android.util.Log.w(TAG, "safOpen: seek unsupported in practice for ${args.uri}")
                }
            }
            val handle = StreamHandle(
                handleId = UUID.randomUUID().toString(),
                uri = uri,
                input = input,
                fd = pfd.fileDescriptor,
                pfd = pfd,
                seekable = seekable,
                size = if (pfd.statSize == -1L) 0L else pfd.statSize,
            )
            streams[args.uri] = handle
            invoke.resolve(
                JSObject().apply {
                    put("handleId", handle.handleId)
                    put("effectiveOffset", effectiveOffset)
                    put("seekable", seekable)
                    put("size", handle.size)
                },
            )
        } catch (e: Exception) {
            android.util.Log.e(TAG, "safOpen failed: ${e.message}")
            invoke.reject("safOpen failed: ${e.message}")
        }
    }

    /// 按 handleId 查找流句柄（句柄表以 uri 为 key，任务内同 uri 复用 fd；
    /// 表项数 = 并发流任务数，线性查找开销可忽略）
    private fun streamByHandleId(handleId: String): StreamHandle? =
        streams.values.firstOrNull { it.handleId == handleId }

    /// 从流句柄读取至多 len 字节（base64 编码返回；EOF 返回空串）
    ///
    /// 传输格式权衡：JSON 数组数字（每字节 4 字符）与 hex（2 字符）均劣于
    /// base64（4/3 字符，无填充换行），选 base64 为跨桥最省空间格式。
    @Command
    fun safRead(invoke: Invoke) {
        val args = invoke.parseArgs(SafReadArgs::class.java)
        val handle = streamByHandleId(args.handleId)
        if (handle == null) {
            invoke.reject("safRead: unknown handleId ${args.handleId}")
            return
        }
        try {
            val buf = ByteArray(args.len.coerceIn(1, MAX_READ_BYTES))
            val n = handle.input.read(buf, 0, buf.size)
            handle.lastUsed = System.currentTimeMillis()
            val data = if (n < 0) {
                ""
            } else if (n == buf.size) {
                Base64.encodeToString(buf, Base64.NO_WRAP)
            } else {
                Base64.encodeToString(buf, 0, n, Base64.NO_WRAP)
            }
            invoke.resolve(JSObject().apply { put("data", data) })
        } catch (e: Exception) {
            android.util.Log.e(TAG, "safRead failed: ${e.message}")
            invoke.reject("safRead failed: ${e.message}")
        }
    }

    /// 移动流句柄到指定偏移（仅可 seek 句柄；pipe 流拒绝）
    @Command
    fun safSeek(invoke: Invoke) {
        val args = invoke.parseArgs(SafSeekArgs::class.java)
        val handle = streamByHandleId(args.handleId)
        if (handle == null) {
            invoke.reject("safSeek: unknown handleId ${args.handleId}")
            return
        }
        if (!handle.seekable) {
            invoke.reject("safSeek: stream is not seekable (pipe stream)")
            return
        }
        try {
            Os.lseek(handle.fd, args.offset, OsConstants.SEEK_SET)
            handle.lastUsed = System.currentTimeMillis()
            invoke.resolve(JSObject().apply { put("ok", true) })
        } catch (e: Exception) {
            android.util.Log.e(TAG, "safSeek failed: ${e.message}")
            invoke.reject("safSeek failed: ${e.message}")
        }
    }

    /// 关闭流句柄（任务终态后调用；任务内恢复不调用，fd 保留续读）
    @Command
    fun safClose(invoke: Invoke) {
        val args = invoke.parseArgs(SafStreamHandleArgs::class.java)
        val handle = streamByHandleId(args.handleId)
        if (handle == null) {
            invoke.reject("safClose: unknown handleId ${args.handleId}")
            return
        }
        streams.remove(handle.uri.toString())
        closeStream(handle)
        invoke.resolve(JSObject().apply { put("ok", true) })
    }

    /// 探测 SAF 源是否可 seek（getStatSize()==-1 为 pipe 流，不可 seek）
    @Command
    fun safSeekable(invoke: Invoke) {
        val args = invoke.parseArgs(SafUriArgs::class.java)
        if (args.uri.isEmpty()) {
            invoke.reject("safSeekable: uri is required")
            return
        }
        try {
            val uri = Uri.parse(args.uri)
            var seekable = false
            activity.contentResolver.query(
                uri,
                arrayOf(OpenableColumns.SIZE),
                null,
                null,
                null,
            )?.use { c ->
                if (c.moveToFirst()) {
                    val idx = c.getColumnIndex(OpenableColumns.SIZE)
                    if (idx >= 0 && !c.isNull(idx)) {
                        seekable = c.getLong(idx) != -1L
                    }
                }
            }
            invoke.resolve(JSObject().apply { put("seekable", seekable) })
        } catch (e: Exception) {
            android.util.Log.e(TAG, "safSeekable failed: ${e.message}")
            invoke.reject("safSeekable failed: ${e.message}")
        }
    }

    // ==================== 「保存到…」（M3） ====================

    /// 弹出系统「保存到…」单文件对话框并把 srcPath 流拷贝到用户选择的位置
    ///
    /// ACTION_CREATE_DOCUMENT 让用户选目标位置（含文件名）；选择后经
    /// ContentResolver.openOutputStream 流拷贝（写完即达）。用户取消返回
    /// {ok:false, cancelled:true}，调用方保留私有副本（回退语义）。
    /// 本命令在下载任务完成后由宿主调用（suggestedName 为远端文件名）。
    @Command
    fun saveToDocument(invoke: Invoke) {
        val args = invoke.parseArgs(SaveToDocumentArgs::class.java)
        if (args.srcPath.isEmpty() || args.suggestedName.isEmpty()) {
            invoke.reject("saveToDocument: srcPath and suggestedName are required")
            return
        }
        // 纵深防御：suggestedName 将作为系统对话框默认文件名（不含路径），
        // 含路径分隔符的名称可逃逸用户选择目录，一律拒绝
        if (args.suggestedName.contains('/') || args.suggestedName.contains('\\')) {
            invoke.reject("saveToDocument: suggestedName must be a plain file name")
            return
        }
        val src = File(args.srcPath)
        if (!src.isFile) {
            invoke.reject("saveToDocument: src not found: ${args.srcPath}")
            return
        }
        try {
            val mime = if (args.mimeType.isNotEmpty()) {
                args.mimeType
            } else {
                // 按扩展名推断（推断失败退化为通用二进制类型）
                val ext = args.suggestedName.substringAfterLast('.', "")
                MimeTypeMap.getSingleton().getMimeTypeFromExtension(ext.lowercase())
                    ?: "application/octet-stream"
            }
            val intent = Intent(Intent.ACTION_CREATE_DOCUMENT).apply {
                addCategory(Intent.CATEGORY_OPENABLE)
                type = mime
                putExtra(Intent.EXTRA_TITLE, args.suggestedName)
                addFlags(Intent.FLAG_GRANT_WRITE_URI_PERMISSION)
            }
            startActivityForResult(invoke, intent, "saveDocumentResult")
        } catch (e: Exception) {
            android.util.Log.e(TAG, "saveToDocument launch failed: ${e.message}")
            invoke.reject("saveToDocument launch failed: ${e.message}")
        }
    }

    /// 保存对话框回调：用户选位置后流拷贝（写完即达）
    @ActivityCallback
    fun saveDocumentResult(invoke: Invoke, result: ActivityResult) {
        when (result.resultCode) {
            Activity.RESULT_OK -> {
                val data = result.data
                // Intent.data 为 Java 平台类型，需显式标注 Uri? 才能经空检查智能转换
                val uri: Uri? = data?.data
                if (uri == null) {
                    invoke.reject("No save target selected")
                    return
                }
                // 目标文件名以用户确认的显示名为准：从选中 uri 解析，
                // 与 suggestedName 的路径逃逸防御独立（对话框已限定文件名）
                val args = invoke.parseArgs(SaveToDocumentArgs::class.java)
                val src = File(args.srcPath)
                try {
                    activity.contentResolver.openOutputStream(uri, "w")?.use { output ->
                        copyToOutputStream(src, output)
                    } ?: throw IOException("openOutputStream returned null for $uri")
                    android.util.Log.i(TAG, "saveToDocument ok: ${args.suggestedName} -> $uri")
                    invoke.resolve(JSObject().apply { put("ok", true) })
                } catch (e: Exception) {
                    // 拷贝失败：删除用户所选位置的半成品文档（spec M3「.part 语义 =
                    // createDocument + 拷贝 + 删除」），公共位置不残留残缺文件
                    try {
                        activity.contentResolver.delete(uri, null, null)
                    } catch (delErr: Exception) {
                        android.util.Log.w(
                            TAG,
                            "saveToDocument cleanup failed: ${delErr.message}",
                        )
                    }
                    android.util.Log.e(TAG, "saveToDocument copy failed: ${e.message}")
                    invoke.reject("saveToDocument copy failed: ${e.message}")
                }
            }
            Activity.RESULT_CANCELED -> {
                // 用户放弃：调用方保留私有副本（回退语义，不视为错误）
                invoke.resolve(JSObject().apply { put("ok", false); put("cancelled", true) })
            }
            else -> invoke.reject("saveToDocument failed (resultCode=${result.resultCode})")
        }
    }

    // ==================== 授权有效性 ====================

    /// 检测树授权是否仍有效（用户回收授权后返回 false）
    @Command
    fun checkAuthorized(invoke: Invoke) {
        val args = invoke.parseArgs(CheckAuthorizedArgs::class.java)
        val authorized = if (args.treeUri.isEmpty()) {
            false
        } else {
            activity.contentResolver.persistedUriPermissions.any {
                it.uri.toString() == args.treeUri && it.isReadPermission
            }
        }
        invoke.resolve(JSObject().apply { put("authorized", authorized) })
    }

    // ==================== 复制实现 ====================

    /// 顺序流复制主循环：openFileDescriptor("r") → 512KB 缓冲写 cache 文件
    ///
    /// 取消/异常立即删除半成品并标记终态（残留清理语义，见类注释）。
    private fun runCopy(copyId: String, uri: Uri, destFile: File, handle: CopyHandle) {
        var input: InputStream? = null
        var output: OutputStream? = null
        try {
            val pfd = activity.contentResolver.openFileDescriptor(uri, "r")
                ?: throw IllegalStateException("openFileDescriptor returned null for $uri")
            input = FileInputStream(pfd.fileDescriptor)
            output = FileOutputStream(destFile)
            val buffer = ByteArray(COPY_BUFFER_BYTES)
            while (!handle.cancelled.get()) {
                val n = input.read(buffer)
                if (n < 0) break
                output.write(buffer, 0, n)
                handle.done.addAndGet(n.toLong())
            }
            if (handle.cancelled.get()) {
                // 用户取消：删除半成品后结束（不保留 .part 语义，顺序流无续传）
                output.close()
                output = null
                if (destFile.exists()) destFile.delete()
            } else {
                output.flush()
            }
        } catch (e: Exception) {
            android.util.Log.e(TAG, "copy $copyId failed: ${e.message}")
            // 失败：删除半成品（残留清理），错误原因透传给前端
            try {
                if (destFile.exists()) destFile.delete()
            } catch (cleanup: Exception) {
                android.util.Log.w(TAG, "copy $copyId cleanup failed: ${cleanup.message}")
            }
            handle.error = e.message
        } finally {
            try {
                input?.close()
            } catch (_: Exception) {
            }
            try {
                output?.close()
            } catch (_: Exception) {
            }
            handle.finished = true
            // 终态句柄保留在表中供轮询方取最终状态（小文件可能在首次轮询前
            // 就已复制完成；过早移除会导致 copyProgress 报 unknown copyId，
            // 把成功误判为失败）。已终态句柄在下次 safToCache 时统一清扫
            // （见 sweepFinishedCopies），表大小有界
        }
    }

    /// 关闭流句柄（幂等：关闭 input 与底层 fd）
    ///
    /// 任务终态（完成/失败/取消）后由宿主调用；任务内恢复路径不调用——
    /// fd 保留使顺序续读成为可能（spec M3 续传策略）。
    private fun closeStream(handle: StreamHandle) {
        try {
            handle.input.close()
        } catch (e: Exception) {
            android.util.Log.w(TAG, "closeStream input close failed: ${e.message}")
        }
        try {
            handle.pfd.close()
        } catch (e: Exception) {
            android.util.Log.w(TAG, "closeStream pfd close failed: ${e.message}")
        }
    }

    /// 本地文件 → SAF 目标的顺序流拷贝（512KB 缓冲，写完即达）
    private fun copyToOutputStream(src: File, output: OutputStream) {
        FileInputStream(src).use { input ->
            val buffer = ByteArray(COPY_BUFFER_BYTES)
            while (true) {
                val n = input.read(buffer)
                if (n < 0) break
                output.write(buffer, 0, n)
            }
            output.flush()
        }
    }

    /// 清扫超时/超限的流句柄（safOpen 时调用，防 fd 泄漏）
    ///
    /// 任务终态后句柄可能残留（宿主仅在完成时 close；取消/失败保留供
    /// 任务内恢复），空闲超时后在此回收。超限时淘汰最久未用的句柄。
    private fun sweepStaleStreams() {
        val now = System.currentTimeMillis()
        val expired = streams.entries.filter { now - it.value.lastUsed > STREAM_IDLE_TIMEOUT_MS }
        for ((_, handle) in expired) {
            closeStream(handle)
            streams.remove(handle.uri.toString())
        }
        if (streams.size > MAX_STREAMS) {
            // 淘汰最久未用（按 lastUsed 升序取多余部分）
            val excess = streams.size - MAX_STREAMS
            streams.entries
                .sortedBy { it.value.lastUsed }
                .take(excess)
                .forEach { (_, handle) ->
                    closeStream(handle)
                    streams.remove(handle.uri.toString())
                }
        }
    }

    /// staging 目录（app 私有 cache/bedcode_uploads，懒创建）
    private fun stagingDir(): File {
        val dir = File(activity.cacheDir, STAGING_DIR)
        if (!dir.exists()) dir.mkdirs()
        return dir
    }

    /// 目标文件重名消解：追加 -1/-2 序号（保留扩展名），返回实际文件
    ///
    /// createNewFile() 原子占位（存在即返回 false）消除 TOCTOU：并发同名
    /// safToCache 不会选到同一目标文件（前端单 preparing 串行实际难触发，
    /// 纵深防御）。
    private fun uniqueDestFile(staging: File, destName: String): File {
        var candidate = File(staging, destName)
        if (candidate.createNewFile()) return candidate
        val dot = destName.lastIndexOf('.')
        val base = if (dot > 0) destName.substring(0, dot) else destName
        val ext = if (dot > 0) destName.substring(dot) else ""
        var seq = 1
        // 上限防文件系统异常时死循环；正常路径序号远到不了上限
        while (seq < 10000) {
            candidate = File(staging, "$base-$seq$ext")
            if (candidate.createNewFile()) return candidate
            seq++
        }
        throw IOException("cannot allocate unique dest file for $destName")
    }

    /// 清扫已终态的复制句柄（表大小有界；终态结果已可被轮询读取后不再需要）
    private fun sweepFinishedCopies() {
        val expired = copies.entries.filter { it.value.finished }.map { it.key }
        for (copyId in expired) {
            copies.remove(copyId)
        }
    }
}
