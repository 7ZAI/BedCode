package com.bedcode.mobile

import android.app.Activity
import android.app.Application
import android.content.Intent
import android.net.Uri
import android.os.Bundle
import android.os.Environment
import android.provider.DocumentsContract
import androidx.activity.ComponentActivity
import androidx.activity.result.ActivityResult
import androidx.activity.result.ActivityResultLauncher
import androidx.activity.result.contract.ActivityResultContracts
import app.tauri.annotation.Command
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.JSObject
import app.tauri.plugin.Invoke
import app.tauri.plugin.Plugin

/**
 * Tauri 插件 - SAF 文件/目录选择器
 *
 * 用 Android Storage Access Framework 弹系统选择器（免任何存储权限）：
 * - pickDirectory：ACTION_OPEN_DOCUMENT_TREE，选共享目录（挂载根）
 * - pickFile：ACTION_OPEN_DOCUMENT，选本地文件（上传用）
 *
 * 选择后调用 takePersistableUriPermission 持久化授权（重启仍有效），
 * 并把 SAF Uri 拆成 (authority, documentId, primaryDir) 返回；
 * 真实路径解析（externalstorage/downloads provider → /storage/...）由 Rust 侧
 * saf_path 模块完成（可单测），Kotlin 侧只负责选择器与 Uri 元数据提取。
 *
 * 文件选择额外尝试 _data 列直读真实路径（Downloads/Media 等 provider 有效），
 * 命中则优先返回，Rust 侧解析失败时以此兜底。
 *
 * 由 Rust 端 android_plugins.rs 注册（SafPickerPlugin）。
 * gen/android 重建恢复清单：本文件须恢复（参见 AGENTS.md「Android」节）。
 *
 * 为什么不用 Tauri 的 Plugin.startActivityForResult()：
 * Tauri 2.11.1 的 PluginManager.onActivityCreate() 只在首次 onCreate 注册
 * startActivityForResultLauncher（`if (::activity.isInitialized) return`，
 * 源码留有 `// TODO: on destroy, we should change to a different activity`），
 * 且 Plugin 构造注入的 activity 引用不随宿主 Activity 重建更新。宿主 Activity
 * 被系统销毁重建（后台被杀恢复、SAF 选择器弹出后内存压力回收等）后，
 * launcher 绑定旧 registry 失效，launch() 抛
 * "Attempting to launch an unregistered ActivityResultLauncher"，选择器必然失败。
 * tauri-plugin-dialog 等成熟插件同样走 startActivityForResult，一样踩此坑。
 * 因此这里完全自持 launcher：每次调用用 Application 级 ActivityLifecycleCallbacks
 * 跟踪的**最新前台 Activity** 的 ActivityResultRegistry 幂等注册，永不缓存失效引用。
 */
@TauriPlugin
class SafPickerPlugin(private val activity: Activity) : Plugin(activity) {

    companion object {
        private const val TAG = "BedCode-SafPicker"
        private const val FLAG_READ_WRITE =
            Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION
        // launcher key 前缀：随机后缀避免 ActivityResultRegistry 重复注册冲突
        private const val KEY_PREFIX = "bedcode.saf"
    }

    /// 跟踪最新前台 Activity（绕开 Tauri PluginManager 对 activity 重建的缺陷）
    ///
    /// 必须在构造期注册到 Application 才能接收 onActivityResumed 等回调；
    /// 否则跟踪器等于死代码，current 永远停在构造时的 activity，宿主
    /// Activity 一旦被系统重建（SAF 选择器弹出后内存压力回收即触发），
    /// require() 仍返回已销毁的旧实例，launcher 绑定旧 registry 失效，
    /// launch() 必抛 "Attempting to launch an unregistered
    /// ActivityResultLauncher" —— 选择器必然失败，根因即此处未注册。
    private val currentActivity = CurrentActivityTracker(activity)

    init {
        // Application 是进程级单例，注册后随任意 Activity 的 resume/destroy
        // 持续更新 current；plugin 实例与进程同生命周期（见下注释），不注销
        (activity.application as? Application)?.registerActivityLifecycleCallbacks(currentActivity)
    }

    // ==================== 目录选择（共享根） ====================

