# 交接：票据 08 完成 → 票据 09 开工（2026-08-16）

> 新会话入口：先读本文件 + `.scratch/ocr-plugin/issues/09-插件壳UI与API.md` + `.scratch/ocr-plugin/spec.md` §6，再开工。
> 08 的实现细节与遗留见 `08-Kotlin桥取图与解码.md` Comments（新会话不必重读 08 代码，除非 09 需要）。

## 1. 当前状态

- **票据 08（Kotlin 桥取图与解码）已完成**：相册 SAF pickImage + 拍照 CameraPlugin + 统一 BitmapFactory 解码降采样 → RGBA8 临时文件。
  - `./gradlew :app:compileUniversalDebugKotlin` 通过
  - `cargo test` 366 全过（新增 7 个单测）
  - 真机取图验证（JPEG/PNG/WebP/HEIC）留 10（无在线设备）
- **票据 09 待开工**：插件壳 UI 与 API（前端 TS + Rust WASM 壳 + context.ocr API + 宿主 SDK types 扩展）。票据状态 ready-for-agent。
- 票据 07（识别流水线）亦已完成，真机识别验证与 08 的真机取图一并留 10。

## 2. 工作区改动归属（⚠️ 提交前必读）

**本会话（08）产生的改动**——新会话可提交（建议作为 08 的 commit，含 AGENTS.md 恢复清单同步）：
```
M bedcode-mobile/src-tauri/src/ocr/mod.rs                          # +OcrImageSource（+1 测试）
M bedcode-mobile/src-tauri/src/plugin/android_plugins.rs           # mod ocr + re-export
?? bedcode-mobile/src-tauri/src/plugin/android_plugins/ocr.rs      # camera_plugin/camera_capture_android/parse_ocr_image_response（+5 测试）
M bedcode-mobile/src-tauri/src/plugin/android_plugins/picker.rs    # +pick_image_android()
M bedcode-mobile/src-tauri/src/plugin/commands.rs                  # +plugin_pick_image/plugin_camera_capture/识别后临时文件清理（+1 测试）
M bedcode-mobile/src-tauri/src/lib.rs                              # 注册 camera_plugin() + 2 命令入 invoke_handler
?? bedcode-mobile/src-tauri/gen/android/app/src/main/java/com/bedcode/mobile/CameraPlugin.kt
?? bedcode-mobile/src-tauri/gen/android/app/src/main/java/com/bedcode/mobile/OcrImageDecoder.kt
M bedcode-mobile/src-tauri/gen/android/app/src/main/java/com/bedcode/mobile/SafPickerPlugin.kt  # +pickImage/pickImageResult
M AGENTS.md                                                         # Android 恢复清单补 CameraPlugin/OcrImageDecoder/res-xml
M .scratch/ocr-plugin/issues/08-Kotlin桥取图与解码.md               # 完成勾选 + 交接 Comments
```

**其他会话/他人的并行在途改动——禁止触碰、禁止提交、禁止回滚**（git checkout 会覆盖丢失）：
```
bedcode-desktop/src-tauri/Cargo.toml
bedcode-desktop/src-tauri/src/pty/command.rs
bedcode-desktop/src-tauri/src/pty/frontend_output_handler.rs
bedcode-desktop/src-tauri/src/pty/pty_handler.rs
bedcode-desktop/src-tauri/src/pty/pty_output.rs
bedcode-desktop/src-tauri/src/pty/pty_process.rs
bedcode-desktop/src-tauri/src/pty/pty_reader.rs
bedcode-desktop/src-tauri/src/server/client_info.rs
bedcode-desktop/src-tauri/src/server/metrics.rs
bedcode-desktop/src-tauri/src/server/middleware/jwt_auth.rs
bedcode-desktop/packages/plugin-sdk-test/Cargo.lock
```
（desktop pty/server 重构在途；与 09 无关则全程无视）

## 3. 票据 09 开工要点

**实现依据**：spec.md §6（插件壳形态/UI 流程/数据契约/i18n）。

