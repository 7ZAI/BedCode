# 05: `commands/` 面收敛——按「域归属插件是否使用该能力」逐命令判定

**What to build:** 收敛宿主 Tauri 命令面（`src-tauri/src/commands/`，当前 90 条注册命令）：
**只保留宿主页面（外壳 / 引擎 UI）直接调用的命令**，其余随其业务域下沉到对应插件。

**判据（本票口径，务必按此判定）:**

> **主判据 = 该命令承载的能力，其业务域的归属插件是否在用。**
> 插件在用（无论经插件自身命令面、SDK 原语，还是插件内部 dispatch）→ 该能力的产品面归插件：
> 宿主命令面应**注销**；宿主侧若仍需执行能力，则收敛为**原语**（插件经 SDK 调用）。
> 插件不用 → 再看宿主页面是否用：都不用则**直接删**；宿主页面用则**保留**。
>
> **辅判据 = 宿主页面是否直接调用**——仅用于「终端渲染管道留宿主」这类红线场景
> （输入 / 尺寸 / 输出订阅：即使插件业务也在用，宿主终端窗口仍需直调通道）。
>
> **明确排除的判据**：不以「当前宿主前端还剩谁 invoke」为主判据。历史孤儿 plumbing
> （`commands/sessionCommands.ts` / `deviceCommands.ts` 一族）会把它判歪——这些文件
> 自身就是要删的对象，不能拿来当「有消费者」的证据。

**决策依据:** ADR 0022 裁剪线（宿主只留引擎原语 + 移动端 wire + 平台能力；产品面归插件）；
AGENTS §5/§6（宿主业务清零、前端只留插件运行时与通用外壳）；
`.scratch/2026-09-20-host-business-decarriage/spec.md`（已完成批次）。

**Blocked by:** 无（可与票 01/03/04 并行；票 03 的 WSL / local-ip 与本票清单有交集，见下）

**Status:** 主体 done（2026-09-21）；遗留 = ④ `plugin_reveal_in_dir`（票 04）+ ⑤ auth 降级轨死代码裁决（见下 ④/⑤）

## 方法（三步，逐命令走完才算判定）

1. **建域映射**：命令 → 业务域 → 域归属插件（或引擎）。域清单见下表。
2. **查证插件是否在用该能力**（证据三选一，须落到文件/命令名）：
   - 插件命令面：`plugins/*/plugin.json` 的 `contributes.commands`
   - 插件内部 dispatch：`plugins/*/rust/src/lib.rs` 的 command 分发表
   - SDK 原语调用：`plugins/*/rust/src/**` 里对 `WasmHost.<primitive>(...)` 的调用
3. **裁决处置**：
   - ①插件在用 + 宿主页面不直调 → **注销宿主命令面**（插件面/原语已存在）
   - ②插件在用 + 宿主页面直调（终端红线）→ **保留**（并在命令注释里写明保留理由）
   - ③插件不用 + 宿主页面不用 → **直接删**
   - ④插件要用但缺原语 → **先补原语再注销**（例：`reveal-in-dir`，见票 04）

## 域归属与当前判定（证据）

各插件命令面证据（`plugin.json contributes.commands` 实读）：`com.bedcode.session` 28 条
（任务域为主 + `session.devices.*` / `session.config.*` / `session.action.*` / `session.pairing.*` /
`session.environment.*` / `session.network.*` 内部 dispatch）、`com.bedcode.file-transfer` 33 条、
`com.bedcode.agent-hub` 34 条、`com.bedcode.ai-chatbox` 8 条。

