# @binblink/plugin-sdk-mobile

BedCode 移动端插件开发工具包 — 提供插件所需的**类型定义**、**运行时代理**、**Vite 构建插件**与**共享 UI 组件**。相比桌面端 SDK，额外封装了移动端专属能力：SAF 存储访问、对话框 / 系统通知、动态路由、生命周期钩子与 dev-shell 演示数据协议。

> 完整开发指南见仓库文档 [`plugin-dev-mobile.md`](../../plugin-dev-mobile.md)。

[English](README_en.md) | 简体中文

## 安装

```bash
npm install --save-dev @binblink/plugin-sdk-mobile
```

Peer 依赖（宿主已在运行时提供，插件侧安装用于构建与类型检查）：

| 依赖 | 版本 |
|------|------|
| vite | ^5.0.0 |
| vue | ^3.4.0 |

## 快速开始（推荐：脚手架）

```bash
# 创建插件工程（交互式选择插件名 / 类型；--rust 附带 WASM 后端）
npx bedcode-plugin create my-plugin --rust

cd my-plugin
npm install
npm run dev        # 浏览器开发环境（Dev Shell：mock 宿主 + 移动端骨架，HMR）
npm run build      # 构建产物 dist/index.js
npm run package    # 打包 dist/{id}.zip 插件包
npm run manifest   # 按源码自动填充 plugin.json 的 contributes / permissions
npm run validate   # 校验 plugin.json 合法性
npm run doctor     # 环境自检
```

> [!NOTE]
> `npm run dev` 首次运行自动安装 dev-shell 依赖，浏览器打开 http://localhost:5173；WASM 后端与 SAF / 系统返回键等真机专属能力仍需真机验证。

**安装到手机**：将 `dist/{id}.zip` 传到手机 → BedCode Mobile → 插件管理 → 从文件安装。

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
  "icon": "🧩",
  "main": "index.js",
  "pluginType": "ts-only",
  "permissions": ["storage", "ui:input"],
  "contributes": {
    "commands": [{ "id": "my-plugin.hello", "title": "Hello" }],
    "terminal": {
      "toolbarItems": [{ "id": "my-plugin.toolbar", "title": "My Plugin", "icon": "🧩" }]
    }
  }
}
```

**`src/index.ts`** — 插件入口（必须导出 `activate`）：

```ts
import type { PluginContext } from '@binblink/plugin-sdk-mobile'

