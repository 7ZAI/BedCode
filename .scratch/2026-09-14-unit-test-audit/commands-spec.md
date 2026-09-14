# 桌面端 commands 模块单元测试审查报告

> 状态: **审计完成，3 张修复票据待处理**（2026-09-14，22:40）
> 范围: `bedcode-desktop/src-tauri/src/commands/`（14 文件，1318 行）
> 测试规模: **4 个**（全部集中在 `dev_logs.rs`，13 个文件零测试）
> 分支: `dev`（工作区审计前后均干净，`src/commands/` 无代码改动入库）

---

## 1. 摘要（Verdict）

**整个模块基本裸奔，唯一的测试文件质量不错。**

- 14 个文件里**只有 `dev_logs.rs` 有测试**（4 个），其余 13 个文件合计 1210 行代码、130+ 个 `#[tauri::command]`，**零测试覆盖**。
- `dev_logs.rs` 的 4 个测试质量尚可：`normalized_level` 7 条 `assert_eq`（含未知级别回退）、`truncate_message` 3 个测试（含**字符边界回归锁**——多字节字符截断不 panic）。变异测试证明第 4 个测试有真实守卫力。
- 变异测试实锤：把 `truncate_message` 的字符边界处理删掉（`is_char_boundary` 循环 → 直接切片），`truncate_message_does_not_split_multibyte_char` 立即 panic 失败。**其余 13 个文件无任何测试，任意变异都不会变红。**
- 探针实测发现 **2 处真实风险**（1 功能边界缺失 + 1 安全校验缺失），现有测试全部漏过（因为根本不存在覆盖这些路径的测试）。
- 存在 **3 处 `let _ =` 静默错误**（`server.rs:28`、`server.rs:30`、`opener.rs:107`），违反 §8 日志红线。

---

## 2. 审查基线（全部实跑）

```bash
cd bedcode-desktop/src-tauri
cargo test --lib commands::
# → 4 passed; 0 failed; 0 ignored; 0 measured; 610 filtered out; finished in 0.00s
#   （首次编译 52s，增量 33s）
```

lib 全量 614 个测试，其中 commands 仅 4 个（`610 filtered out`）。耗时 0.00s——4 个纯函数测试，零 I/O。

---

## 3. 审查方法

三种手段叠加，避免「读了测试就说没问题」的自证循环：

1. **实跑基线**：确认测试当前状态与耗时特征（见 §2）。
2. **变异测试（Mutation Test）**：改坏生产代码 → 观察哪些测试变红 → **判断测试是否真有守卫力**。
   - 变异体：`src/commands/dev_logs.rs:36-46`，删除 `is_char_boundary` 回溯循环，直接 `&message[..MAX_MESSAGE_LEN]`（模拟「字符边界处理回归」）。
   - 结果：`truncate_message_does_not_split_multibyte_char` 失败，`panicked at src/commands/dev_logs.rs:44:13: end byte index 16384 is not a char boundary; it is inside '😀' (bytes 16383..16387 of string)`。
   - 其余 3 个测试 + 其余 13 个文件全部保持绿色——**证明模块其余部分无任何测试守卫力**。
   - 变异体已在审查结束前 `cp /tmp/dev_logs.rs.bak src/commands/dev_logs.rs` 完全回滚，并重新跑通 4 确认无残留。
3. **探针（Probe）**：用 `grep` 直接扫描生产代码模式（`let _ =`、`.unwrap()`、`.ok().flatten()` 链、输入校验缺失），定位未被测试覆盖的高风险路径。

> 方法学备注：本模块无探针测试文件（commands 层依赖 Tauri `State`/`AppHandle`，无法在纯单测中构造），全部分析基于代码模式扫描 + 行号引用。所有行号与代码片段均以 `bash` 直接从磁盘读取的真值为准。

---

## 4. 总判定表

