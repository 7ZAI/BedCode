package com.bedcode.mobile

import android.Manifest
import android.app.Activity
import android.content.ActivityNotFoundException
import android.content.Intent
import android.net.Uri
import android.provider.MediaStore
import android.util.Log
import androidx.core.content.FileProvider
import app.tauri.PermissionState
import app.tauri.annotation.ActivityCallback
import app.tauri.annotation.Command
import app.tauri.annotation.Permission
import app.tauri.annotation.PermissionCallback
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.JSObject
import app.tauri.plugin.Invoke
import app.tauri.plugin.Plugin
import java.io.File

/**
 * 相机拍照（OCR 取图入口二，spec §5.2）
 *
 * ACTION_IMAGE_CAPTURE + FileProvider 输出 URI（app cache ocr/camera_<ts>.jpg；
 * authority = <package>.fileprovider，AndroidManifest 已声明且 file_paths.xml
 * 含 cache-path 覆盖）。CAMERA 运行时权限用 Tauri 内嵌声明（@TauriPlugin(permissions =
 * [@Permission(...)])，getPermissionState 只扫描该参数）：
 * 未授权时 requestPermissionForAlias 弹系统权限框，经 @PermissionCallback
 * 回调 onCameraPermission 判定；拒绝 → reject 明确错误（前端可提示去设置，
 * 不重复弹窗）。
 *
 * 拍照成功后与相册同一解码链路（OcrImageDecoder）→ RGBA8 临时文件，随后删除
 * 相机临时 jpg。EXTRA_OUTPUT 模式下回调 data 为 null，输出文件由本插件自持
 * （pendingOutput）。
 *
 * 由 Rust 端 android_plugins/ocr.rs 注册（Builder 名 "ocr-camera"——必须与
 * SafPickerPlugin 的 "saf-picker" 不同名：register_android_plugin 以 Builder
 * 名作 HashMap key，同名互相覆盖，见 android_plugins.rs 模块注释与 spec §5.3）。
 * gen/android 重建恢复清单：本文件须恢复（参见 AGENTS.md「Android」节）。
 */
// 权限必须以 @TauriPlugin(permissions = [...]) 内嵌声明，tauri 运行时只读
// @TauriPlugin 注解的 permissions 参数（getPermissionStates 遍历 annotation.permissions）；
// 类上独立标注的 @Permission 不会被扫描 → getPermissionState(alias) 返回 null、
// requestPermissionForAlias 静默失败（invoke 永不 resolve），写法参照 TaskNotificationPlugin。
@TauriPlugin(
    permissions = [
        Permission(
            strings = [Manifest.permission.CAMERA],
            alias = "camera",
        ),
    ],
)
class CameraPlugin(private val activity: Activity) : Plugin(activity) {

    companion object {
        private const val TAG = "BedCode-Camera"
        private const val PERMISSION_ALIAS = "camera"
    }

    /// 本次拍照输出文件（EXTRA_OUTPUT 模式下回调 data 为 null，需自持）
    private var pendingOutput: File? = null

    /**
     * 拍照命令：权限未授予 → 请求（系统弹窗后经 onCameraPermission 回调判定）；
     * 已授予 → 启动相机。拒绝在回调中终结（不会重复弹窗）。
     */
    @Command
    fun capture(invoke: Invoke) {
        val state = getPermissionState(PERMISSION_ALIAS)
        if (state != PermissionState.GRANTED) {
            Log.i(TAG, "capture: CAMERA state=$state, requesting permission")
            requestPermissionForAlias(PERMISSION_ALIAS, invoke, "onCameraPermission")
            return
        }
        launchCamera(invoke)
    }

    /// 权限请求回调（Tauri 框架在系统弹窗结果落地后以原 invoke 重入本方法）
    @PermissionCallback
    fun onCameraPermission(invoke: Invoke) {
        if (getPermissionState(PERMISSION_ALIAS) == PermissionState.GRANTED) {
            launchCamera(invoke)
        } else {
            Log.w(TAG, "capture: CAMERA permission denied")
            invoke.reject(
                "Camera permission denied; grant it in system settings and retry (state=" +
                    getPermissionState(PERMISSION_ALIAS) + ")",
            )
        }
    }

    private fun launchCamera(invoke: Invoke) {
        try {
            val ocrDir = File(activity.cacheDir, "ocr")
            if (!ocrDir.exists() && !ocrDir.mkdirs()) {
                throw IllegalStateException("cannot create ${ocrDir.absolutePath}")
            }
            val output = File(ocrDir, "camera_${System.currentTimeMillis()}.jpg")
            val uri: Uri = FileProvider.getUriForFile(
                activity,
                "${activity.packageName}.fileprovider",
                output,
            )
            pendingOutput = output
            val intent = Intent(MediaStore.ACTION_IMAGE_CAPTURE).apply {
                putExtra(MediaStore.EXTRA_OUTPUT, uri)
                addFlags(
                    Intent.FLAG_GRANT_WRITE_URI_PERMISSION or Intent.FLAG_GRANT_READ_URI_PERMISSION,
                )
            }
            startActivityForResult(invoke, intent, "captureResult")
        } catch (e: ActivityNotFoundException) {
            pendingOutput = null
            Log.e(TAG, "capture: no camera app", e)
            invoke.reject("No camera app available: ${e.message}")
        } catch (e: Exception) {
            pendingOutput = null
            Log.e(TAG, "capture launch failed", e)
            invoke.reject("Failed to launch camera: ${e.message}")
        }
    }

    /// 拍照回调：成功 → 统一解码链路（spec §4.4）→ RGBA8 临时文件；取消 → cancelled
    @ActivityCallback
    fun captureResult(invoke: Invoke, result: androidx.activity.result.ActivityResult) {
        val output = pendingOutput
        pendingOutput = null
        when (result.resultCode) {
            Activity.RESULT_OK -> {
                if (output == null || !output.exists()) {
                    invoke.reject("Camera returned no image (output file missing)")
                    return
                }
                try {
                    val img = OcrImageDecoder.decode(activity, Uri.fromFile(output))
                    if (!output.delete()) {
                        Log.w(TAG, "captureResult: failed to delete temp jpg ${output.absolutePath}")
                    }
                    invoke.resolve(
                        JSObject().apply {
                            put("path", img.path)
                            put("width", img.width)
                            put("height", img.height)
                        },
                    )
                } catch (e: Exception) {
                    Log.e(TAG, "captureResult: decode failed", e)
                    invoke.reject("Failed to decode captured image: ${e.message}")
                }
            }
            Activity.RESULT_CANCELED -> invoke.resolve(JSObject().apply { put("cancelled", true) })
            else -> invoke.reject("Camera capture failed (resultCode=${result.resultCode})")
        }
    }
}
