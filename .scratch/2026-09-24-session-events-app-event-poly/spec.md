# 会话枚举/事件下沉插件 + AppEvent 多态统一事件传播

- **日期**：2026-09-24
- **状态**：已裁定范围（用户确认：全量四类 / SDK 为 wire 真源 / trait 方法 + 统一 publish）
- **关联**：ADR 0022（宿主原语边界）、AGENTS §5（无业务内核）、`.scratch/2026-09-23-session-engine-downsink/`（P1-b 真源下沉）

---

## 1. 背景与问题

P1-b 后会话真源已在 `com.bedcode.terminal-session`，宿主事件路径仍是**三段重复转换**：

```
插件 SDK SyncEvent（内部标签 PascalCase，字段平铺）
  → host-api broadcast_sync 反序列化
  → DesktopSyncEvent::from() 穷尽镜像（events/sync_event.rs）
  → SyncEventHandler 按 11 个变体 match，重建 enums::SyncPayload
  → Message::SyncData → WS 广播（adjacently tagged snake_case + data）
```

问题：

1. **宿主仍持会话业务枚举/事件镜像**——`DesktopSyncEvent` 与 SDK `SyncEvent` 字段级双份，`From` 与 Handler match 是会话语义在宿主的残留面；与 ADR 0022「业务编排归插件」冲突。
2. **线协议形状散落**——`SessionSummary` / `SyncPayload` / `SessionControlAction` / `TerminalAction` / `KeyCombo` 定义在宿主 `enums/`，SDK 与移动端各持平行副本，无单一真源。
3. **`AppEvent` 未承担多态发送**——目前是空 marker trait（`Clone + Send + Sync + Debug`），事件发送靠专用 `sync_tx: broadcast::Sender<DesktopSyncEvent>` 与 Handler 内业务 match，插件事件无法经统一 publish 入口进入。

用户裁定：会话相关枚举与事件应进插件/SDK；宿主只提供统一事件传播 API；经 **AppEvent trait 方法 + 统一 publish** 多态发送。

---

## 2. 范围

### 2.1 四类残留（全量）

| 类 | 现状 | 目标 |
| --- | --- | --- |
| ① 事件镜像 | `DesktopSyncEvent` + `From<SyncEvent>` + `SyncEventHandler` 变体业务 match | 删除镜像；Handler 瘦身为信封 + 源设备排除 + WS 广播 |
| ② 同步 wire | `enums/sync.rs::SyncPayload`、`enums/summary.rs::SessionSummary` | 真源进 SDK `bedcode-plugin-api`；宿主 re-export |
| ③ 控制 wire | `enums/control.rs` SessionControl/Terminal + 被嵌入的 `KeyCombo` | 真源进 SDK；宿主 re-export（H2：宿主仍持 `Message` 枚举，但形状定义不宿主业务化） |
| ④ AppEvent | 空 marker | 扩展 trait 方法 + `publish` 统一入口 |

### 2.2 非目标

- `enums/auth.rs`（认证 wire，非会话；另线）
- `enums/pty_status.rs`（引擎枚举 `PtySessionStatus`，ADR 0022 留宿主）
- `enums/plugin.rs`（已是 SDK re-export，保持）
- `protocol/session.rs`（票 08 已归位的会话记录/状态 wire，本票不动语义）
- `events/matcher.rs`（事件基础设施保留，只被 AppEvent 扩展触达）
- 移动端业务逻辑与 UI（只保 wire 形状锁，不改消费语义）
- WIT/ABI 版本 bump（`host-events.broadcast-sync` 签名不变，仍为 `event-json: string`）

---

## 3. 现状取证（2026-09-24）

### 3.1 两跳 wire 并非同一格式

| 跳 | 类型 | serde | 示例形状 |
| --- | --- | --- | --- |
| 插件 → 宿主 | SDK `SyncEvent` | `tag = "type"`（内部标签，PascalCase 变体名，字段平铺） | `{"type":"SessionCreated","session":{…},"source_device":""}` |
| 宿主 → 移动端 | 宿主/移动 `SyncPayload` | `tag = "type", content = "data", rename_all = "snake_case"` | `{"type":"session_created","data":{…}}` |

`From` 转换与 Handler match **同时承担**：类型镜像 + 线格式改写（平铺 → `data` 嵌套 + snake_case）。

### 3.2 宿主触点清单

