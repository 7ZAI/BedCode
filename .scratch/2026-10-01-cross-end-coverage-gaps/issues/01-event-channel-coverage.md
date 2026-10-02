# 票 01 — 事件通道 session-control 跨端覆盖（最高优先）

**状态**：resolved · 2026-10-01
**类型**：task
**依赖**：spec.md §5.1（前置工程 cross-end-tests 已交付）

## 落点

新增 `cross-end-tests/tests/event_channel_flow.rs`（独立测试二进制，进程隔离）+
两处共享装配增量：

- `tests/common/mobile_ctx.rs`：`MobileEventRecorder`（订阅 `ConnectionManager` 的
  `MobileEvent` 广播并落盘）+ `assert_no_mobile_event`（带预算的「一条都不许有」反例）。
- `tests/common/desktop_ctx.rs`：插件 WS 端点的**只读**观测
  （`plugin_ws_endpoint_clients` / `wait_plugin_ws_endpoint_authenticated` /
  `wait_new_plugin_ws_endpoint_client` / `assert_no_new_plugin_ws_client` /
  `disconnect_plugin_ws_endpoint_clients`），依赖 `bedcode-server-websocket`（只读传输面）。

## 契约落表（实际实现）

| 契约 | 行为 | 场景 |
|---|---|---|
| E-001a | 移动端 HTTP 建会话 → `session:created` → `session.id` / `status` / `config_id` / `source_device`（= JWT claims 的 deviceName）逐字对齐 | 正例 |
| E-001b | 停会话 → `session:stopped`，id + `session_name`（退出前快照） | 正例 |
| E-001c | 删会话 → `session:removed` | 正例 |
| E-002a | 桌面命令面 `set-auto-mode` → `session:mode-changed`（`auto_approve`） | 正例 |
| E-002b | `queue-add` → `task:queue-changed`（`action=="add"` / `queue_count==1`，数字段不是字符串） | 正例 |
| E-002c | 开 `auto_execute` → 调度下发 → `task:status-changed`（`in_progress` + reason 非空） | 正例 |
| E-002d | `scheduled-create` → `task:scheduled-changed`，`job_id` 与命令回执逐字一致 | 正例 |
| E-003a | 桌面 1001 断开（非致命）→ 监督任务自愈，**新 client_id** 且已认证 → 新事件重新可达 | 正例 |
| E-003b | 4003 致命关闭 → **不自愈**（无重连风暴）→ 断链期事件丢失 → 重新认证后重建 → **不重放** → HTTP 对账补齐 → 新事件可达 | 正例 + 反例 |
| E-004 | 伪造 JWT 建通道 → 桌面 close 且**认证类致命**；期间桌面真实广播一帧都不落地 | 反例 |

## 与票面清单的偏差（实查为准）

1. **E-003 拆成 a/b**：原票写「断连期间事件丢失 → 重连后对账」。实测监督任务的
   自愈是**毫秒级**（HTTP reauth → 重建全程 <10ms），任何轮询间隔都观察不到
   「空窗」，普通断链造不出可判定的 outage 窗口。改用**认证类致命关闭 4003**
   （M1/ADR 0031 明文规定不自愈）造窗口，普通断链的自愈另立 E-003a 用
   「client_id 变了 + 已认证」判（可靠且正是要的性质）。
2. **E-004 重写**：原票的「无头以事件替身承接 `ws_event_channel_ready`」**不成立**
   —— `app_handle=None` 时该事件根本不发射（`manager.rs:351`）。改为断言其
   **语义等价物**：首帧认证被桌面端接受（注册表 `authenticated`）后事件即刻可达，
   并补 fail-closed 反例（伪造 JWT → 4001/4003 + 零事件落地）。
3. **驱动侧口径**：任务/模式/定时域走**桌面命令面**（`plugin_command` = 桌面 UI 那条路）
   而非 HTTP——这些端点移动端**无 Rust 客户端**（只经前端 HTTP 代理，见票 02），
   用 reqwest 造请求只会引入第三套手写客户端，对断言面无增益。

## 就绪判据（为什么不是 sleep / 不是 `ws_event_channel_ready`）

三条时序：移动端 `ws_event_channel_ready`（首帧**发出后**即发射）→ 插件零客户端
早退（看 `clientCount`）→ 宿主注册表 `authenticated`（首帧**被接受后**）。
只有第三条是「这一帧真的会被广播」的终点，故就绪等待取它。

## 验证

- `cd cross-end-tests && cargo test --test event_channel_flow` 绿（6.4s，**连跑 4 次稳定**）
- `cross-end-tests` 全量 **9 个二进制全绿**（既有 8 个无回归）
- 变异自检 3 项：
  - M1「移动端读不到 `source_device` wire 键（改名）」→ E-001a 精确打红；
  - M2「监督任务对致命关闭也自愈」→ E-003b「断链期间事件不得落地」打红；
  - M3「反例助手恒真自检」（健康通道 + 不存在的旧 id）→ 报出「不该出现的重连真的发生了」。
- `cargo clippy --tests`：本包 0 诊断
- 变异全部逐字回滚（`git diff` 两端生产代码为空）
- 收尾 `pgrep` / `ss -tlnp`：无残留进程与端口；临时目录零残留（新增 `TempDirGuard`
  让 panic 路径也清目录——实测失败一轮会留一对 `/tmp/bedcode-crossend-*`）

## 遗留（不阻塞）

- `sync_config_*` 三类 `MobileEvent` 变体在事件通道上**无帧源**（插件不广播配置事件，
  只经 bus/emit），本文件如实不测；要覆盖需插件侧新增广播，属产品面改动。