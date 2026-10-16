# @binblink/bedcode-plugin-sdk-desktop

BedCode Desktop wasm 应用开发工具包 — 提供应用所需的**类型定义**、**运行时代理**、**Vite 构建插件**与**共享 UI 组件**，应用只需依赖本包即可开发，无需引用宿主源码。

> 术语：本仓把「插件」称为 wasm 应用；包名 / CLI 名 / 标识符保留 `plugin` 原词（`plugin.json`、`bedcode-plugin-api`）。
>
> 完整开发指南见仓库文档 [`wasm-app-dev-desktop.md`](../../wasm-app-dev-desktop.md)；API 与扩展点参考 [`wasm-app-dev-guide.md`](../../../bedcode-desktop/docs/knowledge/wasm-app-dev-guide.md)。

[English](README_en.md) | 简体中文

## 安装

```bash
pnpm add -D @binblink/bedcode-plugin-sdk-desktop
```

Peer 依赖（宿主已在运行时提供，插件侧安装用于构建与类型检查）：

| 依赖 | 版本 |
|------|------|
| vite | ^5.0.0 |
| vue | ^3.4.0 |

## 快速开始（推荐：脚手架）

```bash
# 创建 wasm 应用工程（id 与 name 为位置参数，非交互；--rust 附带 WASM 后端脚手架）
bedcode-plugin-desktop create com.example.my-app "My App" --author "you" --rust

cd my-app
pnpm install
ppnpm run dev        # 浏览器开发环境（Dev Shell：空壳宿主 + HMR）
ppnpm run build      # 构建产物 dist/（vite 前端 + rust-ts 时 cargo wasm32）
ppnpm run manifest   # 按源码自动填充 plugin.json 的 contributes / permissions
```

更多命令（直接在应用工程里跑 CLI，非 npm script）：

```bash
bedcode-plugin-desktop build --resources-dir ../bedcode-desktop/src-tauri/resources/plugins/desktop
# 把产物复制到 src-tauri/resources/plugins/desktop/{id}/（内置 wasm 应用随安装包分发的方式）

bedcode-plugin-desktop build --frontend-only / --rust-only   # 只构建一半（rust-ts）
bedcode-plugin-desktop dev [pluginDir] [--port <port>] [--host] [--open]
bedcode-plugin-desktop manifest --check   # 只检查不写入（CI）
bedcode-plugin-desktop validate [--dir]   # 校验 plugin.json（id 格式 / 必填 / pluginType / 权限白名单 / 产物存在性）
bedcode-plugin-desktop doctor             # 环境自检（Node / Rust / wasm32 target / dev-shell / SDK）
```

构建产物复制到宿主 `src-tauri/resources/plugins/desktop/{id}/` 后，在宿主应用管理页启用。
用户侧安装走 zip 包（桌面端「插件」页 → 选 zip，落盘 `app_data_dir/plugins/{id}/`）。

## 手动创建插件

一个最小插件只需两个文件：

**`plugin.json`** — 插件清单（id / 权限 / 扩展点声明）：

```json
{
  "id": "com.example.my-plugin",
  "name": "My Plugin",
  "version": "0.1.0",
  "platform": "desktop",
  "description": "示例插件",
  "author": "you",
  "main": "index.js",
  "pluginType": "ts-only",
  "permissions": ["storage", "ui:sidebar"],
  "contributes": {
    "commands": [{ "id": "my-plugin.hello", "title": "Hello" }],
    "views": [{ "id": "my-plugin.sidebar", "type": "sidebar", "title": "My Plugin", "component": "MyPanel" }]
  }
}
```

必填字段：`name` / `version` / `platform` / `main` / `pluginType` / `permissions` / `contributes`
（校验真源 `bin/manifest-validate.js`；宿主反序列化无缺省，缺失即拒装）。
`pluginType` 取值 `rust` / `rust-ts` / `ts-only`；`rust-ts` 时必须给 `rustLibrary`
（与 Cargo.toml 包名一致）。

> `sandbox` 已从 manifest 退役（审计票 06 裁决 2：前端不做隔离）——Rust 端类型会
> 直接忽略并丢弃该字段，写进清单无效，请勿再声明。

**`src/index.ts`** — 插件入口（必须导出 `activate`）：

```ts
import { defineComponent, h } from 'vue'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-desktop'

export async function activate(context: PluginContext): Promise<void> {
  // 注册侧边栏面板（组件内可通过 inject('pluginContext') 再次获取 context）
  context.ui.registerSidebarPanel({
    id: 'my-plugin.sidebar',
    title: 'My Plugin',
    order: 600,
    component: defineComponent({
      render: () => h('div', 'Hello BedCode!'),
    }),
  })
}

export async function deactivate(): Promise<void> {
  // 清理资源（context 注册的所有 Disposable 宿主会自动回收）
}
```

