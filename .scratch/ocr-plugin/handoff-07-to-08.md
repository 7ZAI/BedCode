# 交接：票据 07 完成 → 票据 08 开工（2026-08-16）

> 新会话入口：先读本文件 + `.scratch/ocr-plugin/issues/08-Kotlin桥取图与解码.md` + `.scratch/ocr-plugin/spec.md` §4.4/§5，再开工。
> 07 的实现细节与遗留见 `07-识别流水线.md` Comments（新会话不必重读 07 代码，除非 08 需要）。

## 1. 当前状态

- **票据 07（识别流水线）已完成**：PP-OCRv4 det/cls/rec 三段 + 惰性加载常驻 + 单飞队列。
  - Windows `cargo test`：359 全过（ocr 新增 58 测试，含 Python cv2 参考 golden 对比）
  - Android 编译：`cargo build --target aarch64-linux-android --lib` 通过（libbedcode_lib.so 175MB debug）
  - 真机推理验证留 09/10（无在线 adb 设备）
- **票据 08 待开工**：Kotlin 桥取图与解码（相册 SAF pickImage + 拍照 CameraPlugin + BitmapFactory 解码降采样 → RGBA8 临时文件）。票据状态 ready-for-agent，无 Blocked by 未决项。

## 2. 工作区改动归属（⚠️ 提交前必读）

**本会话（07）产生的改动**——新会话可提交（建议作为 07 的 commit）：
```
M bedcode-mobile/src-tauri/Cargo.toml                     # android target 补 ndarray = "0.17"
M bedcode-mobile/src-tauri/src/ocr/engine.rs              # route 带上下文 / spawn_blocking / probe
M bedcode-mobile/src-tauri/src/ocr/ppocr.rs               # 引擎主体重写
M bedcode-mobile/src-tauri/src/ocr/preprocess.rs          # +to_rgb()
M bedcode-mobile/src-tauri/src/plugin/commands.rs         # delete_models 先 reset_resident
?? bedcode-mobile/src-tauri/src/ocr/ppocr/                # dict/geom/imgops/pipeline 四模块
?? bedcode-mobile/src-tauri/testdata/synth_pmap.bin       # det 后处理 golden 测试数据
M .scratch/ocr-plugin/issues/07-识别流水线.md             # 完成勾选 + 交接 Comments
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
（看起来有人在重构 desktop pty/server；若与 08 无关则全程无视）

## 3. 票据 08 开工要点

**实现依据**：spec.md §4.4（解码链路）+ §5.1（SafPickerPlugin.pickImage）+ §5.2（CameraPlugin）+ §5.3（注册模式）。

**现状速查**：
- 宿主命令 `plugin_pick_image` / `plugin_camera_capture` **尚未创建**（08 新建，`require_ocr` 门控，仿 `plugin_ocr_*` 模式，commands.rs OCR 段）
- `android_plugins/picker.rs`：现有 `saf_picker_plugin()`（Builder 名 `"saf-picker"`）→ Kotlin `SafPickerPlugin`（pickFile 硬编码 `*/*`）。**§5.3 陷阱**：register_android_plugin 以 Builder 名做 key，同名互相覆盖——pickImage 必须用独立 Builder 名（新插件 `ocr-picker` 或扩 `saf-picker` 类但注意注册名唯一）
- Kotlin 插件清单（gen/android 重建后须恢复）：SafPickerPlugin.kt / SafTransferPlugin.kt / CameraPlugin.kt（08 新建）/ OcrModelExtractorPlugin.kt 等，恢复清单在 AGENTS.md「Android」节
- AndroidManifest 现状：**CAMERA 权限已存在**（第 19 行，其他功能已用）；**FileProvider 声明与 file_paths.xml 待 08 加**（检查是否已有 provider——grep 结果无 provider）
- 解码链路：`contentResolver.openInputStream` → BitmapFactory → inSampleSize 步进降采样长边 ≤1600 → `cache/ocr/ocr_<ts>.rgba`（纯 RGBA8，无头）+ 返回 `{path, width, height}`；识别完成后宿主删临时文件（07 的 recognize 不删，删除归 08 或 09 前端？spec §4.4 说宿主删除）
- RGBA 文件格式已被 07 消费：`preprocess::RgbaImage::load_from_file(path, width, height)`（字节数 = w*h*4 严格校验、像素数 ≤40MP 兜底、长边 >maxSide 服务端再降采样）——**Kotlin 侧降采样到 1600 与 Rust 侧 max_side 默认 1600 是两段式设计，保持即可**

**验证**：
- `cd bedcode-mobile/src-tauri/gen/android && ./gradlew :app:compileUniversalDebugKotlin`（Kotlin 改动必跑；AGENTS.md 规范）
- 前端无改动则无需 npm test；Rust 无改动则无需 cargo test（若动了 commands.rs 则跑 `cargo test`）
- 真机相册/拍照验证留 10（无在线设备）

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
- **Python 参考验证工具链**（Windows 可用）：`.scratch/ocr-plugin/ref/` 存有 RapidOCR v2.x 源码、v4 配置（v13_*.yaml）、`ppocr_keys_v1.txt`、`ref_pipeline.py`（真实三模型跑通合成样张）、golden json；pip 已装 onnxruntime/opencv/pyclipper/shapely/pillow。08 若需验证 RGBA 输出可直接用 Python 读文件比对。
- 模型元数据（07 已钉死，08 不涉及）：det 736/min/ImageNet(BGR)；rec 6625 类动态宽；cls 48x192；输出 sigmoid/softmax 已内嵌。

## 5. 08 交付物清单（票据验收标准）

- [ ] SafPickerPlugin 扩展 pickImage（或独立插件，Builder 名唯一）→ 宿主 `plugin_pick_image`（require_ocr）
- [ ] CameraPlugin（ACTION_IMAGE_CAPTURE + FileProvider + CAMERA 运行时权限请求 + 拒绝明确错误）→ 宿主 `plugin_camera_capture`
- [ ] AndroidManifest：FileProvider 声明 + res/xml/file_paths.xml（cache 路径）；同步进重建恢复清单
- [ ] 统一解码：openInputStream → BitmapFactory → 长边 ≤1600（inSampleSize 步进）→ ocr/ocr_<ts>.rgba + {path,width,height}
- [ ] gradlew compileUniversalDebugKotlin 通过
- [ ] 真机取图验证（JPEG/PNG/WebP/HEIC）留 10