- `events/sync_event.rs`：`DesktopSyncEvent` 定义 + 穷尽 `From`
- `events/sync_handler.rs`：11 分支业务 match → `SyncPayload` → `Message::sync_data`
- `wasm_core/host_api/events.rs::broadcast_sync`：唯一生产路径（`From` + `sync_tx.send`）
- `system/app_context.rs` / `lib.rs`：`sync_tx: broadcast::Sender<DesktopSyncEvent>` 装配
- `enums/sync.rs` / `enums/summary.rs` / `enums/control.rs`：wire 类型定义
- `server/websocket/message.rs`：`Message::SyncData` / `SessionControl` / `Terminal` / `SessionEvent` 持宿主枚举
- 集成测试：`broadcast_shutdown` / `pty_session_chain` / `http_auth_biometric` / `ws_auth_rules`

### 3.3 插件已是唯一会话事件生产者

`com.bedcode.terminal-session` 经 `WasmHost.broadcast_sync(SyncEvent::{Session*|Task*|…})` 发布；宿主不再构造会话 `DesktopSyncEvent`（仅 `From` 一处）。

---

## 4. 目标架构

### 4.1 分层

```
插件 terminal-session
  │  构造 SDK 类型化事件（产出口形状锁 + 必填字段校验前移）
  ▼
host-events.broadcast-sync (WIT 不变, event-json: string)
  │  反序列化为 SDK SyncEvent（未知 type → 显性 Err）
  │  包装 HostSyncEvent(SyncEvent) —— 宿主唯一薄适配
  ▼
events::publish(e: impl AppEvent)          ← 统一入口
  │  matcher 泛型分发（TypeId 源/处理器，机制不变）
  ▼
SyncEventHandler: EventHandler<HostSyncEvent>
  │  e.validate()? → e.to_sync_payload() → Message::SyncData
  │  e.source_device() → 排除源设备广播
  │  ★ 零会话变体业务 match
  ▼
WebSocket 广播 → 移动端（SyncPayload wire 不变）
```

### 4.2 SDK 单一真源（`bedcode-plugin-api`）

新增/收编（wire 以**现有移动端/宿主 `SyncPayload` 形状**为准）：

| 类型 | 说明 |
| --- | --- |
| `SyncPayload` | 出站同步载荷（adjacently tagged snake_case + `data`）——真源迁入 |
| `SessionSummary` | 会话概要（camelCase，与插件 `session::view` 形状锁对齐） |
| `SessionControlPayload` / `SessionControlAction` | WS 会话控制动作 |
| `TerminalPayload` / `TerminalAction` | WS 终端控制帧 JSON 面 |
| `KeyCombo` | 按键组合（TerminalAction / 控制帧嵌入；宿主 `KeyCombo::parse` 路径改 re-export） |
| `SyncEvent` | 插件构造用事件：**对齐 `SyncPayload` 线格式与字段类型**（见 D1），消灭双格式 |

宿主 `enums/{sync,summary,control}.rs` 与 `enums/special_key.rs` **缩为 re-export**（仿既有 `PluginQuestion` 先例）；定义与形状锁测试主战场移至 SDK。

移动端保留本地 `SyncPayload` 副本（双端契约分叉先例，ADR 0018/0019 口径）——以**形状锁往返对照**钉与 SDK 真源逐字节一致，不要求移动端直接依赖桌面 SDK crate。

### 4.3 AppEvent 多态 API

```rust
/// 全局事件顶层 trait：约束 + 统一发送协议
pub trait AppEvent: Clone + Send + Sync + Debug {
    /// 触发源设备（WS 同步广播排除语义；非同步通道可 None）
    fn source_device(&self) -> Option<&str> {
        None
    }

    /// 可广播前置校验（必填字段等）；Err → 发送侧 fail-visible，不进入广播
    fn validate(&self) -> Result<(), String> {
        Ok(())
    }

    /// 转为出站同步线协议载荷；None = 本事件不走 SyncData 通道
    fn to_sync_payload(&self) -> Option<bedcode_plugin_api::events::SyncPayload>;
}

/// 统一发布入口：插件路径与宿主内部路径共用
pub async fn publish<E: AppEvent + 'static>(event: E) -> Result<(), PublishError>;
```

