# BedCode - Tauri 2.0 Project Guidelines

## Project Overview

BedCode 是一个跨平台应用，支持移动设备远程控制 Claude Code。桌面端 (Tauri + Vue 3) 作为主机，移动端作为远程终端。

而你是一个资深的Tauri开发专家！

**Tech Stack:**
- **Desktop**: Tauri 2.0 + Vue 3 + TypeScript + TailwindCSS
- **Backend**: Rust (Tokio async runtime)
- **Database**: SQLite
- **Communication**: WebSocket
- **I18n**: vue-i18n@9 (zh-CN / en)
- **State Management**: Pinia

---

## Code Map

**完整的项目目录结构和模块索引请参阅 [`docs/code-map.md`](docs/code-map.md)。**

该文档包含：
- 完整的目录树结构
- 各模块职责说明
- 按功能和类型的快速导航索引

**重要：当用户命令包含以下动作时，请先阅读 `docs/code-map.md`：**

- 探索代码 / 查看代码 / 了解代码结构
- 查找文件 / 定位模块 / 寻找某个功能
- 理解架构 / 分析项目组成
- 修改某模块前需要了解上下文

---

## Rust Backend Guidelines

### 模块组织

- **shared/**: 桌面端和移动端共享代码
- **desktop/**: 仅桌面端使用 (PTY、WebSocket 服务器、会话管理)
- **mobile/**: 仅移动端使用

### 模块文件组织 (重要)

**使用与现代 Rust 最佳实践一致的文件命名：**

```
src/
├── module.rs        # ✅ 推荐：模块入口文件与目录同名
├── submodule.rs     # 子模块
└── subdir/          # 复杂子模块
    └── mod.rs       # 子目录仍使用 mod.rs
```

**❌ 不再使用旧的 mod.rs 模式：**

```
src/
└── module/
    ├── mod.rs       # ❌ 已废弃
    └── submodule.rs
```

**原因**：与目录同名的 `.rs` 文件是 Rust 社区推荐的标准，IDE 支持更好，导入更清晰。

### Error Handling

使用 `shared/system/error.rs` 中的 `AppError` 统一错误类型：

```rust
pub type Result<T> = std::result::Result<T, AppError>;
```

**全局机制（已启用）：**
- **Panic Hook** (`main.rs`) — 全局 `set_hook` 捕获所有未处理 panic，输出到 stderr。**注意：** panic hook 中禁止调用 `tracing::error!`（可能因锁冲突导致死锁），只使用 `eprintln!`
- **Error Boundary** (`shared/system/error_boundary.rs`) — `spawn_with_error_boundary()` 包装 `tokio::spawn`，后台任务 panic 时自动捕获并记录日志，不传播到整个进程
- **anyhow::Context** — `.context()` / `.with_context()` 可在 `crate::Result` 函数中使用（自动将 `anyhow::Error` → `AppError::Internal`），为错误添加调用链上下文

**错误处理规范：**

```rust
// ✅ 好：使用 typed error 保留错误类型
Err(AppError::WebSocket("Not connected".to_string()))

// ✅ 好：在关键调用链使用 anyhow::Context 添加上下文
use anyhow::Context;
let data = fetch_data().await
    .with_context(|| format!("Failed to fetch data for session {}", id))
    .map_err(|e| AppError::WebSocket(e.to_string()))?;

// ✅ 好：结构化的 tracing 错误日志
tracing::error!(
    error = %e,
    session_id = %session_id,
    "Failed to write input",
);

// ✅ 好：tokio::spawn 使用 error boundary
use crate::shared::system::error_boundary::spawn_with_error_boundary;
spawn_with_error_boundary("task_name", async move {
    // 可能 panic 的任务逻辑
});
```

**禁止：**
- 使用 `unsafe impl Send/Sync` 解决并发问题
- 在重要路径上使用 `let _ =` 静默忽略错误（仅在断开清理等预期场景允许）
- 无上下文的裸字符串错误：`AppError::WebSocket("failed".to_string())` — 应说明什么操作在哪失败

### Thread Safety

使用 `Arc<Mutex<T>>` 或 `Arc<RwLock<T>>` 进行状态共享，**不要用 `unsafe impl Send/Sync`**。

### Tauri Commands

在 `commands.rs` 中定义，使用清晰的分节注释：

```rust
// ==================== Session Commands ====================

#[tauri::command]
pub async fn start_session(...) -> Result<String> {
    // ...
}
```

**命名规范：**
- `list_*` - 返回多个
- `get_*` - 返回单个
- `create_*` - 创建
- `delete_*` - 删除
- `start_*` / `stop_*` - 生命周期

### Logging

全部使用 `tracing`，桌面端和移动端统一：

```rust
use tracing::{info, debug, error, warn};

info!("Session created: {} ({})", name, id);
debug!("Processing request: {:?}", request);
```

**Android 平台**：`tracing` 的 `log` feature 自动将 `tracing::` 宏转发到 `log` crate，再由 `android_logger` 发送到 `adb logcat`。开发者无需关心底层实现，统一写 `tracing::info!()` 即可。

**日志级别规范：**
- `debug!`: 常规操作日志，记录函数调用、流程步骤
- `info!`: 关键信息日志，如连接建立、会话创建、用户操作
- `warn!`: 警告日志，如重试、超时、降级处理
- `error!`: 错误日志，如连接失败、异常处理

---

## Frontend Guidelines (Vue 3 + TypeScript)

### Component Structure

使用 `<script setup lang="ts">` 语法。

### Composables Pattern

业务逻辑放在 composables 中，组件只负责 UI：

```typescript
export function useSession() {
  const sessions = ref<SessionInfo[]>([])

  async function loadSessions() {
    sessions.value = await invoke('list_sessions')
  }

  return { sessions, loadSessions }
}
```

**命名：** `use<Resource>` / `use<Action>`

### Pinia Stores

全局状态使用 Pinia store 包装 composables。

### Platform Detection

使用 `@tauri-apps/plugin-os`：

```typescript
import { usePlatform } from '@/composables/usePlatform'
const { platformInfo } = usePlatform()
```

**禁止使用屏幕宽度检测桌面/移动端。**

---

## Code Comment Standards

### 核心原则

**注释解释"为什么"而非"是什么"。** 代码本身已经说明了做了什么，注释的价值在于补充代码无法表达的信息。

### Rust 注释规范

#### 1. 模块级文档 (`//!`)

每个模块文件**必须**有模块级文档，说明模块职责和核心设计：

```rust
//! Session Manager
//!
//! 会话管理器 - 负责协调会话生命周期、状态管理和事件发布
//! 重构后只负责流程编排，各职责已拆分到独立模块
```

#### 2. 公开项文档 (`///`)

所有 `pub` 的 struct、enum、trait、函数/方法**必须**有 `///` 文档注释：

```rust
/// 从配置创建会话（带来源设备）
///
/// # Arguments
/// * `config_id` - 会话配置 ID
/// * `source_device` - 触发操作的设备名称，桌面本地操作为 None
///
/// # Errors
/// 配置不存在时返回 `AppError::NotFound`
pub async fn create_session_with_source(&self, config_id: &str, source_device: Option<String>) -> Result<String> {
```

**简单 accessor 可省略多行文档**，但必须有单行 `///`：

```rust
/// 获取会话状态变化广播发送器
pub fn status_tx(&self) -> broadcast::Sender<SessionStatusEvent> {
```

**纯 getter（`fn db(&self)`）** 可以不加 `///`，因为签名已足够清晰。

#### 3. 内联注释 (`//`)

关键逻辑**必须**添加内联注释：

| 场景 | 说明 |
|------|------|
| 业务逻辑判断 | 解释判断原因，而非判断什么 |
| 复杂算法 | 说明关键步骤思路 |
| 异常处理 | 为什么捕获这个错误 |
| 并发控制 | 锁的获取顺序、死锁避免策略 |
| 性能优化 | 为什么这样优化 |
| 非显而易见的选择 | 为什么用这种方式而非更直觉的方式 |

```rust
// ❌ 不好：描述代码做什么
// 遍历所有会话
for session in sessions.iter() { }

// ✅ 好：解释为什么这样做
// 清理已停止超过 24 小时的会话，避免会话列表无限增长占用内存
for session in sessions.iter() { }
```

#### 4. 分隔注释

使用 `// ======` 风格分隔逻辑区块（Tauri commands 文件中按领域分组）：

```rust
// ==================== Session Commands ====================
```

#### 5. TODO / FIXME 标记

```rust
// TODO(username): 添加断线重连逻辑
// FIXME: 并发访问时可能 panic，需要加锁
```

#### 6. 文档中的代码块和示例

对于复杂 API，在 `///` 文档中添加 `# Examples`：

```rust
/// 连接到目标设备
///
/// # Examples
/// ```no_run
/// let manager = ConnectionManager::new();
/// manager.connect(app_handle, "192.168.1.1".to_string(), 8080, None).await?;
/// ```
pub async fn connect(&self, ...) -> Result<()> {
```

---

### TypeScript / Vue 注释规范

#### 1. 文件头注释

composable / 工具文件**必须**有文件头注释：

```typescript
/**
 * Desktop Commands - Rust 后端命令封装
 *
 * 所有桌面端可用的 Tauri 命令调用
 */
