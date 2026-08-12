# 移动端 OCR 插件规格（v1：离线识别）

> 会话：`.scratch/ocr-plugin/map.md`。本规格是唯一实现依据；实现者无需再做重大决策。
> 开放项（不阻塞启动）见第 11 节。

## 1. 目标与非目标

**目标**：移动端提供「图片 → 文本」离线识别能力，作为可启用插件呈现：
- 相册选图 / 相机拍照取图，设备本地识别（无网络依赖）
- 印刷体中英混排（PP-OCRv4 ch 模型），不限版式
- 结果页：文本行列表 + 复制全文 + 行级点击复制
- 模型管理：删除释放空间 / 从 APK 恢复
- 为 v2 在线识别（任意供应商适配器）留接缝

**非目标（v1 明确不做）**：手写体；表格/公式结构还原；批量识别；识别历史；在线识别实现；语言切换 UI（仅 ch 模型）；模型热更新。

## 2. 架构总览

```
┌────────────── OCR 插件（WASM 壳，plugins/ocr/，随插件系统分发）──────────────┐
│  工具箱入口 → 主页（选图/拍照）→ 识别中 → 结果页（行列表+复制）│ 设置区：模型管理 │
│  前端经 context.commands 调宿主命令；manifest 声明权限                          │
└──────────────┬───────────────────────────────────────────────┬───────────────┘
               │ 宿主命令（invoke）                              │ Kotlin 桥（invoke）
┌──────────────▼───────────────────────────────┐  ┌────────────▼───────────────┐
│ 宿主 OCR 引擎（src-tauri/src/ocr/，Rust）     │  │ SafPickerPlugin.pickImage  │
│  ocr_recognize / ocr_engine_status           │  │  （相册，扩展现有桥）        │
│  ocr_delete_models / ocr_restore_models      │  │ CameraPlugin（拍照，新桥）  │
│  engine trait（offline 实现；online 接缝）    │  │  BitmapFactory 解码→RGBA    │
│  PP-OCRv4 ONNX + ort（惰性加载、spawn_blocking）│  └────────────┬───────────────┘
│  模型文件：assets/models → 首次解压到数据目录    │               │
└───────────────────────────────────────────────┘   RGBA 临时文件 + 尺寸
```

**关键原则**：识别数据（RGBA 像素）不经 WASM；WASM 只传路径与元数据。

## 3. 引擎选型（已定，勿重开）

| 候选 | 结论 | 理由 |
|------|------|------|
| **PaddleOCR PP-OCRv4 + ort** | ✅ 采用 | 开源中文精度标杆；ONNX 模型（det 4.7MB + cls 1.3MB + rec ch 10.9MB ≈ 17MB）适合移动端；Apache-2.0；Rust 侧用 `ort` 运行时 |
| ocrs | ❌ | 官方仅支持拉丁字母（README 明示），中文需自训模型 |
| Tesseract（Rust 绑定） | ❌ | 中文精度显著落后；NDK 编译 C++ 依赖重 |
| WASM 内跑识别 | ❌ | wasm32-unknown-unknown 无多线程、内存受限、onnxruntime 不支持 |

## 4. 宿主侧（bedcode-mobile/src-tauri/）

### 4.1 模块结构

```
src-tauri/src/ocr/
├── mod.rs          // 模块入口：注册命令、初始化引擎状态
├── engine.rs       // OcrEngine trait + 路由（engine 字段 → 具体实现）
├── ppocr.rs        // offline 实现：PP-OCRv4 三段流水线（det/cls/rec）+ ort session
├── models.rs       // 模型资源：assets 解压、存在性、删除/恢复、占用大小
└── preprocess.rs   // RGBA 输入 → 引擎预处理（灰度、归一化、det 缩放）
```

宿主命令注册于移动端命令注册表（与现有命令同处），按宿主权限机制门控：新增权限类型 `ocr`（或复用现有门控模式，实现时确认 PermissionManager 移动端对应实现），插件 manifest 声明。

### 4.2 命令契约

所有命令输入输出为 JSON。错误遵循宿主 `AppError`（带上下文的错误字符串，禁止裸字符串）。

#### `ocr_recognize`

请求：
```json
{
  "engine": "offline",
  "image": { "rgbaPath": "/data/user/0/com.bedcode.mobile/cache/ocr/xxx.rgba", "width": 1600, "height": 1200 },
  "maxSide": 1600
}
```
- `image`：**已解码 RGBA 文件**（Kotlin 桥产出，见 4.4）+ 宽高。v1 固定 `engine: "offline"`；其他值返回 `unsupported engine`（接缝路由已就位）。
- `maxSide`：可选，服务端（Rust）二次降采样上限，默认 1600。

