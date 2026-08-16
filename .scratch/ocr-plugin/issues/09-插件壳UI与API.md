# 09 — 插件壳：UI 四页 + context.ocr API + i18n

**What to build:** 可安装的 OCR 插件（前端 TS + Rust WASM 壳）：工具箱入口 → 主页（相册/拍照 + 引擎状态条）→ 识别中 loading → 结果页（文本行列表 + 行级点击复制 + 复制全文 + 低置信度弱化 + 空结果空态）→ 设置区模型管理（占用/删除/恢复）；SDK types 扩展 `OcrAPI` 接口与 `PluginContext.ocr` 字段（前端直接 invoke 宿主命令，识别数据不经 WASM）。实现依据为 spec §6。用户可感知：工具箱点「OCR」完成「选图 → 识别 → 复制」全流程，模型缺失时引导恢复。

**Blocked by:** 07 + 08（端到端需要真实识别与取图；dev-shell mock 联调可先行）

**Status:** ready-for-agent

- [x] 按模板生成插件壳，manifest 按 spec §6.1：`permissions: ["ui:toolbox", "ocr", "ui:route", "ui:settings", "ui:back"]`（按需补），contributes views/settings/routes 齐全
- [x] SDK types 扩展（spec §11 开放项 4 落实）：`OcrApi` 接口 + `PluginContext.ocr` 字段（参照 `fileService` 模式），`ocr.recognize` / `ocr.engineStatus` / `ocr.deleteModels` / `ocr.restoreModels` 直通宿主命令
- [x] 四页 UI 按 spec §6.2 全流程：识别中 loading 禁按钮防重入；`confidence < 0.6` 行弱化；空结果空态；模型缺失（`models not extracted`）引导「恢复模型」；删除后识别入口禁用、恢复后解禁
- [x] 识别命令不经 WASM：Rust WASM 壳为最小实现（激活/停用日志），不扩 WIT host 接口（spec §6）
- [x] i18n zh-CN + en 同步（§6.4，key 命名 `{domain}.{section}.{key}`）；样式遵循 frontend-styles skill（token-bound、无原生控件外观、明暗主题），插件源码加入宿主 tailwind content 扫描（宿主 glob 已覆盖 `plugins/**/src/**`，零改动）
- [x] dev-shell mock 宿主命令联调全流程通过；前端 `npm run test:run` 通过

## Comments

### 会话交接（2026-08-16，票据 09 完成）

**实现落点**：
- 插件工程 `bedcode-mobile/plugins/ocr/`（`bedcode-plugin create` 模板）：plugin.json（permissions: ocr/ui:toolbox/ui:route/ui:settings/ui:back；contributes toolbox 视图+ToolboxEntry 入口、settings、result 路由）；`src/index.ts`（i18n 注册 + 视图/设置/路由注册）；`src/mock.ts`（devMock.ocrLinesSeed）；i18n 扁平 key 三件套；`src/composables/useOcr.ts`（enginePhase 状态机 + 识别流防重入 + 模型管理 + 错误映射）；四个组件（ToolboxEntry / OcrView / ResultPage / OcrSettings）；`rust/src/lib.rs` 最小 WASM 壳（激活/停用日志，命令恒 unknown——识别命令不经 WASM）
- SDK（packages/plugin-sdk-mobile）：`types.ts` +OcrApi/OcrLine/OcrBBox/OcrResult/OcrEngineStatus/OcrImageSource/OcrLinesSeed + `PluginContext.ocr` + `PluginDevMock.ocrLinesSeed`；`index.ts` 补导出；dev-shell `mock-context.ts` +ocr mock（delete/restore 内存态翻转演示模型缺失引导链）
- 宿主：`src/plugin/commands.ts` +6 个 OCR invoke 封装；`context.ts` +ocr API（requireOcrPermission 门控）；`permission.ts` ocr 权限映射 +pickImage/cameraCapture；locales +noOcrPermission key
- 产物已部署 `src-tauri/resources/plugins/mobile/com.bedcode.ocr/`（index.js 29.5kB + wasm 350KB + plugin.json，经 `npm run plugins:build -- --plugin com.bedcode.ocr`）

**验证**：插件 vitest 21/21；vue-tsc 0 error；`npm run build` 全链（vite+cargo wasm32+componentize）通过；宿主 vitest 179 通过；dev-shell 构建通过 + **headless Chrome 实机冒烟**（`.scratch/ocr-plugin/devshell-smoke.mjs` + 截图 devshell-result.png）：激活 → 入口点击 → 主页「引擎已就绪」→ 相册选图（mock）→ 结果页 3 行 + 低置信度 chip + 复制全文按钮，断言全绿

**踩坑（#lesson）**：插件 i18n 必须用**扁平 key 且含域前缀**（`'ocr.toolbox.title'`）——嵌套对象（`toolbox: {title}`）经 registerMessages 顶层加 `{pluginId}.` 前缀后路径错位，渲染出原始 key（dev-shell 首轮冒烟即现形）；vite/client 不声明 `*.vue` 模块，插件类型检查用 vue-tsc（tsc 对 .vue 导入报错属正常）；`bedcode-plugin create` 模板 tsconfig 缺 paths/types 映射，需补 `@binblink/plugin-sdk-mobile` paths（参照 file-transfer）