## PluginContext API

`activate(context)` 接收的 `PluginContext` 是插件访问宿主能力的**唯一通道**，各 API 按权限分组：

| API | 说明 | 所需权限 |
|-----|------|----------|
| `context.commands` | 注册 / 执行命令 | 默认授予 |
| `context.terminal` | 向会话发送输入、订阅输出 / 输入 | `terminal:input` / `terminal:output` / `terminal:observe` |
| `context.session` | 会话列表、状态变更订阅 | `session:read` / `session:write` |
| `context.ui` | 注册侧边栏 / 工具箱 / 状态栏 / 设置分组 / 输入扩展 / 终端工具栏 / 标题栏 / 文件处理器（`registerFileHandler`） | `ui:sidebar` / `ui:toolbox` / `ui:statusbar` / `ui:pageToolbar` / `ui:settings` / `ui:input` / `ui:fileHandler` |
| `context.events` | 宿主事件订阅与发布（`on` / `emit`） | 订阅默认授予；发布需 `broadcast` |
| `context.storage` | 键值存储（`get` / `set` / `delete` / `flush`） | `storage`（**桌面端无默认授予，必须显式声明**） |
| `context.http` | 注册 HTTP 端点（`registerEndpoint`，供 Agent CLI hooks 等调用） | `network:http` |
| `context.i18n` | 宿主 i18n：注册应用翻译、`t()` 快捷翻译、`getI18n()` 取实例 | 默认授予 |
| `context._disposables` | 宿主在注销时统一回收的 Disposable 列表（内部使用） | — |
| `context.id` / `context.extensionPath` | 应用 id / 安装目录路径（只读） | — |


权限声明示例：`"permissions": ["storage", "ui:sidebar", "terminal:output", "network:http"]`。

所有 `register*` / `on*` 调用返回 `Disposable`；插件注销时宿主自动回收，无需手动 `dispose()`。

## 共享模块（避免重复打包 vue 等）

插件构建时 `vue` / `vue-i18n` / `pinia` / `vue-sonner` 会被外部化，运行时从宿主全局 `window.__BEDCODE_SHARED__` 读取。**请通过 SDK 代理函数访问，不要直接操作全局变量**：

> 收录标准：模块内部持有进程级单例状态，自带第二份实例会导致状态错位。`vue-sonner` 是宿主 toast 队列本体——插件 `import { toast } from 'vue-sonner'` 会被改写成读宿主实例，因此 `toast.success(...)` 与宿主提示同队列、同外观（自带第二份则提示不显示）。

```ts
import { getVue, getI18n, getPinia, getRouter, getPluginContext } from '@binblink/bedcode-plugin-sdk-desktop'

const { ref, computed } = getVue()      // 宿主 Vue 实例（组件内直接 import 即可，构建期已外部化）
const i18n = getI18n()                  // 宿主 vue-i18n 实例（模块级代码用；组件内用 useI18n()）
const context = getPluginContext()      // 组件 setup 内从 inject 获取 PluginContext
```

## Vite 构建集成

在插件 `vite.config.ts` 中引入 `bedcodePlugin()`，负责将共享模块标记为 external 并改写为全局变量读取：

```ts
import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import { bedcodePlugin } from '@binblink/bedcode-plugin-sdk-desktop/vite'

export default defineConfig({
  plugins: [vue(), bedcodePlugin()],
  build: {
    lib: {
      entry: 'src/index.ts',
      formats: ['es'],
      fileName: () => 'index.js',
    },
    outDir: 'dist',
  },
})
```

产物文件名必须为 `index.js`（与 `plugin.json` 的 `main` 对应）。

## Rust WASM 后端（`--rust` 插件）

`--rust` 脚手架附带 Rust 侧 SDK crate **`bedcode-plugin-api`**（本包 `rust/` 目录，MIT），用于编写编译为 WASM 组件、在宿主 wasmtime 沙箱内运行的后端逻辑。

### 启用

```toml
[dependencies]
bedcode-plugin-api = { path = "<宿主>/packages/plugin-sdk-desktop/rust", features = ["wasm"] }
```

`wasm` feature 提供：

- `WasmPlugin` trait + `wasm_entry!` 宏 — 生成 WIT 契约（`rust/wit/bedcode.wit`，单一事实来源）定义的全部组件导出
- `WasmHost` — 宿主 API 绑定（import 后端），经 bindgen 调用宿主能力
- `#[plugin_api]` 属性宏（由 `rust-macros/` 的 `bedcode-plugin-api-macros` crate 提供）— 插件互调 IDL：trait 定义 → JSON-RPC 分派 + client 生成 + 防漂移比对

### 最小示例