- `HostSyncEvent(pub SyncEvent)`：宿主本地 newtype（解决 orphan rule），实现 `AppEvent`；`to_sync_payload` 做 SDK `SyncEvent` → `SyncPayload`（D1 对齐后应为字段级近恒等转换）。
- 既有 `broadcast::Sender<DesktopSyncEvent>` 改为 `Sender<HostSyncEvent>`（或经 `global_matcher().publish`），`AppContext.sync_tx` 类型随之替换。
- `impl AppEvent for DesktopSyncEvent` **不保留**——镜像类型删除（contract 票）。

### 4.4 Handler 瘦身契约

| 现 Handler 职责 | 去向 |
| --- | --- |
| 逐变体字段搬运 → `SyncPayload` | `AppEvent::to_sync_payload`（适配层，无业务分支或机械 From） |
| `SessionCreated` 缺概要 → warn 不广播 等票 09 校验 | **前移插件产出口**（`broadcast_sync` 调用前）；宿主 `validate` 只留信封级兜底 |
| `SessionRemoved` 空名照广播 等不对称语义 | 插件侧保证载荷自足；宿主不再解释 |
| `source_device` 排除 | `AppEvent::source_device` + 既有 `broadcast_sync_to_others` |
| Debug 格式化状态串（`format!("{:?}")`） | 删除——插件已给 wire 形态；禁止宿主再解读 `SessionStatus` |

### 4.5 校验与 fail-visible

- 插件：`broadcast_sync` 前保证会话四类必填字段自足（与票 09 口径一致，生产者负责）。
- 宿主 `broadcast_sync`：反序列化失败 / `validate` 失败 → `Err` 返回 WASM（现有「unknown or malformed」路径扩展），**禁止静默丢弃**。
- Handler 信封失败 → `error!` 留痕，不伪造推送。

---

## 5. 决策记录

| # | 决策 | 理由 |
| --- | --- | --- |
| **D1** | **wire 以现有 `SyncPayload`（adjacently tagged snake_case + `data`）为唯一出站真源**；SDK `SyncEvent` 的 serde 属性与字段类型对齐之（内部标签 PascalCase 废除） | 移动端零改动；消灭「双格式 + Handler 改写」。插件→宿主跳变 JSON 内容不进 ABI 类型（仍是 `string`），**无需 ABI bump** |
| **D2** | wire 类型**单一事实源放 SDK**；宿主 `enums/` 只 re-export；移动端平行副本靠形状锁对齐 | 用户裁定；与 `PluginQuestion` / 既有 SDK 事件先例一致 |
| **D3** | 用 **`HostSyncEvent` newtype + `AppEvent` trait 方法 + `publish`** 统一发送；**删除** `DesktopSyncEvent` | orphan rule；镜像类型是问题本体，不能边多态边留双份 |
| **D4** | H2 表述修订：宿主仍**持有** `Message::{SyncData,SessionControl,Terminal}` 枚举（传输面），但**形状定义与解释权在 SDK/插件**；宿主不解动作语义 | 兼容票 09b 转发层与 ADR 0022 |
| **D5** | WIT `host-events`、权限位 `broadcast`、`PERMISSION_BROADCAST` **不变** | 传播 API 升级在宿主内完成，插件调用面 ABI 稳定 |
| **D6** | 校验前移插件产出口；宿主 Handler **禁止**按会话变体做业务分支（含状态 Debug 重格式化） | 防「真源在插件、解释权回宿主」回接 |
| **D7** | `SessionEvent`（`Message::session_event` 构造器）核查生产调用方；若仅测试使用则随本线退役或标注遗留 | 缩小会话出站面 |

### 5.1 实施期补记（票 01 / 02 落地后）

本文的 §4 代码片段是**方向示意**，落地时的三处偏离以本节为准（详细理由见各票末实施记录）：

1. **wire 类型落点是 SDK 的 `bedcode_plugin_api::wire`，不是 `events`**（票 01）。
   §4.2 / §4.3 里写的 `bedcode_plugin_api::events::SyncPayload` 请读作
   `bedcode_plugin_api::wire::SyncPayload`：`events` 是插件面「能发布什么」，
   `wire` 是跨端「线上长什么样」，且 `wire/` 下的文件与宿主 `enums/` 一一对应，
   垫片可逐文件 diff 核对「搬空了没有」。
2. **`to_sync_payload` 是同一 wire 的机械折算，不做逐变体 match**（票 02，
   与 D6 同向：机械 match 落在宿主就是解释权重回宿主的第一块跳板）。
   变体面一致性改由 SDK 三把对齐锁钉住。