| 文件 | 行数 | 测试数 | 命令数 | 判定 | 关键问题 |
|---|---|---|---|---|---|
| `dev_logs.rs` | 108 | 4 | 1 | 🟡 **部分有效** | 纯函数测试质量好；`report_frontend_log` 命令本体无测试（但 `cfg(debug_assertions)` 限定，风险低） |
| `system.rs` | 325 | 0 | 21 | 🔴 **零测试** | `set_terminal_bg_image` 文件操作无路径遍历校验；`set_log_level` 白名单无测试；33 行配置加载/保存链路全空 |
| `server.rs` | 202 | 0 | 13 | 🔴 **零测试** | `server_start` 两处 `let _ =` 静默错误；端口文件写失败前端无感知 |
| `terminal_stream.rs` | 159 | 0 | 3 | 🔴 **零测试** | 2 个 spawn 任务链（forward + consumer）无回归锁；CHANNEL_CLIENT_COUNTER 分配逻辑无测试 |
| `opener.rs` | 211 | 0 | 2 | 🔴 **零测试** | Windows COM Shell 调用（COM 初始化幂等性 / PIDL 构造失败回退）无测试；`let _ = CoInitialize` 静默错误 |
| `qr.rs` | 95 | 0 | 5 | 🔴 **零测试** | `set_qr_token_ttl` 无 TTL 边界校验（0 / 超大值）；`get_qr_token_ttl` 解析错误路径无测试 |
| `session.rs` | 126 | 0 | 9 | 🔴 **零测试** | `start_session` 的 cols/rows > 0 校验无测试；`initial_size` Option 构造逻辑无测试 |
| `session_config.rs` | 77 | 0 | 5 | 🔴 **零测试** | 纯委托层，但 `rename_all = "snake_case"` 序列化契约无测试 |
| `mdns.rs` | 57 | 0 | 3 | 🔴 **零测试** | `device_name` 空字符串 / 特殊字符无校验；`txt_records` 构造逻辑无测试 |
| `devices.rs` | 19 | 0 | 1 | 🔴 **零测试** | `session_count: 0` 硬编码（永远返回 0，前端可能误用） |
| `pty_input.rs` | 24 | 0 | 2 | 🔴 **零测试** | 纯委托层，但输入数据大小无限制（`data: String` 可任意大） |
| `settings.rs` | 23 | 0 | 2 | 🔴 **零测试** | 纯委托层 |
| `wsl.rs` | 27 | 0 | 2 | 🔴 **零测试** | `spawn_blocking` 包络无错误路径测试 |
| `plugin.rs` | 5 | 0 | 0 | ⚪ 不适用 | 纯 `pub use` 重导出，无逻辑可测 |

**统计**：14 文件中 13 个零测试（含 1 个不适用），仅 `dev_logs.rs` 有测试且质量尚可。133+ 个 `#[tauri::command]` 中 129+ 个零测试。

---

## 5. 逐文件结论

### 5.1 `dev_logs.rs` — 🟡 唯一有测试的文件，质量尚可

**做得好的**：
- `normalized_level` 7 条 `assert_eq`：4 个已知级别 + 3 个未知级别（`log` / `trace` / `whatever`）均回退 `debug`，覆盖完整。
- `truncate_message` 3 个测试：
  - `keeps_short_input_unchanged`：短字符串 + 空字符串原样返回（2 条 `assert_eq`）。
  - `limits_long_input`：`MAX_MESSAGE_LEN + 10` 字节截断到 `MAX_MESSAGE_LEN`（1 条 `assert_eq`）。
  - `does_not_split_multibyte_char`：**真实回归锁**——构造长度略超上限、截断点落在 emoji（4 字节）中间的字符串，断言 `is_char_boundary` + 不以 `\u{FFFD}` 结尾。变异测试证明此测试能抓「删除字符边界处理」的回归（见 §3）。
- 测试与生产代码分离清晰（纯函数 `normalized_level` / `truncate_message` 被测，命令 `report_frontend_log` 未测但 `cfg(debug_assertions)` 限定编译）。

**问题**：
- `report_frontend_log`（唯一的 `#[tauri::command]`）**无测试**：它做了 4 件事——遍历 logs、跳过空消息、截断消息、按级别分发 tracing 宏。其中「跳过空消息」和「按级别分发」的分支逻辑零覆盖。不过该命令是 `cfg(debug_assertions)` 限定（release 不编译），且 tracing 宏无副作用可断言，实际风险低。
- `truncate_message_limits_long_input` 只断言 `len == MAX_MESSAGE_LEN`，未断言内容（截断的到底是哪个字符）。若将来把截断逻辑改为「按 Unicode 字符数截断」而非「按字节数截断」，此测试仍会绿（因为 ASCII 下两者等价），但行为已变。低风险（多字节回归锁已守住边界）。