```rust
use bedcode_plugin_api::{CommandArgs, WasmHost, WasmPlugin};

struct MyPlugin;

impl WasmPlugin for MyPlugin {
    fn activate() -> anyhow::Result<()> {
        // 经 WasmHost 访问宿主能力（事件订阅、存储、HTTP 端点等）
        Ok(())
    }
}

bedcode_plugin_api::wasm_entry!(MyPlugin);
```

### 构建

```bash
cargo build --target wasm32-unknown-unknown --no-default-features --features wasm --release
```

产物为 WASM 组件（Component Model），宿主以 wasmtime 沙箱加载运行；插件崩溃不影响宿主。不带 `wasm` feature 时可作为普通 Rust crate 使用（静态注册 / 库依赖）。

## 配置声明

插件配置在 `plugin.json` 的 `contributes.configuration` 声明，宿主据此渲染配置页；运行时经 `context.storage` 读写（统一键 `PLUGIN_CONFIG_STORAGE_KEY = 'config'`）。SDK 提供声明式助手保持两端一致：

```ts
import { defineConfiguration, PLUGIN_CONFIG_STORAGE_KEY } from '@binblink/bedcode-plugin-sdk-desktop'

const config = defineConfiguration('My Plugin Settings', {
  apiKey: { type: 'string', title: 'API Key' },
  maxRetries: { type: 'number', title: 'Max Retries', default: 3 },
  debugMode: { type: 'boolean', title: 'Debug Mode', default: false },
})

// 运行时读取用户配置
const saved = await context.storage.get<typeof config.properties>(PLUGIN_CONFIG_STORAGE_KEY)
```

## 事件常量

事件主题遵循 `{domain}:{action}` 命名规范；各领域事件名由对应 wasm 应用工程自持（单一事实来源），
SDK 不内置具体业务事件名。例如终端会话应用（`wasm-apps/terminal-session`）订阅的任务状态事件：

```ts
// 应用内订阅本领域事件（字符串字面量或抽为本地常量）
context.events.on('task:status-changed', (payload) => {
  // 任务状态变更（如 Agent CLI idle → in_progress）
})
```

## 共享 UI 组件

`./ui` 子路径导出的组件遵循宿主设计 token，插件 UI 与宿主观感一致（**禁止使用原生 `<select>` 等系统控件外观**）：

```vue
<script setup lang="ts">
import { ref } from 'vue'
import Select from '@binblink/bedcode-plugin-sdk-desktop/ui'

const value = ref('a')
const options = [
  { value: 'a', label: '选项 A' },
  { value: 'b', label: '选项 B' },
]
</script>

<template>
  <Select v-model="value" :options="options" size="sm" />
</template>
```

当前提供（每个子路径 = 一个独立导出，默认导出组件本身）：

| 子路径 | 组件 | 用途 |
|--------|------|------|
| `./ui` | `Select` | 下拉选择（`update:modelValue` / `open` 事件，`md` / `sm` 尺寸） |
| `./ui/markdown-editor` | `MarkdownEditor` | Markdown 编辑区（语法工具栏 + 预览切换，`v-model`） |
| `./ui/terminal-input-rail` | `TerminalInputRail` | 终端输入导航条（终端右侧悬浮，一根横线 = 一次用户输入，位置映射 buffer 相对位置） |
| `./ui/plugin-global-dialog` | `PluginGlobalDialog` | 宿主全局弹窗（应用根挂载一次；渲染由全局弹窗控制器驱动，插件经 `context.ui.showDialog` 操控，挂载方无 props） |
| `./ui/zoom-compensation` | （无组件） | fixed 定位覆盖层在根元素 CSS zoom 下的坐标换算（自校准；当前 F=1，作防御性 no-op 保留） |

## 子路径导出总览

| 子路径 | 内容 |
|--------|------|
| `@binblink/bedcode-plugin-sdk-desktop` | 主 API：类型 + 运行时代理 + 配置助手 + 事件常量 |
| `@binblink/bedcode-plugin-sdk-desktop/vite` | `bedcodePlugin()` Vite 构建插件 |
| `@binblink/bedcode-plugin-sdk-desktop/types` | 纯类型导出（仅类型，无运行时） |
| `@binblink/bedcode-plugin-sdk-desktop/ui` | 共享 Vue 组件：`Select` |
| `@binblink/bedcode-plugin-sdk-desktop/ui/markdown-editor` | `MarkdownEditor` |
| `@binblink/bedcode-plugin-sdk-desktop/ui/terminal-input-rail` | `TerminalInputRail` |
| `@binblink/bedcode-plugin-sdk-desktop/ui/plugin-global-dialog` | `PluginGlobalDialog` |
| `@binblink/bedcode-plugin-sdk-desktop/ui/zoom-compensation` | zoom 坐标换算工具 |

## 本地开发本 SDK

```bash
pnpm run build      # tsup 构建 dist（ESM + d.ts）
pnpm run test:run   # vitest run
```
