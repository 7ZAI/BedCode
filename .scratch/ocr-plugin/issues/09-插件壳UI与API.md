# 09 — 插件壳：UI 四页 + context.ocr API + i18n

**What to build:** 可安装的 OCR 插件（前端 TS + Rust WASM 壳）：工具箱入口 → 主页（相册/拍照 + 引擎状态条）→ 识别中 loading → 结果页（文本行列表 + 行级点击复制 + 复制全文 + 低置信度弱化 + 空结果空态）→ 设置区模型管理（占用/删除/恢复）；SDK types 扩展 `OcrAPI` 接口与 `PluginContext.ocr` 字段（前端直接 invoke 宿主命令，识别数据不经 WASM）。实现依据为 spec §6。用户可感知：工具箱点「OCR」完成「选图 → 识别 → 复制」全流程，模型缺失时引导恢复。

**Blocked by:** 07 + 08（端到端需要真实识别与取图；dev-shell mock 联调可先行）

**Status:** ready-for-agent

- [ ] 按模板生成插件壳，manifest 按 spec §6.1：`permissions: ["ui:toolbox", "ocr", "ui:route", "ui:settings", "ui:back"]`（按需补），contributes views/settings/routes 齐全
- [ ] SDK types 扩展（spec §11 开放项 4 落实）：`OcrApi` 接口 + `PluginContext.ocr` 字段（参照 `fileService` 模式），`ocr.recognize` / `ocr.engineStatus` / `ocr.deleteModels` / `ocr.restoreModels` 直通宿主命令
- [ ] 四页 UI 按 spec §6.2 全流程：识别中 loading 禁按钮防重入；`confidence < 0.6` 行弱化；空结果空态；模型缺失（`models not extracted`）引导「恢复模型」；删除后识别入口禁用、恢复后解禁
- [ ] 识别命令不经 WASM：Rust WASM 壳为最小实现（激活/停用日志），不扩 WIT host 接口（spec §6）
- [ ] i18n zh-CN + en 同步（§6.4，key 命名 `{domain}.{section}.{key}`）；样式遵循 frontend-styles skill（token-bound、无原生控件外观、明暗主题），插件源码加入宿主 tailwind content 扫描
- [ ] dev-shell mock 宿主命令联调全流程通过；前端 `npm run test:run` 通过