响应：
```json
{
  "engine": "offline",
  "durationMs": 1234,
  "lines": [
    { "text": "你好，世界", "confidence": 0.98,
      "bbox": { "x": 12, "y": 34, "w": 200, "h": 40 } },
    { "text": "Hello World", "confidence": 0.87,
      "bbox": { "x": 12, "y": 80, "w": 180, "h": 36 } }
  ]
}
```
- `lines` 按阅读顺序（det 输出自上而下、行内从左到右）；`bbox` 为**原图坐标系**（Kotlin 解码尺寸）。
- `confidence`：rec 模型逐行置信度（0~1），低置信度行 UI 可弱化显示。
- 空文本行剔除。

错误语义：模型缺失 → `models not extracted`（UI 引导恢复模型）；图片尺寸非法 → 带上下文的 `AppError`；引擎加载失败 → 带原因。

#### `ocr_engine_status`

响应：
```json
{
  "available": true,
  "modelsPresent": true,
  "modelsBytes": 17123456,
  "engineLoaded": false,
  "supportedEngines": ["offline"]
}
```
- `available`：当前 ABI 是否包含 onnxruntime + 引擎编译可用（v1 恒 true，为 v2 兜底）。
- `engineLoaded`：识别引擎是否已加载（UI 显示「引擎加载中/已就绪」）。
- `supportedEngines`：v1 恒 `["offline"]`，v2 追加在线 provider id。

#### `ocr_delete_models`

删除已解压模型文件（数据目录 `ocr_models/`）。响应 `{ "deleted": true, "freedBytes": 17123456 }`。引擎已加载时先释放 session 再删；返回 `"inUse": true` 需前端确认？——不，v1 简化：删除前若引擎加载中，命令阻塞至本次识别完成（单飞队列保证）再删。删除成功即 `engineLoaded` 复位。

#### `ocr_restore_models`

从 APK assets 重新解压（无需网络）。响应 `{ "restored": true }`。幂等：已存在则跳过。

### 4.3 引擎生命周期与线程

- **惰性加载**：首次 `ocr_recognize` 时初始化（解压模型 + 建 ort session，预计 2~4s），成功后常驻缓存（`Arc<Mutex<Option<Engine>>>` 或 `OnceCell`）。
- **后台线程**：识别全程 `tokio::task::spawn_blocking`（ort 推理是阻塞调用，禁止占 async executor）。
- **单飞队列**：v1 一次一张，串行执行；并发请求排队（`tokio::sync::Mutex` 或任务队列）。UI 侧同时置「识别中」态防重入。
- **引擎释放**：`ocr_delete_models` 释放；App 进程存活期间不主动卸载（常驻）。

### 4.4 解码链路（Kotlin 桥 → RGBA）

识别引擎只消费 **RGBA8 未压缩像素**，解码全部在 Kotlin 桥完成（BitmapFactory 原生支持 JPEG/PNG/WebP/HEIC，避免 Rust 侧 libheif 依赖）：

- 取图成功后，Kotlin 侧用 `BitmapFactory` 解码并**降采样到长边 ≤1600px**（`inSampleSize` 步进，保持比例；12MP 照片 ≈ 降 3 级，识别速度与精度最佳平衡点）。
- 输出：写入 app cache 目录 `ocr/ocr_<ts>.rgba`（文件头无格式，纯 RGBA8 字节流）+ 返回 `{ path, width, height }` 给调用方；文件名即请求 id，识别完成后宿主删除临时文件。
- 图源 URI 读取经 `contentResolver.openInputStream`（SAF/camera FileProvider URI 统一处理）。

### 4.5 模型资源

- 打包位置：APK `assets/ocr_models/`：`ch_PP-OCRv4_det_infer.onnx`、`ch_PP-OCRv4_rec_infer.onnx`、`ch_PP-OCRv4_cls_infer.onnx`（或等价命名，实现期定；来源见 11）。
- 解压目标：app 数据目录 `ocr_models/`（`app_data_dir` 下，与插件解压同风格）；首次识别前惰性解压，带「已解压版本标记」避免重复（同 PluginAssetExtractor 思路）。
- 删除 = 删 `ocr_models/` 目录；恢复 = 重解压。
- 许可：PaddleOCR 模型 Apache-2.0；APK「关于」或插件设置区注明来源与许可（实现期加一行）。

