# BedCode 插件系统

> 桌面端插件系统核心机制文档，涵盖架构、生命周期、权限模型、扩展点、前后端通信和存储。

## 1. 架构概览

插件系统采用 **Rust 后端仲裁 + TypeScript 前端执行** 的双层架构，支持三种插件类型：

- **Rust+TS**: Rust 端通过 `BedcodePlugin` trait 提供后端能力（自定义 command、终端处理），前端提供 UI 组件
- **Rust only**: 纯 Rust 插件，无前端组件
- **TS only**: 纯前端插件，仅通过 `plugin.json` 声明 + TypeScript 入口文件

Rust 插件通过 `inventory` crate 实现静态注册（编译期链接），TS-only 插件通过文件扫描动态加载。

```
┌─────────────────────────────────────────────────────────────┐
│                      Frontend (Vue 3)                       │
│                                                             │
│  PluginLoader ──► PluginContext ──► PluginModule            │
│       │                │                    │                │
│       ▼                ▼                    ▼                │
│  PluginRegistry   EventAPI          Plugin UI Components    │
│  (views/commands/statusbar)                                 │
└──────────┬──────────────────────────────────┬───────────────┘
           │ Tauri invoke                     │ Tauri events
           ▼                                  ▼
┌─────────────────────────────────────────────────────────────┐
│                      Rust Backend                           │
│                                                             │
│  PluginHost ──► PluginLoader ──► PermissionManager          │
│       │                                              │      │
│       ▼                                              ▼      │
│  PluginRegistry ◄──────────────── 权限仲裁 ◄──── API 调用  │
│       │                                                     │
│       ▼                                                     │
│  PluginStorage (SQLite)                                     │
└─────────────────────────────────────────────────────────────┘
```

**核心原则：**

- **声明式注册**：插件通过 `plugin.json` 声明能力，无需代码即可注册扩展点
- **权限双重校验**：前端快速失败 + Rust 端最终仲裁
- **懒激活**：有 contributes 的插件不立即激活，按需加载
- **隔离存储**：每个插件只能读写自己的 SQLite 存储空间
- **Inline 模式**：MVP 仅支持 inline 模式（插件 JS 与宿主同进程运行）

## 2. 目录结构

### Rust 后端 (`src-tauri/src/desktop/plugin/`)

| 文件 | 职责 |
|------|------|
| `types.rs` | `PluginManifest`、`PluginState`、`LoadedPlugin`、`PluginInfo`、`PluginContributes`、`PluginConfiguration` |
| `permission.rs` | `PermissionManager` — 合法权限枚举、权限授予/校验/撤销、权限→API 映射 |
| `loader.rs` | `PluginLoader` — 扫描 `{resource_dir}/plugins/desktop/`、解析 `plugin.json`、验证必填字段 |
| `registry.rs` | `PluginRegistry` — 扩展点注册表（commands/views/terminal/http/file_handlers） |
| `host.rs` | `PluginHost` — 生命周期协调器，组合 loader + permission + registry + storage |
| `storage.rs` | `PluginStorage` — SQLite `plugin_storage` 表 CRUD，按 plugin_id 隔离 |
| `api_bridge.rs` | Tauri commands — 前端 invoke 的 Rust 端入口，含权限校验 |
| `manager.rs` | 旧 `PluginManager`（任务状态管理，非插件系统核心） |
| `setup.rs` | 旧 hooks/token 设置（非插件系统核心） |

### 前端 (`src/modules/shared/plugin/`)

| 文件 | 职责 |
|------|------|
| `types.ts` | TypeScript 类型定义 — manifest、PluginContext API 接口、扩展点描述符 |
| `commands.ts` | Tauri invoke 封装 — 所有 `plugin_*` 命令的 TypeScript 调用 |
| `permission.ts` | 前端权限快速失败检查 |
| `loader.ts` | `PluginLoaderClass` — 前端插件加载/激活/停用，动态 import 入口文件 |
| `context.ts` | `createPluginContext()` — 为每个插件构建 PluginContext 代理 |
| `registry.ts` | 前端 `PluginRegistryClass` — Vue 组件注册表（views/statusbar/input/terminalToolbar/titleBar/fileHandlers） |
| `events.ts` | 插件事件总线 — 基于内存 Map 的发布/订阅 |
| `index.ts` | barrel export |
| `components/PluginViewHost.vue` | 动态组件渲染器 |
| `components/PluginCommandPalette.vue` | Ctrl+Shift+P 命令面板 |
| `components/PluginStatusBar.vue` | 插件状态栏项渲染 |
| `components/PluginTerminalToolbar.vue` | 终端工具栏插件项渲染 |
| `components/PluginTitleBarItems.vue` | 标题栏插件项渲染 |

