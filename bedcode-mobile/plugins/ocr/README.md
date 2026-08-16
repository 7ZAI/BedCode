# OCR

离线文字识别插件 — BedCode Mobile（前端 TS + Rust WASM 壳）。

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

## 真机验证

spec §10 清单见 `.scratch/ocr-plugin/issues/10-端到端真机验证.md`；
dev-shell 冒烟回归脚本：`.scratch/ocr-plugin/devshell-smoke.mjs`。
