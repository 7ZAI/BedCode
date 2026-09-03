# 06 — 模型资源打包与 ort 集成

**What to build:** PP-OCRv4 三模型（det/cls/rec，约 17MB）随 APK assets 打包，首次访问惰性解压到数据目录（带版本标记防重复解压），支持删除/从 APK 恢复；onnxruntime Android 库随 APK 打包并打通 Rust `ort` 加载链路。实现依据为 spec §4.5–§4.6。用户可感知：`engine_status` 返回真实的 `modelsPresent` / `modelsBytes`；设置区可删除模型释放空间、可一键恢复。

**Blocked by:** 05（命令契约与状态机在 05 建立）

**Status:** 已实现（2026-08-16）

- [ ] 三模型 ONNX 来源确定并打包进 APK assets（spec §11 开放项 2 落实：Paddle2ONNX 自转 vs RapidOCR 导出，记录出处与 Apache-2.0 许可，设置区注明一行来源/许可）
- [ ] 首次访问惰性解压 + 「已解压版本标记」防重复（同 PluginAssetExtractor 思路），解压失败带原因
- [ ] `plugin_ocr_engine_status` 返回真实 `modelsPresent` / `modelsBytes`（engineLoaded 此时为 false）
- [ ] `delete_models`：删除数据目录、`freedBytes` 正确、engineLoaded 复位；`restore_models`：重解压、幂等（已存在则跳过）
- [ ] onnxruntime 集成打通（spec §4.6 兜底链）：优先 `ort` download-binaries 支持 android 目标；兜底手动 .so 进 jniLibs + `load-dynamic` dlopen；真机 arm64 `available=true`，x86_64 模拟器亦可用
- [ ] 若改动 `gen/android`（jniLibs/Manifest）：`./gradlew :app:compileUniversalDebugKotlin` 通过，改动同步进「gen/android 重建恢复清单」
- [ ] APK 增量在预期范围（模型 ~17MB 可删 + .so 不可删，spec §8）

## Comments

### 实现记录（2026-08-16，票据 06 完成）

**模型**（spec §11 开放项 2 落实）：RapidOCR 官方 ModelScope 托管，v2.1.0 tag，Apache-2.0（RapidOCR 仓库声明）。det SHA256 与官方 `default_models.yaml` 一致（d2a7720d…）。三文件已入库 `bedcode-mobile/src-tauri/resources/ocr_models/`（16MB），打包路径 `resources/ocr_models/*`（tauri.conf.json bundle.resources）。

**ort 集成**（spec §4.6 兑底链）：`download-binaries` 确认不支持 Android → 官方 onnxruntime-android 1.20.0 AAR 的 .so 经 `bedcode-mobile/scripts/fetch-ort-android.sh` 放 jniLibs（arm64-v8a + x86_64，与现有 ABI 对齐）；Cargo.toml `ort 2.0.0-rc.13`（**无 2.0.0 稳定版**）`default-features=false + load-dynamic + ndarray + api-20`（默认 api-27 会拒绝 1.20 .so，必须显式锁低版本）。

**解压桥**：新 Kotlin `OcrModelExtractorPlugin`（Builder 名 ocr-model-extractor，独立防覆盖）+ Rust `android_plugins/ocr_models.rs`；惰性（models_present=false 才解压）+ `.bedcode-source` 版本标记（同 PluginAssetExtractor 思路）；`getNativeLibraryDir` 供 available 探测。

**验证**：宿主 cargo test 321 全过（新增 status_available / onnx_so_path 测试）；`gradlew :app:compileUniversalDebugKotlin` 通过（踩坑：注释里 `ocr_models/*.onnx` 的 `/*` 序列开启嵌套注释致 Unclosed comment）；debug APK 打包验证：assets/resources/ocr_models/ 三模型 15.4MB + lib/{arm64-v8a,x86_64}/libonnxruntime.so 36MB ✓（构建需 RUST_MIN_STACK=67108864，rustc 栈溢出老坑）。

**真机验证未做**：无在线设备；且 `plugin_ocr_engine_status` 有 require_ocr 门控，需插件壳（票据 09）激活后才能真正机跑通——该项验收推迟到 09 联调（APK 打包链路本身已在本票据验证）。

**git 策略**：模型入库（离线构建核心资产）；.so 不入库（gen/android .gitignore 已忽略 `jniLibs/**/*.so`），fetch 脚本幂等可重建。