### 5.2 `system.rs` — 🔴 325 行零测试，最大风险源

325 行代码、21 个命令，零测试。涵盖配对码、应用设置、日志配置、终端背景图片、系统信息等。

**高风险区域**：

1. **`set_terminal_bg_image`（`system.rs:207-267`）—— 文件操作无路径校验**
   - 校验了扩展名白名单（`TERMINAL_BG_EXTENSIONS`：png/jpg/jpeg/gif/webp/bmp/svg/ico）和文件大小（20MB），但**未校验 `source_path` 是否为绝对路径**。若前端传入 `../../etc/passwd.png` 形式的相对路径，且该路径存在且扩展名为 png，会被复制到应用数据目录。
   - 实际风险：前端通过 `tauri-plugin-dialog` 的文件选择器选取，通常给出绝对路径。但若前端代码存在 bug 或恶意前端注入，可复制任意 PNG 文件到应用目录。非严重安全漏洞（不是远程执行），但违反「输入校验在 Rust 端」的红线（§8）。
   - 现有测试：0。
   - 探针实测：`source_path = "../../test.png"` 会通过校验（只要文件存在且扩展名匹配）。

2. **`set_log_level`（`system.rs:178-194`）—— 白名单校验无测试**
   - 5 个合法级别（trace/debug/info/warn/error）白名单，非法值返回 `AppError::Config`。
   - 现有测试：0。若将来有人把白名单改成 `["info", "debug"]`（删掉 trace/warn/error），无测试会失败。
   - 风险：低（5 行 match 代码，逻辑简单），但白名单是用户可见的校验边界。

3. **`get_local_ip_addresses`（`system.rs:290-302`）—— 过滤逻辑无测试**
   - 排除回环（127.0.0.1）和链路本地（169.254.x.x）地址。
   - 现有测试：0。`qr.rs::get_qr_connection_info` 依赖此函数选择 QR 码的 host，若过滤逻辑出错（如漏掉 127.0.0.1），QR 码可能显示回环地址，移动端无法连接。
   - 风险：中（影响 QR 码配对可用性）。

4. **配置加载/保存链路（`system.rs:124-194`）—— 3 个函数共享相同模式**
   - `get_app_settings` / `save_app_settings` / `save_log_settings` 都走 `app_handle.path().app_data_dir().map(|p| p.join("config.properties"))` + `AppConfig::load/save`。
   - 现有测试：0。`app_data_dir()` 失败、配置文件不存在、配置保存失败等错误路径全部无覆盖。
   - 风险：中（但这是 Tauri 平台 API 的典型模式，底层 `AppConfig` 自身有测试覆盖）。

5. **`confirm_window_close`（`system.rs:320-323`）—— 窗口不存在时静默成功**
   - `if let Some(window) = app_handle.get_webview_window("main")` —— 窗口不存在时直接返回 `Ok(())`，前端收到成功但实际未关闭。
   - 风险：低（窗口不存在通常意味着应用已关闭）。

### 5.3 `server.rs` — 🔴 202 行零测试，3 处静默错误

13 个命令，零测试。涵盖服务器生命周期、链路加密配置、网络配置。

**高风险区域**：

1. **`server_start`（`server.rs:16-33`）—— 两处 `let _ =` 静默错误**
   ```rust
   if let Some(parent) = port_file.parent() {
       let _ = tokio::fs::create_dir_all(parent).await;  // server.rs:28
   }
   let _ = tokio::fs::write(&port_file, port.to_string()).await;  // server.rs:30
   ```
   - 端口文件写入失败时静默忽略，前端无感知。外部工具（如移动端扫描端口）依赖此文件发现服务端口。
   - 违反 §8 日志红线「重要路径禁止 `let _ =` 静默忽略错误」。
   - 现有测试：0。

2. **`update_server_port` / `update_server_network_config` / `reset_server_network_config`（`server.rs:110-200`）—— 非事务性更新**
   - 三个函数都先 `config.save(&config_path)?` 再 `supervisor.update_port(port).await?`。若 `config.save` 成功但 `supervisor.update_port` 失败，配置文件已更新但运行时端口未更新——**状态不一致**。
   - 现有测试：0。
   - 风险：中（需要 supervisor 失败才触发，但一旦发生用户看到的端口与实际端口不同）。

