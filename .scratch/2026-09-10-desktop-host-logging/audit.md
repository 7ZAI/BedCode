# Audit: 存量日志拼串迁移清单（刀 1 / Ticket 04）

> 基线：`git show HEAD:bedcode-desktop/src-tauri` 之后 `rg 'tracing::(debug|info|warn|error|trace)!' | grep '{}'` = **186 处**（跨 42 文件）。
> 分类标准（spec 刀 1）：机器检索用的 ID → 字段化（`key = %v` / `key = ?v`）；人读内容（错误文本、路径、命令名、数量、URL）→ 留在消息。
> 迁移原则：不改变日志级别/时机/频率；每个文件迁移后跑该模块测试。

## 图例

- ✅ **字段化**：`plugin_id` / `session_id` / `config_id` / `client_id` / `peer` / `pid` 等 canonical key
- ➖ **保留**：错误文本、路径、文件名、命令名、数量、端口、类型名等（无检索需求或非 canonical key）
- 🔀 **复合标识拆字段**：`format!("{}::{}", plugin_id, name)` → `plugin_id` + `command` 双字段

## 插件域（plugin/）

### plugin/host.rs（34 处，字段化 26）

| 行 | 现消息 | 处理 |
| --- | --- | --- |
| 180 | Found {} file-based plugin(s) | ➖ 数量 |
| 370 | Registered Rust command: {} | ➖ 命令名（人读） |
| 387 | Registered Rust terminal handler for plugin {} | ✅ `plugin_id = %entry.id` |
| 435 | list_plugins() returning {} plugin(s) | ➖ 数量 |
| 461 | is_activated({}) = {} | ✅ `plugin_id = %plugin_id` + `activated = result` |
| 474 | Notifying plugin {} on_startup | ✅ `plugin_id = %entry.id` |
| 481 | Plugin {} on_startup timed out | ✅ `plugin_id = %entry.id` |
| 485 | Plugin {} on_startup failed: {} | ✅ `plugin_id = %entry.id`；➖ 错误文本留消息 |
| 513 | Notifying plugin {} on_shutdown | ✅ `plugin_id = %entry.id` |
| 520 | Plugin {} on_shutdown timed out | ✅ `plugin_id = %entry.id` |
| 523 | Plugin {} on_shutdown failed: {} | ✅ `plugin_id = %entry.id`；➖ 错误留消息 |
| 557 | Failed to deactivate plugin {} during shutdown: {} | ✅ `plugin_id = %id`；➖ 错误留消息 |
| 660 | activate_plugin({}, persist={}) | ✅ `plugin_id = %plugin_id` + `persist = persist` |
| 689 | Plugin {} already activated, skipping | ✅ `plugin_id = %plugin_id` |
| 779 | WASM plugin {} not found in wasm_plugins map | ✅ `plugin_id = %plugin_id` |
| 792 | Plugin '{}' activated | ✅ `plugin_id = %plugin_id` |
| 808 | Plugin '{}' activate() failed: {} | ✅ `plugin_id`；➖ 错误留消息 |
| 829 | Calling on_startup for plugin '{}' | ✅ `plugin_id = %plugin_id` |
| 832 | Plugin '{}' on_startup completed | ✅ `plugin_id` |
| 968 | Timer aborted for '{}' | ✅ `plugin_id`（定时器归属插件） |
| 999 | deactivate_plugin({}, persist={}) | ✅ `plugin_id` + `persist` |
| 1013 | Calling on_shutdown for plugin '{}' | ✅ `plugin_id` |
| 1018 | Plugin '{}' on_shutdown completed | ✅ `plugin_id` |
| 1046 | Plugin '{}' deactivated | ✅ `plugin_id` |
| 1056 | Plugin '{}' deactivate() failed: {} | ✅ `plugin_id`；➖ 错误留消息 |
| 1060 | Plugin '{}' deactivate() panicked: {} | ✅ `plugin_id`；➖ panic 文本留消息 |
| 1135 | Hot-reloading WASM plugin: {} | ✅ `plugin_id` |
| 1168 | WASM plugin hot-reloaded successfully: {} | ✅ `plugin_id` |
| 1288 | Persist: {} = {} | ✅ `plugin_id = %id` + `persist = active` |
| 1291 | Failed to persist plugin activation state: {} | ➖ 错误文本 |
| 1304 | Persisted: {} = {} | ✅ `plugin_id = %id` + `persist = active` |
| 1345 | Auto-activating plugin: {} | ✅ `plugin_id` |
| 1347 | Failed to auto-activate plugin {}: {} | ✅ `plugin_id`；➖ 错误留消息 |
| 1362 | Failed to clean up stale activation entries: {} | ➖ 错误文本 |

