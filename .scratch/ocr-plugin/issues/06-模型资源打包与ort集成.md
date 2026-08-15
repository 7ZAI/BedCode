# 06 — 模型资源打包与 ort 集成

**What to build:** PP-OCRv4 三模型（det/cls/rec，约 17MB）随 APK assets 打包，首次访问惰性解压到数据目录（带版本标记防重复解压），支持删除/从 APK 恢复；onnxruntime Android 库随 APK 打包并打通 Rust `ort` 加载链路。实现依据为 spec §4.5–§4.6。用户可感知：`engine_status` 返回真实的 `modelsPresent` / `modelsBytes`；设置区可删除模型释放空间、可一键恢复。

**Blocked by:** 05（命令契约与状态机在 05 建立）

**Status:** ready-for-agent

- [ ] 三模型 ONNX 来源确定并打包进 APK assets（spec §11 开放项 2 落实：Paddle2ONNX 自转 vs RapidOCR 导出，记录出处与 Apache-2.0 许可，设置区注明一行来源/许可）
- [ ] 首次访问惰性解压 + 「已解压版本标记」防重复（同 PluginAssetExtractor 思路），解压失败带原因
- [ ] `plugin_ocr_engine_status` 返回真实 `modelsPresent` / `modelsBytes`（engineLoaded 此时为 false）
- [ ] `delete_models`：删除数据目录、`freedBytes` 正确、engineLoaded 复位；`restore_models`：重解压、幂等（已存在则跳过）
- [ ] onnxruntime 集成打通（spec §4.6 兜底链）：优先 `ort` download-binaries 支持 android 目标；兜底手动 .so 进 jniLibs + `load-dynamic` dlopen；真机 arm64 `available=true`，x86_64 模拟器亦可用
- [ ] 若改动 `gen/android`（jniLibs/Manifest）：`./gradlew :app:compileUniversalDebugKotlin` 通过，改动同步进「gen/android 重建恢复清单」
- [ ] APK 增量在预期范围（模型 ~17MB 可删 + .so 不可删，spec §8）
