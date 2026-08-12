# 移动端 OCR 插件（宿主引擎 + WASM 壳）

Label: wayfinder:map

> ✅ **已到达目的地**：grilling 决策全部落定，最终规格见 **[spec.md](spec.md)**，可直接交给实现会话。

## Destination

一份可直接交给实现会话的完整规格：《移动端 OCR 插件（v1：离线识别）规格》，覆盖：宿主 OCR 引擎（PaddleOCR PP-OCRv4 ONNX + ort）与 WASM 插件壳的分工、宿主命令契约、Kotlin 桥（相册/拍照）、模型打包与删除/恢复、在线识别接缝（v2 供应商无关适配器）。到达目的地 = 实现者无需再做重大决策。

## Notes

**领域**：BedCode 移动端插件系统（WASM 前端/后端 + 宿主 Rust + Kotlin 桥）。识别能力不随插件 zip 分发——引擎驻留宿主 App，插件是 UI/编排壳。
**应咨询的 skill**：`/grilling`、`/domain-modeling`、`frontend-styles`（插件 UI 改动时强制）。

**宿主现状事实**（勘察结论）：
- 移动端插件后端是 WASM（wasm32-unknown-unknown）：跑不了 onnxruntime、无多线程；插件 zip 只含前端 + WASM。
- 原生能力两条路：Kotlin 桥（SafPickerPlugin 等，注册于 `android_plugins.rs`，`Plugin.startActivityForResult` + `@ActivityCallback` 模式已验证）+ 宿主 Rust（`src-tauri/src/`）。
- 插件调宿主命令：前端 `context.commands` → 宿主命令（PermissionManager 门控，manifest 声明权限）。
- 内置插件结构参照 `plugins/file-transfer/`（toolbox 入口、i18n zh-CN/en、Tailwind 复用需把插件源码加入宿主 tailwind content 扫描）。
- `gen/android` 重建后需恢复自定义 Kotlin 文件与 AndroidManifest.xml（AGENTS.md「Android」节）。

**已锁定共识**（grilling 结论）：
1. 引擎层：**宿主原生**（PaddleOCR PP-OCRv4 ONNX 模型 + Rust `ort` 运行时），插件为薄 WASM 壳（取图 UI + 结果展示 + 模型管理）。引擎随 App 发版演进，不随插件 zip 分发。
2. 识别范围：印刷体中英混排（ch 模型），不限版式；手写、表格/公式结构还原 v1 不做（表格按行输出文本）。
3. 输入：相册选图（SAF/Photo Picker，零权限）+ 相机拍照（ACTION_IMAGE_CAPTURE + FileProvider + CAMERA 运行时权限）双入口。
4. 交互：单张串行；结果页 = 文本行列表 + 复制全文 + 行级点击复制；无历史、无批量。
5. 分发：模型打包进 APK assets（~17MB），首次识别惰性解压；设置区「删除模型/从 APK 恢复」；onnxruntime `.so` 随 APK 不可删。
6. 在线接缝：宿主命令带 `engine` 字段 + 引擎 trait（offline 实现；online 留**供应商无关适配器**，参照桌面端 AI 供应商适配层 ADR 0010 的语言），v1 不实现在线逻辑。
7. 解码链路：Kotlin BitmapFactory（HEIC/WebP 兼容）→ 降采样 ≤1600px → RGBA 临时文件 → Rust 引擎消费；避免 Rust 侧 libheif 依赖。
8. 引擎生命周期：首次识别惰性加载 + 常驻缓存；`spawn_blocking` 后台线程；一次一张单飞队列。
9. 入口：工具箱页（ui:toolbox），如 file-transfer。
10. 宿主命令集：`ocr_recognize` / `ocr_engine_status` / `ocr_delete_models` / `ocr_restore_models`。

## Decisions so far

- [引擎选型与宿主驻留](issues/01-引擎选型与宿主驻留.md) — 宿主原生 PaddleOCR PP-OCRv4（ONNX + ort）；拒绝 ocrs（仅拉丁字母）与 Tesseract（中文精度差距大、NDK C++ 构建重）。ADR：`docs/adr/0015-ocr-engine-host-resident.md`。
- [v1 范围与交互](issues/02-v1范围与交互.md) — 印刷中英混排不限版式；相册+拍照双入口；单张、复制全文+行级复制、无历史。
- [模型分发](issues/03-模型分发.md) — 全打包进 APK + 可删除/从 APK 恢复（17MB 模型可删；.so 不可删）。
- [在线识别接缝](issues/04-在线识别接缝.md) — 供应商无关适配器（任意在线 OCR 供应商），v1 只留 `engine` 字段 + trait 路由。

## Open questions

- ort 2.x `download-binaries` 对 `aarch64-linux-android` 的预编译产物可用性（实现期验证；兜底：手动下载 onnxruntime Android .so 进 jniLibs + `load-dynamic`）。
- PP-OCRv4 的 ONNX 模型具体来源（Paddle2ONNX 自转官方模型 vs RapidOCR 社区导出的 ONNX；Apache-2.0 许可兼容，实现期确认）。