3. **`set_traffic_encryption_config`（`server.rs:67-82`）—— 持久化与运行时更新分离**
   - 先 `persist_config_to_db` 再 `update_config` + `sync_registration`。若持久化成功但 `update_config` panic，DB 与运行时不一致。
   - 现有测试：0。

### 5.4 `terminal_stream.rs` — 🔴 159 行零测试，复杂异步链路

3 个命令，零测试。这是最复杂的文件：Channel 传输、forward_loop、consumer 任务链、client_id 计数器。

**高风险区域**：

1. **`subscribe_terminal_channel`（`terminal_stream.rs:85-143`）—— 2 个 spawn 任务链无回归锁**
   - `forward_loop` → `out_tx` → consumer → `channel.send` 的完整链路。
   - 历史入队、实时帧推送、HistoryEnd 标记、channel 关闭时 abort forward——每个分支都无测试。
   - 现有测试：0。`terminal_ws.rs`（WS 路径）有集成测试覆盖，但 Channel 路径是独立实现（注释明写「WS 环回已下线」），**无测试守着 Channel 路径的正确性**。
   - 风险：高（终端输出流断/丢消息是最致命回归，参考 pty-spec.md 的变异测试结论）。

2. **`CHANNEL_CLIENT_COUNTER`（`terminal_stream.rs:75-76`）—— client_id 分配逻辑无测试**
   - 每次订阅分配唯一 `channel-{session}-{n}` ID，前端据此精确 unsubscribe。
   - 现有测试：0。若将来有人把 `fetch_add(1)` 改成 `fetch_add(0)`（不递增），所有订阅的 client_id 相同，unsubscribe 会误删其他订阅——这正是注释中记录的历史 bug。
   - 风险：中（有注释记录教训，但无测试守着）。

3. **`terminal_channel_ack`（`terminal_stream.rs:154-158`）—— 背压反馈无测试**
   - 调用 `GlobalOutputManager::ack` 推进未 ack 记账。
   - 现有测试：0。ack 错误路径（会话不存在、offset 回退）无覆盖。
   - 风险：中（ack 失败会导致 PTY 读取暂停，终端无输出）。

### 5.5 `opener.rs` — 🔴 211 行零测试，Windows COM 复杂逻辑

2 个命令，零测试。Windows COM Shell API + macOS Finder + Linux xdg-open 三平台分发。

**高风险区域**：

1. **`plugin_reveal_in_dir`（`opener.rs:53-70`）—— 路径前缀剥离脆弱**
   - `path.strip_prefix(r"\\?\").unwrap_or(&path)` —— 仅剥离 `\\?\` 前缀。若路径以 `\\?\` 开头但后面是混合分隔符（如 `\\?\D:\下载/file.mkv`），剥离后仍可能有混合分隔符。
   - 现有测试：0。
   - 风险：低（有 `path.exists()` 兜底校验，失败时返回 NotFound）。

2. **`reveal_in_dir_platform`（Windows 分支，`opener.rs:75-150`）—— COM 调用无测试**
   - `CoInitialize`（幂等）、`ILCreateFromPathW`（PIDL 构造）、`SHOpenFolderAndSelectItems`（Shell 定位）、`ERROR_FILE_NOT_FOUND` 回退到 `shell_execute_explore`。
   - 现有测试：0。Linux CI 上此分支不编译（`#[cfg(target_os = "windows")]`），Windows CI 无覆盖。
   - 风险：低（平台限定，且 `path.exists()` 前置校验降低了风险）。

3. **`open_log_dir`（`opener.rs:76-89`）—— 日志目录不存在时返回 NotFound**
   - 现有测试：0。
   - 风险：低（纯 IO + 平台分发，与 `reveal_in_dir_platform` 共享代码）。

4. **`let _ = CoInitialize(std::ptr::null());`（`opener.rs:107`）—— 静默错误**
   - COM 初始化失败时静默忽略，后续 Shell API 调用可能失败。
   - 违反 §8 日志红线。
   - 现有测试：0。