    /// 弹系统目录树选择器
    @Command
    fun pickDirectory(invoke: Invoke) {
        try {
            val intent = Intent(Intent.ACTION_OPEN_DOCUMENT_TREE)
            intent.addFlags(
                Intent.FLAG_GRANT_READ_URI_PERMISSION or
                    Intent.FLAG_GRANT_WRITE_URI_PERMISSION or
                    Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION or
                    Intent.FLAG_GRANT_PREFIX_URI_PERMISSION,
            )
            launchForResult("$KEY_PREFIX.directory", intent) { result ->
                onDirectoryResult(invoke, result)
            }
        } catch (e: Exception) {
            android.util.Log.e(TAG, "pickDirectory launch failed: ${e.message}")
            invoke.reject("Failed to launch directory picker: ${e.message}")
        }
    }

    /// 目录树选择回调
    private fun onDirectoryResult(invoke: Invoke, result: ActivityResult) {
        when (result.resultCode) {
            Activity.RESULT_OK -> {
                val data = result.data
                // Intent.data 为 Java 平台类型，需显式标注 Uri? 才能经空检查智能转换
                val uri: Uri? = data?.data
                if (uri == null) {
                    invoke.reject("No directory selected")
                    return
                }
                persistPermission(uri)
                invoke.resolve(
                    baseResult(uri).apply {
                        put("documentId", DocumentsContract.getTreeDocumentId(uri))
                    },
                )
            }
            Activity.RESULT_CANCELED -> invoke.resolve(JSObject().apply { put("cancelled", true) })
            else -> invoke.reject("Directory picker failed (resultCode=${result.resultCode})")
        }
    }

    // ==================== 文件选择（上传） ====================

    /// 弹系统文件选择器
    @Command
    fun pickFile(invoke: Invoke) {
        try {
            val intent = Intent(Intent.ACTION_OPEN_DOCUMENT)
            intent.addCategory(Intent.CATEGORY_OPENABLE)
            intent.type = "*/*"
            launchForResult("$KEY_PREFIX.file", intent) { result ->
                onFileResult(invoke, result)
            }
        } catch (e: Exception) {
            android.util.Log.e(TAG, "pickFile launch failed: ${e.message}")
            invoke.reject("Failed to launch file picker: ${e.message}")
        }
    }

    /// 文件选择回调：优先 _data 列直读真实路径，否则回退交给 Rust 解析
    private fun onFileResult(invoke: Invoke, result: ActivityResult) {
        when (result.resultCode) {
            Activity.RESULT_OK -> {
                val data = result.data
                // Intent.data 为 Java 平台类型，需显式标注 Uri? 才能经空检查智能转换
                val uri: Uri? = data?.data
                if (uri == null) {
                    invoke.reject("No file selected")
                    return
                }
                persistPermission(uri)
                invoke.resolve(
                    baseResult(uri).apply {
                        put("documentId", DocumentsContract.getDocumentId(uri))
                        put("dataPath", queryDataPath(uri))
                    },
                )
            }
            Activity.RESULT_CANCELED -> invoke.resolve(JSObject().apply { put("cancelled", true) })
            else -> invoke.reject("File picker failed (resultCode=${result.resultCode})")
        }
    }

    // ==================== 工具 ====================

    /// 用当前前台 Activity 的 ActivityResultRegistry 注册 launcher 并 launch。
    /// 每次调用注册新 key（带纳秒时间戳）：ActivityResultRegistry.unregister 在
    /// 新版 androidx.activity 中为 internal，跨包不可调用，复用固定 key 重复
    /// register 会抛 IllegalArgumentException。随机 key 天然避碰，旧 key 随
    /// Activity 销毁（registry.unregisterAll）而清理，无实际泄漏。
    /// Activity 重建后新 registry 干净，launcher 永远绑定最新 Activity。
    /// 回调在 UI 线程分发，同一时刻只有一个 pending 选择（前端串行 await），
    /// 闭包捕获该次 invoke 是安全的。
    private fun launchForResult(baseKey: String, intent: Intent, onResult: (ActivityResult) -> Unit) {
        val host = this.host
        val registry = host.activityResultRegistry
        val key = "$baseKey.${System.nanoTime()}"
        // 必须使用带 LifecycleOwner 的 register 重载：androidx.activity 中
        // `register(key, contract, callback)` 无 lifecycle 版本会跳过
        // registerKey 的 requestCode 令牌分配（mKeyToRc[key] 不绑），
        // launch() 时 mKeyToRc.get(key) == null → 抛
        // "Attempting to launch an unregistered ActivityResultLauncher"。
        // host 是 ComponentActivity（LifecycleOwner），传给 lifecycle 版本的
        // register 时，若 currentState >= INITIALIZED（请求时刻 RESUMED）会同步
        // 分配 requestCode，launch() 即可正常发起、DocumentsUI 返回后回调即触发。
        // 随机 key 无需 unregister（internal 不可跨包调用），随 Activity 销毁
        // 的 registry.unregisterAll 自然回收，无实际泄漏。
        val launcher: ActivityResultLauncher<Intent> = registry.register(
            key,
            host,
            ActivityResultContracts.StartActivityForResult(),
        ) { result ->
            onResult(result)
        }
        launcher.launch(intent)
    }

