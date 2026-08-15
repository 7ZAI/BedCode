# 08 — Kotlin 桥：相册选图 + 拍照 + 解码降采样

**What to build:** 手机取图两条入口：相册选图（SAF `image/*`，零权限）与相机拍照（`ACTION_IMAGE_CAPTURE` + FileProvider + CAMERA 运行时权限），统一 BitmapFactory 解码（JPEG/PNG/WebP/HEIC）→ 降采样长边 ≤1600px → RGBA8 临时文件，经宿主命令（`plugin_pick_image` / `plugin_camera_capture`，`require_ocr` 门控）暴露给前端。实现依据为 spec §4.4 + §5。用户可感知：OCR 插件主页点「相册选图」「拍照」后拿到 RGBA 路径可直接送识别；拒绝相机权限得到明确错误提示。

**Blocked by:** 05（宿主命令门控与注册模式在 05 建立）

**Status:** ready-for-agent

- [ ] 相册桥落地（spec §11 开放项 3 落实：SafPickerPlugin 扩展 `pickImage` 或独立新插件；注意 §5.3 Builder 名覆盖陷阱，同名会互相覆盖）
- [ ] 新 CameraPlugin：`ACTION_IMAGE_CAPTURE` + FileProvider 输出 URI（app cache）+ CAMERA 运行时权限请求；拒绝 → 明确错误，UI 可提示去设置
- [ ] AndroidManifest：CAMERA 权限 + FileProvider 声明 + `file_paths.xml`（cache 路径），改动同步进「gen/android 重建恢复清单」（AGENTS.md Android 节）
- [ ] 统一解码链路：两种图源经 `contentResolver.openInputStream` → BitmapFactory → 长边 ≤1600 降采样（inSampleSize 步进）→ `ocr/ocr_<ts>.rgba` 纯 RGBA8 文件 + 返回 `{path, width, height}`；识别完成后宿主清理临时文件
- [ ] 全程不经 WASM：RGBA 路径直接作为 `plugin_ocr_recognize` 的 `image.rgbaPath` 参数（spec §4.4 调用路径）
- [ ] `./gradlew :app:compileUniversalDebugKotlin` 通过；真机相册/拍照取图成功（JPEG/PNG/WebP/HEIC 各验一张）