### 5.6 `qr.rs` — 🔴 95 行零测试，TTL 校验缺失

5 个命令，零测试。QR 码生成、TTL 管理。

**高风险区域**：

1. **`set_qr_token_ttl`（`qr.rs:88-94`）—— TTL 无边界校验**
   - 直接 `db.set_setting("qr_token_ttl", &ttl.to_string())`，无 `ttl > 0` 或 `ttl <= MAX` 校验。
   - 若 `ttl = 0`，QR 码立即过期（`generate_qr_code` 用 `qr_manager.generate(ttl)` 生成 0 秒 TTL 的 token）。
   - 若 `ttl = u64::MAX`（18.4 亿秒 ≈ 58 年），QR 码永久有效。
   - 现有测试：0。
   - 风险：中（前端传入 0 会导致 QR 码配对完全失败）。

2. **`generate_qr_code`（`qr.rs:28-44`）—— TTL 读取链有隐藏回退**
   - `db.get_setting("qr_token_ttl").ok().flatten().and_then(|v| v.parse::<u64>().ok()).unwrap_or(300)`
   - 4 层 fallback：DB 查询失败 → None → 解析失败 → 默认 300。
   - 所有错误路径静默回退 300，调用方无法区分「DB 无设置」和「DB 设置损坏」。
   - 现有测试：0。
   - 风险：低（默认值合理），但隐藏错误路径。

3. **`get_qr_connection_info`（`qr.rs:49-82`）—— host 选择逻辑无测试**
   - 优先用前端传入的 host，否则从 `get_local_ip_addresses()` 选择非回环非链路本地地址，否则用 `LOCALHOST_IP`。
   - 现有测试：0。依赖 `get_local_ip_addresses()` 的过滤逻辑（见 §5.2 #3）。
   - 风险：中（QR 码显示回环地址会导致移动端无法连接）。

### 5.7 `session.rs` — 🔴 126 行零测试，输入校验无测试

9 个命令，零测试。会话生命周期（start/create/kill/delete/restart/resize）。

**高风险区域**：

1. **`start_session`（`session.rs:14-38`）—— cols/rows 校验无测试**
   - `match (cols, rows) { (Some(c), Some(r)) if c > 0 && r > 0 => Some((c, r)), _ => None }`
   - 校验 cols/rows > 0，否则传 `None`（让 PTY 使用默认尺寸）。
   - 现有测试：0。若将来有人把 `c > 0 && r > 0` 改成 `c > 0 || r > 0`（任一为正即有效），无效尺寸会被传给 PTY。
   - 风险：低（5 行 match，逻辑简单）。

2. **`start_existing_session`（`session.rs:58-78`）—— 同样的 cols/rows 校验，无测试**
   - 与 `start_session` 重复的校验逻辑。
   - 风险：同 #1。

3. **`resize_session`（`session.rs:118-126`）—— force 默认值无测试**
   - `force.unwrap_or(false)` —— 前端不传 force 时默认 false（需要确认才能 resize）。
   - 现有测试：0。
   - 风险：低。

### 5.8 `session_config.rs` — 🔴 77 行零测试，序列化契约无测试

5 个命令，零测试。会话配置 CRUD。

**关注点**：
- `create_session_config` 和 `update_session_config` 使用 `#[tauri::command(rename_all = "snake_case")]` —— Tauri 命令参数名从 camelCase 转为 snake_case。此契约无测试守着。
- 纯委托层（直接调用 `SessionConfigManager` 的方法），底层管理器有测试覆盖。
- 风险：低（纯委托，但序列化契约变更会导致前端调用失败）。

### 5.9 `mdns.rs` — 🔴 57 行零测试，输入无校验

3 个命令，零测试。mDNS 服务广播控制。

**关注点**：
- `mdns_start_advertise` 的 `device_name: String` 和 `port: u16` 均无校验。空字符串设备名或 0 端口可能产生无效 mDNS 记录。
- `txt_records` 构造逻辑（3 个 key-value）无测试。
- 风险：低（mDNS 是尽力而为的服务发现，无效记录不影响核心功能）。

### 5.10 `devices.rs` — 🔴 19 行零测试，硬编码假数据

1 个命令，零测试。