| 域（宿主文件） | 域归属 | 插件业务是否在用（证据） | 处置 |
| --- | --- | --- | --- |
| 会话生命周期 `session.rs`（11） | 插件 `com.bedcode.session` | **在用**：插件 dispatch `session.action.{create,close,remove,rename,resize}` + `session.create` 系列，走 `host-session` 原语 | 拆分（见下①） |
| 会话配置 `session_config.rs`（5） | 同上 | **在用**：插件 `config/ops.rs` 自持真源 + `session.config.*` 命令面 | 注销（宿主面已无独立职责，票 02 阶段 B 一并） |
| 终端 I/O `terminal_stream.rs`（3）/ `pty_input.rs`（2） | 引擎（终端渲染管道留宿主） | 插件也用（`host-terminal` 原语），但宿主终端窗口必须直调 | **保留**（辅助判据②红线） |
| 设备/配对/QR/连接历史 `devices.rs` `qr.rs` + `system.rs` 内配对一族 | 插件 `com.bedcode.session` | **在用**：`session.pairing.*` / `session.qr.*` / `session.devices.*` / `session.trust.*` / `session.history.*` | **注销** |
| 快捷指令 | 域已下沉 | 命令**已退役**（宿主无实现）；前端死 invoke 已删（本批） | 已收敛 |
| WSL `wsl.rs`（2）/ 本地 IP `system.rs::get_local_ip_addresses` | `host-platform` 原语 | **在用**：插件 `session.environment.wsl-distros` / `session.network.info` | 注销（与**票 03** 同一件事） |
| mDNS `mdns.rs`（3） | mDNS 引擎（`host-mdns`） | **插件的 mdns 业务走 `host-mdns` 原语**（`component.rs` 转发），不用宿主命令面 | **注销命令面**（原语保留） |
| HTTP/WS 服务 `server.rs`（13） | 引擎 | 插件不用（插件自带 `_http_endpoint`） | **保留**（宿主 ServerView 直调） |
| 插件运行时 `plugin.rs`（27） | 插件系统外壳 | — | **保留** |
| 设置/系统外壳 `settings.rs`（2）+ `system.rs` 其余 | 宿主外壳 | 插件不用 | **保留** |
| 诊断 `dev_logs.rs`（1） | 宿主 | — | **保留** |
| 平台 `opener.rs`（2） | 混合 | `plugin_reveal_in_dir` 属插件能力 | `open_log_dir` **保留**；`plugin_reveal_in_dir` 见**票 04** |

### ① 会话域的拆分口径（本票最容易判错的一处）

会话域命令**不能整组注销**，因为宿主终端窗口（`views/TerminalWindowView.vue`）是引擎 UI：
它需要「读会话元信息 / 写输入 / 调尺寸 / 订阅输出」。拆分：

- **保留（宿主终端 + 引擎）**：`list_sessions`、`get_session`、`resize_session`、
  `write_to_session`、`send_special_key`、`terminal_stream::{subscribe,ack,unsubscribe}`。
  理由：终端渲染管道留宿主（性能红线），这些是它的执行通道。
- **注销（插件域）**：`start_session`、`create_session_no_start`、`start_existing_session`、
  `kill_session`、`delete_session`、`restart_session` + 配置 CRUD 5 条。
  前置 = 迁移它们在前端的调用方（会话兜底壳 `stores/session.ts` 的编排与
  `commands/sessionCommands.ts` 封装）——见「已完成 / 待做」。

### ② 已完成（本批，前端 TS 侧一并做了）

- 删除宿主前端「业务 plumbing」整族（命令封装 + 孤儿 composable/store + 其测试）：
  `commands/deviceCommands.ts`、`commands/eventListeners.ts`、`usePairing`、`useQrCode`、
  `useNetwork`、`useConnectedDevices`、`useWsl`、`useRunTime`、`useSessionStatusListener`、
  `stores/{device,quickAction,inputAssistant}` 及其 `__tests__`。
- 删除 4 处指向**已退役命令**的死 invoke（`create/list/update/delete_quick_action`）。
- `useDesktopCommands.ts` 收敛为纯 re-export（删 `useDesktopCommands()` composable：0 调用方）；
  删 `composables/useTauri.ts`（历史兼容层，唯一消费方改从 `@/composables/model` 取类型）；
  删 `sessionCommands.ts::startSession`（无调用方）。

### ③ 待做（Rust 注销 + 前端调用方迁移）

- [x] 注销清单（**必须逐条走完方法三步再删**）：
  - 配对 / QR / 连接历史 / 已配对设备：`generate_pairing_code`、`verify_pairing_code`、
    `get_current_pairing_code`、`clear_pairing_code`、`get_pairing_code_ttl`、
    `set_pairing_code_ttl`、`generate_qr_code`、`clear_qr_code`、`get_qr_connection_info`、
    `get_qr_token_ttl`、`set_qr_token_ttl`、`list_paired_devices`、`remove_paired_device`、
    `list_connection_history`、`delete_connection_history`
  - mDNS：`mdns_start_advertise`、`mdns_stop_advertise`、`mdns_is_advertising`
  - 系统：`get_system_info`（引擎事实，无插件业务用、宿主页面也不用）
  - 会话域（插件域那半）：`start_session`、`create_session_no_start`、
    `start_existing_session`、`kill_session`、`delete_session`、`restart_session` +
    会话配置 CRUD 5 条
  - WSL / 本地 IP：随**票 03**（已同批做，`system.rs` 只动了一次）
- [x] 前端调用方迁移（注销的前置）：`stores/session.ts` 的会话编排与
  `TerminalWindowView.vue` 的会话读取，改走插件命令面（宿主侧通道 = `pluginInvoke`，
  即 `context.commands.execute` 的宿主等价物）；迁移后 `commands/sessionCommands.ts` 已收紧
- [x] `get_connected_devices` **保留**：它是引擎事实面（连接注册表原始记录），
  宿主通知种子化在用（`useGlobalNotifications`），且已去掉派生视图（批次一）