### plugin/message_bus.rs（9 处，字段化 7）

| 行 | 现消息 | 处理 |
| --- | --- | --- |
| 99 | no subscribers for topic '{}', message dropped | ➖ topic 名（非 canonical key） |
| 128 | subscriber '{}' not activated, skipping | ✅ `plugin_id` |
| 132 | dispatch to WASM plugin '{}' failed: {} | ✅ `plugin_id`；➖ 错误留消息 |
| 142 | handler for static plugin '{}' failed: {} | ✅ `plugin_id`；➖ 错误留消息 |
| 169 | plugin '{}' already subscribed to '{}' | ✅ `plugin_id`；➖ topic 留消息 |
| 175 | plugin '{}' subscribed to '{}' | ✅ `plugin_id`；➖ topic 留消息 |
| 186 | static plugin '{}' subscribed to '{}' | ✅ `plugin_id`；➖ topic 留消息 |
| 199 | plugin '{}' unsubscribed from '{}' | ✅ `plugin_id`；➖ topic 留消息 |
| 214 | removed plugin '{}' from topic '{}' | ✅ `plugin_id`；➖ topic 留消息 |

### plugin/api_bridge.rs（8 处，字段化 7）

| 行 | 现消息 | 处理 |
| --- | --- | --- |
| 19 | plugin_list_loaded returning {} plugin(s) | ➖ 数量 |
| 29 | plugin_get_info({}) | ✅ `plugin_id` |
| 44 | plugin_preauthorize({}) | ✅ `plugin_id` |
| 47 | plugin_preauthorize({}) failed: {} | ✅ `plugin_id`；➖ 错误留消息 |
| 55 | plugin_activate({}) | ✅ `plugin_id` |
| 58 | plugin_activate({}) failed: {} | ✅ `plugin_id`；➖ 错误留消息 |
| 66 | plugin_deactivate({}) | ✅ `plugin_id` |
| 69 | plugin_deactivate({}) failed: {} | ✅ `plugin_id`；➖ 错误留消息 |

### plugin/host/services.rs（4 处，字段化 3）

| 行 | 现消息 | 处理 |
| --- | --- | --- |
| 33 | SessionLifecycle: dispatch to plugin '{}' failed: {} | ✅ `plugin_id`；➖ 错误留消息 |
| 67 | InputSubmitted: dispatch to plugin '{}' failed: {} | ✅ `plugin_id`；➖ 错误留消息 |
| 105 | Plugin {} self-check failed: {} | ✅ `plugin_id`；➖ 错误留消息 |
| 293 | CLI uninstalled for '{}': {} | ✅ `plugin_id`；➖ 文件路径留消息 |

### plugin/watcher.rs（4 处，字段化 2）

| 行 | 现消息 | 处理 |
| --- | --- | --- |
| 47 | Plugin watcher error: {} | ➖ 错误文本 |
| 85 | debounced reload for '{}' | ✅ `plugin_id` |
| 100 | WASM hot-reloaded '{}' | ✅ `plugin_id` |
| 137 | Plugin dev watcher started: watching '{}' | ➖ 路径 |

### plugin/wasm_runtime/host_impl/mod.rs（1 处，字段化 1）

| 行 | 现消息 | 处理 |
| --- | --- | --- |
| 47 | "{}: permission denied"（api） | ✅ `api = %api`（消息只留 "permission denied"） |

### 插件域保留（不迁移）：loader.rs:38/133（错误+目录路径）、fs_auth.rs:143（错误）、
approval.rs:69（错误）、storage.rs:99（错误）、app_cli.rs:215/232/287（路径）、
wasm_runtime/component.rs:661/679/700/720（WASM guest 报错文本，均在 plugin span 内）

## 会话域（session/ + commands/session*）

