# BedCode 插件开发指南

本文档面向第三方开发者，介绍如何为 BedCode 桌面端开发插件。

---

## 目录

1. [插件架构概览](#1-插件架构概览)
2. [插件类型](#2-插件类型)
3. [插件目录结构](#3-插件目录结构)
4. [plugin.json 清单文件](#4-pluginjson-清单文件)
5. [权限系统](#5-权限系统)
6. [TS-only 插件开发](#6-ts-only-插件开发)
7. [Rust+TS 插件开发（WASM）](#7-rustts-插件开发wasm)
8. [PluginContext API 参考](#8-plugincontext-api-参考)
9. [扩展点详解](#9-扩展点详解)
10. [配置系统](#10-配置系统)
11. [测试与调试](#11-测试与调试)
12. [构建与打包](#12-构建与打包)
13. [常见问题](#13-常见问题)

---

## 1. 插件架构概览

BedCode 插件系统采用 **Rust + TypeScript 双层架构**：

- **Rust 层**：处理计算密集型任务、系统 API 访问、数据库操作、网络请求等
- **TypeScript 层**：负责 UI 组件渲染、用户交互、事件响应

两层通过 `PluginContext` API 和 Tauri 事件系统通信：

```
┌─────────────────────────────────────────┐
│            BedCode 宿主应用               │
│  ┌─────────────┐   ┌─────────────────┐  │
│  │ PluginHost   │   │ PluginLoader    │  │
│  │ (Rust 生命周期)│   │ (TS 动态导入)   │  │
│  └──────┬───────┘   └────────┬────────┘  │
│         │                    │            │
│  ┌──────┴────────────────────┴────────┐  │
│  │         PluginContext              │  │
│  │  commands / terminal / session /   │  │
│  │  storage / events / ui / http      │  │
│  └───────────────────────────────────┘  │
└─────────────────────────────────────────┘
```

### 插件生命周期

```
扫描 plugin.json → 加载(Loaded) → 激活(Activated) → 停用(Deactivated)
                                        ↕
                                    错误(Error)
```

- **Loaded**：manifest 解析完成，权限已授予，但尚未执行初始化逻辑
- **Activated**：插件 activate() 调用成功，可正常使用所有 API
- **Deactivated**：插件 deactivate() 调用后，权限收回，注册表清理
- **Error**：激活失败或运行时异常，可重新激活

---

## 2. 插件类型

| 类型 | pluginType | Rust 端 | TS 端 | 适用场景 |
|------|-----------|---------|-------|---------|
| **TS-only** | `ts-only` | 无 | 完整 | 纯 UI 扩展、简单工具 |
| **Rust+TS** | `rust-ts` | WASM 组件 | UI 组件 | 需要后端计算/数据库/网络的完整插件 |
| **Rust** | `rust` | 静态注册 | 无 | 纯后端处理（仅限内置插件） |

> **第三方插件仅支持 `ts-only` 和 `rust-ts` 两种类型。** `rust` 类型通过 `inventory` 静态注册，仅用于内置插件。

---

## 3. 插件目录结构

### TS-only 插件

```
plugins/desktop/com.example.my-plugin/
├── plugin.json           # 清单文件（必需）
├── index.ts              # 入口文件（必需，与 plugin.json main 字段对应）
├── components/           # Vue 组件
│   └── MyView.vue
├── composables/          # 业务逻辑
│   └── useMyFeature.ts
├── types.ts              # 类型定义
└── vite.config.ts        # 独立构建配置
```

### Rust+TS 插件

```
plugins/desktop/com.example.my-plugin/
├── plugin.json                          # 清单文件
├── index.ts                             # TS 入口
├── components/                          # Vue 组件
├── composables/                         # 业务逻辑
├── vite.config.ts                       # TS 构建配置
└── rust/                                # Rust 源码目录
    ├── Cargo.toml
    └── src/
        ├── lib.rs                       # WASM 组件入口（WasmPlugin trait 实现）
        ├── commands.rs                  # command handler
        └── ...
```

> Rust+TS 插件的 WASM 编译产物（`{rustLibrary}.wasm`）需放在插件目录，文件名与 `plugin.json` 的 `rustLibrary` 字段一致。

### 插件目录约定

- **插件根目录**：`plugins/desktop/{plugin-id}/`
- **plugin-id 格式**：反向域名，如 `com.example.my-plugin`
- **入口文件**：由 `plugin.json` 的 `main` 字段指定，TS-only 默认为 `index.ts`

---

## 4. plugin.json 清单文件

`plugin.json` 是插件的唯一入口描述，放在插件根目录。

### 完整字段说明

```jsonc
{
  // ==================== 必填字段 ====================
  "id": "com.example.my-plugin",       // 反向域名格式，全局唯一
  "name": "My Plugin",                 // 显示名称
  "version": "1.0.0",                  // 语义化版本号

  // ==================== 可选字段 ====================
  "description": "插件描述文本",         // 插件功能说明
  "author": "Author Name",             // 作者
  "main": "index.js",                  // TS 入口文件（编译后），TS-only 必填
  "sandbox": "inline",                 // 沙箱模式，目前仅支持 "inline"
  "pluginType": "ts-only",            // 插件类型：ts-only | rust-ts | rust
  "rustLibrary": "",                   // WASM 库文件名（仅 rust-ts 需要）
  "permissions": ["storage", "ui:sidebar"],  // 请求的权限列表

  // ==================== 扩展点声明 ====================
  "contributes": {
    "commands": [],                     // 命令注册
    "views": [],                        // 视图注册
    "terminal": null,                   // 终端扩展
    "toolProviders": [],                // 外部工具端点
    "fileHandlers": [],                 // 文件处理器
    "configuration": null,              // 配置声明
    "lifecycle": null                   // 生命周期钩子
  }
}
```

### sandbox 字段

目前仅支持 `"inline"` 模式 — 插件代码在宿主进程内运行，共享主线程。

### rustLibrary 字段（仅 rust-ts）

指定 WASM 库文件名（WASM 组件产物），**不含 `.wasm` 后缀**，宿主按 `{rustLibrary}.wasm` 查找。

例如 `rustLibrary: "bedcode_plugin_ai_chatbox"`，宿主查找 `bedcode_plugin_ai_chatbox.wasm`。

> **重要**：`rustLibrary` 不得包含路径分隔符（`/` 或 `\`）或父目录引用（`..`），否则加载会被拒绝。

### contributes 字段

#### commands — 命令注册

```json
"commands": [
  { "id": "my-plugin.hello", "title": "Say Hello" },
  { "id": "my-plugin.calc", "title": "Calculate", "icon": "🧮" }
]
```

#### views — 视图注册

```json
"views": [
  {
    "id": "my-plugin.sidebar",
    "type": "sidebar",           // sidebar | toolbox | statusbar
    "title": "My Panel",
    "component": "MyView"        // Vue 组件名（在 index.ts 中注册）
  }
]
```

#### terminal — 终端扩展

```json
"terminal": {
  "inputHandlers": ["on_input"],     // 输入处理器名称
  "outputParsers": []                // 输出解析器名称
}
```

#### toolProviders — 外部工具端点

```json
"toolProviders": [
  {
    "id": "my-plugin.tool",
    "name": "My Tool",
    "endpoint": "tool"              // 实际路径: /api/plugin/{plugin-id}/tool
  }
]
```

#### fileHandlers — 文件处理器

```json
"fileHandlers": [
  {
    "id": "my-plugin.json-viewer",
    "extensions": [".json", ".jsonc"],
    "viewer": "JsonViewer",          // Vue 组件名
    "icon": "📄"
  }
]
```

#### configuration — 配置声明

```json
"configuration": {
  "title": "My Plugin Settings",
  "properties": {
    "apiKey": {
      "type": "string",
      "title": "API Key",
      "description": "Your API key for the service",
      "default": ""
    },
    "maxResults": {
      "type": "number",
      "title": "Max Results",
      "default": 10
    },
    "autoRefresh": {
      "type": "boolean",
      "title": "Auto Refresh",
      "default": false
    },
    "theme": {
      "type": "string",
      "title": "Theme",
      "enum": ["light", "dark", "auto"],
      "default": "auto"
    }
  }
}
```

#### lifecycle — 生命周期钩子

```json
"lifecycle": {
  "onStartup": true,
  "onShutdown": true
}
```

TS-only 插件通过监听 `lifecycle:startup` / `lifecycle:shutdown` Tauri 事件接收回调。

---

## 5. 权限系统

插件采用 **双重权限校验**：前端快速失败 + Rust 端最终仲裁。

### 权限列表

| 权限 | 允许的 API 方法 | 说明 |
|------|----------------|------|
| `terminal:input` | `terminal.sendInput`, `terminal.onInput` | 向终端发送输入、监听输入事件 |
| `terminal:output` | `terminal.onOutput` | 监听终端输出 |
| `session:read` | `session.list`, `session.get`, `session.onStatusChange` | 读取会话信息 |
| `session:write` | `session.create`, `session.stop` | 创建/停止会话 |
| `ui:sidebar` | `ui.registerSidebarPanel` | 注册侧边栏面板 |
| `ui:toolbox` | `ui.registerToolboxPage` | 注册工具箱页面 |
| `ui:statusbar` | `ui.registerStatusBarItem`, `ui.registerTitleBarItem` | 注册状态栏/标题栏项 |
| `ui:input` | `ui.registerInputExtension`, `ui.registerTerminalToolbarItem` | 注册输入扩展/终端工具栏 |
| `network:http` | `http.registerEndpoint` | 注册 HTTP 端点 |
| `storage` | `storage.get`, `storage.set`, `storage.delete` | 插件持久化存储 |

### 权限规则

1. **`storage` 权限默认授予**，所有插件都有存储能力
2. 未在 `permissions` 中声明的权限无法使用对应 API
3. 非法权限字符串会被自动过滤（不会报错，只是忽略）
4. 权限在插件停用时全部收回，再次激活时重新授予

---

## 6. TS-only 插件开发

### 入口文件约定

入口文件必须导出 `activate` 函数，可选导出 `deactivate` 函数：

```typescript
// index.ts
import type { PluginContext } from '@bedcode/plugin-sdk'
import MyView from './components/MyView.vue'

export async function activate(context: PluginContext): Promise<void> {
  // 注册侧边栏面板
  context.ui.registerSidebarPanel({
    id: 'my-plugin.sidebar',
    title: 'My Panel',
    component: MyView,
  })

  // 注册命令
  context.commands.register('my-plugin.hello', () => {
    console.log('Hello from my plugin!')
  })

  // 注册插件翻译（可选）
  context.i18n.registerMessages('zh-CN', { hello: '你好' })
  context.i18n.registerMessages('en', { hello: 'Hello' })
}

export async function deactivate(): Promise<void> {
  // 清理资源
}
```

### Vue 组件中获取 PluginContext

在 `PluginViewHost` 渲染的组件中，通过 `inject` 获取上下文：

```vue
<script setup lang="ts">
import { inject } from 'vue'
import type { PluginContext } from '@bedcode/plugin-sdk'

const context = inject<PluginContext>('pluginContext')!
</script>
```

### 国际化（i18n）

插件通过 `context.i18n` API 访问宿主 i18n 能力：

```typescript
// 1. 在 activate() 中注册插件翻译
context.i18n.registerMessages('zh-CN', {
  hello: '你好',
  configTitle: '配置',
})
context.i18n.registerMessages('en', {
  hello: 'Hello',
  configTitle: 'Configuration',
})

// 2. 组件内使用 useI18n()（共享宿主 vue-i18n 实例）
import { useI18n } from 'vue-i18n'
const { t } = useI18n()

// 3. 模块级代码使用 SDK 代理
import { getI18n } from '@bedcode/plugin-sdk'
const i18n = getI18n()
i18n.global.t('my-plugin.key')  // 完整 i18n 实例

// 4. 使用 context.i18n.t() 快捷方法（自动添加插件 ID 前缀）
context.i18n.t('hello')  // 等同于 i18n.global.t('com.example.my-plugin.hello')
```

> **注意**：插件翻译 key 会自动添加 `插件ID.` 前缀隔离。注册 `{ hello: '你好' }` 实际存储为 `{ 'com.example.my-plugin.hello': '你好' }`。

### 事件通信

```typescript
// 监听事件（返回 Disposable）
const disposable = context.events.on('my-event', (payload) => {
  console.log('Received:', payload)
})

// 发射事件
context.events.emit('my-event', { data: 'hello' })

// 清理（deactivate 时自动清理，也可手动 dispose）
disposable.dispose()
```

---

## 7. Rust+TS 插件开发（WASM）

Rust+TS 插件将 Rust 代码编译为 **WASM 组件**（Component Model），由宿主 wasmtime 沙箱加载运行，TS 前端通过 invoke 调用 WASM 命令。

### 7.1 Rust 项目配置

**Cargo.toml**：

```toml
[lib]
crate-type = ["cdylib", "rlib"]
```

> `cdylib` 是 wasm32 目标的产物类型，与传统动态库无关。

```bash
# 编译 WASM 组件（release）
cargo build --target wasm32-unknown-unknown --no-default-features --features wasm --release
```

### 7.2 WASM 入口

实现 `WasmPlugin` trait（`bedcode_plugin_api`），通过 `wasm_entry!` 宏生成导出：

```rust
use bedcode_plugin_api::host::{HostConfig, HostFs, HostLog};
use bedcode_plugin_api::types::PluginManifest;
use bedcode_plugin_api::{WasmHost, WasmPlugin};

struct MyPlugin;

impl WasmPlugin for MyPlugin {
    const ID: &'static str = "com.example.my-plugin";

    fn manifest() -> PluginManifest {
        serde_json::from_str(include_str!("../../plugin.json"))
            .expect("plugin.json must be valid PluginManifest")
    }

    fn activate() -> anyhow::Result<()> {
        // 初始化（数据目录授权、注册监听器等）
        Ok(())
    }

    fn invoke_command(name: &str, args: serde_json::Value) -> anyhow::Result<serde_json::Value> {
        // 命令路由
        Ok(serde_json::Value::Null)
    }
}

bedcode_plugin_api::wasm_entry!(MyPlugin);
```

### 7.3 宿主 API（沙箱内能力边界）

| 能力 | 宿主接口 |
|------|---------|
| HTTP 请求 | `WasmHost::http_fetch()`（沙箱内无法直连网络，必须经宿主代理） |
| 文件读写 | `fs_read` / `fs_write` / `fs_copy` Host Function |
| 数据库 | `plugin_db_execute` / `plugin_db_query`（独立 SQLite） |
| 配置 | `config_get` |
| 事件 | `emit_event` / `broadcast_sync` / 消息总线 |

> 完整示例参考官方插件：`plugins/auto-task`、`plugins/ai-chatbox`、`plugins/file-transfer`（`rust/src/`）。

## 8. PluginContext API 参考

`PluginContext` 是插件访问宿主能力的唯一通道，在 `activate(context)` 时获取。

### context.commands — 命令注册表

```typescript
// 注册前端命令
const disposable = context.commands.register('my-cmd', (args) => { /* ... */ })

// 执行命令（先查前端注册，再查 Rust 插件命令）
const result = await context.commands.execute('my-plugin.hello', { name: 'world' })
```

### context.terminal — 终端 API

```typescript
// 发送输入到指定会话（需 terminal:input 权限）
await context.terminal.sendInput('session-id', 'ls -la\n')

// 监听终端输出（需 terminal:output 权限）
const disposable = context.terminal.onOutput((sessionId, data) => {
  console.log(`Output from ${sessionId}:`, data)
})

// 监听终端输入（需 terminal:input 权限）
const disposable = context.terminal.onInput((sessionId, text) => {
  return text.toUpperCase() // 返回修改后的文本，或 null 不修改
})
```

### context.session — 会话 API

```typescript
// 列出所有会话（需 session:read 权限）
const sessions = await context.session.list()

// 获取单个会话（需 session:read 权限）
const session = await context.session.get('session-id')

// 监听会话状态变化（需 session:read 权限）
const disposable = context.session.onStatusChange((event) => { /* ... */ })
```

### context.ui — UI 注册表

```typescript
// 侧边栏面板（需 ui:sidebar 权限）
context.ui.registerSidebarPanel({ id, title, component })

// 工具箱页面（需 ui:toolbox 权限）
context.ui.registerToolboxPage({ id, title, component })

// 状态栏项（需 ui:statusbar 权限）
context.ui.registerStatusBarItem({ id, label, icon?, onClick? })

// 输入扩展（需 ui:input 权限）
context.ui.registerInputExtension({ id, label, icon?, onActivate? })

// 终端工具栏项（需 ui:input 权限）
context.ui.registerTerminalToolbarItem({ id, label, icon?, onClick? })

// 标题栏项（需 ui:statusbar 权限）
context.ui.registerTitleBarItem({ id, label, icon?, onClick? })

// 文件处理器（需 ui:fileHandler 权限）
context.ui.registerFileHandler({ id, extensions, component })
```

### context.events — 事件 API

```typescript
// 监听事件
const disposable = context.events.on('my-event', (payload) => { /* ... */ })

// 发射事件
context.events.emit('my-event', { data: 'hello' })
```

### context.storage — 存储 API

```typescript
// 读取（无需额外权限，storage 默认授予）
const value = await context.storage.get<string>('my-key')

// 写入
await context.storage.set('my-key', { foo: 'bar' })

// 删除
await context.storage.delete('my-key')

// 刷盘（当前为 no-op，存储即时写入）
await context.storage.flush()
```

### context.http — HTTP API

```typescript
// 注册 HTTP 端点（需 network:http 权限）
const disposable = context.http.registerEndpoint('/my-endpoint', async (req) => {
  return { status: 200, body: { message: 'ok' } }
})
```

### context.i18n — 国际化 API

```typescript
// 获取宿主 i18n 实例（vue-i18n I18n 对象）
const i18n = context.i18n.getI18n()
i18n.global.t('some.key')  // 翻译任意宿主 key

// 注册插件翻译（自动添加插件 ID 前缀隔离）
context.i18n.registerMessages('zh-CN', {
  hello: '你好',
  config: '配置',
})
context.i18n.registerMessages('en', {
  hello: 'Hello',
  config: 'Configuration',
})

// 快捷翻译（自动添加插件 ID 前缀）
context.i18n.t('hello')           // 等同于 i18n.global.t('com.example.my-plugin.hello')
context.i18n.t('hello', { name: 'World' })  // 带参数
```

---

## 9. 扩展点详解

### 9.1 侧边栏面板

侧边栏面板显示在桌面端左侧边栏，点击图标切换显示：

```typescript
context.ui.registerSidebarPanel({
  id: 'my-plugin.sidebar',    // 唯一标识
  title: 'My Panel',           // 面板标题
  component: MyViewComponent,  // Vue 组件
})
```

Vue 组件通过 `inject('pluginContext')` 获取 `PluginContext`。

### 9.2 终端工具栏

在终端视图底部工具栏添加按钮：

```typescript
context.ui.registerTerminalToolbarItem({
  id: 'my-plugin.toolbar-action',
  label: 'Action',
  icon: '⚡',
  onClick: () => { /* 触发动作 */ },
})
```

### 9.3 命令面板

声明了 `contributes.commands` 的插件，其命令会出现在命令面板中。前端注册的命令 handler 优先于 Rust 端命令。

### 9.4 终端输入/输出处理

Rust+TS 插件可通过 WASM 命令注册 terminal handler 拦截/修改终端数据：

- `bedcode_plugin_on_terminal_input`：接收 JSON 字符串 `{"sessionId": "...", "text": "..."}`，返回修改后的文本或 `null`
- `bedcode_plugin_on_terminal_output`：同上

> **注意**：当前 terminal handler 签名只接收文本，不含 session_id 上下文。若需会话级区分，建议通过 `session_list` API 获取当前活跃会话。

### 9.5 HTTP 端点

插件可注册 HTTP 端点，外部可通过 `/api/plugin/{plugin-id}/{path}` 访问：

```typescript
context.http.registerEndpoint('/query', async (req) => {
  return { status: 200, body: { result: 'ok' } }
})
```

认证方式：plugin token 或 JWT Bearer token。

---

## 10. 配置系统

插件通过 `contributes.configuration` 声明配置 schema，BedCode 自动生成配置表单。

### 配置存储

配置值存储在插件 storage 的 `config` key 下：

```typescript
// 读取配置
const config = await context.storage.get('config')

// 保存配置
await context.storage.set('config', { apiKey: '...', maxResults: 10 })
```

### 配置页面

已激活的插件在管理页面会显示"配置"链接，路由到 `/plugins/{plugin-id}/config`，自动渲染配置表单。

---

## 11. 测试与调试

### 前端调试

1. 插件通过 `convertFileSrc()` 从 `resources/plugins/desktop/` 加载**编译后的** `index.js`，修改源码后必须先 `vite build` 再刷新页面才能看到变化
2. 可使用 `pnpm exec vite build --watch` 监听源码变更自动重新构建，减少手动 build 步骤
3. 浏览器 DevTools 中查看 `console.log` / `console.error`
4. 插件加载错误会标记为 Error 状态，在管理页面查看详情

### Rust 调试

1. 使用 `tracing::info!` / `tracing::error!` 输出日志
2. 日志通过 Tauri stderr 输出，开发模式下可见
3. WASM 沙箱内 panic 会被宿主捕获，不会崩溃宿主
4. WASM 编译产物需手动复制到 `resources/plugins/desktop/{plugin-id}/` 目录（或使用统一构建脚本）

### 常见调试技巧

- 检查插件是否出现在 `/plugins` 页面
- 查看插件状态是否为 Activated
- 查看浏览器 DevTools 和终端日志
- 使用 `plugin_get_info` 命令检查插件信息

---

## 12. 构建与打包

### TS-only 插件构建

```bash
# 在插件目录下构建
cd plugins/desktop/com.example.my-plugin
pnpm exec vite build

# 产物输出到 src-tauri/resources/plugins/desktop/com.example.my-plugin/index.js
```

**vite.config.ts 示例**：

```typescript
import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import { resolve } from 'path'
import { bedcodePlugin } from '@bedcode/plugin-sdk/vite'

const pluginId = 'com.example.my-plugin'

export default defineConfig({
  plugins: [vue(), bedcodePlugin()],
  build: {
    lib: {
      entry: resolve(__dirname, 'index.ts'),
      formats: ['es'],
      fileName: () => 'index.js',
    },
    outDir: resolve(__dirname, '../../../src-tauri/resources/plugins/desktop', pluginId),
    emptyOutDir: false,
  },
})
```

> **重要**：`bedcodePlugin()` 会自动将 `vue`、`vue-i18n`、`pinia` 标记为外部依赖，并在构建后将 `import` 语句替换为从宿主全局变量读取。**不要**在插件 vite 配置中手动内联这些模块，否则 `provide`/`inject` 会跨 Vue 实例失效。

### Rust+TS 插件构建

```bash
# 1. 编译 Rust WASM 组件
cd rust/
cargo build --target wasm32-unknown-unknown --no-default-features --features wasm --release

# 2. 复制 wasm 到插件目录（或使用统一构建脚本 scripts/build.js）
# target/wasm32-unknown-unknown/release/bedcode_plugin_my_plugin.wasm

# 3. 编译 TS 前端
cd ..
pnpm exec vite build
```

### 插件分发

插件以目录形式分发，将整个 `com.example.my-plugin/` 目录放入 `plugins/desktop/` 即可。

---

## 13. 常见问题

### Q: 插件加载后状态一直是 Loaded，没有变为 Activated？

有 `views` 的插件会自动激活；只有 `commands` / `terminal` 的插件会懒激活（按需激活）。可在管理页面手动开启。

### Q: 权限检查失败？

确保 `plugin.json` 的 `permissions` 数组包含所需权限。前端和 Rust 端都会校验。

### Q: TS-only 插件如何做网络请求？

使用 `fetch` 或第三方 HTTP 库，宿主不会拦截网络请求。如果需要暴露 HTTP 端点供外部访问，则需 `network:http` 权限。

### Q: 插件如何与 Claude Code 交互？

通过 `context.terminal.sendInput()` 向终端发送命令。插件可通过 `terminal:output` 监听输出，解析 Claude Code 的状态。

### Q: 插件按钮点击无反应 / provide/inject 不工作？

确保插件 vite.config.ts 使用了 `bedcodePlugin()`（来自 `@bedcode/plugin-sdk/vite`）。如果 Vue 运行时被内联到插件产物中，`provide`/`inject` 会跨 Vue 实例失效。检查构建产物大小 — 正常应小于 100KB（不含 Vue 运行时），如果超过 200KB 说明 Vue 被内联了。

### Q: 插件如何使用国际化？

1. 组件内使用 `useI18n()`（与宿主共享 vue-i18n 实例）
2. 模块级代码使用 `import { getI18n } from '@bedcode/plugin-sdk'`
3. 注册插件翻译使用 `context.i18n.registerMessages(locale, messages)`
4. 类型引用使用 `import type { PluginContext } from '@bedcode/plugin-sdk'`
