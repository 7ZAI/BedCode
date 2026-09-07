# OCR

离线文字识别插件 — BedCode Mobile（前端 TS + Rust WASM 壳，识别/取图命令由宿主直供）。

- 工具箱入口「OCR 文字识别」→ 主页（相册选图 / 拍照）→ 识别 → 结果页（行级复制 / 复制全文 / 低置信度弱化 / 空态）
- 识别引擎为宿主 PP-OCRv4 离线引擎（det/cls/rec），**识别命令不经 WASM**：前端经 `context.ocr.*` 直通宿主命令（spec §4.2/§6）
- 取图由 Kotlin 桥完成（SAF 相册 + CameraPlugin 拍照 + BitmapFactory 解码降采样 ≤1600 → RGBA8 临时文件，宿主识别后自动清理）
- 设置区模型管理：查看占用 / 删除 / 恢复（首次使用需恢复 ~17MB 模型）

## 开发

```bash
npm install        # 安装依赖（含 SDK、vite、vue）
npm run dev        # 浏览器开发环境（Dev Shell：mock 宿主，HMR；mock 识别结果见 src/mock.ts）
npm run test:run   # vitest run（业务逻辑单测）
npm run build      # 构建：vite（前端）+ cargo（WASM 组件）
npm run package    # 打包 dist/com.bedcode.ocr.zip 插件包
```

部署到宿主 APK 资源（构建产物 + 刷新 dev 副本）：

```bash
cd bedcode-mobile && npm run plugins:build -- --plugin com.bedcode.ocr
```

## i18n

key 为扁平式且含 `ocr.` 域前缀（`'ocr.toolbox.title'`）——与宿主/dev-shell 的
`registerMessages` 前缀机制配合；新增 key 必须同时进 `src/i18n/zh-CN.ts` 与 `en.ts`
（`messages.ts` 的 MessageSchema 为结构约束）。

## 目录结构

```
plugins/ocr/
├── plugin.json            # 插件清单（id/name/version/description/permissions/contributes.*）
├── package.json           # npm scripts（dev/build/test:run/package）
├── vite.config.ts         # vite lib 构建（入口 src/index.ts，产物 dist/index.js）
├── tsconfig.json
├── pnpm-lock.yaml / pnpm-workspace.yaml
├── rust/
│   ├── Cargo.toml         # 包名 bedcode_plugin_ocr（crate-type cdylib + rlib）
│   └── src/lib.rs         # WASM 壳：WasmPlugin 最小实现（activate/deactivate 仅日志；无命令面）
└── src/
    ├── index.ts           # 插件入口（注册 i18n / 工具箱 / 设置 / 结果页路由；导出 activate/deactivate/devMock）
    ├── mock.ts            # Dev Shell mock 识别结果
    ├── styles.css         # 插件全局样式（宿主不加载 dist/style.css，运行时注入）
    ├── vite-env.d.ts
    ├── components/
    │   ├── ToolboxEntry.vue  # 工具箱入口长条卡片
    │   ├── OcrView.vue       # 主页（相册选图 / 拍照 + 引擎状态条）
    │   ├── OcrSettings.vue   # 设置区（模型占用 / 删除 / 恢复）
    │   └── ResultPage.vue    # 结果页（行复制 / 复制全文 / 低置信度弱化 / 空态）
    ├── composables/
    │   └── useOcr.ts       # 业务逻辑（引擎状态机 / 识别流 / 模型管理 / 错误映射）
    ├── i18n/
    │   ├── index.ts
    │   ├── messages.ts
    │   ├── zh-CN.ts
    │   └── en.ts
    └── __tests__/
        └── useOcr.test.ts  # vitest 单测（useOcr 状态机 / 超时 / 错误映射）
```

## 权限

来自 `plugin.json.permissions`：

| 权限 | 用途 |
| --- | --- |
| `ocr`          | 使用宿主 OCR 引擎与模型管理（`context.ocr.*`） |
| `ui:back`      | 结果页返回 / 主页退出的返回手势 |
| `ui:route`     | 主页 → 结果页路由跳转（`context.ui.openPage('result')`） |
| `ui:settings`  | 设置区注册（模型管理） |
| `ui:toolbox`   | 工具箱入口注册（`ocr.toolbox`） |

## 命令

`plugin.json` **未声明** `contributes.commands` —— 本插件**无 WASM 命令面**（`rust/src/lib.rs`
的 `invoke_command` 一律返回 Unknown command）。识别 / 取图 / 引擎状态 / 模型管理全部经
`context.ocr.*` 直通宿主命令（spec §6）：

- `engineStatus()` / `recognize({ image: { rgbaPath, width, height } })`
- `pickImage()` / `cameraCapture()`
- `deleteModels()` / `restoreModels()`

## 真机验证

spec §10 清单见 `.scratch/ocr-plugin/issues/10-端到端真机验证.md`；
dev-shell 冒烟回归脚本：`.scratch/ocr-plugin/devshell-smoke.mjs`。