### 前端 UI (`src/modules/desktop/`)

| 文件 | 职责 |
|------|------|
| `views/PluginsView.vue` | 插件管理页面 — 列表、启用/停用切换、详情展开 |
| `views/PluginConfigView.vue` | 插件配置页面 — 根据 `contributes.configuration` 自动生成表单 |
| `composables/usePluginManager.ts` | 插件管理业务逻辑 composable |

## 3. 插件生命周期

```
  ┌──────────┐    ┌──────────┐    ┌───────────┐    ┌────────────┐
  │ Scanned  │───►│  Loaded  │───►│ Activated │───►│Deactivated │
  └──────────┘    └────┬─────┘    └─────┬─────┘    └────────────┘
                       │                │
                       ▼                ▼
                  ┌──────────┐    ┌──────────┐
                  │  Error   │◄───│  Error   │
                  └──────────┘    └──────────┘
```

### 3.1 启动阶段（Rust 端）

1. **`PluginHost::new(db)`** 在 `lib.rs` 的 Tauri setup 闭包中创建
2. **`PluginLoader::load_all()`** 扫描安装目录下 `plugins/desktop/` 目录
   - 目录路径通过 `resource_dir` 解析：`{resource_dir}/plugins/desktop/{plugin-id}/plugin.json`
   - 开发模式：`src-tauri/resources/plugins/desktop/`
   - 生产安装：安装目录下 `resources/plugins/desktop/`
   - 每个子目录需包含 `plugin.json`
   - 解析并验证必填字段（id, name, version, main）
   - 验证 sandbox 模式（MVP 仅允许 `inline`）
3. **`PermissionManager::grant_permissions()`** 过滤非法权限，storage 默认授予
4. **`register_manifest_contributions()`** 将所有已加载插件的 contributes 注册到 `PluginRegistry`
5. `PluginHost` 存入 `AppContext` 和 Tauri managed state

### 3.2 启动阶段（前端）

1. `main.ts` 调用 `pluginLoader.loadAll()`
2. 通过 `plugin_list_loaded` invoke 获取所有插件信息
3. 对每个插件判断是否懒激活（有 commands/terminal/views → 懒激活）
4. 非懒激活插件立即调用 `loadInline()`:
   - `plugin_activate` 通知后端
   - `import()` 动态导入插件入口文件（通过 Tauri asset protocol）
   - `createPluginContext()` 构建 PluginContext
   - 调用 `module.activate(context)`

### 3.3 激活/停用

**激活** (`plugin_activate`):
- Rust 端：状态 Loaded/Error → Activated，记录 activated_at
- 前端：动态 import + 调用 `activate(context)`

**停用** (`plugin_deactivate`):
- Rust 端：`registry.unregister_plugin()` + `permission.revoke_all()` + 状态 → Deactivated
- 前端：清理所有 Disposable → 清理事件监听 → 调用 `module.deactivate()` → 通知后端

**错误处理** (`plugin_mark_error`):
- Rust 端：状态 → Error(message)
- 前端：激活失败时自动调用

## 4. plugin.json 格式

```json
{
  "id": "com.bedcode.example",
  "name": "Example Plugin",
  "version": "1.0.0",
  "description": "An example plugin",
  "author": "BedCode Team",
  "main": "index.ts",
  "sandbox": "inline",
  "permissions": ["terminal:input", "storage", "ui:sidebar"],
  "contributes": {
    "commands": [
      { "id": "example.hello", "title": "Say Hello", "icon": "👋" }
    ],
    "views": [
      { "id": "example.sidebar", "type": "sidebar", "title": "Example Panel", "component": "SidebarPanel" }
    ],
    "terminal": {
      "input_handlers": ["onInput"],
      "output_parsers": ["parseOutput"]
    },
    "tool_providers": [
      { "id": "example.tool", "name": "Example Tool", "endpoint": "/tools/example" }
    ],
    "file_handlers": [
      { "id": "example.markdown", "extensions": [".md"], "viewer": "MarkdownViewer", "icon": "📄" }
    ],
    "configuration": {
      "title": "Example Settings",
      "properties": {
        "greeting": {
          "type": "string",
          "title": "Greeting Message",
          "description": "The message to display",
          "default": "Hello!"
        },
        "count": {
          "type": "number",
          "title": "Count",
          "default": 0
        },
        "enabled": {
          "type": "boolean",
          "title": "Enabled",
          "default": true
        }
      }
    }
  }
}
```

