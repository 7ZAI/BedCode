# BedCode - Tauri 2.0 Project Guidelines

## Project Overview

BedCode 是一个跨平台应用，支持移动设备远程控制 Claude Code。桌面端 (Tauri + Vue 3) 作为主机，移动端作为远程终端。

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
├── src-tauri/
│   └── src/
│       ├── shared/           # 共享模块 (desktop + mobile)
│       │   ├── auth/         # 设备配对与认证
│       │   ├── config.rs
│       │   ├── db/           # SQLite 操作
│       │   ├── error.rs
│       │   ├── notify/
│       │   ├── parser/
│       │   ├── websocket/    # 共享 WebSocket (消息、客户端)
│       │   └── commands.rs   # 共享 Tauri commands
│       ├── desktop/          # 桌面端模块
│       │   ├── pty/
│       │   ├── session/      # 会话管理
│       │   ├── websocket/    # WebSocket 服务器
│       │   ├── plugin/
│       │   └── commands.rs
│       ├── mobile/           # 移动端模块
│       │   ├── websocket/    # 移动端 WebSocket 客户端
│       │   └── commands.rs
│       └── lib.rs
└── docs/
```

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

使用 `shared/error.rs` 中的统一错误类型：

```rust
pub type Result<T> = std::result::Result<T, AppError>;
```

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

根据平台选择日志库：

**桌面端 (desktop/):** 使用 `tracing`
```rust
use tracing::{info, debug, error, warn};

info!("Session created: {} ({})", name, id);
debug!("Processing request: {:?}", request);
```

**移动端 (mobile/):** 使用 `log`
```rust
use log::{info, debug, error, warn};

info!("Connection established: {}", addr);
debug!("Sending message: {:?}", msg);
```

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

核心逻辑**必须**添加注释，解释"为什么"而非"是什么"。

| 场景 | 说明 |
|------|------|
| 业务逻辑判断 | 解释判断原因，而非判断什么 |
| 复杂算法 | 说明关键步骤思路 |
| 异常处理 | 为什么捕获这个错误 |
| 并发控制 | 锁的获取顺序、死锁避免策略 |
| 性能优化 | 为什么这样优化 |

**示例：**

```rust
// ❌ 不好：描述代码做什么
// 遍历所有会话
for session in sessions.iter() { }

// ✅ 好：解释为什么这样做
// 遍历所有会话，清理已停止超过 24 小时的会话
// 避免会话列表无限增长占用内存
for session in sessions.iter() { }
```

使用 TODO/FIXME 标记待办：

```rust
// TODO(username): 添加断线重连逻辑
// FIXME: 并发访问时可能 panic，需要加锁
```

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