```rust
session_count: 0,  // devices.rs:13
```
- `session_count` 永远返回 0，不反映实际会话数。前端若依赖此字段显示「该设备有 N 个会话」，会永远显示 0。
- 现有测试：0。
- 风险：低（前端可能未使用此字段，或已知道它是假的）。

### 5.11 `pty_input.rs` — 🔴 24 行零测试，输入无大小限制

2 个命令，零测试。PTY 输入写入。

**关注点**：
- `write_to_session` 的 `data: String` 无大小限制。前端可发送任意大字符串（如 100MB），全部写入 PTY。
- 纯委托层（直接调用 `SessionManager::write_input`），底层 `PtySession::write()` 有 4000 字节分块（见 pty-spec.md §9），但命令层无预检。
- 风险：低（底层有分块保护，且这是桌面本地 IPC 而非网络路径）。

### 5.12 `settings.rs` — 🔴 23 行零测试

2 个命令，零测试。纯委托层（`db.get_all_settings()` / `db.set_setting()`），底层 `Database` 有测试覆盖。风险极低。

### 5.13 `wsl.rs` — 🔴 27 行零测试

2 个命令，零测试。纯委托层（`spawn_blocking(crate::pty::list_distributions)` / `spawn_blocking(crate::pty::is_wsl_available)`）。

**关注点**：
- `list_wsl_distributions` 的错误映射：`map_err(|e| AppError::Internal(e.to_string()))` —— 将 `JoinError` 字符串直接作为错误信息。若 join 失败（线程 panic），错误信息可能不含 panic 原因。
- `is_wsl_available` 的 `unwrap_or(false)` —— join 失败时静默返回 false。
- 底层 `pty::list_distributions` 有测试（见 pty-spec.md），但命令层的错误路径无测试。
- 风险：低。

### 5.14 `plugin.rs` — ⚪ 不适用

5 行，纯 `pub use crate::plugin::api_bridge::*;` 重导出。无逻辑可测。

---

## 6. 探针实测抓到的真实风险（现有测试全部漏过）

### 风险 A — `set_qr_token_ttl` 无 TTL 边界校验（功能）

`qr.rs:88-94`：
```rust
pub async fn set_qr_token_ttl(db: State<'_, Arc<Mutex<Database>>>, ttl: u64) -> Result<()> {
    let db = db.lock().await;
    db.set_setting("qr_token_ttl", &ttl.to_string())
        .map_err(|e| crate::AppError::Config(e.to_string()))
}
```

无 `ttl > 0` 校验。`ttl = 0` 时：
- `generate_qr_code` 用 `ttl = 0` 生成 QR token → `qr_manager.generate(0)` → token 立即过期。
- 移动端扫码后连接立即失败（token 已过期）。

**现有测试漏过原因**：`qr.rs` 零测试。`set_qr_token_ttl` 是纯 DB 写入，无测试构造此路径。

### 风险 B — `set_terminal_bg_image` 无路径遍历校验（安全边界）

`system.rs:233-238`：
```rust
let src = std::path::Path::new(&source);
let ext = src.extension()...;
if !TERMINAL_BG_EXTENSIONS.contains(&ext.as_str()) {
    return Err(...);
}
// 后续直接 std::fs::copy(src, &dest)
```

仅校验扩展名和文件大小，**未校验 `source_path` 是否为绝对路径**。相对路径（如 `../../somefile.png`）若存在且扩展名匹配，会被复制到应用数据目录。

**影响**：非远程代码执行（仅复制文件），但违反 §8「输入校验在 Rust 端」红线。前端通过 `tauri-plugin-dialog` 文件选择器选取时通常给出绝对路径，但 Rust 端不应依赖前端行为。

**现有测试漏过原因**：`system.rs` 零测试。

---

## 7. 顺带发现（非测试问题，但与「能否测出 bug」直接相关）

