# BedCode Desktop 插件开发指南

本文档说明如何为 BedCode Desktop 开发插件，以及如何使用浏览器开发环境（Dev Shell）
快速迭代插件前端。开发工具包为 `@bedcode/plugin-sdk-desktop`（SDK），
命令行为 `bedcode-plugin-desktop`。

---

## 1. 整体架构

```
┌──────────────────────────── 插件工程（开发期）────────────────────────────┐
│  src/index.ts (前端) + rust/src/lib.rs (WASM 后端，可选) + plugin.json    │
└───────────────┬──────────────────────────────────────┬────────────────────┘
                │ vite build                            │ cargo build (wasm32)
                ▼                                       ▼
           dist/index.js                           {rustLibrary}.wasm
                └──────────────┬───────────────────────┘
                               ▼
                   插件目录（resources/plugins/{id}/ 或安装目录）
```

桌面端插件类型：`ts-only`（纯前端）、`rust-ts`（前端 + Rust WASM 后端）、
`rust`（纯后端）。`plugin.json` 声明权限与扩展点（sandbox / contributes）。

**关键概念**：

- **扩展点**：`views`（sidebar / toolbox / statusbar）、`terminal`、
  `toolProviders`、`fileHandlers`、`commands`、`configuration`、`lifecycle`
- **PluginContext**：插件访问宿主能力的唯一通道（commands / terminal / session /
  ui / events / storage / http / fileService / system / i18n）
- **共享运行时**：宿主将 vue / vue-i18n / pinia / i18n / router 暴露到
  `window.__BEDCODE_SHARED__`，插件构建时由 SDK vite 插件外部化，
  经 `getVue()` / `getI18n()` 等复用宿主实例

## 2. 创建插件工程

开发工具包为 `@bedcode/plugin-sdk-desktop`，命令行为 `bedcode-plugin-desktop`。

```bash
bedcode-plugin-desktop create com.example.my-plugin "My Plugin" --author "you"
cd my-plugin
npm install
```

- 默认生成 **ts-only**（纯前端）插件；`--rust` 附带 WASM 后端脚手架（`pluginType: rust-ts`）
- `create` 从 SDK 内置模板生成：`plugin.json` / `vite.config.ts`（vue 等外部化到宿主）/ `src/index.ts` / 可选 `rust/`（WasmPlugin 实现 + `wasm_entry!`）；`--dir <dir>` 指定生成目录，`--registry` 改为引用已发布 SDK 版本（npm + crates.io，默认引用本地 SDK 相对路径）

构建与分发：

```bash
npm run build    # = bedcode-plugin-desktop build：vite（+ rust-ts 时 cargo wasm32）
npm run build -- --resources-dir <宿主resources/plugins父目录>   # 复制产物到宿主（内置插件分发方式）
npm run build -- --frontend-only / --rust-only   # 只构建一半（rust-ts 插件）
```

## 3. 前端 API（context）

`activate(context: PluginContext)` 提供：

| 子 API | 说明 |
|---|---|
| `context.ui` | 注册侧边栏面板 / 工具箱页 / 状态栏项 / 输入扩展 / 终端工具栏项 / 标题栏项 / 页面工具栏项 / 文件处理器 |
| `context.commands` | 调用宿主命令（前端 handler 优先，未注册时回退 Rust 后端） |
| `context.events` | 事件订阅与发射 |
| `context.storage` | 插件键值存储（宿主 SQLite，dev-shell 中为 localStorage） |
| `context.http` | 注册 HTTP 端点（宿主 Rust 服务端挂载，插件经 `/api/plugin/{pluginId}/...` 访问） |
| `context.terminal` / `context.session` | 终端输入输出 / 会话能力 |
| `context.fileService` | 文件服务挂载（mount / updateRoots / dispose）、对端信息、目录与多文件选择；v2 批量传输应答（approveTransferRequest / rejectTransferRequest / setApprovalTimeout / cancelReceivingSession） |
| `context.system` | `revealInDir(path)` 在系统文件管理器中打开目录并选中文件（Shell COM 直调，中文路径原生支持） |
| `context.i18n` | `getI18n()` 取宿主 i18n 实例；`registerMessages` / `t` 自动加插件 ID 前缀 |

