# @bedcode/plugin-sdk-desktop

BedCode Desktop 插件开发工具包 — 提供插件所需的**类型定义**、**运行时代理**、**Vite 构建插件**与**共享 UI 组件**，插件只需依赖本包即可开发，无需引用宿主源码。

> 完整开发指南见仓库文档 [`plugin-dev-desktop.md`](../../plugin-dev-desktop.md)。

[English](README_en.md) | 简体中文

## 安装

```bash
npm install --save-dev @bedcode/plugin-sdk-desktop
```

Peer 依赖（宿主已在运行时提供，插件侧安装用于构建与类型检查）：

| 依赖 | 版本 |
|------|------|
| vite | ^5.0.0 |
| vue | ^3.4.0 |

## 快速开始（推荐：脚手架）

```bash
# 创建插件工程（交互式选择插件名 / 类型；--rust 附带 WASM 后端）
npx bedcode-plugin-desktop create my-plugin --rust

cd my-plugin
npm install
npm run dev        # 浏览器开发环境（Dev Shell：mock 宿主 + HMR）
npm run build      # 构建产物 dist/index.js（可选 -- --resources-dir <目录> 直接复制进宿主）
npm run manifest   # 按源码自动填充 plugin.json 的 contributes / permissions
npm run validate   # 校验 plugin.json 合法性
npm run doctor     # 环境自检
```

构建产物复制到宿主 `plugins/desktop/{plugin-id}/` 目录后，在宿主插件管理页启用。

## 手动创建插件

一个最小插件只需两个文件：

**`plugin.json`** — 插件清单（id / 权限 / 扩展点声明）：

```json
{
  "id": "com.example.my-plugin",
  "name": "My Plugin",
  "version": "0.1.0",
  "description": "示例插件",
  "author": "you",
  "main": "index.js",
  "sandbox": "inline",
  "pluginType": "ts-only",
  "permissions": ["storage", "ui:sidebar"],
  "contributes": {
    "commands": [{ "id": "my-plugin.hello", "title": "Hello" }],
    "views": [{ "id": "my-plugin.sidebar", "type": "sidebar", "title": "My Plugin", "component": "MyPanel" }]
  }
}
```

**`src/index.ts`** — 插件入口（必须导出 `activate`）：

```ts
import { defineComponent, h } from 'vue'
import type { PluginContext } from '@bedcode/plugin-sdk-desktop'

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
| `context.ui` | 注册侧边栏 / 工具箱 / 状态栏 / 输入扩展 / 终端工具栏 / 标题栏 / 文件查看器 | `ui:sidebar` / `ui:toolbox` / `ui:statusbar` / `ui:pageToolbar` / `ui:input` / `ui:fileHandler` |
| `context.events` | 宿主事件订阅与发布（`on` / `emit`） | 订阅默认授予；发布需 `broadcast` |
| `context.storage` | 键值存储（`get` / `set` / `delete` / `flush`） | `storage`（默认附带） |
| `context.http` | 注册 HTTP 端点（供 Agent CLI hooks 等调用） | `network:http` |
| `context.fileService` | 文件服务挂载（`mount`）、对端信息、系统目录 / 文件选择 | `fileservice` |
| `context.i18n` | 宿主 i18n：注册插件翻译、`t()` 快捷翻译 | 默认授予 |
| `context.system` | 系统级操作（如在文件管理器中显示） | `system:open` |

权限声明示例：`"permissions": ["storage", "ui:sidebar", "terminal:output", "network:http"]`。

所有 `register*` / `on*` 调用返回 `Disposable`；插件注销时宿主自动回收，无需手动 `dispose()`。

## 共享模块（避免重复打包 vue 等）

插件构建时 `vue` / `vue-i18n` / `pinia` 会被外部化，运行时从宿主全局 `window.__BEDCODE_SHARED__` 读取。**请通过 SDK 代理函数访问，不要直接操作全局变量**：

```ts
import { getVue, getI18n, getPinia, getRouter, getPluginContext } from '@bedcode/plugin-sdk-desktop'

const { ref, computed } = getVue()      // 宿主 Vue 实例（组件内直接 import 即可，构建期已外部化）
const i18n = getI18n()                  // 宿主 vue-i18n 实例（模块级代码用；组件内用 useI18n()）
const context = getPluginContext()      // 组件 setup 内从 inject 获取 PluginContext
```

## Vite 构建集成

在插件 `vite.config.ts` 中引入 `bedcodePlugin()`，负责将共享模块标记为 external 并改写为全局变量读取：

```ts
import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import { bedcodePlugin } from '@bedcode/plugin-sdk-desktop/vite'

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
import { defineConfiguration, PLUGIN_CONFIG_STORAGE_KEY } from '@bedcode/plugin-sdk-desktop'

const config = defineConfiguration('My Plugin Settings', {
  apiKey: { type: 'string', title: 'API Key' },
  maxRetries: { type: 'number', title: 'Max Retries', default: 3 },
  debugMode: { type: 'boolean', title: 'Debug Mode', default: false },
})

// 运行时读取用户配置
const saved = await context.storage.get<typeof config.properties>(PLUGIN_CONFIG_STORAGE_KEY)
```

## 事件常量

宿主事件名以常量形式导出（与 Rust SDK 同步，单一事实来源）：

```ts
import { EVENT_TASK_STATUS_CHANGED } from '@bedcode/plugin-sdk-desktop'

context.events.on(EVENT_TASK_STATUS_CHANGED, (payload) => {
  // 任务状态变更（如 Agent CLI idle → in_progress）
})
```

## 共享 UI 组件

`./ui` 子路径导出的组件遵循宿主设计 token，插件 UI 与宿主观感一致（**禁止使用原生 `<select>` 等系统控件外观**）：

```vue
<script setup lang="ts">
import { ref } from 'vue'
import Select from '@bedcode/plugin-sdk-desktop/ui'

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

当前提供：`Select`（下拉选择，支持 `update:modelValue` / `open` 事件、`md` / `sm` 尺寸）。

## 子路径导出总览

| 子路径 | 内容 |
|--------|------|
| `@bedcode/plugin-sdk-desktop` | 主 API：类型 + 运行时代理 + 配置助手 + 事件常量 |
| `@bedcode/plugin-sdk-desktop/vite` | `bedcodePlugin()` Vite 构建插件 |
| `@bedcode/plugin-sdk-desktop/types` | 纯类型导出（仅类型，无运行时） |
| `@bedcode/plugin-sdk-desktop/ui` | 共享 Vue 组件 |

## 本地开发本 SDK

```bash
npm run build      # tsup 构建 dist（ESM + d.ts）
npm run test:run   # vitest run
```
