# BedCode Mobile 插件开发指南

本文档说明如何为 BedCode Mobile 开发、构建、打包插件，以及插件在移动端的加载与生命周期。开发工具包为 `@binblink/plugin-sdk-mobile`（SDK），通过 `bedcode-plugin` 命令行提供脚手架与构建能力。

---

## 1. 整体架构

```
┌─────────────────────────────── 插件工程（开发期）──────────────────────────────┐
│  src/index.ts (前端) + rust/src/lib.rs (WASM 后端) + plugin.json (清单)        │
└───────────────┬──────────────────────────────────────┬────────────────────────┘
                │ bedcode-plugin build                  │ cargo build wasm32
                ▼                                       ▼
           dist/index.js                          {lib}.wasm
                └──────────────┬───────────────────────┘
                               │ bedcode-plugin package
                               ▼
                        {id}.zip 插件包（分发单元）
```

```
┌─────────────────────── BedCode Mobile 宿主（运行期）────────────────────────┐
│  APK assets/resources/plugins/**  ──首启解压──►  app_data_dir/plugins/{id}/   │
│       （内置插件，Kotlin 桥）                        │（用户安装的也在同目录）     │
│                                                     ▼                          │
│  PluginManager.scan_and_load: 扫描 plugin.json → 编译 WASM → 实例化            │
│  前端 pluginLoader: convertFileSrc() 经 asset protocol 加载 index.js          │
│  load_all: 按启用偏好（plugin.enabled.{id}）自动激活                           │
└──────────────────────────────────────────────────────────────────────────────┘
```

**关键概念**（见仓库根 [`../CONTEXT.md`](../CONTEXT.md) 词汇表）：

- **启用 (Enabled)**：用户的持久化偏好，决定启动时是否自动激活；Toggle 绑定它。插件上报错误时自动撤销启用。
- **激活 (Activated)**：运行时状态（Activated / Loaded / Deactivated / Error），由状态徽章展示。
- **插件包 (Plugin Package)**：单个 zip 分发单元，内含 `plugin.json`、`index.js` 与可选 `.wasm`。
- **卸载 (Uninstall)**：删除用户安装插件的文件与偏好；内置插件只可停用不可卸载。

---

## 2. 环境要求

| 依赖 | 版本 | 用途 |
|---|---|---|
| Node.js | ≥ 20 | SDK CLI、vite 构建 |
| Rust | stable | WASM 后端 |
| wasm32 target | `rustup target add wasm32-unknown-unknown` | WASM 编译 |
| BedCode Mobile | 开发版 | 真机/模拟器验证 |

---

## 3. 创建插件工程

```bash
bedcode-plugin create com.example.my-plugin "My Plugin" --author "you"
cd my-plugin
npm install
```

`create` 从 SDK 内置模板生成完整工程（frontend + WASM + manifest），并填充：

| 文件 | 说明 |
|---|---|
| `plugin.json` | 插件清单（id/name/权限/扩展点） |
| `src/index.ts` | 前端入口：`activate(context)` / `deactivate()` |
| `rust/Cargo.toml` | Rust 后端（wasm32 目标） |
| `rust/src/lib.rs` | `WasmPlugin` 实现（含 manifest 声明） |
| `vite.config.ts` | 前端构建（产物 `dist/index.js`） |
| `tsconfig.json` | TS 配置 |

模板使用 `file:` 相对路径依赖本地 SDK；SDK 发布到 GitHub 后改为 npm 依赖。

---

## 4. 构建与打包

```bash
npm run build       # = bedcode-plugin build：vite + cargo wasm32
npm run package     # = bedcode-plugin package：产出 dist/{id}.zip
```

`bedcode-plugin build` 支持：

- `--frontend-only` / `--rust-only`：只构建一半
- `--watch`：监听源码变更自动重建
- `--resources-dir <父目录>`：额外把产物复制到 `<父目录>/{id}/`（宿主资源目录）

> **组件化构建链**：`build` 内置 **componentize**（幂等）——cargo wasm32（`--features wasm`）
> 产物经 wit-component 编码为 **Component Model 组件**（字节头 `00 61 73 6d 0d 00 01 00`）。
> 宿主只接受组件形态；裸 `cargo build --target wasm32` 会把产物还原为 core module
> （宿主加载报错），**重编后必须重跑 `bedcode-plugin build` 再部署**。

`bedcode-plugin package --hash`：计算 WASM SHA256 写入 `plugin.json` 的 `wasmHash`
（安装时宿主校验完整性；`--hash` 之外的常规打包不强制要求）。

内置插件由仓库脚本统一构建：

