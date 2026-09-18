# 02 — 通用连接骨架抽取（阶段 A 门禁）

**What to build:** 把「连接生命周期」从终端业务里剥出来：一个连接级状态机（握手 → 认证策略 → 心跳 → 帧泵 → 优雅关闭）+ 一个通道处理器 trait（收帧 / 关闭回调 / 认证通过回调 / 认证策略声明），终端通道与事件通道改为该 trait 的两个实现。抽完之后，往宿主 WS 服务器上挂任何新通道都只是「加一个实现」，不再动骨架。

认证策略是骨架参数而非硬编码（`Required` / `None`），这是后续插件端点 `auth: "none"` 的地基，避免阶段 B 再回头改骨架。

**Blocked by:** 01

**Status:** done（2026-09-18；真机门禁待用户补做，见 Comments）

- [x] 骨架 + 通道处理器 trait 落地，终端与事件两实现就位；认证策略可声明（必需 / 无需）
- [x] 既有注册表 / 终端 WS 测试**原样通过**（未改动断言的通过 = 行为等价基线）；`cargo test` 全绿
- [x] 类型改名/泛化的波及点全部处理干净（路由侧构造点、注册表条目类型与其测试构造 helper 等），无残留旧名
- [x] wire 协议零变更：终端 wire 定义文件未改动，帧大小上限、心跳间隔（5s ping / 45s 超时）、认证超时（10s）、首消息认证回执与错误帧形状全部与现状一致
- [x] 帧级过滤链（inbound / outbound）仍在骨架层对每帧执行，链路加密失败仍以 4003 关闭
- [ ] 阶段 A 门禁留证：桌面开发态启动 + 移动端真机连 `/ws/event`（认证 + 广播收取）与 `/ws/terminal/session/{id}`（订阅 + 输出 + 输入）正常（操作记录或截图）——**本环境无真机/无 GUI，待用户补做**

## Comments

### 2026-09-18 实施记录

**文件结构（`bedcode-desktop/src-tauri/src/server/`）**

| 文件 | 职责 |
| --- | --- |
| `ws/conn.rs`（新增） | 骨架 actor `WsConnBase` + `ChannelHandler` trait + `AuthMode` + `ChannelMessage`；连接生命周期、帧级过滤链、注册表登记/离线判定、`authenticate_jwt` 核心、`SendTextMessage`/`TerminateConnection` 出口 |
| `ws/subscription.rs`（新增） | `SubscriptionState`（任务表 / 流代数 / 传播模式）+ 订阅原语（`subscribe_output` / `unsubscribe_output` / `set_output_mode`）+ 背压 ack + 桥接；两个通道共用（旧 `Message::Terminal(Subscribe)` 兼容面也走这里） |
| `ws/channel/terminal.rs`（新增） | `TerminalChannel`：控制帧协议（auth / subscribe / mode / input）、会话停止监听、ack 入口 |
| `ws/channel/event.rs`（新增） | `EventChannel`：旧 `Message` 协议面（Auth / Terminal / SessionControl） |
| `ws/terminal_ws.rs`（瘦身 1857 → 9 行） | 仅保留 `control_frame` / `forward` / `subscriber` 三个子模块声明 |

**关键设计**

- `ChannelHandler` = `auth_mode` + `on_started` + `on_text`/`on_binary`（＝收帧）+ `on_auth_ok`（＝认证通过回调）+ `on_close`（＝关闭回调，恰好一次）+ `on_channel_msg`（通道私有消息，`Box<dyn Any>` 透传，骨架零解释）；
- 挂新通道 = 新增一个 `ChannelHandler` 实现 + 路由构造点（`WsConnBase::new(addr, kind, bound_session, handler)`），骨架不改——阶段 B 插件端点即此路径；
- `AuthMode::{Required,None}` 落地：`None` 跳过认证窗口（阶段 B 的 `auth:"none"` 地基），且连接建立后立即回调一次 `on_auth_ok`（语义 = 连接可用，注册表 `authenticated` 保持 false）；
- 认证翻转探测：骨架在每次收帧后比较 `authenticated` 前后值，false→true 则回调 `on_auth_ok`（阶段 A 两通道均未实现该钩子 → 行为零变化）。

**验证证据**

- `cd bedcode-desktop/src-tauri && cargo test` → lib 849 passed / 0 failed，全部集成测试 passed；
- 既有测试断言**未修改**，仅随类型改名调整符号路径（`TerminalWs::*` → `WsConnBase::*` / `SubscriptionState::cleanup_subscription_state`），移动后 8 个 WS 测试全在（2 分派契约 + 4 ack + 2 清理）；
- `grep -rn "TerminalWs" src/` → 无残留；`websocket_manager.rs` 注释同步更名；
- `cargo check --lib` 新增文件零告警；`rustfmt --check` 对本次新增文件无差异（仓库整体 fmt 基线本就不干净，未整仓格式化）；
- wire 协议文件（`ws/message.rs`、`terminal_ws/control_frame.rs`）零改动。

**未覆盖项（需用户补做）**：真机门禁（桌面 `pnpm run tauri:dev` + 移动端实连 `/ws/event` 与 `/ws/terminal/session/{id}`）。本环境无移动设备与 GUI，无法执行；风险由「既有测试全绿 + wire 零变更 + 逻辑逐段搬运（无重写）」控制。