- [x] `system.rs` 的配对常量 / `system/constants/auth.rs`（配对码位数与 TTL）：**保留**——
  `PAIRING_CODE_TTL_SECS` 仍是宿主 D7 降级轨（`utils/auth/pairing.rs`）与插件默认值对齐的锚点；
  见下方 ⑤ 待裁决

## 验收

- [x] 每条注销命令都有「插件在哪用 / 宿主页面不用」的两条证据（见下方 ④ 注销清单证据表）
- [x] 桌面 `cargo test --lib` 全绿（1088/0）；前端 `pnpm run test:run`（受影响用例 37 passed）；
      根目录 `pnpm exec eslint .` 见收尾记录
- [x] `src-tauri/src/commands/` 剩余命令都能回答「哪个宿主页面/终端引擎在调」（保留了理由，
      写入 `commands/session.rs` 头部注释与 lib.rs 注册分组注释）
- [x] 插件产物重建 + manifest 逐字一致（`scripts/plugin-build.js --plugin com.bedcode.session`，
      wasm 内含本批 log 变更）；无残留进程

### ④ 实施记录（2026-09-21）

**Rust 注销 30 条**（`lib.rs` 注册项 + 实现体 + 模块声明同步）：

| 域 | 注销项 | 插件侧落点（证据） |
| --- | --- | --- |
| 会话编排 6 | `start_session` / `create_session_no_start` / `start_existing_session` / `kill_session` / `delete_session` / `restart_session` | 插件命令面 `session.create` / `session.close` / `session.action.{remove,restart,rename,resize}`（`plugins/session/rust/src/lib.rs:787-813`），宿主只经 `host-session` 原语执行 |
| 会话配置 5 | `create/list/get/delete/update_session_config` | 插件 `session.config.list/upsert/delete` + 私有库真源（`config/ops.rs`）；命令实现文件 `commands/session_config.rs` 整文件删除（`SessionConfigManager` 保留给插件迁移通道，退役见票 02 B） |
| 配对 / QR / 连接历史 15 | `generate_pairing_code` 一族 + `generate_qr_code` 一族 + `list/remove_paired_device` + `list/delete_connection_history` | 插件 `session.pairing.*` / `session.qr.*` / `session.devices.{paired-list,revoke,history-list,history-clear}` / `session.trust.*`（lib.rs:820-910） |
| mDNS 3 | `mdns_start_advertise` / `mdns_stop_advertise` / `mdns_is_advertising` | 插件走 `host-mdns` 原语（`component.rs` 转发）；宿主自有广播由 `server/supervisor.rs` 启停，不经命令面 |
| 系统 1 | `get_system_info` | 无消费者（宿主页面不用、插件不用）：`SystemInfo` 仍是宿主内部引擎事实（`AppContext::global().system_info()`） |
| WSL / 本地 IP 3 | `list_wsl_distributions` / `is_wsl_available` / `get_local_ip_addresses` | 见票 03（WSL → 插件 `session.environment.wsl-distros`；本地 IP → `session.network.info`；命令实现体搬 `system/info.rs`） |

- 删文件：`commands/{qr.rs, mdns.rs, session_config.rs, wsl.rs}`；`commands.rs` 模块声明同步收紧。
- **二阶段启动退役**（本票最容易漏的语义变化）：注销 `create_session_no_start` /
  `start_existing_session` 后，全仓已无 `start = false` 生产者——插件 `session.create` 与
  定时任务域、移动端 HTTP/WS 启动线一律 `start = true`（`launch.rs:504`、
  `task/scheduled.rs:321`、`session_controller.rs:65`、`session_control.rs:68`）。
  `TerminalPreview.vue` 的 `status === 'starting'` 分支（唯一第二阶段触发点）随之删除；
  内核 `SessionManager::start_existing_session` 与 spec 的 `start: false` 支持**保留**为休眠
  能力（`create-with-spec` 的 JSON 契约不变，删它属另一张票）。

- **前端迁移**（宿主→插件通道 = `pluginInvoke`，`src/plugin/commands.ts`）：
  - `stores/session.ts` 精简为：`loadSessions`（引擎事实）+ `loadSessionConfig`
    （`session.config.list`，命中/未命中返回 null）+ `stopSession`（`session.close` 后刷新列表）
    + `writeToSession` / `sendSpecialKey` / `resizeSession`；删除
    `createSession` / `startSession` / `killSession` / `deleteSession` / `restartSession` /
    `loadConfigs` / `createConfig` / `deleteConfig` / `updateConfig` 与 `activeSession` / `configs` 状态
  - `TerminalWindowView.vue`：`killSession` → `sessionStore.stopSession`（插件 `session.close`）；
    `getSessionConfig` → `sessionStore.loadSessionConfig`（插件 `session.config.list`）
  - `TerminalPreview.vue`：删除第二阶段启动分支与随之失用的 `computeDesktopInitialTerminalSize` 导入
  - 删除孤儿：`src/__tests__/fixtures/pairing.ts`（其类型来源已随配对域退役且已 import 失效）、
    `model.ts` 的 `QrConnectionInfo` / `SessionStatusEvent` / `SessionRestartEvent`