### session/session_manager.rs（16 处，字段化 11）

| 行 | 现消息 | 处理 |
| --- | --- | --- |
| 146 | SessionLifecycleListener registered (total: {}) | ➖ 数量 |
| 159 | Removed {} lifecycle listener(s) for plugin '{}' | ✅ `plugin_id`；➖ 数量留消息 |
| 185 | SessionInputListener registered (total: {}) | ➖ 数量 |
| 198 | Removed {} input listener(s) for plugin '{}' | ✅ `plugin_id`；➖ 数量留消息 |
| 250 | Registered session {} in GlobalOutputManager | ✅ `session_id` |
| 392 | Session created: {} ({}) | ✅ `session_id`；➖ 名称留消息 |
| 462 | Session created (not started): {} ({}) | ✅ `session_id`；➖ 名称留消息 |
| 516 | Session started: {} ({}) | ✅ `session_id`；➖ 名称留消息 |
| 630 | Session restarted: {} ({}) | ✅ `session_id`；➖ 名称留消息 |
| 794 | kill_session called for: {} | ✅ `session_id` |
| 805 | Failed to kill PTY for session {}: {} | ✅ `session_id`；➖ 错误留消息 |
| 845 | Session killed: {} | ✅ `session_id` |
| 858 | remove_session called for: {} | ✅ `session_id` |
| 888 | Session removed: {} ({}) | ✅ `session_id`；➖ 名称留消息 |
| 937 | Cleaned up stopped session: {} | ✅ `session_id` |
| 948 | Failed to kill all sessions: {} | ➖ 错误文本 |

### commands/session.rs（9 处，字段化 7）

| 行 | 现消息 | 处理 |
| --- | --- | --- |
| 15 | start_session called with config_id: {} | ✅ `config_id` |
| 26 | Session created successfully: {} | ✅ `session_id` |
| 30 | Failed to create session: {} | ➖ 错误文本 |
| 41 | create_session_no_start called with config_id: {} | ✅ `config_id` |
| 45 | Session created (not started) successfully: {} | ✅ `session_id` |
| 49 | Failed to create session (not started): {} | ➖ 错误文本 |
| 62 | start_existing_session called with session_id: {} | ✅ `session_id` |
| 73 | Session started successfully: {} | ✅ `session_id` |
| 77 | Failed to start session: {} | ➖ 错误文本 |

### session/session_config.rs（4 处，字段化 3）

| 行 | 现消息 | 处理 |
| --- | --- | --- |
| 115 | Session config created: {} ({}) | ✅ `config_id`；➖ 名称留消息 |
| 230 | Session config updated: {} ({}) | ✅ `config_id`；➖ 名称留消息 |
| 262 | Session config deleted: {} | ✅ `config_id` |
| 298 | Unknown environment type: {} | ➖ 类型名 |

### session/session_output.rs（4 处，字段化 4）

| 行 | 现消息 | 处理 |
| --- | --- | --- |
| 569 | Failed to send history to {}: {} | ✅ `client_id`；➖ 错误留消息 |
| 717 | Session {} registered | ✅ `session_id` |
| 724 | Session {} unregistered | ✅ `session_id` |
| 808 | Session {} not found for subscribe | ✅ `session_id` |

### 其余会话域：session_components.rs:114 → ✅ `session_id`（`Failed to kill session {}: {}`）；
commands/session_config.rs:28 → ✅ `config_id`（`create_session_config success: id={}`）；29 ➖ 错误

## 网络域（server/ + pty/ + peer_* + events/）

### server/ws/terminal_ws.rs（10 处，字段化 4 处 addr→peer）

| 行 | 现消息 | 处理 |
| --- | --- | --- |
| 188 | WebSocket heartbeat timeout for {} | ✅ `peer = %addr`（地址） |
| 354 | Terminal WS connected: {} | ✅ `peer = %addr` |
| 407 | Terminal WS disconnected: {} | ✅ `peer = %addr` |
| 638 | Unsupported WS message type from {} | ✅ `peer = %addr` |
| 757/762/1194/1204/1327/1333 | Aborting previous output/subscribe: {} | ➖ fwd_key/sub_key 内部复合键 |

### server/ws/registry.rs（3 处，字段化 1）