export async function activate(context: PluginContext): Promise<void> {
  // 注册终端工具栏按钮
  context.ui.registerTerminalToolbarItem({
    id: 'my-plugin.toolbar',
    label: 'My Plugin',
    icon: '🧩',
    onClick: () => {
      context.dialogs.showToast('Hello BedCode!', 'success')
    },
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
| `context.terminal` | 向会话发送输入、订阅输出 | `terminal:input` / `terminal:output` |
| `context.session` | 会话列表、状态变更订阅 | `session:read` / `session:write` |
| `context.ui` | 工具箱页面、底部导航 Tab、终端工具栏、设置区、动态路由 | `ui:toolbox` / `ui:navtab` / `ui:settings` / `ui:input` / `ui:route` / `ui:back` |
| `context.events` | 宿主事件订阅与发布（`on` / `emit`） | 默认授予 |
| `context.storage` | 键值存储（`get` / `set` / `delete`） | `storage`（默认附带） |
| `context.fileService` | 文件服务挂载、SAF 存储访问、系统选择器 | `fileservice` |
| `context.lifecycle` | 应用生命周期钩子（启动 / 连接 / 会话 / 终端输入输出） | 默认授予 |
| `context.dialogs` | 对话框（`showDialog` / `showConfirm` / `showPrompt` / `showToast`） | 默认授予 |
| `context.notifications` | 系统通知 | 默认授予 |
| `context.status` | 生命周期状态上报（`reportReady` / `reportError`） | 默认授予 |
| `context.logger` | 分级日志（`info` / `debug` / `warn` / `error`） | 默认授予 |
| `context.i18n` | 注册插件翻译、`t()` 快捷翻译 | 默认授予 |
| `context.system` | 系统级操作（如用系统查看器打开文件） | `system:open` |

所有 `register*` / `on*` 调用返回 `Disposable`；插件注销时宿主自动回收，无需手动 `dispose()`。

## 移动端专属能力

### 动态路由

插件可注册任意深度的页面路由，宿主挂载到 `/mobile/plugins/{pluginId}/{id}`：

```ts
context.ui.registerRoute({
  id: 'settings',                    // 可含 '/' 支持深路径
  title: '插件设置',                  // 宿主页头标题（header: false 时省略）
  component: SettingsPage,           // 任意 Vue 组件
})

context.ui.openPage('settings')      // 整体跳转
context.ui.goBack()                  // 返回上一页
```

> [!NOTE]
> `context.ui.onBackPressed()` 可接管 Android 系统返回键（仅 Android 真机触发）；iOS / dev-shell 静默降级为永不触发。

### 对话框与通知

```ts
const ok = await context.dialogs.showConfirm({ title: '确认', message: '确定删除？', variant: 'danger' })
const name = await context.dialogs.showPrompt({ title: '命名', inputPlaceholder: '输入名称' })
context.dialogs.showToast('已保存', 'success')

await context.notifications.notify('任务完成', '会话 #3 已结束')
```

### 生命周期钩子

```ts
context.lifecycle.onAuthSuccess(() => { /* 配对成功后刷新对端数据 */ })
context.lifecycle.onDisconnect((reason) => { /* 与桌面端断开 */ })
context.lifecycle.onTerminalOutput((sessionId, data) => { /* 实时终端输出 */ })
```

### 对端桌面端 HTTP 能力（MobileHostApi）

经 `getMobileApi()` 获取响应式连接状态与对端 REST API（任务队列、会话自动模式、任务历史、定时任务等）：

```ts
import { getMobileApi } from '@binblink/plugin-sdk-mobile'

const api = getMobileApi()
console.log(api.isConnected.value, api.activeSessionId.value)

const res = await api.httpTaskQueueList(sessionId)
// → { code, message, data: { tasks, queue_count } }
```

### SAF 存储访问（Android）

`fileService.saf` 提供分区存储下的目录树遍历与中转复制（`listTree` / `copyStart` / `copyStatus` / `copyCancel`）；`pickSharedDirectory` 弹系统目录树选择器并持久化授权，`requestAllFilesAccess` 引导授予全部文件访问权限。

> [!NOTE]
> SAF 相关 API 仅 Android 可用，其他平台 reject；`pickDirectory` / `pickFile` 在云盘 / iOS 等不支持的 provider 上也会 reject，插件应捕获后回退到手动路径输入。

### dev-shell 演示数据（devMock）

插件入口可导出 `devMock` 字段，为浏览器开发环境提供业务演示数据（任务队列种子、SAF 目录树等）；真实宿主忽略该字段，无需条件编译：

```ts
export const devMock: PluginDevMock = {
  queueSeed: [{ id: '1', prompt: '示例任务', position: 0, status: 'pending', created_at: '' }],
}
```

## 共享模块（避免重复打包 vue 等）

插件构建时 `vue` / `vue-i18n` / `pinia` 会被外部化，运行时从宿主全局 `window.__BEDCODE_SHARED__` 读取。**请通过 SDK 代理函数访问，不要直接操作全局变量**：

```ts
import { getVue, getPinia, getRouter, getPresetTasks, getMobileApi, getPluginContext } from '@binblink/plugin-sdk-mobile'

const { ref, computed } = getVue()      // 宿主 Vue 实例（组件内直接 import 即可，构建期已外部化）
const presetTasks = getPresetTasks()    // 宿主预设任务 composable
const context = getPluginContext()      // 组件 setup 内从 inject 获取 PluginContext
```

## Vite 构建集成

在插件 `vite.config.ts` 中引入 `bedcodePlugin()`，负责将共享模块标记为 external 并改写为全局变量读取：

```ts
import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import { bedcodePlugin } from '@binblink/plugin-sdk-mobile/vite'

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

`--rust` 脚手架附带 Rust 侧 SDK crate **`bedcode-plugin-api-mobile`**（本包 `rust/` 目录，MIT），用于编写编译为 WASM 组件、在宿主 wasmtime 沙箱内运行的后端逻辑。

### 启用

```toml
[dependencies]
bedcode-plugin-api-mobile = { path = "<宿主>/packages/plugin-sdk-mobile/rust", features = ["wasm"] }
```

`wasm` feature 提供：

- `WasmPlugin` trait + `wasm_entry!` 宏 — 生成 WIT 契约（`rust/wit/bedcode.wit`，单一事实来源）定义的全部组件导出
- `WasmHost` — 宿主 API 绑定（import 后端），经 bindgen 调用宿主能力

### 最小示例

```rust
use bedcode_plugin_api_mobile::{CommandArgs, WasmHost, WasmPlugin};

struct MyPlugin;

impl WasmPlugin for MyPlugin {
    fn activate() -> anyhow::Result<()> {
        // 经 WasmHost 访问宿主能力（事件订阅、存储、文件服务等）
        Ok(())
    }
}

bedcode_plugin_api_mobile::wasm_entry!(MyPlugin);
```

### 构建

```bash
cargo build --target wasm32-unknown-unknown --no-default-features --features wasm --release
```

产物为 WASM 组件（Component Model），宿主以 wasmtime 沙箱加载运行；插件崩溃不影响宿主。SAF / 系统返回键等真机专属能力需在 Android 真机验证。

## 共享 UI 组件

`./ui` 子路径导出的组件遵循宿主设计 token，插件 UI 与宿主观感一致（**禁止使用原生 `<select>` 等系统控件外观**）：

```vue
<script setup lang="ts">
import { ref } from 'vue'
import Select from '@binblink/plugin-sdk-mobile/ui'

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
| `@binblink/plugin-sdk-mobile` | 主 API：类型 + 运行时代理 |
| `@binblink/plugin-sdk-mobile/vite` | `bedcodePlugin()` Vite 构建插件 |
| `@binblink/plugin-sdk-mobile/types` | 纯类型导出（仅类型，无运行时） |
| `@binblink/plugin-sdk-mobile/ui` | 共享 Vue 组件 |

## 本地开发本 SDK

```bash
npm run build      # tsup 构建 dist（ESM + d.ts）
npm run test:run   # vitest run
```