```

#### 2. 导出函数文档 (`/** */`)

所有 `export` 的函数、interface、type、class **必须**有 JSDoc 注释：

```typescript
/**
 * 连接到目标设备
 *
 * @param device - 远程设备信息
 * @throws 连接失败时抛出错误
 */
export async function connect(device: RemoteDevice): Promise<void> {
```

**简单 Tauri invoke 包装**可省略参数文档，但必须有单行描述：

```typescript
/** 获取会话列表 */
export async function listSessions(): Promise<SessionInfo[]> {
```

#### 3. Vue 组件文档

组件 `<script setup>` 顶部添加组件说明：

```vue
<script setup lang="ts">
/**
 * 终端视图 - 显示 PTY 输出和输入栏
 * 支持多会话切换和 ANSI 渲染
 */
```

#### 4. Interface / Type 文档

导出的 interface 和 type **必须**有文档，字段使用行内注释：

```typescript
/** 已配对设备信息 */
export interface PairedDevice {
  address: string       // 设备 IP 地址
  port: number          // WebSocket 端口
  name: string          // 设备显示名称
  fingerprint: string   // 设备指纹，用于识别同一设备
  pairedAt: string      // 配对时间 (ISO 8601)
  connectCount: number  // 连接次数统计
}
```

#### 5. 内联注释

与 Rust 相同原则：解释"为什么"而非"是什么"。

```typescript
// ❌ 不好
// 设置状态为 connecting
connectionStatus.value = 'connecting'

// ✅ 好
// 后端事件驱动更新状态，此值供前端 UI 响应式渲染
connectionStatus.value = 'connecting'
```

#### 6. 分隔注释

与 Rust 统一风格：

```typescript
// ==================== State ====================
// ==================== Operations ====================
// ==================== Composable ====================
```

---

### 通用规则（所有语言）

1. **语言**：注释使用中文，技术术语保留英文（如 PTY、WebSocket、JWT）
2. **注释必须与代码同步**：修改代码时必须更新相关注释，过时注释比没有注释更危险
3. **避免冗余注释**：不要注释代码显而易见的行为
4. **禁止注释掉的代码**：使用版本控制而非注释保留旧代码，除非有明确 TODO 说明保留原因
5. **私有方法**：复杂私有方法仍需注释，简单私有方法可省略

---

## Testing

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_waiting() {
        // ...
    }
}
```

集成测试放在 `src-tauri/tests/`。

---

## Build Commands

```bash
# Development
npm run tauri:dev

# Build
npm run tauri:build           # Desktop
npm run tauri:android:build   # Android

# Test
npm run test
cargo test
```

---

## Target Directory Management

编译前检查 `src-tauri/target` 目录大小，超过 15GB 执行 `cargo clean`。

---

## Android Build Setup

See `docs/android-setup.md` for detailed instructions.

---

## Internationalization (i18n)

使用 vue-i18n@9 Composition API 模式，支持 zh-CN（默认）和 en 两种语言。

### 目录结构

```
src/locales/
├── index.ts          # createI18n 实例，legacy: false
├── errorCodes.ts     # 后端错误码 → i18n key 映射
├── zh-CN/
│   ├── index.ts      # 合并所有域的中文翻译
│   ├── common.ts     # 通用：按钮、状态、时间、错误码、通知
│   ├── settings.ts   # 设置页各分区
│   ├── desktop.ts    # 桌面端：侧边栏、会话、表单、设备、终端
│   └── mobile.ts     # 移动端：导航、连接、扫描、会话、终端等
└── en/
    ├── index.ts
    ├── common.ts
    ├── settings.ts
    ├── desktop.ts
    └── mobile.ts
```

### 翻译 key 命名规范

按域分层，用点号分隔：`{domain}.{section}.{key}`

| 域 | 示例 | 用途 |
|----|------|------|
| `common.button.*` | `common.button.cancel` | 通用按钮 |
| `common.status.*` | `common.status.running` | 通用状态 |
| `common.time.*` | `common.time.minutesSecondsAgo` | 时间格式 |
| `common.errorCode.*` | `common.errorCode.authError` | 错误码翻译 |
| `common.notification.*` | `common.notification.deviceConnected` | 全局通知 |
| `common.misc.*` | `common.misc.lineCount` | 杂项 |
| `settings.*` | `settings.appearance.theme` | 设置页 |
| `desktop.session.*` | `desktop.session.confirmStop` | 桌面端会话 |
| `desktop.form.*` | `desktop.form.name` | 桌面端表单 |
| `desktop.device.*` | `desktop.device.title` | 桌面端设备配对 |
| `desktop.terminal.*` | `desktop.terminal.clearScreen` | 桌面端终端 |
| `mobile.connection.*` | `mobile.connection.timeout` | 移动端连接 |
| `mobile.scan.*` | `mobile.scan.title` | 移动端扫描 |
| `mobile.session.*` | `mobile.session.noSessions` | 移动端会话 |
| `mobile.terminal.*` | `mobile.terminal.autoMode` | 移动端终端 |
| `mobile.file.*` | `mobile.file.fetchTreeFailed` | 移动端文件 |
| `mobile.toolbox.*` | `mobile.toolbox.sendFailed` | 移动端工具箱 |

**新增 key 时必须同时添加到 zh-CN 和 en 两个文件，否则 TypeScript 会报错。**

### Vue 组件中使用

```vue
<template>
  <!-- 模板中用 $t() -->
  <h1>{{ $t('mobile.connection.title') }}</h1>
  <button :title="$t('desktop.terminal.clearScreen')">×</button>

  <!-- 带参数的翻译 -->
  <p>{{ $t('desktop.session.runTime', { time: runTime }) }}</p>
</template>

<script setup lang="ts">
import { useI18n } from 'vue-i18n'
const { t } = useI18n()

// 脚本中用 t()
toast.success(t('desktop.session.sessionStarted'))

// 响应式标签数组必须用 computed
const items = computed(() => [
  { label: t('mobile.terminal.autoMode'), value: 'auto' },
  { label: t('mobile.terminal.manualMode'), value: 'manual' },
])
</script>
```

### Composable 中使用（模块级代码）

Composable 是模块级代码，**不能使用 `useI18n()`**（需要组件上下文）。使用 `i18n.global.t()`：

```typescript
import i18n from '@/locales'

// ✅ 正确：模块级代码用 i18n.global.t()
toast.error(i18n.global.t('common.notification.connectionDisconnected', { reason }))

// ❌ 错误：模块级代码不能用 useI18n()
const { t } = useI18n() // 会报错
```

### 错误码机制

**核心原则：composable 中不包含任何中文硬编码字符串。**

1. **状态变量存 i18n key**，模板用 `$t()` 翻译：

```typescript
// composable 中
connectionError.value = 'mobile.connection.reauthFailed'  // 存 i18n key

// Vue 模板中
<p>{{ $t(connectionError) }}</p>
```

2. **后端错误码** 通过 `errorCodes.ts` 映射翻译：

```typescript
import { ERROR_CODE_I18N_KEY } from '@/locales/errorCodes'

function getErrorMessage(code: string): string {
  const i18nKey = ERROR_CODE_I18N_KEY[code]
  return i18nKey ? i18n.global.t(i18nKey) : i18n.global.t('common.errorCode.unknownError')
}
```

3. **throw new Error** 中使用 i18n key 作为 fallback：

```typescript
// ✅ composable 中 throw i18n key
throw new Error(result.message || 'mobile.file.fetchTreeFailed')

// ❌ 禁止 throw 中文
throw new Error(result.message || '获取文件树失败')
```

### 语言切换与持久化

通过 `useI18nStore` Pinia store 管理：

```typescript
import { useI18nStore } from '@/modules/shared/stores/i18n'
const i18nStore = useI18nStore()

// 切换语言（自动持久化到 Settings.ui.language）
await i18nStore.setLanguage('en')

// 应用启动时恢复语言偏好（在 main.ts 中调用）
await i18nStore.initLanguage()
```

语言偏好存储在 `Settings.ui.language` 字段，默认值 `'zh-CN'`。

### 不翻译的内容

以下内容**不翻译**，保持原样：
- 代码注释（中文注释保留中文）
- `console.log/error` 等调试字符串
- 发送给 Claude Code 的终端输入（如 `'继续'`）
- 语言选项的显示名称（`<option value="zh-CN">中文</option>`）
- 品牌名称（如 `BedCode`）

---

## Key Architecture Decisions

1. **Separation of Concerns**: Composables 处理 API，stores 管理全局状态，components 只负责 UI
2. **Async Everywhere**: Rust 用 Tokio，前端用 async/await + Tauri commands
3. **Event-Driven**: PTY 输出通过 `broadcast` 通道分发到 WebSocket 和前端
4. **Graceful Shutdown**: 使用 `AtomicBool` 信号通知后台任务关闭
5. **Platform Modules**: `shared/` + `desktop/` + `mobile/` 三层架构

---

## File Naming Conventions

| Type | Pattern |
|------|---------|
| Vue Component | PascalCase (`TitleBar.vue`) |
| Composable | camelCase with `use` prefix |
| Store | camelCase |
| Rust module | snake_case |
| Rust test | `*_test.rs` |