```bash
cd bedcode-mobile && npm run plugins:build          # 全部
cd bedcode-mobile && npm run plugins:build -- --plugin com.bedcode.auto-task
```

---

### 创建纯前端插件

默认模板为 wasm（前端 + WASM 后端）；`--ts-only` 生成纯前端插件（无 `rust/` 目录，`pluginType: ts-only`）：

```bash
bedcode-plugin create com.example.ui-only "UI Only" --ts-only
```

### CLI 其他命令

| 命令 | 说明 |
|---|---|
| `bedcode-plugin manifest [--check]` | 按源码自动填充 plugin.json 的 contributes/permissions；`--check` 只检查（CI） |
| `bedcode-plugin validate [--dir]` | 校验 plugin.json 结构（id 格式、必填字段、权限白名单、wasmHash 格式、产物存在性）；CI 用，exit 1 表示不合法 |
| `bedcode-plugin doctor` | 环境自检：Node ≥ 20 / Rust / wasm32 target / dev-shell 依赖 / SDK 构建产物 |
| `bedcode-plugin --version` | 打印 SDK 版本 |

## 5. 插件清单 plugin.json

```json
{
  "id": "com.example.my-plugin",
  "name": "My Plugin",
  "version": "0.1.0",
  "description": "",
  "author": "you",
  "main": "index.js",
  "pluginType": "wasm",
  "rustLibrary": "bedcode_plugin_my_plugin",
  "permissions": ["storage"],
  "contributes": {}
}
```

| 字段 | 必填 | 说明 |
|---|---|---|
| `id` | ✅ | 反域名风格，全局唯一 |
| `name` / `version` | ✅ | 展示名 / 版本 |
| `main` | ✅ | 前端入口（产物文件名，通常 `index.js`） |
| `pluginType` | ✅ | `wasm`（前端 + WASM 后端）或 `ts-only` |
| `rustLibrary` | wasm 必填 | Rust crate 名（与 `Cargo.toml` 一致） |
| `icon` | | 列表图标（SVG path 字符串或图标文件路径） |
| `permissions` | | 权限声明，`storage` 默认授予 |
| `contributes` | | 扩展点声明（命令/视图/navTab/终端/设置区/配置） |
| `wasmHash` | 可选 | WASM 文件 SHA256（`sha256-` 前缀），安装时校验 |

---

## 6. 权限

| 权限 | 说明 |
|---|---|
| `storage` | 插件键值存储（**默认授予**，无需声明） |
| `ui:toolbox` | 工具箱页注册 |
| `ui:navtab` | 底部导航 Tab 注册 |
| `ui:input` | 终端工具栏注册 |
| `ui:settings` | 设置区注册 |
| `ui:route` | 插件路由注册 / 页面跳转（registerRoute / openPage / goBack） |
| `ui:back` | 系统返回键拦截（onBackPressed） |
| `terminal:input` / `terminal:output` | 终端输入/输出事件 |
| `session:read` / `session:write` | 会话信息读取 / 会话创建与停止 |
| `network:http` | HTTP 请求（经宿主代理） |
| `fs:read` / `fs:write` | 文件系统访问（经授权弹窗） |
| `bus` | 插件间消息总线 |

未授予的权限在敏感 host 函数调用前会被拒绝。

---

## 7. 前端 API（context）

`activate(context: PluginContext)` 提供以下 API：

| 子 API | 说明 |
|---|---|
| `context.ui` | 注册工具箱页 / navTab / 终端工具栏项 / 设置区 / 插件路由（registerRoute / openPage / goBack / onBackPressed） |
| `context.commands` | 调用宿主命令（前端 handler 优先，未注册时回退 WASM 后端） |
| `context.events` | 事件订阅与发射 |
| `context.storage` | 插件键值存储（宿主 SQLite，dev-shell 中为 localStorage） |
| `context.fileService` | 文件服务挂载（mount / updateRoots / dispose）与授权弹窗 |
| `context.i18n` | 国际化（`registerMessages` / `t` 自动加插件 ID 前缀） |
| `context.logger` | 日志（转发宿主 tracing） |
| `context.dialogs` | 弹窗 |
| `context.notifications` | 系统通知 |
| `context.lifecycle` | 应用生命周期事件（onAppStartup / onAppShutdown / onAuthSuccess / onDisconnect 等） |
| `context.system` | `openFile(path)` 打开文件、`revealInDir(path)` 在文件管理器中显示文件（SAF 授权） |
| `context.status` | 生命周期状态上报（`reportReady` / `reportError`） |
| `context.terminal` / `context.session` | 终端 / 会话能力 |

