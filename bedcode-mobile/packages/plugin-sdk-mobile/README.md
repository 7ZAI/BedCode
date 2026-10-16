# @binblink/bedcode-plugin-sdk-mobile

BedCode 移动端 wasm 应用开发工具包 — 提供应用所需的**类型定义**、**运行时代理**、**Vite 构建插件**与**共享 UI 组件**。相比桌面端 SDK，额外封装了移动端专属能力：SAF 存储访问、对话框 / 系统通知、动态路由、生命周期钩子与 dev-shell 演示数据协议。

> 术语：本仓把「插件」称为 wasm 应用；包名 / CLI 名 / 标识符保留 `plugin` 原词（`plugin.json`、`bedcode-plugin-api-mobile`）。
>
> 完整开发指南见仓库文档 [`wasm-app-dev-mobile.md`](../../wasm-app-dev-mobile.md)；开发红线清单 [`wasm-app-development-checklist.md`](../../../docs/knowledge/wasm-app-development-checklist.md)。

[English](README_en.md) | 简体中文

## 安装

```bash
pnpm add -D @binblink/bedcode-plugin-sdk-mobile
```

Peer 依赖（宿主已在运行时提供，插件侧安装用于构建与类型检查）：

| 依赖 | 版本 |
|------|------|
| vite | ^5.0.0 |
| vue | ^3.4.0 |

## 快速开始（推荐：脚手架）

```bash
# 创建 wasm 应用工程（id 与 name 为位置参数，非交互；默认带 WASM 后端，--ts-only 仅前端）
bedcode-plugin create com.example.my-app "My App" --author "you"

cd my-app
pnpm install
ppnpm run dev        # 浏览器开发环境（Dev Shell：空壳宿主 + 移动端骨架，HMR）
ppnpm run build      # 构建产物 dist/
ppnpm run package    # 打包 dist/{id}.zip 插件包
```

更多命令（直接在应用工程里跑 CLI，非 npm script）：

```bash
bedcode-plugin manifest [--check]   # 按源码自动填充 plugin.json 的 contributes / permissions
bedcode-plugin validate [--dir]     # 校验 plugin.json 合法性
bedcode-plugin doctor               # 环境自检
bedcode-plugin build --watch        # 监听重建
bedcode-plugin dev --host           # 监听局域网，手机浏览器访问 http://<电脑IP>:5173
```

> [!NOTE]
> `ppnpm run dev` 首次运行自动安装 dev-shell 依赖，浏览器打开 http://localhost:5173；WASM 后端与 SAF / 系统返回键等真机专属能力仍需真机验证。

**安装到手机**：将 `dist/{id}.zip` 传到手机 → BedCode Mobile → 插件管理 → 从文件安装（支持远程下载 + SHA256 校验）。

## 手动创建插件

一个最小插件只需两个文件：

**`plugin.json`** — 插件清单（id / 权限 / 扩展点声明）：

