# 交接：票据 09 完成 → 票据 10 开工（2026-08-16）

> 新会话入口：先读本文件 + `.scratch/ocr-plugin/issues/10-端到端真机验证.md` + `.scratch/ocr-plugin/spec.md` §10（真机验证清单），再开工。
> 09 的实现细节与遗留见 `09-插件壳UI与API.md` Comments（新会话不必重读 09 代码，除非 10 需要）。

## 1. 当前状态

- **票据 08（Kotlin 桥取图与解码）已完成**：相册 SAF pickImage + 拍照 CameraPlugin + 统一解码降采样 → RGBA8。
- **票据 09（插件壳 UI 与 API）已完成**：可安装 OCR 插件（工具箱入口 → 主页 → 结果页 → 设置区模型管理）+ SDK `context.ocr` API + i18n 双语。
  - 插件 vitest 21/21、vue-tsc 0 error、构建全链通过、产物已部署 `src-tauri/resources/plugins/mobile/com.bedcode.ocr/`
  - dev-shell headless Chrome 冒烟全流程通过（截图 `.scratch/ocr-plugin/devshell-result.png`，脚本 `devshell-smoke.mjs` 可复用）
  - 宿主 vitest 179 通过；gradlew compileUniversalDebugKotlin 通过
- **票据 10 待开工**：端到端真机验证（spec §10 清单 9 项；需要在线 adb 设备——此前会话确认无在线设备，10 可能需等待设备或先做可做的部分）。

## 2. 工作区改动归属（⚠️ 提交前必读）

**09 会话（本次）产生的改动**——新会话可提交（建议 08+09 分两个 commit 或合并提交，含恢复清单同步）：
```
?? bedcode-mobile/plugins/ocr/                          # 全新插件工程（前端 + rust 壳 + i18n + 测试）
M bedcode-mobile/packages/plugin-sdk-mobile/src/types.ts            # +OcrApi 等 7 类型 + PluginContext.ocr + ocrLinesSeed
M bedcode-mobile/packages/plugin-sdk-mobile/src/index.ts            # 补类型导出
M bedcode-mobile/packages/plugin-sdk-mobile/dev-shell/src/mock-context.ts  # +ocr mock
M bedcode-mobile/src/plugin/commands.ts                 # +6 个 OCR invoke 封装
M bedcode-mobile/src/plugin/context.ts                  # +ocr API（requireOcrPermission 门控）
M bedcode-mobile/src/plugin/permission.ts               # ocr 权限映射 +pickImage/cameraCapture
M bedcode-mobile/src/locales/zh-CN/mobile.ts            # +noOcrPermission
M bedcode-mobile/src/locales/en/mobile.ts               # +noOcrPermission
M bedcode-mobile/src-tauri/gen/android/.../CameraPlugin.kt  # 权限回调改 @PermissionCallback（08 遗留 bug 修复）
?? bedcode-mobile/src-tauri/resources/plugins/mobile/com.bedcode.ocr/  # 构建产物（index.js+wasm+plugin.json）
M .scratch/ocr-plugin/issues/08-*.md / 09-*.md          # 完成勾选 + 交接 Comments
?? .scratch/ocr-plugin/devshell-smoke.mjs                # dev-shell 冒烟脚本（10 可复用）
```

**其他会话/他人的并行在途改动——禁止触碰、禁止提交、禁止回滚**（git checkout 会覆盖丢失）：
```
bedcode-desktop/src-tauri/Cargo.toml
bedcode-desktop/src-tauri/src/pty/*（command/frontend_output_handler/pty_handler/pty_output/pty_process/pty_reader）
bedcode-desktop/src-tauri/src/server/*（client_info/metrics/middleware/jwt_auth）
bedcode-desktop/packages/plugin-sdk-test/Cargo.lock
bedcode-desktop/src-tauri/tests/server_integration.rs（在途新增）
bedcode-desktop/src/__tests__/fixtures/（在途新增）
bedcode-desktop/src/composables/model.ts / useServer.ts 及 __tests__ 若干
```
（desktop pty/server 重构在途；与 10 无关则全程无视）

## 3. 票据 10 开工要点

**实现依据**：spec.md §10（真机验证清单 9 项）+ `10-端到端真机验证.md`。

**现状速查**：
- 全部代码就位：宿主 6 命令（plugin_ocr_* + plugin_pick_image + plugin_camera_capture，require_ocr 门控）；Kotlin 桥（SafPickerPlugin.pickImage + CameraPlugin + OcrImageDecoder）；插件 UI 四页 + context.ocr；模型打包 `resources/ocr_models/` 三 onnx（已解压标记 + OcrModelExtractorPlugin 惰性解压）
- 验证命令：`adb devices` 先确认设备；`npm run tauri:android:dev`（日志落盘用 `npm run tauri:android:dev:log`，路径 `bedcode-mobile/.dev-logs/android-dev.YYYY-MM-DD.log`）；真机 UI 走查按 spec §10 清单逐项
- **已知真机风险点**（10 重点盯）：
  1. 首次识别性能：模型解压（~17MB 三 onnx）+ 3 session 建图 ≤5s 目标；稳态 ≤3s（spec §8）；不达标调 ort with_intra_threads / 优化级别（07 遗留）
  2. 相机权限拒绝态：前端错误映射已就位（permissionDenied → 引导去设置），真机走查确认文案与行为
  3. HEIC/WebP 相册样张：BitmapFactory 原生支持（Android 9+ HEIC 硬件），真机各验一张
  4. 识别正确性：中/英/混排样张（07 遗留，模型推理首次真实验证）
  5. 单飞/常驻：连续两张识别第二次不加载模型；识别中再触发排队不崩（spec §10 第 6/7 项）