**08 遗留修复（本会话顺手）**：CameraPlugin 权限回调原为「重入 capture」，拒绝时若再次请求会**无限循环弹窗**；改为独立 `@PermissionCallback onCameraPermission` 回调——授权→launch，拒绝→reject 明确错误（tauri 2.11.1 源码确认 permissionCallbackMethods 注册机制）；`./gradlew compileUniversalDebugKotlin` 通过

**遗留（10 端到端）**：真机全链路（模型解压→取图→识别→复制）；设置区模型管理真机验证；en 语言包真机验证；相册 HEIC/WebP 样张；相机权限拒绝态 UI 文案（前端已映射，真机走查）；`npm run tauri:android:dev:log` 观察日志；dev-shell 冒烟脚本可复用于真机前回归

### UI 审核优化（2026-08-16，vision 三组评审 + 修复 + 复审闭环）

**流程**：handoff-ui-review.md 步骤 1-5 —— 12 张 dev-shell 截图（`screenshot-all.mjs`，本轮+2 张）→ vision agent 三组并行评审（入口+主页四态 / 结果页三态 / 设置区+浅色）→ 修复 → 复截复审三组全闭环 → 回归。

**vision 首轮问题 → 修复落点**（全部已修，复审确认闭环）：

| 严重度 | 问题 | 修复 |
|--------|------|------|
| P0 | 识别中 loading 无动效，仅文字变灰 | OcrView 加 `activeAction`（album/camera）区分触发按钮：触发按钮 icon 位换 `animate-spin` 环形 spinner（border-t-transparent），另一按钮保持原文字+icon 禁用（opacity 0.5） |
| P0 | 缺失态禁用按钮无原因说明 | 已有 desc 文案 + 按钮禁用态保留；desc 加 `text-pretty` 平衡中文断行（修复「即/可」断词尴尬） |
| P1 | 入口卡 icon 语义错误（放大镜=搜索）且无 chevron | ToolboxEntry icon 换 heroicons document-magnifying-glass（文档+放大镜组合）；chip 加 `chip-violet` 变体（与宿主插件入口同语言）；右侧补 chevron（照 file-transfer / 宿主默认卡片） |
| P1 | 「恢复模型」按钮层级弱（边框/弱底不像主 CTA） | styles.css `.ocr-btn-accent` 弱底 14% → **填充 accent** + `--mobile-text-on-accent` 对比 token（主页状态条/设置区/结果页空态 CTA 三处共用，深浅色自动适配） |
| P1 | 行卡片无可点击 affordance + 无按压反馈 | 行卡片 `active:scale-[0.98]` + `transition-[color,background-color,transform]`（替代原 active:opacity-80）；scoped hover 背景微变（color-mix 5%/4%，仅桌面鼠标） |
| P1 | 空态无操作入口 | 结果页空态加「返回主页」CTA（`ocr-btn-accent` h-11，`context.ui.goBack()`）；复用既有 `ocr.result.backToHome` key，文案改「返回主页」/「Back to Home」 |
| P1 | 低置信度 chip 0.625rem 偏小 + 浅色对比度不足 | chip 字号 → `var(--font-size-xs)`（12px 档 token）+ 字重 600 + 高度 1.125rem→1.5rem；浅色对比靠 token（--mobile-warning 浅色 #d97706）自然改善 |
| P2 | meta 行数字未确认等宽 | 加 `tabular-nums` |
| P2 | 设置页浅色无截图验证 | 脚本+2 张：`settings-confirm`（删除确认弹窗）、`settings-present-light`；vision 确认浅色 token 适配完整 |
| — | manifest icon 用 emoji（🔍/📄） | **禁 emoji**：index.ts icon 改为 SVG path d（宿主 `isSvgIcon` 识别 M 开头渲染 stroke 图标），与入口卡同一 document-magnifying-glass path |

**确认为误报/不改的**（记录澄清）：
- 状态条背景「就绪蓝紫 tint vs 缺失灰」——代码是单一 `--mobile-bg-tertiary` token，三态一致，vision 观察偏差
- 删除无确认弹窗——代码本就有 `showConfirm`（宿主 DialogHost），截图流程即「点删除→确认」；补 settings-confirm 截图证实，弹窗按钮中文「取消/确定」走宿主 i18n
- 浅色复制按钮「近纯黑 #1A1A18」——是宿主 token `--mobile-accent`（浅色 #1D1A14 刻意反转设计），非插件硬编码；不改宿主全局 token
- 入口实时状态角标——v1 spec 未要求，主页已展示引擎状态，保持静态
- 主页首帧 unknown 闪变——onMounted 立即刷新，视觉无感，保持
- 空态 document 图标超长 path——渲染正常无残缺，保持

**回归**：vue-tsc 0 error；vitest 21/21；`npm run build` 全链通过；部署 `src-tauri/resources/plugins/mobile/com.bedcode.ocr/`（index.js 33813B + wasm 350910B，`node scripts/plugin-build.js --plugin com.bedcode.ocr`，注意参数是 manifest.id 不是目录名）

**遗留**：active:scale 按压反馈为静态截图无法验证项，真机走查时确认（vision 建议查 touch-manipulation / tap-highlight）；入口卡「恢复模型」按钮在真机缺失态的首屏表现随票据 10 一并验证
