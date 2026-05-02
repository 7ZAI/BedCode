# BedCode - Tauri 2.0 Project Guidelines

## Project Overview

BedCode is a cross-platform application that enables remote control of Claude Code from mobile devices. It uses a desktop application (Tauri + Vue 3) as the host and mobile devices as remote terminals.

**Tech Stack:**
- **Desktop**: Tauri 2.0 + Vue 3 + TypeScript + TailwindCSS
- **Backend**: Rust (Tokio async runtime)
- **Database**: SQLite
- **Communication**: WebSocket + mDNS discovery
- **State Management**: Pinia

---

## Project Structure

```
bedcode/
├── src/                      # Vue 3 frontend
│   ├── components/
│   │   ├── common/           # Shared UI components
│   │   ├── desktop/          # Desktop-specific components
│   │   └── mobile/           # Mobile-specific components
│   ├── views/
│   │   ├── desktop/          # Desktop page views
│   │   └── mobile/           # Mobile page views
│   ├── stores/               # Pinia stores
│   ├── composables/          # Vue Composition API functions
│   └── router/               # Vue Router configuration
├── src-tauri/                # Rust backend
│   ├── src/
│   │   ├── auth/             # Device pairing & authentication
│   │   ├── commands.rs       # Tauri IPC commands
│   │   ├── config.rs         # App configuration
│   │   ├── db/               # SQLite operations
│   │   ├── discovery/        # mDNS device discovery
│   │   ├── error.rs          # Error types
│   │   ├── lib.rs            # App setup & initialization
│   │   ├── notify/           # Notification system
│   │   ├── parser/           # ANSI/Markdown parsing
│   │   ├── pty/              # PTY management (Windows/WSL2)
│   │   ├── session/          # Session state management
│   │   └── websocket/        # WebSocket server
│   └── tauri.conf.json       # Tauri configuration
└── docs/
    ├── superpowers/specs/    # Design specifications
    └── implementation-plans/ # Implementation plans
```

---

## Rust Backend Guidelines

### Module Organization

Each module follows this pattern:
```
module/
├── mod.rs          # Public exports and main types
├── submodule.rs    # Private implementations
└── submodule/      # For complex submodules
```

**Example:**
```rust
// mod.rs - Export public API
mod pairing;
mod storage;

pub use pairing::*;
pub use storage::*;
```

### Error Handling

Use the unified error type from `error.rs`:

```rust
// Define errors with thiserror
#[derive(Error, Debug)]
pub enum AppError {
    #[error("PTY error: {0}")]
    Pty(String),

    #[error("Session error: {0}")]
    Session(String),
    // ...
}

pub type Result<T> = std::result::Result<T, AppError>;
```

**Always return `Result<T>` from fallible functions.**

### Async Patterns

Use Tokio with `Arc<Mutex<T>>` or `Arc<RwLock<T>>` for shared state:

```rust
pub struct SessionManager {
    pty_sessions: Arc<RwLock<HashMap<String, PtySession>>>,
    db: Arc<Mutex<Database>>,
    output_tx: broadcast::Sender<PtyOutputEvent>,
    running: Arc<AtomicBool>,  // For graceful shutdown
}
```

**Guidelines:**
- Use `Mutex` for exclusive access (database, writers)
- Use `RwLock` for read-heavy access (session maps)
- Use `broadcast` for one-to-many event distribution
- Include `AtomicBool` for shutdown signals

### Thread Safety

**Never use `unsafe impl Send/Sync`.** Instead, wrap types properly:

```rust
// ✅ Correct - Use Arc<Mutex<T>> for thread safety
pub struct PtySession {
    state: Arc<Mutex<PtySessionState>>,
    running: Arc<AtomicBool>,
    output_tx: broadcast::Sender<PtyOutputEvent>,
}

// ❌ Wrong - Never do this
unsafe impl Send for PtySession {}
unsafe impl Sync for PtySession {}
```

### Tauri Commands

Commands are defined in `commands.rs` with clear section comments:

```rust
// ==================== Session Commands ====================

#[tauri::command]
pub async fn start_session(
    session_manager: State<'_, Arc<SessionManager>>,
    config_id: String,
) -> Result<String> {
    session_manager.create_session(&config_id).await
}
```

**Naming conventions:**
- `list_*` - Return multiple items
- `get_*` - Return single item
- `create_*` - Create new resource
- `delete_*` - Remove resource
- `start_*` / `stop_*` - Lifecycle operations

### Logging

Use `tracing` for structured logging:

```rust
use tracing::{info, debug, error, warn};

info!("Session created: {} ({})", name, id);
debug!("PTY output received: {} bytes", data.len());
error!("Failed to start session: {}", e);
warn!("Output channel lagged {} messages", n);
```

Log files are stored in `app_log_dir/` with daily rotation (7 days retention).

### Configuration

App configuration uses `AppConfig::load()` and `AppConfig::save()`:

```rust
let config_path = app_handle
    .path()
    .app_data_dir()
    .map(|p| p.join("config.json"))?;

let config = AppConfig::load(&config_path)?;
config.save(&config_path)?;
```

---

## Frontend Guidelines (Vue 3 + TypeScript)

### Component Structure

Use `<script setup>` syntax with TypeScript:

```vue
<template>
  <div class="...">
    <!-- Template content -->
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'

// Props
const props = defineProps<{
  sessionId: string
}>()

// Emits
const emit = defineEmits<{
  close: []
}>()

// Reactive state
const isLoading = ref(false)

// Computed
const displayText = computed(() => `Session: ${props.sessionId}`)

// Lifecycle
onMounted(async () => {
  // Initialize
})
</script>

<style scoped>
/* Component-specific styles */
</style>
```

### Composables Pattern

Business logic goes in composables, not components:

```typescript
// composables/useSession.ts
export function useSession() {
  const sessions = ref<SessionInfo[]>([])

  async function loadSessions() {
    sessions.value = await invoke('list_sessions')
  }

  return { sessions, loadSessions }
}
```

**Naming:**
- `use<Resource>` - e.g., `useSession`, `useDevice`
- `use<Action>` - e.g., `useErrorHandler`, `useToast`

### Pinia Stores

Stores wrap composables for global state:

```typescript
// stores/session.ts
export const useSessionStore = defineStore('session', () => {
  const sessions = ref<SessionInfo[]>([])
  const sessionApi = useSession()  // Use composable

  async function loadSessions() {
    await sessionApi.loadSessions()
    sessions.value = sessionApi.sessions.value
  }

  return { sessions, loadSessions }
})
```

### Platform Detection

Use `@tauri-apps/plugin-os` for platform detection:

```typescript
import { platform } from '@tauri-apps/plugin-os'
import { usePlatform } from '@/composables/usePlatform'

const { platformInfo } = usePlatform()
// platformInfo.value.isDesktop, isMobile, isWindows, etc.
```

**Never use screen width to detect desktop/mobile.**

### Error Handling

Use the centralized error handler:

```typescript
import { useErrorHandler, useToast } from '@/composables/useErrorHandler'

const { handleError, withErrorHandling } = useErrorHandler()
const toast = useToast()

// Option 1: Manual handling
try {
  await invoke('start_session', { configId })
} catch (e) {
  handleError(e)
  toast.error('Failed to start session')
}

// Option 2: Automatic wrapping
const { data, error } = await withErrorHandling(
  () => invoke('start_session', { configId })
)
if (error) {
  toast.error(error.message)
}
```

### TypeScript Types

Define types in composables for reusability:

```typescript
// composables/useTauri.ts
export interface SessionInfo {
  id: string
  configId: string
  name: string
  status: 'Starting' | 'Running' | 'WaitingInput' | 'Stopped' | 'Error'
  createdAt: string
}
```

### TailwindCSS Styling

Use the dark theme palette defined in `tailwind.config.js`:

```html
<!-- Background colors -->
<div class="bg-dark-900">      <!-- Main background -->
<div class="bg-dark-800">      <!-- Card/panel background -->
<div class="bg-dark-700">      <!-- Hover/active states -->

<!-- Text colors -->
<span class="text-dark-100">   <!-- Primary text -->
<span class="text-dark-300">   <!-- Secondary text -->
<span class="text-primary-400"> <!-- Accent color -->
```

Use `scoped` styles for component-specific CSS:
```vue
<style scoped>
.titlebar {
  -webkit-app-region: drag;
}
</style>
```

---

## Tauri 2.0 Specific Patterns

### Plugin Registration

Register plugins in `lib.rs`:

```rust
tauri::Builder::default()
    .plugin(tauri_plugin_shell::init())
    .plugin(tauri_plugin_notification::init())
    .plugin(tauri_plugin_dialog::init())
    .plugin(tauri_plugin_os::init())
    .setup(|app| { /* ... */ })
```

### Window Management

Use `@tauri-apps/api/window`:

```typescript
import { getCurrentWindow } from '@tauri-apps/api/window'

const appWindow = getCurrentWindow()
await appWindow.minimize()
await appWindow.toggleMaximize()
await appWindow.close()
```

### IPC Communication

**Frontend → Backend (Commands):**
```typescript
import { invoke } from '@tauri-apps/api/core'
const result = await invoke<string>('start_session', { configId })
```

**Backend → Frontend (Events):**
```rust
// Rust: Emit event
app_handle.emit("pty-output", &event)?;
```

```typescript
// TypeScript: Listen to event
import { listen } from '@tauri-apps/api/event'
const unlisten = await listen<PtyOutputEvent>('pty-output', (event) => {
  console.log(event.payload)
})
```

### Custom Title Bar

Enable in `tauri.conf.json`:
```json
{
  "app": {
    "windows": [{
      "decorations": false
    }]
  }
}
```

Add drag region:
```html
<div data-tauri-drag-region class="titlebar">
```

---

## Code Comment Standards

核心逻辑**必须**编写注释，解释"为什么"而非"是什么"。

### Rust 注释规范

```rust
// ==================== 单行注释 ====================
// 用于：简短说明、TODO、FIXME

// TODO: 待实现的功能
// FIXME: 已知问题，需要修复
// NOTE: 重要提示，容易误解的地方

// ==================== 文档注释 ====================
/// 用于：公开 API 文档
///
/// # Arguments
/// * `session_id` - 会话唯一标识符
///
/// # Returns
/// 会话状态，若不存在则返回 None
///
/// # Example
/// ```ignore
/// let status = manager.get_status("session-123").await;
/// ```
pub async fn get_status(&self, session_id: &str) -> Option<SessionStatus> {
    // ...
}

// ==================== 代码块注释 ====================
// 用于：复杂逻辑块前的说明

// 检查会话是否处于可终止状态
// - Running: 正常运行中，可以终止
// - WaitingInput: 等待用户输入，需要先取消等待
// - Stopped/Error: 已结束，无需操作
match session.status {
    SessionStatus::Running => self.terminate_session(id).await?,
    SessionStatus::WaitingInput => self.cancel_input_wait(id).await?,
    _ => return Ok(()),
}