- dev-shell 冒烟脚本可先回归（无设备时证明前端无回归）：`node .scratch/ocr-plugin/devshell-smoke.mjs`（需先起 dev-shell：`cd bedcode-mobile/plugins/ocr && npx bedcode-plugin dev . --port 5174` + headless Chrome --remote-debugging-port=9222）

**验证**：
- 真机验证以 spec §10 清单为准，结果记录进 10 票据 Comments
- 无设备时：可做的部分（日志链路检查、Kotlin 编译、测试全量）做完后挂起，不硬造结论

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
- **磁盘告警**：D 盘曾 100% 满；08 会话已清 Android 四 ABI（21.7GiB），**09 会话期间磁盘再次紧张过（rust wasm 构建 + dev-shell 缓存）**——真机构建前检查 `df -h /d`，target >15GB 按 AGENTS.md 执行 clean（本次会话插件 rust target 已占用 ~500MB，可在 10 完成后清理）
- **日志**：真机链路排查看 `.dev-logs/android-dev.YYYY-MM-DD.log`（grep `OcrModelExtractor` / `CameraPlugin` / `OcrImageDecoder` / `ppocr` / `onnxruntime`）；桌面 dev 窗口看 `%LOCALAPPDATA%\com.bedcode.app\logs\runtime.*.log`（移动端进程在手机，桌面端日志仅桌面窗口场景）

## 5. 10 交付物清单（票据验收标准，来自 10 票据）

- [ ] spec §10 清单 9 项真机走查全部通过（首启首次识别 ≤5s、稳态 ≤3s；相册 JPEG/PNG/WebP/HEIC；拍照授权/拒绝两态；复制全文/行级/弱化/空态；模型删除→入口禁用→恢复；常驻；单飞；后台恢复；深浅色）
- [ ] 真机日志采集（模型解压耗时、识别耗时、错误路径）记录进票据
- [ ] 无设备时：dev-shell 冒烟回归 + 全量测试 + Kotlin 编译，挂起待设备

## 5. UI 审核优化已完成（handoff-ui-review.md 步骤 1-5 全闭环，2026-08-16）

**入口**：新会话直接进票据 10（真机验证）。`handoff-ui-review.md` 的步骤 1（vision 三组评审）、2（修复）、3（复截复审三组全闭环）、4（回归）、5（记录）已全部完成；截图脚本已增强至 12 张（+settings-confirm 删除确认弹窗、+settings-present-light 设置页浅色），可复跑。

**本轮新增改动**（UI 审核优化，均在 `bedcode-mobile/plugins/ocr/` 内，可并入 09 的提交）：
```
M plugins/ocr/src/components/OcrView.vue     # activeAction + 触发按钮 spinner + desc text-pretty
M plugins/ocr/src/components/ResultPage.vue  # 空态返回主页 CTA + 行卡片 active:scale/hover + chip token 字号 + meta tabular-nums
M plugins/ocr/src/components/ToolboxEntry.vue# 图标换 document-magnifying-glass svg + chip-violet + chevron
M plugins/ocr/src/components/OcrSettings.vue # （无改动，恢复按钮样式经 styles.css 生效）
M plugins/ocr/src/styles.css                 # .ocr-btn-accent 弱底→填充（对比 token）
M plugins/ocr/src/i18n/zh-CN.ts / en.ts      # ocr.result.backToHome 文案「返回主页」
M plugins/ocr/src/index.ts                   # manifest icon：emoji → SVG path d（禁 emoji）
M .scratch/ocr-plugin/screenshot-all.mjs     # +2 张截图
M .scratch/ocr-plugin/issues/09-*.md         # 「UI 审核优化」Comments
```
产物已重新部署 `src-tauri/resources/plugins/mobile/com.bedcode.ocr/`（index.js 33813B + wasm 350910B，`node scripts/plugin-build.js --plugin com.bedcode.ocr`，参数是 manifest.id）。

**票据 10 需真机确认的动态项**（静态截图无法验证）：行卡片 `active:scale-[0.98]` 按压反馈（查 touch-manipulation / tap-highlight 无闪白）；主页 loading spinner 动效；删除确认弹窗按钮语言跟随宿主；en 语言下弹窗与空态 CTA 文案。

**vision 判定不改的**（记录于 issue 09 Comments，10 无需处理）：状态条背景 token（单一 token 无不一致）、浅色 accent #1D1A14（宿主全局设计）、入口无实时角标（spec v1 如此）、空态 document 图标长 path（渲染正常）。