### 必填字段

| 字段 | 说明 | 格式 |
|------|------|------|
| `id` | 唯一标识 | 反向域名格式，如 `com.bedcode.quick-snippets` |
| `name` | 显示名称 | 任意字符串 |
| `version` | 语义化版本号 | 如 `1.0.0` |
| `main` | 入口文件路径 | 相对于插件根目录，如 `index.ts` |

### 可选字段

| 字段 | 默认值 | 说明 |
|------|--------|------|
| `description` | `""` | 插件描述 |
| `author` | `""` | 作者 |
| `sandbox` | `"inline"` | 沙箱模式，MVP 仅支持 `inline` |
| `permissions` | `[]` | 请求的权限列表 |
| `contributes` | `{}` | 扩展点声明 |

## 5. 权限模型

### 5.1 合法权限枚举

| 权限 | 说明 | 对应 API |
|------|------|----------|
| `terminal:input` | 终端输入 | `terminal.sendInput`, `terminal.onInput` |
| `terminal:output` | 终端输出 | `terminal.onOutput` |
| `session:read` | 会话读取 | `session.list`, `session.get`, `session.onStatusChange` |
| `session:write` | 会话写入 | `session.create`, `session.stop` |
| `ui:sidebar` | 侧边栏面板 | `ui.registerSidebarPanel` |
| `ui:toolbox` | 工具箱页面 | `ui.registerToolboxPage` |
| `ui:statusbar` | 状态栏项/标题栏项 | `ui.registerStatusBarItem`, `ui.registerTitleBarItem` |
| `ui:input` | 输入扩展/终端工具栏 | `ui.registerInputExtension`, `ui.registerTerminalToolbarItem` |
| `network:http` | HTTP 端点 | `http.registerEndpoint` |
| `storage` | 持久化存储 | `storage.get/set/delete/flush` (**默认授予**) |

### 5.2 双重校验流程

```
插件调用 API
    │
    ▼
前端 requirePermission() ─── 快速失败，避免无效 invoke
    │ (通过)
    ▼
Tauri invoke ─────────────── 传输到 Rust 端
    │
    ▼
Rust permission.check() ──── 最终仲裁，拒绝越权调用
    │ (通过)
    ▼
执行操作
```

- **前端**：`hasPermissionForApi(grantedPermissions, apiMethod)` — 基于 manifest permissions 的内存检查
- **Rust 端**：`PermissionManager::check()` 或 `check_api()` — 基于 `granted` HashMap 的权威检查

### 5.3 权限生命周期

- **授予**：`PluginLoader::load_all()` 时调用 `grant_permissions()`，过滤非法权限
- **撤销**：`deactivate_plugin()` 时调用 `revoke_all()`，清除所有已授予权限
- **重新激活**：`activate_plugin()` 不重新授权（权限在 load 时已授予，deactivate 后被撤销，再次 activate 无权限）

## 6. PluginContext API

`PluginContext` 是插件访问宿主能力的唯一通道，由 `createPluginContext()` 创建。

### 6.1 CommandRegistry

```typescript
context.commands.register(id: string, handler: (...args) => any): Disposable
context.commands.execute(id: string, ...args: any[]): Promise<any>
```

- 注册命令处理器，返回 Disposable 用于清理
- `execute` 仅查找本插件注册的命令，跨插件命令执行未实现

### 6.2 TerminalAPI

```typescript
context.terminal.sendInput(sessionId: string, text: string): Promise<void>  // 需 terminal:input
context.terminal.onOutput(handler): Disposable                              // 需 terminal:output
context.terminal.onInput(handler): Disposable                               // 需 terminal:input
```

