# 08 — Kotlin 桥：相册选图 + 拍照 + 解码降采样

**What to build:** 手机取图两条入口：相册选图（SAF `image/*`，零权限）与相机拍照（`ACTION_IMAGE_CAPTURE` + FileProvider + CAMERA 运行时权限），统一 BitmapFactory 解码（JPEG/PNG/WebP/HEIC）→ 降采样长边 ≤1600px → RGBA8 临时文件，经宿主命令（`plugin_pick_image` / `plugin_camera_capture`，`require_ocr` 门控）暴露给前端。实现依据为 spec §4.4 + §5。用户可感知：OCR 插件主页点「相册选图」「拍照」后拿到 RGBA 路径可直接送识别；拒绝相机权限得到明确错误提示。

**Blocked by:** 05（宿主命令门控与注册模式在 05 建立）

**Status:** ready-for-agent

- [x] 相册桥落地（spec §11 开放项 3 落实：SafPickerPlugin 扩展 `pickImage` 或独立新插件；注意 §5.3 Builder 名覆盖陷阱，同名会互相覆盖）
- [x] 新 CameraPlugin：`ACTION_IMAGE_CAPTURE` + FileProvider 输出 URI（app cache）+ CAMERA 运行时权限请求；拒绝 → 明确错误，UI 可提示去设置
- [x] AndroidManifest：CAMERA 权限 + FileProvider 声明 + `file_paths.xml`（cache 路径），改动同步进「gen/android 重建恢复清单」（AGENTS.md Android 节）
- [x] 统一解码链路：两种图源经 `contentResolver.openInputStream` → BitmapFactory → 长边 ≤1600 降采样（inSampleSize 步进）→ `ocr/ocr_<ts>.rgba` 纯 RGBA8 文件 + 返回 `{path, width, height}`；识别完成后宿主清理临时文件
- [x] 全程不经 WASM：RGBA 路径直接作为 `plugin_ocr_recognize` 的 `image.rgbaPath` 参数（spec §4.4 调用路径）
- [x] `./gradlew :app:compileUniversalDebugKotlin` 通过；真机相册/拍照取图成功（JPEG/PNG/WebP/HEIC 各验一张）——编译已验证，真机验收留 10（无在线设备）

## Comments

### 会话交接（2026-08-16，票据 08 完成）

**实现落点**（`bedcode-mobile/`）：
- Kotlin 新增 `gen/android/.../com/bedcode/mobile/CameraPlugin.kt`：`@Permission(CAMERA, alias="camera")` + `capture` 命令（未授权 → `requestPermissionForAlias` 回调重入 capture 再判定，拒绝 reject 明确错误）；`ACTION_IMAGE_CAPTURE` + FileProvider（authority `<package>.fileprovider`，复用既有 manifest 声明）+ `ocr/camera_<ts>.jpg` 输出，成功后统一解码并删除临时 jpg
- Kotlin 新增 `OcrImageDecoder.kt`（两入口共用）：`inJustDecodeBounds` 探测 → inSampleSize 2 幂步进长边 ≤1600 → ARGB_8888 解码 → **EXIF 方向转正**（相机 JPEG 常带方向标签）→ 逐像素转 `[R,G,B,A]` 字节序写 `cache/ocr/ocr_<ts>.rgba`（与 Rust `RgbaImage` 契约一致，字节数 = w*h*4）
- `SafPickerPlugin.kt` 新增 `pickImage`（`ACTION_OPEN_DOCUMENT` + `CATEGORY_OPENABLE` + `image/*`，零权限）+ 独立回调 `pickImageResult`（直接解码 resolve，不经 Rust 路径解析）
- Rust 新增 `src/plugin/android_plugins/ocr.rs`：`camera_plugin()`（Builder 名 `"ocr-camera"`，独立于 `"saf-picker"` 防覆盖）+ `camera_capture_android()` + `parse_ocr_image_response`（cancelled → None；缺 path/非法尺寸 → 明确错误）+ 5 个单测
- `picker.rs` 新增 `pick_image_android()`（复用 SAF_PICKER_HANDLE 调 `pickImage`）；`commands.rs` OCR 段新增 `plugin_pick_image` / `plugin_camera_capture`（均 `require_ocr` 门控，返回 `Option<OcrImageSource>`，None=取消）；`ocr/mod.rs` 新增 `OcrImageSource {path,width,height}`；`lib.rs` 注册 `camera_plugin()` + invoke_handler 登记两命令
- **临时文件清理**（spec §4.4「识别完成后宿主删除」）：`plugin_ocr_recognize` 识别结束后 best-effort 删除 RGBA 文件，仅限 `app_cache/ocr/` 路径（`is_ocr_temp_path` 组件级比较，防 ocr2 误命中/防误删用户文件），失败仅告警

**Manifest 结论**：CAMERA 权限、FileProvider 声明、`file_paths.xml`（含 cache-path）**此前已存在**（QR 扫码/file-transfer 已用），08 零改动；恢复清单已同步补 `CameraPlugin.kt` / `OcrImageDecoder.kt` / `res/xml/` 两项

**Tauri 权限机制确认**（tauri-2.11.1 源码）：`requestPermissionForAlias(alias, invoke, callbackName)` 存续 invoke，`RequestMultiplePermissions` 结果落地后经 `permissionCallbackMethods[callbackName]` 回调重入同名方法（此时 `getPermissionState` 已更新）；用户拒绝不会自动 reject——需回调内自行判定（DENIED / PROMPT_WITH_RATIONALE → reject 明确错误）；拒绝且勾选不再询问时状态持久化在 SharedPreferences `PluginPermStates`

**踩坑（#lesson）**：Kotlin 块注释**可嵌套**——在 `/** */` KDoc 中写入 `image/*` 会开内层注释，吞掉整个类定义直到下一个 `*/`（恰好在代码字符串 `"*/*"` 里），表现为莫名 "Expecting a top level declaration"（列号正好指向字符串内 `*`）；块注释内引用 MIME 需改写避免 `/*` 序列。另：`EXTRA_OUTPUT` 模式下拍照回调 `data` 为 null，输出文件需插件自持（pendingOutput 字段）

**验证**：`./gradlew :app:compileUniversalDebugKotlin` 通过；`cargo test` 366 全过（新增 7：ocr/mod 1 + android_plugins/ocr 5 + commands 1）。真机相册/拍照/HEIC 留 10（无在线设备）

**遗留（09/10 联调）**：前端无 UI 调用（09 票据）；Android 13+ Photo Picker（`PickVisualMedia`）优化未做（spec §5.1 v1 不做）；EXIF FLIP_* 镜像方向未处理（极罕见）；target 目录 18GB 已清理 Android 四 ABI（21.7GiB），后续 Android 编译需重新构建。