| 行 | 现消息 | 处理 |
| --- | --- | --- |
| 128 | Unregistered client {} by addr {} | ✅ `client_id = %cid` + `peer = %addr` |
| 203 | Broadcast to {} clients | ➖ 数量 |
| 352 | Cleared {} sessions | ➖ 数量 |

### pty/pty_process.rs（8 处，字段化 6）

| 行 | 现消息 | 处理 |
| --- | --- | --- |
| 161 | PTY session started: {} ({}, pid={:?}) | ✅ `session_id` + `pid` |
| 252 | Kill session {}: process_id = {:?} | ✅ `session_id` + `pid` |
| 264 | Executing taskkill for PID {} | ✅ `pid` |
| 269 | taskkill output: {} | ➖ 输出文本 |
| 270 | taskkill failed: {} | ➖ 错误文本 |
| 281 | No process_id available for session {} | ✅ `session_id` |
| 284 | PTY session killed: {} (pid={:?}) | ✅ `session_id` + `pid` |
| 346 | PTY session killed on drop: {} (pid={}) | ✅ `session_id` + `pid` |

### pty/pty_reader.rs（3 处，字段化 2）

| 行 | 现消息 | 处理 |
| --- | --- | --- |
| 69 | PTY output consumer exited: {} | ✅ `session_id`（consumer_session_id） |
| 87 | PTY session ended: {} | ✅ `session_id` |
| 110 | PTY read error: {} | ➖ 错误文本 |

### events/sync_handler.rs（6 处，字段化 3）

| 行 | 现消息 | 处理 |
| --- | --- | --- |
| 133 | Session not found: {} | ✅ `session_id` |
| 229 | Config not found: {} | ✅ `config_id` |
| 260 | Config not found: {} | ✅ `config_id` |
| 373/378/384 | Failed to broadcast: {} | ➖ 错误文本 |

### 网络域保留：server/app.rs（路径/端口）、supervisor.rs（端口/错误）、port_checker.rs（端口×4）、
websocket_manager.rs（端口）、services/session_control.rs:357（source）、services/pairing_service.rs（配对码+TTL）、
services/terminal_service.rs（key combo）、events/forwarder.rs（事件名+错误）、events/matcher.rs（类型名×7）

## 系统域（system/ + lib.rs + commands/）

### system/lifecycle.rs（8 处，字段化 3）

| 行 | 现消息 | 处理 |
| --- | --- | --- |
| 152 | Running {} startup hook(s) | ➖ 数量 |
| 155 | Startup hook: {} (priority={}) | ✅ `hook = %owner`（钩子名可检索）+ `priority` |
| 182 | Running {} shutdown hook(s) | ➖ 数量 |
| 185 | Shutdown hook: {} (priority={}) | ✅ `hook = %owner` + `priority` |
| 216 | Window close prevented by hook: {} (priority={}) | ✅ `hook = %owner` + `priority` |
| 270/279/290 | Failed to ... during shutdown: {} | ➖ 错误文本 |

### lib.rs（8 处，全部保留）：版本/端口/错误文本/文件路径

### commands/（系统域）：commands/system.rs（路径×2，保留）、commands/server.rs（端口/bool，保留）、
commands/qr.rs（TTL，保留）

---

## 迁移统计

- 单行基线：186 处（跨 42 文件）；**含多行宏与 `{name}` 捕获式后全仓约 395 处**（audit 表基于单行 186 处分类，实施时同步处理了多行宏同类项）
- **字段化**：约 90 处（插件域 47 + 会话域 26 + 网络域 16 + 系统域 3）；目标文件内关键 ID 拼串清零（`rg 'plugin_id' / session_id / config_id / client_id` 后无 `{}` 拼串）
- **保留**：约 105 处（人读内容：错误文本/路径/命令名/数量/端口/类型名，语义不变）
- 迁移不改级别/时机/频率，不新增日志行；全量 cargo test 596 通过，clippy 回基线 114 警告（0 新增）
- 实施偏差记录：spec 刀 2 成功路径原写 info，按级别语义表决定落地为 debug（成功不刷屏）；json span 链需配 `JsonFields`（见 issue 02 备注）；bootstrap 用每次重建句柄的 writer 替代 rolling::never（dev reset 删除后自动重建，语义等价）