- `sendInput` 通过 `plugin_terminal_send_input` invoke 发送到 PTY
- `onOutput`/`onInput` 通过前端事件总线监听

### 6.3 SessionAPI

```typescript
context.session.list(): Promise<any[]>                    // 需 session:read
context.session.get(sessionId: string): Promise<any>      // 需 session:read
context.session.onStatusChange(handler): Disposable       // 需 session:read
```

- 动态 import `useDesktopCommands` 调用宿主会话 API

### 6.4 UIRegistry

```typescript
context.ui.registerSidebarPanel(panel): Disposable            // 需 ui:sidebar
context.ui.registerToolboxPage(page): Disposable              // 需 ui:toolbox
context.ui.registerStatusBarItem(item): Disposable            // 需 ui:statusbar
context.ui.registerInputExtension(ext): Disposable            // 需 ui:input
context.ui.registerTerminalToolbarItem(item): Disposable      // 需 ui:input
context.ui.registerTitleBarItem(item): Disposable             // 需 ui:statusbar
context.ui.registerFileHandler(handler): Disposable           // 需 ui:sidebar (复用)
```

- 注册到前端 `PluginRegistryClass`，Vue 组件可响应式读取
- `registerTerminalToolbarItem` 在终端工具栏末尾添加按钮，复用 `ui:input` 权限
- `registerTitleBarItem` 在标题栏 Logo 与窗口控制之间添加项，复用 `ui:statusbar` 权限

### 6.5 EventAPI

```typescript
context.events.on(event: string, handler): Disposable
context.events.emit(event: string, ...args: any[]): void
```

- 基于内存 Map 的进程内事件总线（非 Tauri event 系统）

### 6.6 StorageAPI

```typescript
context.storage.get<T>(key: string): Promise<T | undefined>
context.storage.set(key: string, value: any): Promise<void>
context.storage.delete(key: string): Promise<void>
context.storage.flush(): Promise<void>  // no-op，即时写入
```

- 通过 `plugin_storage_get/set/delete` invoke 读写 SQLite
- 按 plugin_id 隔离，插件只能访问自己的空间

### 6.7 HttpAPI

```typescript
context.http.registerEndpoint(path: string, handler): Disposable  // 需 network:http
```

- **MVP 未实现**：仅在前端记录端点，dispose 为空操作

## 7. 扩展点注册表

### 7.1 Rust 端 PluginRegistry

管理五类扩展点，均使用 `Arc<RwLock<HashMap>>` 实现线程安全：

| 扩展点 | Key | 注册来源 |
|--------|-----|----------|
| Commands | command_id | `contributes.commands` |
| Views | view_id | `contributes.views` |
| Terminal | plugin_id | `contributes.terminal` |
| HTTP Endpoints | path | `contributes.toolProviders` |
| File Handlers | handler_id | `contributes.fileHandlers` |

### 7.2 前端 PluginRegistryClass

管理 Vue 组件注册，提供响应式数据供 UI 渲染：

| 扩展点 | 响应式 Ref | 用途 |
|--------|-----------|------|
| Sidebar Views | `sidebarViews` | 侧边栏导航项 |
| Toolbox Views | `toolboxViews` | 工具箱导航项 |
| StatusBar Items | `statusbarItems` | 底部状态栏按钮 |
| Input Extensions | `inputExts` | 输入框扩展 |
| Terminal Toolbar | `terminalToolbarItems` | 终端工具栏按钮 |
| Title Bar Items | `titleBarItems` | 标题栏插件项 |

## 8. 插件存储

### SQLite 表结构

```sql
CREATE TABLE IF NOT EXISTS plugin_storage (
    plugin_id TEXT NOT NULL,
    key       TEXT NOT NULL,
    value     TEXT NOT NULL,    -- JSON 序列化值
    updated_at TEXT NOT NULL,   -- ISO 8601 时间戳
    PRIMARY KEY (plugin_id, key)
);
```

### 隔离保证

- PRIMARY KEY 为 `(plugin_id, key)`，不同插件的相同 key 互不干扰
- `PluginStorage::get/set/delete` 均需传入 `plugin_id`，无法访问其他插件空间
- `clear_all(plugin_id)` 仅删除指定插件的所有数据
- Rust 端 `api_bridge` 在执行存储操作前校验 `storage` 权限

## 9. 插件入口文件约定