**现状速查**：
- 宿主命令已齐：`plugin_ocr_recognize` / `plugin_ocr_engine_status` / `plugin_ocr_delete_models` / `plugin_ocr_restore_models` / `plugin_pick_image` / `plugin_camera_capture`（均 `require_ocr` 门控，命令名前缀 `plugin_ocr_*`，其余为 `plugin_pick_image`/`plugin_camera_capture`）
- 数据契约（Rust 侧序列化已 camelCase，见 `src/ocr/mod.rs`）：`OcrRecognizeInput{engine?, image:{rgbaPath,width,height}, maxSide?}` → `OcrOutput{engine, durationMs, lines:[{text,confidence,bbox:{x,y,w,h}}]}`；`OcrEngineStatus{available, modelsPresent, modelsBytes, engineLoaded, supportedEngines}`；`OcrImageSource{path,width,height}`（取图返回，取消 = null）
- SDK types 扩展：需在移动端 SDK 的 `PluginContext` 增加 `ocr` 字段（`OcrAPI` 接口，参照 `fileService` 模式，spec §6.1）；插件清单 `permissions` 加 `"ocr"`（宿主权限，05 已建）
- 插件壳：`bedcode-plugin create com.bedcode.ocr "OCR"` 模板 → 前端 TS + Rust WASM 壳（`wasm_entry!` 宏 + 激活/停用日志，**不实现识别命令路由**）；参照 `plugins/file-transfer/` 结构
- 样式：UI 改动前必须加载 `frontend-styles` skill；禁止原生控件外观；i18n key 同时进 zh-CN/en
- 临时 RGBA 文件已由宿主在识别后自动清理（08 实现），前端无需管；但用户选了图不识别会残留（cache 可被系统回收，v1 可接受）

**验证**：
- 前端测试 `npm run test:run`（vitest run，禁止 npm run test）；Rust WASM 壳改动跑 `cargo test`
- 移动端宿主 Rust 若改动 → `cargo test`（注意：08 会话已 `cargo clean` Android 四 ABI 释放 21.7GiB，Android 编译需按 §4 的 NDK env 重新构建，较慢）
- 真机 UI 验证留 10

## 4. 工具链备忘

- **NDK env（Android 编译）**——`ANDROID_NDK_HOME` 必须指向 prebuilt 目录（r30 无顶层 sysroot/bin clang）：
  ```bash
  export PRE="$LOCALAPPDATA/Android/Sdk/ndk/30.0.14904198/toolchains/llvm/prebuilt/windows-x86_64"
  export ANDROID_NDK_HOME="$PRE" && export ANDROID_NDK_ROOT="$PRE"
  export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$PRE/bin/aarch64-linux-android24-clang.cmd"
  export CARGO_TARGET_AARCH64_LINUX_ANDROID_AR="$PRE/bin/llvm-ar.cmd"
  export CC_aarch64_linux_android="$PRE/bin/aarch64-linux-android24-clang.cmd"
  export CFLAGS_aarch64_linux_android="--sysroot=$PRE/sysroot"
  RUST_MIN_STACK=67108864 cargo build --package bedcode-mobile --target aarch64-linux-android --lib
  ```
- **磁盘告警**：D 盘曾 100% 满（target 18GB），08 已清理 Android 四 ABI（21.7GiB）；编译前检查 `src-tauri/target` 大小，>15GB 按 AGENTS.md 执行 clean
- **Python 参考验证工具链**（Windows 可用）：`.scratch/ocr-plugin/ref/` 存有 RapidOCR 参考源码与 golden；pip 已装 onnxruntime/opencv/pyclipper/shapely/pillow。09 若需验证 RGBA 输出可直接用 Python 读文件比对

## 5. 09 交付物清单（票据验收标准，来自 09 票据）

- [ ] 插件壳：`plugins/ocr/` 前端 TS + Rust WASM 壳（`bedcode-plugin create` 模板，参照 file-transfer 结构）
- [ ] 清单：`permissions: ["ui:toolbox", "ocr"]`（+路由/设置页需要时补 `ui:route`/`ui:settings`/`ui:back`）；contributes 工具箱入口/设置/结果路由
- [ ] SDK types：`OcrAPI` 接口（recognize/engineStatus/deleteModels/restoreModels + pickImage/cameraCapture）+ `PluginContext.ocr`
- [ ] UI：主页「相册选图」「拍照」+ 引擎状态条；loading 防重入；结果页（行列表点击复制 + 复制全文 + 低置信度弱化 + 空态）；设置页模型管理
- [ ] i18n zh-CN/en 同步
- [ ] `npm run test:run` 通过；Kotlin 未改不跑 gradlew；Rust 壳改动跑 cargo test