内置插件示例：`plugins/ai-chatbox`（侧边栏 AI 面板 + 终端工具栏项）、`plugins/auto-task`（Claude Code 任务队列 + 定时任务）、`plugins/file-transfer`（局域网文件传输）、`plugins/scheduler`（计划任务）。

## 4. 浏览器开发环境（Dev Shell）

SDK 内置空壳宿主（`dev-shell/`）：桌面端页面骨架（标题栏 / 侧边栏 / 状态栏），
插件前端源码在浏览器中实时运行，支持 HMR。

```bash
bedcode-plugin-desktop dev            # 在当前插件目录启动
bedcode-plugin-desktop dev ../my-plugin --port 5180 --open
```

浏览器打开 `http://localhost:5173`，页面包含：

- **侧边栏**：内置导航 + 插件 `registerSidebarPanel`（按 `order` 排序）
- **工具箱**：插件 `registerToolboxPage` 入口网格
- **模拟终端**：输入发送（触发 `terminal.onInput`）、模拟输出（触发 `onOutput`）、
  会话创建/停止、连接/断开；终端工具栏项 + 输入扩展渲染在顶部
- **插件页**：状态徽章、激活/停用、全部注册项一览（文件处理器 / HTTP 端点 / 挂载）
- **日志面板**：加载错误与插件日志（标题栏按钮开关）
- **主题切换**：设置页切换 `html.dark`，验证插件深浅色适配

**Mock 边界**：`commands.execute` 仅执行前端注册 handler（Rust 后端命令记 warn）；
`storage` 用 localStorage；`http.registerEndpoint` 仅登记展示（浏览器不可达）；
`fileService` pick 系列弹输入框返回模拟路径；权限检查跳过。
Rust 后端逻辑、真实 HTTP 端点、系统文件选择需在真实宿主验证。

详见 `packages/plugin-sdk-desktop/dev-shell/README.md`。

## 5. CLI 其他命令

| 命令 | 说明 |
|---|---|
| `bedcode-plugin-desktop manifest [--check]` | 按源码自动填充 plugin.json 的 contributes/permissions；`--check` 只检查（CI） |
| `bedcode-plugin-desktop validate [--dir]` | 校验 plugin.json（id 格式、必填字段、sandbox/pluginType、权限白名单、产物存在性） |
| `bedcode-plugin-desktop doctor` | 环境自检：Node ≥ 20 / Rust / wasm32 target / dev-shell 依赖 / SDK 构建产物 |
| `bedcode-plugin-desktop --version` | 打印 SDK 版本 |

## 6. 验证清单

1. 插件出现在侧边栏/工具箱，状态「已激活」
2. 注册的各类扩展点正确渲染（面板 / 工具栏 / 状态栏 / 文件处理器）
3. i18n：中英文切换（改 `dev-shell/src/main.ts` 的 i18n locale 后刷新）文案更新
4. 插件事件：模拟终端输入/输出 → 插件 onInput/onOutput 回调生效
5. 停用/激活：dispose 清理、重新 activate 正常
6. Rust 后端命令：真机/桌面宿主验证（dev-shell 中仅前端 handler 可测）

## 7. 相关代码入口

| 模块 | 位置 |
|---|---|
| SDK 命令行 | `bedcode-desktop/packages/plugin-sdk-desktop/bin/cli.js` |
| SDK 模板 | `bedcode-desktop/packages/plugin-sdk-desktop/template/` |
| SDK 浏览器开发环境 | `bedcode-desktop/packages/plugin-sdk-desktop/dev-shell/` |
| 插件宿主（Rust） | `bedcode-desktop/src-tauri/src/plugin/`（host / loader / registry / api_bridge / wasm_runtime / file_service / storage / permission / validation / message_bus / fs_auth / approval / watcher） |
| 前端插件运行时 | `bedcode-desktop/src/plugin/`（context / registry / loader / commands / events / permission / shared-runtime / contributionKinds） |
| 内置插件 | `bedcode-desktop/plugins/ai-chatbox`、`auto-task`、`file-transfer`、`scheduler` |