插件入口文件（`main` 指定的文件）需导出 `PluginModule` 接口：

```typescript
import type { PluginContext } from 'bedcode-plugin-api'  // 假设的类型包

export async function activate(context: PluginContext): Promise<void> {
  // 注册命令
  context.commands.register('myPlugin.hello', () => {
    console.log('Hello from plugin!')
  })

  // 注册侧边栏面板
  context.ui.registerSidebarPanel({
    id: 'myPanel',
    title: 'My Panel',
    component: MyPanelComponent,
  })

  // 注册终端工具栏按钮
  context.ui.registerTerminalToolbarItem({
    id: 'myTool',
    label: 'Run',
    icon: '▶',
    onClick: () => { /* ... */ },
  })

  // 注册标题栏状态项
  context.ui.registerTitleBarItem({
    id: 'myStatus',
    label: 'Ready',
  })

  // 使用存储
  const count = await context.storage.get<number>('count') || 0
  await context.storage.set('count', count + 1)
}

export async function deactivate(): Promise<void> {
  // 清理资源（Disposable 已由 PluginLoader 自动清理）
}
```

### 动态导入机制

前端通过 Tauri asset protocol 加载插件入口文件：

```typescript
const path = `${extensionPath}/${main}`.replace(/\\/g, '/')
const url = `https://asset.localhost/${path}`
const module = await import(url)  // 动态导入
```

导入和激活均有 5 秒超时保护。

## 10. Tauri Commands 清单

| 命令 | 参数 | 返回值 | 说明 |
|------|------|--------|------|
| `plugin_list_loaded` | — | `Vec<PluginInfo>` | 获取所有已加载插件 |
| `plugin_get_info` | `plugin_id` | `Option<PluginInfo>` | 获取单个插件信息 |
| `plugin_activate` | `plugin_id` | `()` | 激活插件 |
| `plugin_deactivate` | `plugin_id` | `()` | 停用插件 |
| `plugin_mark_error` | `plugin_id, error` | `()` | 标记插件错误 |
| `plugin_storage_get` | `plugin_id, key` | `Option<Value>` | 获取存储值 |
| `plugin_storage_set` | `plugin_id, key, value` | `()` | 设置存储值 |
| `plugin_storage_delete` | `plugin_id, key` | `()` | 删除存储值 |
| `plugin_terminal_send_input` | `plugin_id, session_id, text` | `()` | 终端发送输入 |
| `plugin_list_commands` | — | `Vec<CommandEntry>` | 获取所有命令 |
| `plugin_list_views` | `view_type` | `Vec<ViewEntry>` | 获取指定类型视图 |
| `plugin_find_file_handler` | `extension` | `Option<FileHandlerEntry>` | 查找文件处理器 |

## 11. 配置系统

插件可通过 `contributes.configuration` 声明配置 schema，`PluginConfigView` 自动生成表单。

### 支持的配置类型

| 类型 | UI 组件 | 说明 |
|------|---------|------|
| `string` | `<input type="text">` | 文本输入 |
| `string` + `enum` | `<select>` | 下拉选择 |
| `number` | `<input type="number">` | 数字输入 |
| `boolean` | `<Toggle>` | 开关切换 |

### 配置存储

- 配置值存储在 `plugin_storage` 表中，key 为 `"config"`
- 读取时与 manifest defaults 合并：`{ ...defaults, ...saved }`
- 重置操作恢复为 defaults

## 12. UI 扩展插槽

插件通过 `context.ui.register*()` 注册 UI 扩展，宿主组件预留插槽自动渲染。采用**声明式插槽**模式：每个扩展位置对应 Registry 中的一个响应式 Ref，宿主组件直接读取渲染。

### 12.1 扩展位置清单

| 位置 | 宿主组件 | Registry Ref | 插件 API | 所需权限 | 路由 |
|------|---------|-------------|---------|---------|------|
| 侧边栏面板 | `PluginViewHost` | `sidebarViews` | `registerSidebarPanel()` | `ui:sidebar` | `/plugin/sidebar/:pluginId/:viewId` |
| 工具箱面板 | `PluginViewHost` | `toolboxViews` | `registerToolboxPage()` | `ui:toolbox` | `/plugin/toolbox/:pluginId/:viewId` |
| 终端工具栏 | `TerminalPreview.vue` | `terminalToolbarItems` | `registerTerminalToolbarItem()` | `ui:input` | — |
| 标题栏 | `TitleBar.vue` | `titleBarItems` | `registerTitleBarItem()` | `ui:statusbar` | — |
| 底部状态栏 | `DesktopLayout.vue` | `statusbarItems` | `registerStatusBarItem()` | `ui:statusbar` | — |

### 12.2 数据流

```
插件 activate() 调用 context.ui.register*()
       │
       ▼