// ==================== 行内注释 ====================
let timeout = 30_000; // 超时时间（毫秒），与前端心跳间隔一致
```

**必须添加注释的场景：**

| 场景 | 说明 |
|------|------|
| 业务逻辑判断 | 解释为什么这样判断，而非判断什么 |
| 复杂算法 | 关键步骤的思路说明 |
| 异常处理 | 为什么捕获这个错误，如何恢复 |
| 并发控制 | 锁的获取顺序、死锁避免策略 |
| 性能优化 | 为什么这样优化，权衡是什么 |
| 兼容性处理 | 处理特定平台/版本的坑 |

**示例 - 核心逻辑注释：**

```rust
pub async fn create_session(&self, config: &SessionConfig) -> Result<String> {
    let session_id = uuid::Uuid::new_v4().to_string();

    // 创建 PTY 进程
    // 使用 portable-pty 跨平台创建伪终端
    // Windows 上使用 ConPTY，Linux/macOS 使用 Unix PTY
    let pty_pair = native_pty_system()
        .openpty(PtySize {
            rows: config.rows,
            cols: config.cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;

    // 启动 Shell 进程
    // 优先使用用户配置的 shell，否则使用系统默认
    // WSL2 环境下需要特殊处理路径转换
    let shell = config.shell.clone().unwrap_or_else(|| {
        if cfg!(target_os = "windows") {
            "powershell.exe".to_string()
        } else {
            "/bin/bash".to_string()
        }
    });

    let mut child = pty_pair.slave.spawn_command(
        CommandBuilder::new(&shell)
    )?;

    // 注册输出监听
    // 使用 broadcast channel 实现一对多分发
    // 允许多个客户端同时订阅同一个会话的输出
    let (output_tx, _) = broadcast::channel(1024);
    let output_rx = pty_pair.master.try_clone_reader()?;

    // 启动输出读取任务
    // 独立 tokio 任务，避免阻塞主线程
    // 读取到的数据同时发送到：
    // 1. WebSocket 客户端（远程终端显示）
    // 2. 前端事件（本地显示）
    tokio::spawn(async move {
        Self::read_pty_output(output_rx, output_tx).await;
    });

    // 存储会话状态
    // 使用 RwLock 允许并发读取会话列表
    let session = PtySession {
        id: session_id.clone(),
        child: Arc::new(Mutex::new(child)),
        output_tx,
        status: Arc::new(RwLock::new(SessionStatus::Running)),
    };

    self.sessions.write().await.insert(session_id.clone(), session);

    Ok(session_id)
}
```

### TypeScript 注释规范

```typescript
// ==================== 单行注释 ====================
// 用于：简短说明

// ==================== JSDoc 注释 ====================
/**
 * 用于：公开函数/组件的文档
 * @param sessionId - 会话 ID
 * @returns 会话信息
 * @throws {SessionError} 会话不存在时抛出
 */
export async function getSession(sessionId: string): Promise<SessionInfo> {
  // ...
}

// ==================== Vue 组件注释 ====================
<script setup lang="ts">
/**
 * 会话终端组件
 *
 * 功能：
 * - 显示 PTY 输出（ANSI 解析后）
 * - 发送用户输入到后端
 * - 支持复制/粘贴/清屏
 */
</script>

// ==================== 复杂逻辑注释 ====================
// 计算滚动位置
// - isAtBottom: 用户是否在底部（距底部 50px 内）
// - 自动滚动仅当用户在底部时触发，避免打断用户查看历史
const isAtBottom = terminalRef.value
  ? terminalRef.value.scrollHeight - terminalRef.value.scrollTop <= terminalRef.value.clientHeight + 50
  : true
</script>
```

**TypeScript 必须添加注释的场景：**

| 场景 | 说明 |
|------|------|
| Composable 函数 | 用 JSDoc 说明用途、参数、返回值 |
| 复杂的计算属性 | 解释计算逻辑 |
| 异步操作流程 | 说明数据流和错误处理 |
| 平台特定代码 | 说明平台差异处理 |
| 状态管理逻辑 | 解释状态转换规则 |

**示例 - Composable 注释：**

```typescript
/**
 * 会话管理 Composable
 *
 * 提供会话的 CRUD 操作和实时状态更新
 * 通过 WebSocket 接收后端推送的会话变更
 */
export function useSession() {
  const sessions = ref<SessionInfo[]>([])
  const loading = ref(false)
  const error = ref<Error | null>(null)

  /**
   * 加载所有会话列表
   * 从后端获取会话基本信息，不包含实时输出
   */
  async function loadSessions() {
    loading.value = true
    error.value = null

    try {
      sessions.value = await invoke<SessionInfo[]>('list_sessions')
    } catch (e) {
      error.value = e as Error
      // 使用全局错误处理器显示用户友好提示
      handleError(e, '加载会话列表失败')
    } finally {
      loading.value = false
    }
  }

  /**
   * 创建新会话
   * @param config - 会话配置（shell、工作目录等）
   * @returns 新会话的 ID
   */
  async function createSession(config: SessionConfig): Promise<string> {
    const sessionId = await invoke<string>('create_session', { config })

    // 立即添加到本地列表，无需等待 WebSocket 推送
    // 提升用户体验，减少感知延迟
    sessions.value.unshift({
      id: sessionId,
      status: 'Starting',
      createdAt: new Date().toISOString(),
      ...config,
    })

    return sessionId
  }

  /**
   * 终止会话
   * @param sessionId - 要终止的会话 ID
   *
   * 注意：终止操作会先发送 Ctrl+C，等待进程优雅退出
   * 若 5 秒后进程仍未退出，则强制终止
   */
  async function terminateSession(sessionId: string): Promise<void> {
    await invoke('terminate_session', { sessionId })

    // 更新本地状态
    const session = sessions.value.find(s => s.id === sessionId)
    if (session) {
      session.status = 'Stopped'
    }
  }

  return {
    sessions,
    loading,
    error,
    loadSessions,
    createSession,
    terminateSession,
  }
}
```

### 注释原则

1. **解释 Why，而非 What**
   ```rust
   // ❌ 不好：描述代码做了什么（代码本身已说明）
   // 遍历所有会话
   for session in sessions.iter() { }

   // ✅ 好：解释为什么这样做
   // 遍历所有会话，清理已停止超过 24 小时的会话
   // 避免会话列表无限增长占用内存
   for session in sessions.iter() { }
   ```

2. **注释与代码同步更新**
   - 修改代码时必须同步更新注释
   - 过时的注释比没有注释更糟糕

3. **避免废话注释**
   ```rust
   // ❌ 废话
   let count = 0; // 初始化计数器为 0

   // ✅ 有意义
   let count = 0; // 重试计数器，超过 3 次后放弃
   ```

4. **使用 TODO/FIXME 标记待办**
   ```rust
   // TODO(username): 添加断线重连逻辑
   // FIXME: 并发访问时可能 panic，需要加锁
   // NOTE: 此处依赖 sqlite 的默认隔离级别
   // HACK: 临时方案，等待上游库修复
   ```

---

## Testing

### Rust Tests

Unit tests in `#[cfg(test)]` modules:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_waiting() {
        let parser = OutputParser::new();
        assert!(parser.detect_waiting_input("> "));
    }
}
```

Integration tests in `src-tauri/tests/`:
```rust
// tests/session_test.rs
use bedcode_lib::*;