- **测试**：`src/__tests__/stores/session.test.ts` 按行为契约重写（4 契约 × 正例/反例/边界/异常，
  13 用例，含「插件停止失败 → 上抛且不刷新列表」反例）；`terminal-flow.test.ts` 首个场景改为
  「列表刷新 + 插件命令面停止」；`fixtures/drift.test.ts` 摘除 pairing 登记项、机制自检改用
  `SessionInfo`。

### ⑤ auth 降级轨退役（2026-09-21，用户裁决「立即删除」→ done）

注销配对 / QR 命令面后，`utils/auth/auth_center.rs` 的配对 / QR 桥接**已无任何生产调用方**
（其唯一入口就是被删的宿主命令面）。用户裁决：**立即删除**而不是留成「保留但不可达」。
删除清单（连带装配链与用例）：

| 项 | 落点 |
| --- | --- |
| 桥接函数 10 个 | `auth_center.rs` 删 `generate_pairing_code` / `current_pairing_code` / `verify_pairing_code` / `clear_pairing_code` / `generate_qr_code` / `qr_conn_info` / `verify_qr_token` / `QrVerifyOutcome` / `qr_reject_reason` / `clear_qr_code` / `qr_failure_user_message` + 相关 imports（**保留** `call_api` / `session_active` / `enforce_connection_policy` / `format_device_display_name`） |
| 宿主实现 | 删 `server/services/pairing_service.rs`（`PairingService`）、`utils/auth/qr_token.rs`（`QrTokenManager`）、`utils/auth/pairing.rs`（`PairingCode` / `PendingDevice` 与宿主码生成）；`server/services.rs` / `utils/auth.rs` 模块声明同步 |
| 装配链 | `AppContext`（字段 / accessor / builder / `expect`）+ `lib.rs`（构造、builder 链、`app.manage`）全删 |
| 常量 | `system/constants/auth.rs`：删 `PAIRING_CODE_DIGITS` / `QR_TOKEN_BYTES`；**保留** `PAIRING_CODE_TTL_SECS`（`host-auth` 记录面 `pairing_code_ttl` 设置缺省，与插件侧同值锚点） |
| 用例 | 删 `wasm_runtime.rs` 的 `PairingMatrixOutcome` + `run_pairing_matrix` + `test_host_pairing_bridge_closed_loop` + `test_pairing_dual_track_host_and_plugin_paths_agree`（449 行）；`auth_center.rs` 尾部 4 个桥接单测一并删 |
| 文档 | ADR 0022 D7 补偿条目改写（该回退已整体退役，认证只剩 `host-auth` 记录面 + `auth-policy` capability）；AGENTS §8 分层口径同步；CHANGELOG Security 段同步 |

**保留边界**：`auth-policy` capability 的「传输失败回退放行」**不动**——那是防认证中心故障误杀
全部连接的设计降级（与配对降级轨是两件事）。

- `plugin_reveal_in_dir` → 票 04（ABI v22 追加 `host-platform.reveal-in-dir`）**已完成**。

## Comments

### ① 为什么不能按「宿主调用链」判

批内实测：若按「前端还剩谁 invoke」判，配对 / QR / 会话配置一族会被判成「有消费者」而保留——
实际消费者是 `commands/deviceCommands.ts` / `sessionCommands.ts` 这些**业务 plumbing 残留**
（DOM 侧已无页面渲染它们）。判据必须以「域归属插件是否在用该能力」为准，调用链只用于确认
「宿主页面是否仍需直调」（终端红线）。

### ② `get_connected_devices` 的边界

它同时满足「宿主页在用」（通知种子化）与「引擎事实」（连接注册表），**不算业务**：
派生视图（真实会话数 + 任务状态合并）已归插件命令面，宿主命令只回事实（批次一已改）。
本票保留它，并保持「宿主命令面只回引擎事实」这条口径。

### ③ 与票 02/03/04 的重叠处理

- 会话配置 CRUD 与票 02（配置表 / legacy 通道退役）同源：本票只负责**命令面**注销，
  票 02 负责**表与状态管理器**退役；实施时同期做，避免两次触碰 `session_config.rs`。
- WSL / 本地 IP 是票 03 的全部内容，**合并到票 03 实施**。
- `plugin_reveal_in_dir` 是票 04 的全部内容，本票只登记结论。