PluginRegistryClass.register*() — 写入内部 Map + 更新响应式 Ref
       │
       ▼
Vue 组件响应式读取 Ref — 自动渲染新增项
```

### 12.3 扩展位置类型

**视图类**（sidebar / toolbox）：注册 Vue 组件，通过路由导航到 `PluginViewHost` 动态渲染。插件需提供 `component` 属性。

**操作项类**（terminalToolbar / titleBar / statusbar）：注册按钮/标签项，宿主组件直接 v-for 渲染。插件提供 `label`、`icon`、`onClick` 等属性。

### 12.4 插槽渲染组件

| 组件 | 渲染位置 | 读取 Ref |
|------|---------|---------|
| `PluginViewHost.vue` | 路由页面（sidebar/toolbox） | `getViewComponent()` |
| `PluginTerminalToolbar.vue` | 终端 header 工具栏末尾 | `terminalToolbarItems` |
| `PluginTitleBarItems.vue` | 标题栏 Logo 与窗口控制之间 | `titleBarItems` |
| `PluginStatusBar.vue` | 主布局底部 | `statusbarItems` |
| `PluginCommandPalette.vue` | 全局浮层 (Ctrl+Shift+P) | `pluginListCommands()` |

### 12.5 Sidebar 中的插件导航

`Sidebar.vue` 读取 `sidebarViews` 和 `toolboxViews` 两个 Ref，在主导航下方分别渲染插件面板和工具箱导航链接：

- Sidebar 面板链接 → `/plugin/sidebar/:pluginId/:viewId`
- Toolbox 面板链接 → `/plugin/toolbox/:pluginId/:viewId`

两者之间用分隔线区分，空数组时隐藏整个分区。

### 12.6 清理机制

插件停用时，`PluginRegistryClass.clearPlugin()` 清理该插件在所有 Map 中的注册项，并更新对应的响应式 Ref，宿主组件自动移除渲染。

## 13. 已知缺陷与待实现项

> 以下为当前实现中已识别的问题，详见各分类说明。

### 13.1 安全缺陷

| # | 问题 | 严重度 | 说明 |
|---|------|--------|------|
| S1 | **重新激活后权限丢失** | 高 | `deactivate_plugin()` 调用 `revoke_all()` 撤销所有权限，但 `activate_plugin()` 不重新授权。再次激活的插件无任何权限，所有 API 调用都会被拒绝。 |
| S2 | **前端可伪造 plugin_id** | 高 | `plugin_storage_get/set/delete` 等 invoke 命令中 `plugin_id` 由前端传入，恶意插件可传入其他插件的 ID 读写其存储。Rust 端仅校验权限，未校验调用者身份。 |
| S3 | **PermissionManager RwLock unwrap** | 中 | `grant_permissions()`、`check()` 等方法中 `self.granted.read().unwrap()` / `write().unwrap()` — 若 RwLock 被 poison（某线程 panic），整个权限系统崩溃而非降级。 |
| S4 | **无插件签名验证** | 中 | 加载 `plugin.json` 和入口文件时无完整性校验，本地文件可被篡改注入恶意代码。 |

### 13.2 功能缺失

| # | 问题 | 优先级 | 说明 |
|---|------|--------|------|
| F1 | **HttpAPI 未实现** | P2 | `context.http.registerEndpoint()` 仅在前端记录端点，dispose 为空操作，无实际路由功能。 |
| F2 | **跨插件命令执行未实现** | P2 | `CommandRegistry.execute()` 仅查找本插件注册的命令，无法执行其他插件暴露的命令。 |
| F3 | **事件系统未接入 Tauri event** | P2 | `events.ts` 是纯内存 Map 实现，未与 Tauri event 系统集成，无法跨窗口/进程通信。 |
| F4 | **Terminal 事件未桥接** | P2 | `terminal.onOutput`/`onInput` 通过内存事件总线监听，但 PTY 输出事件未转发到此总线。 |
| F5 | **Session 事件未桥接** | P3 | `session.onStatusChange` 同上，会话状态变化事件未转发到插件事件总线。 |
| F6 | **PluginLoader 未在 main.ts 初始化** | P1 | 计划中 `pluginLoader.loadAll()` 调用未确认是否已添加到 `main.ts`。 |
| F7 | ~~PluginCommandPalette 未集成~~ | — | **已集成**：已添加到 `DesktopLayout.vue` |
| F8 | ~~PluginStatusBar 未集成~~ | — | **已集成**：已挂载到 `DesktopLayout.vue` |
| F9 | ~~PluginViewHost 未集成到 Sidebar~~ | — | **已集成**：路由 `/plugin/sidebar/:pluginId/:viewId` 和 `/plugin/toolbox/:pluginId/:viewId` 已添加，Sidebar 已渲染插件导航链接 |
| F10 | **无插件卸载机制** | P3 | 只有 deactivate，无法从磁盘删除插件。`PluginStorage::clear_all()` 存在但未暴露为 Tauri command。 |
| F11 | **无插件安装机制** | P2 | 无 CLI 命令或 UI 入口安装新插件，需手动创建目录和文件。 |
| F12 | **懒激活触发缺失** | P2 | `shouldLazyActivate()` 判断了哪些插件需懒激活，但无触发机制（如命令面板选中时激活、路由访问时激活）。 |

### 13.3 架构问题

| # | 问题 | 说明 |
|---|------|------|
| A1 | **PluginHost::new() 中 block_on** | `new()` 是同步函数，内部使用 `tauri::async_runtime::block_on()` 注册 contributes。若在 Tokio runtime 内调用会 panic（"Cannot start a runtime from within a runtime"）。当前在 Tauri setup 闭包中调用是安全的，但未来若在其他 async 上下文创建会出问题。 |
| A2 | **前后端 Registry 双写** | Rust `PluginRegistry` 和前端 `PluginRegistryClass` 各自维护一份注册表，无同步机制。Rust 端的注册来自 manifest 解析，前端来自运行时 `context.ui.register*()` 调用，两者可能不一致。 |
| A3 | **PluginState 类型处理不一致** | Rust 端 `PluginState` 使用 `#[serde(tag = "state", content = "error")]` 内部标签式序列化，前端 `PluginState` 类型定义需精确匹配。`getStateKey()` 中 `Loaded` 状态映射到 `deactivated` 的 i18n key，语义不正确。 |
| A4 | **context.ts 中 requirePermission 抛中文错误** | `throw new Error(\`插件 ${info.id} 没有 ${apiMethod} 所需的权限\`)` — composable 中不应包含中文硬编码字符串，应使用 i18n key。 |
| A5 | **PluginLoader 超时未清理** | `importWithTimeout` 和 `activateWithTimeout` 使用 `Promise.race` + `setTimeout`，超时后 setTimeout 的 timer 未 `clearTimeout`，可能造成内存泄漏。 |
| A6 | **事件总线 key 解析脆弱** | `events.ts` 的 `emit()` 通过 `key.split(':').slice(1).join(':')` 解析事件名，若事件名本身包含冒号（如 `terminal:output`），解析结果依赖 pluginId 不含冒号的隐式约定。 |

### 13.4 代码质量

| # | 问题 | 说明 |
|---|------|------|
| Q1 | **PluginInfo 包含 main 字段** | `PluginInfo` 返回给前端包含 `main`（入口文件路径），前端需要它来动态 import，但这暴露了服务器文件系统路径信息。 |
| Q2 | **usePluginManager 中 toast.error 混用** | `loadPlugins()` 中 `toast.error(e.message || 'Failed to load plugins')` 使用英文硬编码，其他地方用 i18n key，不一致。 |
| Q3 | **PluginConfigView 中硬编码英文** | `"no configuration available"` 和 `"Failed to load plugin config"` 等字符串未走 i18n。 |
| Q4 | ~~registry.ts 中 sidebarViews 仅过滤 sidebar~~ | **已修复**：`updateReactiveViews()` 现在同时更新 `sidebarViews`、`toolboxViews` 和 `statusbarViews` |