```json
{
  "id": "com.example.my-plugin",
  "name": "My Plugin",
  "version": "0.1.0",
  "platform": "mobile",
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
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-mobile'

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
| `context.commands` | 注册 / 执行命令（`register` / `execute`）——含本应用的自持业务命令（如 `file-transfer.list-remote`） | 默认授予 |
| `context.session` | 会话列表、单会话查询、状态变更订阅 | `session:read` / `session:write` |
| `context.ui` | 宿主壳运行面 / 首页卡片 / 胶囊菜单 / 平台设置入口 / 动态路由（`registerSurface` / `registerSlot` / `registerCapsuleItem` / `registerSettingsEntry` / `registerRoute`）、跳转与返回（`openPage` / `goBack`）、系统返回键接管（`onBackPressed`）、全局弹窗（`showDialog`） | `ui:route` / `ui:back` / `ui:dialog` / `ui:settings` |
| `context.events` | 宿主事件订阅与发布（`on` / `emit`） | 默认授予 |
| `context.storage` | 键值存储（`get` / `set` / `delete`） | `storage`（**移动端恒授予**，见下方说明） |
| `context.lifecycle` | 应用生命周期钩子（`onAppStartup` / `onAppShutdown` / `onAuthSuccess` / `onDisconnect` / `onSessionCreated` / `onSessionStopped`） | 默认授予 |
| `context.dialogs` | 对话框（`showDialog` / `showConfirm` / `showPrompt` / `showToast`） | 默认授予 |
| `context.notifications` | 系统通知（`notify(title, body?)`） | 默认授予 |
| `context.status` | 生命周期状态上报（`reportReady` / `reportError`） | 默认授予 |
| `context.logger` | 分级日志（`info` / `debug` / `warn` / `error`） | 默认授予 |
| `context.i18n` | 注册应用翻译、`t()` 快捷翻译 | 默认授予 |
| `context.system` | 系统级操作（`openFile` / `revealInDir` / `revealReceivedFileLocation` / `requestAllFilesAccess` / `openDownloadDir`） | `system:open` |
| `context._disposables` | 宿主在注销时统一回收的 Disposable 列表（内部使用） | — |

> 注：移动端**无** `context.terminal` / `context.fileService` / `context.http`（那三个仅桌面端有）。
> 移动端终端输出经宿主壳运行面订阅（业务应用自持协议），文件访问经 `system` 域 API
> （受 `system:open` 与 SAF 授权约束），跨设备传输经 `context.commands` 自持命令
> （底层对等网络原语在业务核 crate）。

> 权限**按风险域按需声明**，除 `storage` 外无默认位：写入 manifest `permissions` 后由宿主按生效权限授予，未声明即该能力被拒。
> 例外是 `storage`——移动端把它设为**恒授予**（插件自身配置空间的读写，关掉会让应用无法工作），因此不提供开关：
> 宿主 `src-tauri/src/plugin/approval.rs::effective_permissions` 无条件 `insert("storage")`，
> 壳内 `src/shell/permissions.ts::SHELL_LOCKED_PERMISSIONS` 把它标为锁定项。

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
context.lifecycle.onAppStartup(() => { /* 应用启动（仅前台启动时触发） */ })
context.lifecycle.onAuthSuccess(() => { /* 配对成功后刷新对端数据 */ })
context.lifecycle.onDisconnect((reason) => { /* 与桌面端断开 */ })
context.lifecycle.onSessionCreated((sessionId) => { /* 新会话建立 */ })
context.lifecycle.onSessionStopped((sessionId) => { /* 会话结束 */ })
context.lifecycle.onAppShutdown(() => { /* 应用退出前清理 */ })
```

> `onTerminalInput` / `onTerminalOutput` 已随票 15 阶段 B 退役——终端输入输出不再经
> 生命周期钩子，改由业务应用自持订阅通道（宿主壳运行面）。

### 对端桌面端 HTTP 能力（MobileHostApi）

经 `getMobileApi()` 获取响应式连接状态与对端 REST 客户端（任务队列、会话自动模式、
任务历史、定时任务等均在业务应用自持的 HTTP 路径中调）：

```ts
import { getMobileApi } from '@binblink/bedcode-plugin-sdk-mobile'

const api = getMobileApi()
console.log(api.isConnected.value, api.activeSessionId.value, api.isPaired.value)

// 通用 REST 客户端（路径与载荷形状由业务应用自持）
const res = await api.httpRequest<{ tasks: unknown[]; queue_count: number }>(
  `/task-queue/${api.activeSessionId}`, { method: 'GET' },
)

await api.loadActiveSessions()          // 刷新活动会话
api.onSessionEvent((evt) => { /* 会话事件推送 */ })
const settings = await api.readAllSettings()   // 读全部设置（扁平键值表）
await api.writeSetting('my-key', 'my-value')   // 写单个设置键
```

### 文件与系统访问

移动端**无** `context.fileService`。文件相关能力分两路：

- **`context.system`**（`system:open` 权限）：`openFile(path, displayName?)` 用系统查看器打开、
  `revealInDir(path)` / `revealReceivedFileLocation(fileName)` 在文件管理器中定位、
  `openDownloadDir()` 打开公共下载目录、`requestAllFilesAccess()` 引导授予
  MANAGE_EXTERNAL_STORAGE（未授权时访问公共下载目录会收到以 `needs_all_files_access` 开头的 reject，
  应捕获后弹引导弹窗而非静默失败）。
- **跨设备文件传输**：经 `context.commands` 调业务应用自持命令（如 `file-transfer.list-remote`），
  底层对等网络 / 断点续传在业务核 crate（`packages/bedcode-file-transfer-core`），
  不暴露为共享 SDK API。

### dev-shell 演示数据（devMock）

插件入口可导出 `devMock` 字段，为浏览器开发环境提供业务演示数据（任务队列种子、SAF 目录树等）；真实宿主忽略该字段，无需条件编译：

