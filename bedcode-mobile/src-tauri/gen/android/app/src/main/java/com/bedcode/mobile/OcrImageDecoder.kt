package com.bedcode.mobile

import android.app.Activity
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Matrix
import android.media.ExifInterface
import android.net.Uri
import android.util.Log
import java.io.File
import java.io.FileOutputStream
import java.io.IOException

/**
 * 取图统一解码（spec §4.4）：BitmapFactory 解码 → 长边 ≤1600 降采样 → EXIF 旋转
 * → 纯 RGBA8 字节流 → cache/ocr/ocr_<ts>.rgba
 *
 * 输出约定（与 Rust preprocess::RgbaImage 完全一致）：无文件头、行优先、每像素
 * [R,G,B,A] 各 1 字节；返回宽高为降采样 + 旋转后的实际尺寸。
 * JPEG/PNG/WebP/HEIC 均由 BitmapFactory 原生支持（HEIC 需 Android 9+ 硬件能力），
 * 避免 Rust 侧 libheif 依赖。
 *
 * 相册选图（SafPickerPlugin.pickImage）与拍照（CameraPlugin）共用本解码器，
 * 图源经 contentResolver.openInputStream 统一读取（SAF / FileProvider URI 皆可）。
 *
 * gen/android 重建恢复清单：本文件须恢复（参见 AGENTS.md「Android」节）。
 */
object OcrImageDecoder {

    private const val TAG = "OcrImageDecoder"

    /** 降采样长边上限（与 Rust 侧 max_side 默认 1600 一致，两段式设计见 spec §4.4） */
    const val MAX_LONG_SIDE = 1600

    /** 解码产物：RGBA8 文件绝对路径 + 实际尺寸（识别坐标系） */
    data class Result(val path: String, val width: Int, val height: Int)

    /**
     * 从 content/file URI 解码并降采样为 RGBA8 临时文件。
     *
     * @throws IOException 打开/解码/写入失败（消息带可读上下文，供 invoke.reject 透传）
     */
    fun decode(activity: Activity, uri: Uri): Result {
        // 1. 边界探测（inJustDecodeBounds 不分配像素；该流随后被消费）
        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        activity.contentResolver.openInputStream(uri).use { ins ->
            if (ins == null) throw IOException("cannot open input stream for $uri")
            BitmapFactory.decodeStream(ins, null, bounds)
        }
        if (bounds.outWidth <= 0 || bounds.outHeight <= 0) {
            throw IOException(
                "unsupported or corrupted image (${bounds.outWidth}x${bounds.outHeight})",
            )
        }

        // 2. inSampleSize 2 幂步进降采样：长边 ≤ MAX_LONG_SIDE（12MP 照片 ≈ 降 3 级）
        var sample = 1
        while (bounds.outWidth / sample > MAX_LONG_SIDE || bounds.outHeight / sample > MAX_LONG_SIDE) {
            sample *= 2
        }

        // 3. 真实解码（ARGB_8888；重新开流，bounds 解码已消费前一个流）
        val options = BitmapFactory.Options().apply {
            inSampleSize = sample
            inPreferredConfig = Bitmap.Config.ARGB_8888
        }
        val decoded = activity.contentResolver.openInputStream(uri).use { ins ->
            BitmapFactory.decodeStream(ins, null, options)
        } ?: throw IOException("failed to decode image $uri")

        // 4. EXIF 方向转正（相机 JPEG 与部分 HEIC 常带方向标签，BitmapFactory 不自动处理）
        val bitmap = applyExifRotation(activity, uri, decoded)

        // 5. 写入纯 RGBA8 字节流（getPixels 为 ARGB int，逐像素转 [R,G,B,A] 字节序）
        val w = bitmap.width
        val h = bitmap.height
        val pixels = IntArray(w * h)
        bitmap.getPixels(pixels, 0, w, 0, 0, w, h)
        val bytes = ByteArray(w * h * 4)
        var i = 0
        for (p in pixels) {
            bytes[i++] = (p shr 16 and 0xFF).toByte() // R
            bytes[i++] = (p shr 8 and 0xFF).toByte()  // G
            bytes[i++] = (p and 0xFF).toByte()        // B
            bytes[i++] = (p shr 24 and 0xFF).toByte() // A
        }
        val ocrDir = File(activity.cacheDir, "ocr")
        if (!ocrDir.exists() && !ocrDir.mkdirs()) {
            throw IOException("cannot create cache dir ${ocrDir.absolutePath}")
        }
        val out = File(ocrDir, "ocr_${System.currentTimeMillis()}.rgba")
        FileOutputStream(out).use { it.write(bytes) }
        if (bitmap !== decoded) decoded.recycle()
        bitmap.recycle()
        Log.i(TAG, "decoded $uri -> ${out.absolutePath} ${w}x$h (sample=$sample)")
        return Result(out.absolutePath, w, h)
    }

    /// EXIF 方向转正；无方向标签（PNG/WebP 多数）或读取失败时原样返回
    private fun applyExifRotation(activity: Activity, uri: Uri, bitmap: Bitmap): Bitmap {
        val degrees = try {
            activity.contentResolver.openInputStream(uri).use { ins ->
                if (ins == null) {
                    0
                } else {
                    val exif = ExifInterface(ins)
                    when (exif.getAttributeInt(
                        ExifInterface.TAG_ORIENTATION,
                        ExifInterface.ORIENTATION_NORMAL,
                    )) {
                        ExifInterface.ORIENTATION_ROTATE_90 -> 90
                        ExifInterface.ORIENTATION_ROTATE_180 -> 180
                        ExifInterface.ORIENTATION_ROTATE_270 -> 270
                        // FLIP_* / TRANSPOSE 等镜像场景极罕见，v1 不处理（保持原图）
                        else -> 0
                    }
                }
            }
        } catch (e: Exception) {
            Log.w(TAG, "EXIF read failed (skip rotation): ${e.message}")
            0
        }
        if (degrees == 0) return bitmap
        val matrix = Matrix().apply { postRotate(degrees.toFloat()) }
        val rotated = Bitmap.createBitmap(bitmap, 0, 0, bitmap.width, bitmap.height, matrix, true)
        if (rotated !== bitmap) bitmap.recycle()
        return rotated
    }
}