### 共享运行时模块（SDK 函数直取）

除 `context` 外，SDK 还暴露宿主共享运行时模块，经 `window.__BEDCODE_SHARED__` 注入：

| SDK 函数 | 返回 | 说明 |
|---|---|---|
| `getVue()` / `getVueI18n()` / `getPinia()` / `getRouter()` | 宿主实例 | 复用宿主 Vue / i18n / Pinia / Router（响应式共享） |
| `getPresetTasks()` | `{ usePresetTasks }` | 宿主预设任务 composable |
| `getMobileApi()` | `MobileHostApi` | 宿主连接/HTTP 能力：`activeSessionId`（响应式 ref）+ AutoTask 队列接口（`httpTaskQueue*`、`httpSessionSettings`、`httpSetSessionMode`、`httpCurrentTask`），对端桌面端 REST API |

### 自渲染浮层（createApp 模式）

插件需弹出自定义面板/弹窗时，可像内置 auto-task 插件一样自行挂载 Vue 应用（宿主只负责注册入口按钮，不承载插件 UI）：

```ts
import { createApp } from 'vue'
import MyPanel from './components/MyPanel.vue'
import panelCss from './panel.css?inline'  // 宿主不加载插件 dist/style.css，需运行时注入

const container = document.createElement('div')
document.body.appendChild(container)
const app = createApp(MyPanel)
app.provide('pluginContext', context)        // 组件内经 inject 取 context
app.mount(container)
```

- 样式用 `?inline` 导入并在 activate 时注入 `<style>`（参见 `plugins/auto-task/src/panel.css`）；
- 组件复用宿主 Tailwind 工具类时，需把插件源码加入宿主 `tailwind.config.js` 的 `content` 扫描范围；
- 键盘避让基于 window 级事件（`safeAreaChanged` + `visualViewport`），与终端输入一致（参见 `plugins/auto-task/src/components/AutoTaskPanelHost.vue`）。

---

## 8. WASM 后端（rust）

插件后端编译为 **WASM 组件（Component Model）**，契约定义在 SDK 的
`packages/plugin-sdk-mobile/rust/wit/bedcode.wit`（宿主导入 11 接口 / 插件导出 8 接口，
单一事实来源，wit-bindgen 编译期校验）。`rust/src/lib.rs` 实现 `WasmPlugin` trait 后以
`wasm_entry!` 宏生成组件导出（manifest / activate / deactivate / invoke_command / 生命周期
钩子 / 事件 / 上传与传输钩子 / abi.version）；宿主按 `rustLibrary` 查找 `{crate}.wasm`
（组件产物）编译实例化。自研 ABI（`__bedcode_*` 导出、`(ptr,len)` 内存搬运、签名表）
已在 2025-08 迁移清理删除。

**SDK 依赖**：插件 rust crate 依赖 `bedcode-plugin-api-mobile`（即 plugin-sdk-mobile/rust），
wasm 构建开启 `--features wasm`（`wasm_entry!` / `WasmHost` / `WasmPlugin` 在此 feature 下）；
构建命令见 §4（内置 componentize）。

**契约差异表（移动端 vs 桌面端 WIT）**：

| 项 | 桌面端 | 移动端 | 说明 |
|----|--------|--------|------|
| import 接口 | 17 组 | 11 组 | 无 session/api-call/timer/process/app/plugin-database |
| host-database | 4 函数 | 2 函数 | 无 params 变体、无插件独立库 |
| host-fs | 6 函数 | 8 函数 | 移动端新增 download/document 保存（SAF/MediaStore） |
| host-events | emit/broadcast-sync/notify | emit/notify | 无 broadcast |
| host-log | 5 函数 | 5 函数 | mark-plugin-error 语义对齐 |
| events 导出 | 4 个 | 5 个 | 移动端 WS 认证生命周期事件 |
| abi | version + form | 仅 version | 无 core 共存形态 |

完整 WIT 与迁移背景见 `../docs/implementation-plans/mobile-wasmtime-component-migration.md`。

**重要**：`plugin.json` 与 `rust/src/lib.rs` 中的 `manifest()` 都声明清单——**以 `plugin.json` 为准**（宿主扫描读取），`manifest()` 用于 SDK 内部校验。

---

## 9. 安装到 BedCode Mobile

1. `npm run package` 产出 `dist/{id}.zip`
2. 手机端 → 设置 → 插件管理 → 右上角 **+**
   - **从文件安装**：选择 zip
   - **从 URL 安装**：输入 zip 下载链接
3. 安装成功后列表出现插件，Toggle 启用