#[tokio::test]
async fn test_session_manager_default() {
    let manager = SessionManager::default();
    let sessions = manager.list_sessions().await;
    assert!(sessions.is_empty());
}
```

### Frontend Tests

Use Vitest with Vue Test Utils:
```typescript
describe('SessionStore', () => {
  it('should load sessions', async () => {
    const store = useSessionStore()
    await store.loadSessions()
    expect(store.sessions).toBeDefined()
  })
})
```

---

## Build Commands

```bash
# Development
npm run dev              # Start Vite dev server
npm run tauri:dev        # Start Tauri in dev mode

# Building - Desktop
npm run build            # Build frontend only
npm run tauri:build      # Build production app (Windows/macOS/Linux)

# Building - Android
npm run tauri:android:init   # Initialize Android project (requires Android SDK)
npm run tauri:android:build  # Build Android APK

# Testing
npm run test             # Run Vitest in watch mode
npm run test:run         # Run Vitest once
npm run test:coverage    # Run with coverage
cargo test               # Run Rust tests

# Code Quality
npm run lint             # ESLint
npm run format           # Prettier
cargo clippy             # Rust linter
```

---

## Target Directory Management

Rust 增量编译会导致 `target` 目录持续增长。**每次编译前必须检查并清理：**

```bash
# 检查 target 目录大小，超过 15GB 则执行 cargo clean
check_and_clean_target() {
    TARGET_DIR="src-tauri/target"
    if [ -d "$TARGET_DIR" ]; then
        SIZE_GB=$(du -sb "$TARGET_DIR" 2>/dev/null | awk '{printf "%.0f", $1 / 1073741824}')
        if [ "$SIZE_GB" -gt 15 ]; then
            echo "Target directory is ${SIZE_GB}GB (>15GB), running cargo clean..."
            cd src-tauri && cargo clean
        fi
    fi
}
```

**规则：**
- 执行任何 Rust/Tauri 编译命令前，先检查 `src-tauri/target` 目录大小
- 若超过 **15GB**，自动执行 `cargo clean` 清理
- 原因：增量编译缓存累积会导致磁盘空间耗尽

---

## Android Build Setup

### Prerequisites

1. **Android Studio** with:
   - Android SDK Platform 34
   - Android SDK Build-Tools 34
   - Android SDK Command-line Tools
   - Android NDK (r25c recommended)
   - CMake

2. **Environment Variables** (add to `~/.bashrc` or `~/.zshrc`):
   ```bash
   export ANDROID_HOME=$HOME/Android/Sdk
   export ANDROID_SDK_ROOT=$ANDROID_HOME
   export NDK_HOME=$ANDROID_HOME/ndk/25.2.9519653
   export PATH=$PATH:$ANDROID_HOME/emulator:$ANDROID_HOME/platform-tools
   ```

3. **Verify**:
   ```bash
   source ~/.bashrc
   sdkmanager --list
   ```

### Initialize and Build

```bash
# First time setup
npm run tauri:android:init