3. **`validate()` 在 `publish` 入口执行，不在 Handler 内**（§4.1 图示与 §4.5 要求
   只有入口能同时满足）；**`publish` 在无事件源时 `Err(NoSource)`**（§4.3 未规定，
   按 AGENTS §8 fail-visible 判据补）。
4. 事实修正：§4.2 表格写 `SessionSummary` 是 camelCase —— 实测三方（宿主 / 移动端 /
   插件产出口）**都是 snake_case**，形状锁按实际锁；插件 `session::view` 的文档注释
   同样陈旧，已随票 02 校正。

---

## 6. 迁移策略（expand–contract）

两跳格式与类型双轨，按 **expand → migrate → contract**：

1. **Expand**：SDK 落入全部 wire 类型 + 对齐后的 `SyncEvent`；宿主改为 re-export；`HostSyncEvent` + `AppEvent` 方法 + `publish` 与旧 `DesktopSyncEvent` 路径**双轨并行**（旧 From 暂保留）。
2. **Migrate**：`broadcast_sync` 切到 `publish(HostSyncEvent)`；Handler 切到瘦实现；集成测试与 `sync_tx` 类型切换。
3. **Contract**：删除 `DesktopSyncEvent`、旧 `From`、Handler 业务 match、宿主本地 wire 定义；上防回接锁。

---

## 7. 契约、锁与测试

| 锁 | 钉住内容 |
| --- | --- |
| 形状锁（SDK） | `SyncPayload` 全变体 wire JSON（含 `session_created` / `task_queue_changed` 等 type 标签与 `data` 嵌套） |
| 形状锁（SDK ↔ 移动端） | 同变体往返对照（移动端 `enums/sync.rs` 样例 JSON 进 SDK 反序列化再序列化全等） |
| 形状锁 | `SessionSummary` / `SessionControlAction` / `TerminalAction` / `KeyCombo` 往返（自宿主测试迁移） |
| 行为锁 | 源设备排除广播、`SessionRemoved` 幂等空名广播、缺字段 fail-visible 不伪造推送 |
| 防回接锁 | 宿主 `events/**`、`enums/**` 不得再定义会话业务枚举/`DesktopSyncEvent` 镜像（编译期或 grep 锁，仿 `retired_kernel_session_domain_is_not_reintroduced`） |
| 集成 | `broadcast_shutdown` / `pty_session_chain` 等改用 `HostSyncEvent`/`publish` 后全绿 |

**验证命令（收尾跑）**：

```bash
cd bedcode-desktop/src-tauri && cargo test
cd bedcode-desktop && pnpm run test:run
pnpm exec eslint .
```

开发中只跑针对性过滤（`cargo test <前缀>` / `pnpm exec vitest run <file>`）。

---

## 8. 风险与开放问题

| 风险 | 缓解 |
| --- | --- |
| D1 改插件→宿主 JSON 格式，旧插件产物广播被拒 | 内置插件随包重建；加载期 `wasm_hash` 校验；拒绝路径 fail-visible 可观测 |
| `SyncEvent` 字段与 `SyncPayload` 不完全同构（如 `SessionStatusChanged` 状态现为 `Value` vs `String`） | 对齐票内出对照表；状态一律 wire 字符串/对象原样透传，宿主不解析为 `SessionStatus` |
| 移动端 `ConfigCreated` 等桌面已退役变体 | 桌面 SDK 不恢复 Config 变体；形状锁只覆盖桌面仍发的变体；移动端多出的变体保持 ignored |
| `KeyCombo::to_pty_bytes` 等宿主仍调用 | re-export 后调用方零改动；引擎路径不迁（非本票范围） |
| 双轨期 `sync_tx` 类型分叉 | 里程碑内强制单通道；contract 票删旧类型 |

**开放**：`Message::SessionEvent`（D7）生产调用方清点 → 落在票内验收，不另开决策。

---

## 9. 票据拆分（to-tickets）

见同目录 `issues/`（编号按依赖序）。摘要：

| 票 | 标题 | Blocked by |
| --- | --- | --- |
| 01 | SDK 收编 wire 类型，宿主 enums 改 re-export（expand） | 无 |
| 02 | AppEvent 方法 + HostSyncEvent + SyncEvent 对齐 SyncPayload 线格式（expand 双轨） | 01 |
| 03 | broadcast_sync 切统一 publish，Handler 瘦身，迁 sync_tx（migrate） | 02 |
| 04 | 删除 DesktopSyncEvent 与宿主业务 match，防回接锁 + 全量回归（contract） | 03 |