**加载链路**：内置插件（APK assets）首启由 Kotlin 桥解压到 `app_data_dir/plugins/{id}/`（`.bedcode-source` 标记防重复、按版本刷新）；用户安装的插件由安装器解压到同一目录（标记 `file-install` / `remote-download`）。启动时 `scan_and_load` 统一扫描，按启用偏好自动激活。

**卸载**：详情内「卸载」按钮（仅用户安装的插件）——停用、删除文件、清理偏好与存储，不可恢复。

---

## 10. 浏览器开发环境（Dev Shell）

除真机验证外，SDK 内置浏览器开发环境（`dev-shell/`）：空壳宿主 + 移动端页面骨架，
插件前端源码在浏览器中实时运行，支持 HMR，无需构建、打包、安装。

```bash
bedcode-plugin dev            # 在当前插件目录启动
bedcode-plugin dev ../my-plugin --port 5180 --open
bedcode-plugin dev --host     # 监听局域网，手机浏览器访问 http://<电脑IP>:5173 查看
```

> **Windows 注意**：直接在 cmd / PowerShell 输入 `bedcode-plugin` 会提示「不是内部或外部命令」——
> Windows 不自动把 `node_modules/.bin` 加入 PATH（只有 npm 脚本 / npx 会解析）。
> 请用 `npm run dev`（模板与内置插件已内置该脚本）或 `npx bedcode-plugin dev`。

浏览器打开 `http://localhost:5173`，页面包含：

- **手机框**：390×844 移动端骨架（状态栏/页头/底部导航），可切换全宽渲染
- **工具箱**：插件 `registerToolboxPage` 入口网格（含自定义 entry 卡片）
- **模拟终端**：输入/输出、会话创建/停止、连接/断开、认证成功（触发对应 lifecycle）
- **插件页**：状态徽章、激活/停用、设置区/路由/挂载一览
- **日志面板**：`context.logger` 与加载错误实时展示

**Mock 边界**：`commands.execute` 仅执行前端注册 handler（WASM 后端命令记 warn）；
`storage` 用 localStorage；`fileService` pick 系列弹输入框返回模拟路径；
`getMobileApi()` 完整 mock；权限检查跳过。真机专属能力（WASM 命令、真实 WS、
SAF 选择器、系统通知）仍需真机验证。首次运行自动安装 dev-shell 依赖。

详见 `packages/plugin-sdk-mobile/dev-shell/README.md`。

> 桌面端插件开发与 Dev Shell 见 `../bedcode-desktop/plugin-dev-desktop.md`。

## 11. 验证清单（真机 / 模拟器）

1. 首启：内置插件自动解压（日志 `Extracted bundled plugin` / `Dev-copied builtin plugin`）
2. 列表：插件出现，来源显示「内置」/「已安装」，状态徽章正确
3. 启用：Toggle 打开 → Activated；重启应用后自动激活
4. 停用：Toggle 关闭 → Deactivated；重启后不自动激活
5. 文件安装：选择 zip → 安装成功 → 列表出现 → 可启用
6. URL 安装：输入 zip 链接 → 下载安装成功
7. 卸载：用户安装插件显示卸载按钮，确认后文件删除、列表消失；内置插件无卸载按钮
8. 错误态：插件 activate 报错 → 状态 Error（红）+ 错误信息显示 + 自动撤销启用
9. wasmHash 校验：篡改 zip 内 wasm → 安装失败并提示 SHA256 不匹配

---

## 12. 相关代码入口

| 模块 | 位置 |
|---|---|
| SDK 命令行 | `bedcode-mobile/packages/plugin-sdk-mobile/bin/cli.js` |
| SDK 模板 | `bedcode-mobile/packages/plugin-sdk-mobile/template/` |
| SDK 浏览器开发环境 | `bedcode-mobile/packages/plugin-sdk-mobile/dev-shell/` |
| 插件宿主 | `bedcode-mobile/src-tauri/src/plugin/`（manager / loader / downloader / wasm_runtime / wasm_host / registry / commands / storage / transfer / saf_io / saf_path / fs_auth / approval / message_bus / validation / android_plugins） |
| 内置插件 | `bedcode-mobile/plugins/ai-chatbox`、`plugins/auto-task`、`plugins/file-transfer` |
| 插件管理页 | `bedcode-mobile/src/views/PluginView.vue` |
| 前端插件运行时 | `bedcode-mobile/src/plugin/`（loader / registry / context / commands / events / permission / shared-runtime / routes / dialog-host / components） |
| Kotlin 解压桥 | `bedcode-mobile/src-tauri/gen/android/.../PluginAssetExtractor.kt` |