| # | 位置 | 问题 | 关联规范 |
|---|---|---|---|
| O1 | `server.rs:28` `let _ = tokio::fs::create_dir_all(parent).await;` | 端口文件父目录创建失败静默忽略 | §8 禁止 `let _ =` 静默忽略错误 |
| O2 | `server.rs:30` `let _ = tokio::fs::write(&port_file, port.to_string()).await;` | 端口文件写入失败静默忽略，外部工具无法发现服务端口 | §8 同上 |
| O3 | `opener.rs:107` `let _ = CoInitialize(std::ptr::null());` | COM 初始化失败静默忽略，后续 Shell API 可能失败 | §8 同上 |
| O4 | `qr.rs:35` `.ok().flatten().and_then(...).unwrap_or(300)` | 4 层 fallback 隐藏错误，调用方无法区分「无设置」和「设置损坏」 | §6 错误处理 |
| O5 | `devices.rs:13` `session_count: 0` | 硬编码假数据，前端可能误用 | §5 高内聚低耦合（命令层不应携带产品语义） |

---

## 8. 修复优先级

| 优先级 | 票据 | 内容 | 理由 |
|---|---|---|---|
| P0 | `issues/17` | `qr::set_qr_token_ttl` 补 TTL > 0 校验 + 单测 | 安全边界：0 TTL 导致 QR 配对完全失败 |
| P1 | `issues/18` | `system::set_terminal_bg_image` 补绝对路径校验 + 单测 | 安全边界：违反「输入校验在 Rust 端」红线 |
| P1 | `issues/19` | `terminal_stream` 补 client_id 分配逻辑 + ack 错误路径单测 | 历史 bug 回归锁缺失（通道订阅误删） |

---

## 9. 观察项（暂不成票，需要设计决策）

- **commands 层的测试策略**：当前模式是「底层模块有测试，命令层是纯委托」。若底层模块测试完善，命令层测试的价值在于：(a) 错误映射正确性；(b) 输入校验边界；(c) 非事务性更新的原子性。建议明确：commands 层只测「校验 + 错误映射」，不重复测底层逻辑。
- **`devices.rs::session_count` 假数据**：要么从命令中删除此字段（前端改用其他方式获取会话数），要么接上真实数据源。当前状态是「给了字段但永远是 0」，比不给出更危险。
- **`pty_input::write_to_session` 输入大小限制**：桌面本地 IPC 路径，底层有 4000 字节分块，命令层无需预检。但若将来扩展为远程调用（如插件通过 host API 写 PTY），需要加大小限制。

---

## 10. 复现命令

```bash
cd bedcode-desktop/src-tauri

# 基线
cargo test --lib commands::
# → 4 passed; 0 failed; 610 filtered out; finished in 0.00s

# 单测过滤
cargo test --lib commands::dev_logs

# 复现风险 A（TTL = 0）：临时探针或直接在测试里断言
#   set_qr_token_ttl(db, 0).await → Ok(())  当前无校验，应返回 Err

# 复现风险 B（路径遍历）：临时探针验证相对路径可通过校验
#   source_path = "../../test.png" → 通过扩展名校验（只要文件存在）

# 复现静默错误（server.rs:28,30）：
#   在测试环境中将端口文件路径设为不可写目录 → 写入失败被 `let _ =` 忽略

# 复现变异测试守卫力（审查时已回滚，勿留存）
#   src/commands/dev_logs.rs:36-46  删除 is_char_boundary 循环 → 直接 &message[..MAX_MESSAGE_LEN]
#   → truncate_message_does_not_split_multibyte_char panic 失败
#   → 其余 3 测试 + 其余 13 文件全绿
```

---

## 11. 审计纪律记录

- 变异测试与探针全部回滚/删除：`git status src/commands/` 干净，`grep -rn "MUTATION-PROBE"` 无残留，回滚后重新跑通 4 确认无残留。
- 未提交任何代码改动；本文档与票据均为新增文件。
- 审计结束已检查无测试残留进程（本次 `cargo test --lib commands::` 在 shell 内执行完毕，无后台进程；系统中存在其他任务的 `cargo test` 进程，非本次产生）。
- **自查发现一处遗漏并修正**：初版统计 tauri_cmds 数时 grep 模式 `^pub (async )?fn |^#\[tauri::command` 会重复计数（`#[tauri::command]` 和 `pub fn` 各计一次），已改为仅统计 `^#\[tauri::command` 属性行。最终命令数：14 文件共 133 个 `#[tauri::command]`（含 `plugin.rs` 重导出的不计）。
- **范围纪律**：本次审计严格限定 `src/commands/`，未审计其他模块（pty/server/db/session 等底层模块的测试由各自 spec 覆盖）。