### 4.6 ort 集成（.so 打包）

onnxruntime Android 库随 APK 打包，两种路径（实现期验证，兜底链）：
1. 首选：`ort` crate `download-binaries` feature 支持 `aarch64-linux-android` 等目标时，构建脚本产出的 `libonnxruntime.so` 纳入 jniLibs 合并路径（RustPlugin 的 `merge*JniLibFolders` 链）。
2. 兜底：手动下载 onnxruntime Android 官方 AAR/release 的 `.so`，复制进 `gen/android/app/src/main/jniLibs/<abi>/`，Rust 侧用 `ort` `load-dynamic` feature 运行时 dlopen。

验证点：真机 arm64 识别跑通 + x86_64 模拟器跑通（ABI 全带）。

## 5. Kotlin 桥（gen/android）

### 5.1 相册：SafPickerPlugin 扩展 `pickImage`

- 现有 `pickFile` type 硬编码 `*/*`；新增 `pickImage`：`ACTION_OPEN_DOCUMENT` + `CATEGORY_OPENABLE` + `type = "image/*"`，回调复用 `fileResult`（或独立回调，实现期定）。
- 零权限（SAF/Photo Picker 均免存储权限）。
- 可选优化（v1 不做）：Android 13+ Photo Picker（`PickVisualMedia`）优先，低版本回退 SAF。

### 5.2 拍照：新 `CameraPlugin`

- `ACTION_IMAGE_CAPTURE` + FileProvider 输出 URI（app cache `ocr/camera_<ts>.jpg`）。
- **CAMERA 运行时权限**：插件激活/首次拍照时请求（`requestPermissions` + 回调，参照 Tauri 插件模式）；拒绝 → 返回明确错误，UI 提示去设置。
- AndroidManifest：`<uses-permission android:name="android.permission.CAMERA"/>` + FileProvider 声明 + `res/xml/file_paths.xml`（cache 路径）。
- **必须同步**：上述 Manifest/资源改动与 `CameraPlugin.kt` 一并纳入「gen/android 重建恢复清单」（AGENTS.md「Android」节，重建后恢复 AndroidManifest.xml + 自定义 Kotlin 文件）。
- 拍照后统一走 4.4 解码链路（BitmapFactory → RGBA 临时文件）。

### 5.3 注册

`android_plugins.rs` 注册 `CameraPlugin`（SafPickerPlugin 已注册，仅加方法）。

## 6. 插件侧（plugins/ocr/，WASM 壳）

按 `bedcode-plugin create com.bedcode.ocr "OCR"` 模板生成，参照 `plugins/file-transfer/` 结构。

### 6.1 清单与权限

```json
{
  "id": "com.bedcode.ocr",
  "name": "OCR",
  "version": "0.1.0",
  "pluginType": "wasm",
  "rustLibrary": "bedcode_plugin_ocr",
  "permissions": ["ui:toolbox", "ocr"]
}
```
- `ocr` 为新宿主权限（4.1）；`ui:toolbox` 注册工具箱入口。
- WASM 后端仅承载命令路由与类型（数据不经 WASM，见 4.4），或可纯前端（`--ts-only`）+ 宿主命令——实现期按模板取舍（倾向 ts-only 减负，若 manifest 校验要求 wasm 则保留最小 wasm）。

### 6.2 UI 流程

1. **工具箱入口**：长条块状入口（同 file-transfer ToolboxEntry 风格）。
2. **主页**：两个主按钮「相册选图」「拍照」+ 引擎状态条（`ocr_engine_status`：模型未就绪 → 引导恢复；引擎加载中/已就绪）。
3. **识别中**：loading 态（禁用按钮，防重入）。
4. **结果页**：图片缩略（可选）+ 文本行列表（点击某行复制该行，Toast 反馈）+ 底部「复制全文」；低置信度行弱化（`confidence < 0.6` 时次要色）。空结果给「未识别到文字」空态。
5. **设置区**（插件设置页）：模型管理——显示模型占用（`modelsBytes`）、「删除模型」「恢复模型」按钮（删除后入口禁用，恢复后解禁）。

### 6.3 数据契约（前端类型）

```ts
interface OcrLine { text: string; confidence: number; bbox: { x: number; y: number; w: number; h: number } }
interface OcrResult { engine: string; durationMs: number; lines: OcrLine[] }
```

