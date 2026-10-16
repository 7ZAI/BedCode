# BedCode Mobile WASM 应用开发指南

本文档说明如何为 BedCode Mobile 开发、构建、打包 wasm 应用（工程目录
`wasm-apps/<id>/`），以及应用在移动端的加载与生命周期。开发工具包为
`@binblink/bedcode-plugin-sdk-mobile`（SDK），通过 `bedcode-plugin` 命令行提供脚手架与构建能力。

> **术语**：机制内核与运行时仍把加载单元称为「插件」（`plugin.json`、
> `PluginManager`、`host-*` 原语等标识符不变）；本仓对**业务型 wasm 应用**统一用
> wasm-app 一词。代码标识符、清单字段、WIT 契约保持原样，只有文档与工程目录命名改口。

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
│  load_all: 只按自启动偏好（plugin.autostart.{id}，缺省关）自动激活             │
└──────────────────────────────────────────────────────────────────────────────┘
```

**关键概念**（见仓库根 [`../CONTEXT.md`](../CONTEXT.md) 词汇表）：

- **启用 (Enabled)**：用户的持久化偏好，表示应用「可用」（`plugin.enabled.{id}`）。**不决定启动是否激活**（持久化 ≠ 自启动），插件上报错误时自动撤销。
- **自启动 (Autostart)**：用户的持久化偏好，表示应用应随宿主启动自动激活（`plugin.autostart.{id}`，**缺省关**）。宿主启动只扫描已安装 wasm-app、不启动任何应用；只有显式打开自启动的应用在 boot 自动激活，其余等用户在宿主壳里点击启动。
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
pnpm install
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
pnpm run build       # = bedcode-plugin build：vite + cargo wasm32
pnpm run package     # = bedcode-plugin package：产出 dist/{id}.zip
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
cd bedcode-mobile && pnpm run plugins:build          # 全部
cd bedcode-mobile && pnpm run plugins:build -- --plugin com.bedcode.terminal-session
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
  "platform": "mobile",
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
| `platform` | ✅ | 适配平台，三值：`desktop`（仅桌面）/ `mobile`（仅移动）/ `both`（双端适配）。按工程所在端取值（移动工程写 `mobile`）；宿主加载 / zip 安装时按本端校验，移动拒 `desktop`，不匹配即显性拒绝 |
| `main` | ✅ | 前端入口（产物文件名，通常 `index.js`） |
| `pluginType` | ✅ | 产物形态，四值（真源 `packages/plugin-sdk-mobile/rust/src/types.rs::PluginType`）：`wasm`（前端 + WASM 后端，内置应用一律用它）/ `rust-ts` / `rust` / `ts-only`（纯前端） |
| `rustLibrary` | wasm 必填 | Rust crate 名（与 `Cargo.toml` 一致） |
| `icon` | | 列表图标（SVG path 字符串或图标文件路径） |
| `permissions` | | 权限声明，`storage` 默认授予且不可关闭（`src/shell/permissions.ts::SHELL_LOCKED_PERMISSIONS`） |
| `contributes` | | 扩展点声明（命令/视图/navTab/终端/设置区/配置） |
| `wasmHash` | 可选 | WASM 文件 SHA256（`sha256-` 前缀），安装时校验 |

---

## 6. 权限

| 权限 | 说明 |
|---|---|
| `storage` | 插件键值存储（**默认授予且不可关闭**，无需声明） |
| `ui:toolbox` | 工具箱页注册 |
| `ui:navtab` | 底部导航 Tab 注册 |
| `ui:input` | 终端工具栏注册 |
| `ui:settings` | 设置区注册 |
| `ui:route` | 插件路由注册 / 页面跳转（registerRoute / openPage / goBack） |
| `ui:back` | 系统返回键拦截（onBackPressed） |
| `terminal:input` / `terminal:output` | 终端输入/输出事件 |
| `session:read` / `session:write` | 会话信息读取 / 会话创建与停止 |
| `network:http` | HTTP 请求（经宿主代理） |
| `fs:read` / `fs:write` / `fs:pick` | 文件系统访问（经授权弹窗）/ 系统选择器 |
| `bus` | 插件间消息总线 |
| `peer` / `mdns` / `ws:client` | 对等网络 / mDNS / WS 客户端 |

权限词汇有两份真源，口径不同：

- **内核侧**（`packages/bedcode-wasm-core/src/permission.rs`）——`host-*` 原语的
  最终仲裁表；未在本表内的位即使 manifest 声明也会被激活路径过滤并 `warn` 留痕。
- **移动端 SDK 侧**（`packages/plugin-sdk-mobile/rust/src/permission.rs`）——内核表的
  超集，额外包含只在移动端前端生效的面（`ui:navtab` / `ui:route` / `ui:back` /
  `bus` / `session:write` 等）。前端注册类 API 的闸门在
  `src/plugin/permission.ts` + `src/shell/permissions.ts`。

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

插件需弹出自定义面板/弹窗时，可像内置 bedcode 插件（显示名 BedCode，任务域）一样自行挂载 Vue 应用（宿主只负责注册入口按钮，不承载插件 UI）：

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

- 样式用 `?inline` 导入并在 activate 时注入 `<style>`（参见 `wasm-apps/bedcode/src/task/panel.css`）；
- 组件复用宿主 Tailwind 工具类时，需把插件源码加入宿主 `tailwind.config.js` 的 `content` 扫描范围；
- 键盘避让基于 window 级事件（`safeAreaChanged` + `visualViewport`），与终端输入一致（参见 `wasm-apps/bedcode/src/task/components/AutoTaskPanelHost.vue`）。

---

## 8. WASM 后端（rust）

插件后端编译为 **WASM 组件（Component Model）**，契约真源在
`packages/bedcode-wasm-core/wit/core.wit`（机制必需面，8 import + 4 export）+ 能力域
`packages/bedcode-*/wit/<domain>.wit` + 端 `rust/wit-src/cap-mobile.wit`；端生成物
`packages/plugin-sdk-mobile/rust/wit/bedcode.wit` 由 `compose.json` 驱动
`scripts/compose-wit.mjs` 拼装（`world plugin` = `include` core + 6 个 cap），
**禁止手改生成物**（漂移锁：仓库根 `pnpm run check:wit`，CI + husky 同判据）。
`rust/src/lib.rs` 实现 `WasmPlugin` trait 后以 `wasm_entry!` 宏生成组件导出
（manifest / activate / deactivate / invoke_command / 生命周期钩子 / 事件 / 上传与传输钩子）；
宿主按 `rustLibrary` 查找 `{crate}.wasm`（组件产物）编译实例化。
自研 ABI（`__bedcode_*` 导出、`(ptr,len)` 内存搬运、签名表）已在 2025-08 迁移清理删除。

**版本与形态模型**（整数 ABI 已退役，ADR 0051 D2）：

- **包版本** = WIT package semver `bedcode:plugin@0.2.0`，真源仓库根
  `contracts/bedcode-plugin-package.rs::PACKAGE_VERSION`；compose.json 只声明包名，
  源文件 package 行由 `compose-wit.mjs` 注入，`wit_package_version_lock.rs` 钉一致。
- **组件形态**经 `AbiPolicy` 只管 `abi-form`：桌面要求 `form = 1`，移动端 `None`
  （移动 WIT 无 `abi-form` 导出，不要求）——`compose.json` 里的 `abi.version`
  （桌面 38 / 移动 21）仅作重建提示的参考号，不再由 wasm-core 强制校验。
- **组件形态闸门**：宿主只接受 Component Model 组件，裸 `cargo build --target wasm32`
  产物会被拒（加载期点名缺失 interface 与重建版本）。

**SDK 依赖**：插件 rust crate 依赖 `bedcode-plugin-api-mobile`（即 plugin-sdk-mobile/rust），
wasm 构建开启 `--features wasm`（`wasm_entry!` / `WasmHost` / `WasmPlugin` 在此 feature 下）；
构建命令见 §4（内置 componentize）。

**契约差异表（移动端 vs 桌面端宿主接口）**：

| 项 | 桌面端 | 移动端 | 说明 |
|----|--------|--------|------|
| 宿主导入接口 | 26 组 | 18 组 | 真源 = 两端 `rust/wit/` 生成物（`grep 'interface host-'` 即得） |
| 插件导出接口 | 4 组 | 4 组 | `command` / `lifecycle` / `events-binary` / `manifest`，均住 `core.wit` |
| 核心机制面 | 8 import | 8 import | `host-storage` / `host-plugin-database` / `host-log` / `host-config` / `host-events` / `host-bus` / `host-fs` / `host-platform`（ADR 0052 D1） |
| 桌面独有 | `host-pty` / `host-process` / `host-task` / `host-api-call` / `host-app` / `host-crypto` / `host-auth-center` / `host-connection-registry` / `host-timer` / `host-websocket`（客户端域） / `host-websocket-server` / `host-http-endpoint` / `host-fs-desktop` / `host-platform-desktop` | — | 端扩展的声明与实现均住桌面宿主（ADR 0052 D4） |
| 移动独有 | — | `host-terminal-stream` / `host-fs-mobile` / `host-notify-mobile` / `host-auth-client` / `host-connection-target` | 端独有域整面落移动宿主 |
| 通知 | `host-notify`（共享契约房） | `host-notify` + `host-notify-mobile`（4 函数） | 2026-10-13 通知功能级统一；移动端附加震动 / 声音 / 权限查询面 |
| 认证 | `host-auth-center`（v32 四函数 + 组合式原语） | `host-auth-client` | 入场签发密钥与生物公钥托管均归认证中心自持（ADR 0031 / 0033） |
| 主库直调 | 已退役 | 已退役 | `host-database` 整面退役，主库收归 wasm-core；插件数据库能力 = `host-plugin-database`（私有库，声明 `storage` 位） |

完整契约演进（含每版删改明细）见 ADR 0022 修订记录、
`../docs/knowledge/wasmtime-component-migration.md` 与
[`../docs/knowledge/wasm-app-development-checklist.md`](../docs/knowledge/wasm-app-development-checklist.md)。

**重要**：`plugin.json` 与 `rust/src/lib.rs` 中的 `manifest()` 都声明清单——**以 `plugin.json` 为准**（宿主扫描读取），`manifest()` 用于 SDK 内部校验。

---

## 9. 安装到 BedCode Mobile

1. `pnpm run package` 产出 `dist/{id}.zip`
2. 手机端 → 设置 → 插件管理 → 右上角 **+**
   - **从文件安装**：选择 zip
   - **从 URL 安装**：输入 zip 下载链接
3. 安装成功后列表出现插件，Toggle 启用

**加载链路**：内置插件（APK assets）首启由 Kotlin 桥解压到 `app_data_dir/plugins/{id}/`（`.bedcode-source` 标记防重复、按版本刷新）；用户安装的插件由安装器解压到同一目录（标记 `file-install` / `remote-download`）。启动时 `scan_and_load` 统一扫描并实例化，但**不启动任何应用**；只有显式打开自启动偏好（`plugin.autostart.{id}`）的应用才会自动激活，其余在宿主壳里点击后才激活。

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
> Windows 不自动把 `node_modules/.bin` 加入 PATH（ 只有 pnpm 脚本 / pnpm exec 会解析）。
> 请用 `pnpm run dev`（模板与内置插件已内置该脚本）或 `pnpm exec bedcode-plugin dev`。

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

> 桌面端插件开发与 Dev Shell 见 `../bedcode-desktop/wasm-app-dev-desktop.md`。

## 11. 验证清单（真机 / 模拟器）

1. 首启：内置插件自动解压（日志 `Extracted bundled plugin` / `Dev-copied builtin plugin`）
2. 列表：插件出现，来源显示「内置」/「已安装」，状态徽章正确
3. 自启动：打开自启动开关后，重启应用 → 该应用自动 Activated；未打开的保持 Loaded，点击后才 Activated
4. 点击启动：壳内点击未启动应用 → 后端激活 + 前端运行面加载；停止后回 Deactivated，下次启动不自动复活
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
| 内置 wasm 应用 | `bedcode-mobile/wasm-apps/{ai-chatbox,bedcode,file-transfer}`（`bedcode` 目录 = 显示名 BedCode，插件 id 仍是 `com.bedcode.terminal-session`） |
| 插件管理页 | `bedcode-mobile/src/views/PluginView.vue` |
| 前端插件运行时 | `bedcode-mobile/src/plugin/`（loader / registry / context / commands / events / permission / shared-runtime / routes / dialog-host / components） |
| 宿主壳（平台机制） | `bedcode-mobile/src/shell/`（运行面挂载 / 权限总览与审批 / 通知栏 / 设置）——壳内不落业务页面（ADR 0046） |
| Kotlin 解压桥 | `bedcode-mobile/src-tauri/gen/android/.../PluginAssetExtractor.kt` |
| 机制整核与能力域 crate | 仓库根 `packages/bedcode-wasm-core` 与 `packages/bedcode-*`，索引见 [`../packages/README.md`](../packages/README.md) |
| WIT 契约真源 | `packages/bedcode-wasm-core/wit/core.wit` + 能力域 `packages/bedcode-*/wit/<domain>.wit` + 端 `rust/wit-src/cap-mobile.wit`；改契约后跑 `pnpm run check:wit`（仓库根） |
| 开发约束清单 | [`../docs/knowledge/wasm-app-development-checklist.md`](../docs/knowledge/wasm-app-development-checklist.md)（权限 / 存储 / 事件 / 认证中心 / HTTP 面等红线） |
| 桌面端指南 | [`../bedcode-desktop/wasm-app-dev-desktop.md`](../bedcode-desktop/wasm-app-dev-desktop.md) |