    /// 当前宿主 Activity（跟踪器已保证为最新前台实例）
    private val host: ComponentActivity
        get() = currentActivity.require()

    /// 公共元数据：uri / authority / 显示名 / 主存储根（供 Rust 端路径解析）
    private fun baseResult(uri: Uri): JSObject {
        val displayName = try {
            host.contentResolver.query(
                uri,
                arrayOf(android.provider.OpenableColumns.DISPLAY_NAME),
                null,
                null,
                null,
            )?.use { c ->
                if (c.moveToFirst()) {
                    val idx = c.getColumnIndex(android.provider.OpenableColumns.DISPLAY_NAME)
                    if (idx >= 0) c.getString(idx) ?: "" else ""
                } else {
                    ""
                }
            } ?: ""
        } catch (e: Exception) {
            ""
        }
        return JSObject().apply {
            put("uri", uri.toString())
            put("authority", uri.authority ?: "")
            put("displayName", displayName)
            put(
                "primaryDir",
                Environment.getExternalStorageDirectory().absolutePath,
            )
        }
    }

    /// 持久化 SAF 授权（provider 不支持持久化时静默降级为单次授权）
    private fun persistPermission(uri: Uri) {
        try {
            host.contentResolver.takePersistableUriPermission(uri, FLAG_READ_WRITE)
        } catch (e: SecurityException) {
            android.util.Log.w(TAG, "takePersistableUriPermission unavailable: ${e.message}")
        }
    }

    /// 查询 _data 列直读真实路径（Downloads/Media provider 有效；不可用返回空串）
    private fun queryDataPath(uri: Uri): String {
        return try {
            host.contentResolver.query(uri, arrayOf("_data"), null, null, null)?.use { c ->
                if (c.moveToFirst()) {
                    val idx = c.getColumnIndex("_data")
                    if (idx >= 0) c.getString(idx) ?: "" else ""
                } else {
                    ""
                }
            } ?: ""
        } catch (e: Exception) {
            ""
        }
    }
}

/**
 * Application 级 ActivityLifecycleCallbacks，跟踪最新前台 Activity。
 *
 * Tauri 的 PluginManager.activity 与 Plugin 构造注入的 activity 在宿主 Activity
 * 被系统销毁重建后仍指向旧实例（官方 TODO 未处理），用旧 registry 注册的 launcher
 * launch 必然失败。Application 对象是进程级单例，其 context 不随 Activity 重建失效，
 * 用它注册回调即可持续拿到最新前台 Activity。
 *
 * 不注销回调：plugin 实例与进程同生命周期（PluginManager.plugins 静态持有），
 * 注销反而会导致重建后的新 Activity 不再被跟踪。
 */
private class CurrentActivityTracker(initial: Activity) : Application.ActivityLifecycleCallbacks {

    @Volatile
    private var current: Activity = initial

    /// 取当前 Activity 并校验为 ComponentActivity（ActivityResultRegistry 宿主）
    fun require(): ComponentActivity =
        current as? ComponentActivity
            ?: throw IllegalStateException(
                "Current activity is not a ComponentActivity: ${current.javaClass.name}",
            )

    override fun onActivityResumed(activity: Activity) {
        // 仅记录可直接作为 ActivityResultRegistry 宿主的 ComponentActivity；
        // 系统选择器（DocumentsUI 等）短暂 resume 时不覆盖 current，MainActivity
        // 在选择器返回后会重新 resume 顶替回自己，current 始终指向可用宿主
        if (activity is ComponentActivity) current = activity
    }

    override fun onActivityPaused(activity: Activity) {}
    override fun onActivityCreated(activity: Activity, savedInstanceState: Bundle?) {}
    override fun onActivityStarted(activity: Activity) {}
    override fun onActivityStopped(activity: Activity) {}
    override fun onActivityDestroyed(activity: Activity) {}
    override fun onActivitySaveInstanceState(activity: Activity, outState: Bundle) {}
}