### 6.4 i18n

zh-CN（默认）+ en 双语言，key 遵循 `{domain}.{section}.{key}`；新增 key 同时进两个文件。

### 6.5 样式

遵循 `frontend-styles` skill（token-bound、明暗主题、无原生控件外观；按钮/列表自绘）。插件源码加入宿主 `tailwind.config.js` content 扫描范围（复用宿主工具类）。

## 7. 在线识别接缝（v2，只留形状）

- 宿主：`OcrEngine` trait（`recognize(rgba, params) -> Result<OcrOutput>`）；`engine` 字段路由：`"offline"` → ppocr；`"online:<provider>"` → 未来在线适配器（走宿主 HTTP 代理，参照桌面端 AI 供应商适配层 ADR 0010）。
- 插件：引擎选择器 v1 仅渲染「离线」。
- v1 禁止：任何在线请求逻辑、provider 配置 UI。

## 8. 性能目标与体积账

| 项 | 目标 |
|----|------|
| 首次识别延迟（含模型加载） | ≤ 5s（模型解压 + 3 个 session 建图） |
| 稳态单张识别（1600px 照片） | ≤ 3s |
| 模型常驻内存 | 可接受（PP-OCRv4 移动版，峰值约数百 MB 内） |
| APK 增量 | 模型 ~17MB（可删）+ onnxruntime .so ×4（不可删）≈ +60~80MB universal |
| 线程 | 识别全程后台线程，不卡 UI |

## 9. 分步实施计划

1. **宿主引擎骨架**：`src-tauri/src/ocr/` 模块 + 4 个命令注册 + `OcrEngine` trait + engine 路由 + `AppError` 上下文。验证：`cargo test`（单元：路由、模型状态机、RGBA 预处理）。
2. **模型资源与 ort 集成**：assets 打包 + 惰性解压 + 删除/恢复 + ort 链接打通（4.6 兜底链）。验证：真机/模拟器跑通 `ocr_engine_status`；`./gradlew :app:compileUniversalDebugKotlin`（若动 Kotlin）。
3. **识别流水线**：ppocr.rs det/cls/rec 三段 + 行排序 + 置信度。验证：`cargo test`（用合成 RGBA 图测预处理/排序逻辑，模型推理留真机）。
4. **Kotlin 桥**：SafPickerPlugin.pickImage + CameraPlugin + Manifest/FileProvider + 解码降采样 → RGBA。验证：`./gradlew :app:compileUniversalDebugKotlin` + 真机取图。
5. **插件壳**：`bedcode-plugin create` + UI 四页 + 宿主命令调用 + 模型管理 + i18n。验证：dev-shell 联调（mock 宿主命令）→ 真机全链路。
6. **端到端真机验证**：见第 10 节清单。

每步结束跑对应验证命令（`cargo test` / `npm run test:run` / gradlew Kotlin 编译），改动 `gen/android` 后必跑 Kotlin 编译（AGENTS.md 规范）。

## 10. 真机验证清单

1. 首启后首次识别：模型惰性解压成功，延迟在目标内，结果正确（中文/英文/混排样张）
2. 相册选图（JPEG/PNG/WebP/HEIC 各一张）→ 识别成功
3. 拍照（授予/拒绝 CAMERA 权限两态）→ 授予成功识别；拒绝提示明确
4. 结果页：复制全文 / 行级复制 / 低置信度弱化 / 空结果空态
5. 模型管理：删除 → 识别入口禁用且报「模型缺失」→ 恢复 → 再识别成功
6. 引擎常驻：连续两张识别，第二次不再加载模型
7. 单飞：识别中再触发 → 排队不崩
8. 后台/恢复：识别中切后台再回来 → 结果正常或明确失败
9. 深浅色主题下 UI 正常（token-bound）

## 11. 开放项（不阻塞实现启动）

- **ORT Android 预编译**：`ort` 2.x `download-binaries` 对 android 目标的支持在实现期验证；兜底 dlopen（4.6）。
- **ONNX 模型来源**：PP-OCRv4 三模型 ONNX 具体出处（Paddle2ONNX 官方模型自转 vs RapidOCR 社区导出），实现期确认并记录许可。
- **插件形态**：ts-only vs 最小 wasm 壳（6.1），模板校验后定。
- **新权限 `ocr` 的门控接线**：PermissionManager 移动端对应实现（4.1），实现期确认沿用现有模式即可。