# Build debug APK
npm run tauri:android:build

# Build release APK
npx tauri android build --release
```

### Platform-Specific Dependencies

Some Rust dependencies are desktop-only:

```toml
# Cargo.toml
[target.'cfg(not(any(target_os = "android", target_os = "ios")))'.dependencies]
portable-pty = "0.8"  # PTY not available on mobile
```

See `docs/android-setup.md` for detailed setup instructions.

---

## Key Architecture Decisions

1. **Separation of Concerns**: Composables handle API calls, stores manage global state, components focus on UI
2. **Async Everywhere**: Rust uses Tokio, frontend uses async/await with Tauri commands
3. **Event-Driven Architecture**: PTY output flows through `broadcast` channels to WebSocket and frontend
4. **Graceful Shutdown**: Use `AtomicBool` flags to signal shutdown to background tasks
5. **Type Safety**: Full TypeScript on frontend, strong typing in Rust

---

## Common Patterns Reference

### Adding a New Tauri Command

1. Define in `commands.rs`:
```rust
#[tauri::command]
pub async fn my_new_command(param: String) -> Result<MyResult> {
    // Implementation
}
```

2. Register in `lib.rs`:
```rust
.invoke_handler(tauri::generate_handler![
    commands::my_new_command,
    // ...
])
```

3. Create TypeScript wrapper in `composables/useTauri.ts`:
```typescript
export function useMyFeature() {
  async function doSomething(param: string) {
    return invoke<MyResult>('my_new_command', { param })
  }
  return { doSomething }
}
```

### Adding a New Module

1. Create `src-tauri/src/new_module/mod.rs`
2. Add `pub mod new_module;` to `lib.rs`
3. Export from module: `pub use new_module::*;`
4. Create tests in `tests/new_module_test.rs`

---

## File Naming Conventions

| Type | Pattern | Example |
|------|---------|---------|
| Vue Component | PascalCase | `TitleBar.vue`, `SessionCard.vue` |
| Composable | camelCase with `use` prefix | `useTauri.ts`, `usePlatform.ts` |
| Store | camelCase | `session.ts`, `device.ts` |
| Rust module | snake_case | `session_manager.rs` |
| Rust test | snake_case with `_test` | `session_test.rs` |
| View | PascalCase with `View` suffix | `SessionsView.vue` |