```ts
export const devMock: PluginDevMock = {
  queueSeed: [{ id: '1', prompt: '示例任务', position: 0, status: 'pending', created_at: '' }],
}
```

> `PluginDevMock` 的类型是 `Record<string, unknown>`——键名由应用与 dev-shell 约定，
> 无编译期校验。

## 共享模块（避免重复打包 vue 等）

插件构建时 `vue` / `vue-i18n` / `pinia` 会被外部化，运行时从宿主全局 `window.__BEDCODE_SHARED__` 读取。**请通过 SDK 代理函数访问，不要直接操作全局变量**：

```ts
import { getVue, getPinia, getRouter, getPresetTasks, getMobileApi, getPluginContext } from '@binblink/bedcode-plugin-sdk-mobile'

const { ref, computed } = getVue()      // 宿主 Vue 实例（组件内直接 import 即可，构建期已外部化）
const presetTasks = getPresetTasks()    // 宿主预设任务 composable
const context = getPluginContext()      // 组件 setup 内从 inject 获取 PluginContext
```

### 应用运行期与 store（Pinia）

宿主按「一次运行期」隔离插件 store：**启动（activate）到停止（deactivate，含多任务上滑 / × /
详情「停用」/ 卸载）之间的 store 属于该次运行期，停止即整体作废**——再次启动是全新 store
（回到 setup 初始 state），不会读到上一次运行期残留。

由此有两条契约：

1. **store 定义写在模块顶层**（`const useX = defineStore(...)`）。宿主在装载模块时记录归属，
   模块被 ESM 缓存、只求值一次也不影响下次运行期拿到新作用域；写在异步 chunk 里会回退宿主
   Pinia 并打 warn（不保证随应用停止作废）。
2. **模块级可变单例状态必须在 `deactivate` 里清空**（或挂到 `context` 的 disposable）。宿主
   不会重新求值插件模块，跨 activate 保留的模块级状态属插件缺陷；需要跨运行期保留的数据
   请走 storage / plugin-database。

## Vite 构建集成

在插件 `vite.config.ts` 中引入 `bedcodePlugin()`，负责将共享模块标记为 external 并改写为全局变量读取：

```ts
import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import { bedcodePlugin } from '@binblink/bedcode-plugin-sdk-mobile/vite'

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
import Select from '@binblink/bedcode-plugin-sdk-mobile/ui'

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

当前提供（每个子路径 = 一个独立导出）：

| 子路径 | 内容 | 用途 |
|--------|------|------|
| `./ui` | `Select` 组件 | 下拉选择（`update:modelValue` / `open` 事件，`md` / `sm` 尺寸） |
| `./ui/plugin-global-dialog` | `PluginGlobalDialog` 组件 | 移动端宿主全局弹窗（应用根挂载一次；渲染由全局弹窗控制器驱动，与桌面端同构） |
| `./ui/swipe-tabs` | `useSwipeTabs` composable | 弹窗/页签内容区横向滑动切换（内容区水平主导滑动超阈值触发 `onSwitch`；垂直主导不记录位移；`shouldSkip` 排除输入控件等不应作切页手势的起点） |
| `./global-dialog` | 全局弹窗控制器 | 发布-订阅式弹窗队列（FIFO，同时至多一个；`update()` 生成新条目对象再广播），宿主与 dev-shell 共用 |

## 子路径导出总览

| 子路径 | 内容 |
|--------|------|
| `@binblink/bedcode-plugin-sdk-mobile` | 主 API：类型 + 运行时代理 |
| `@binblink/bedcode-plugin-sdk-mobile/vite` | `bedcodePlugin()` Vite 构建插件 |
| `@binblink/bedcode-plugin-sdk-mobile/types` | 纯类型导出（仅类型，无运行时） |
| `@binblink/bedcode-plugin-sdk-mobile/ui` | 共享 Vue 组件：`Select` |
| `@binblink/bedcode-plugin-sdk-mobile/ui/plugin-global-dialog` | `PluginGlobalDialog` |
| `@binblink/bedcode-plugin-sdk-mobile/ui/swipe-tabs` | `useSwipeTabs` |
| `@binblink/bedcode-plugin-sdk-mobile/global-dialog` | 全局弹窗控制器 |

## 本地开发本 SDK

```bash
pnpm run build      # tsup 构建 dist（ESM + d.ts）
pnpm run test:run   # vitest run
```
