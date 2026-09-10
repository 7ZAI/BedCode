# OCR 引擎驻留宿主而非插件内

移动端 OCR 的识别引擎（PaddleOCR PP-OCRv4 ONNX 模型 + `ort` 运行时）实现为宿主 Rust 模块（`bedcode-mobile/src-tauri/src/ocr/`），随 App 发版演进；「OCR 插件」只是 WASM UI/编排壳（取图、结果展示、模型管理），经宿主命令调用引擎。原因：移动端插件后端是 WASM 沙箱（wasm32-unknown-unknown），跑不了 onnxruntime 也无多线程；纯 Rust 的 ocrs 官方仅支持拉丁字母；而中文识别是 v1 刚需，PaddleOCR 是开源中文精度标杆。代价是引擎不随插件 zip 热更新，升级跟 App 发版走。

## Considered Options

- **纯 WASM 插件（ocrs）**：可独立分发、zip 热更新，但官方仅支持拉丁字母，中文需自训模型（研究级工作量），v1 中文直接出局
- **Tesseract Rust 绑定**：中文可跑但精度显著落后 PaddleOCR，且需 NDK 编译 leptonica+tesseract C++ 依赖
- **宿主原生（PaddleOCR + ort）**：✅ 采用——唯一「Rust 实现 + 最强中文精度 + 离线」的交集；与既有「Kotlin 桥 + WASM 插件」分工模式一致（引擎在宿主、插件做编排）
