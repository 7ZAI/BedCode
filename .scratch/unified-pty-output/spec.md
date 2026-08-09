# 统一 PTY 输出消费逻辑 — 架构方案

> 状态：方案（未实现）· 2026-08-09
> 范围：桌面端 / 移动端 终端输出链路的统一抽象设计

## 1. 现状：同一条输出流，五处重复逻辑

| 维度 | 桌面端（本地） | 移动端（远程） |
|------|---------------|---------------|
| 传输 | Tauri event `pty-output-{id}`（JSON+base64） | WS `ws_output`（JSON+base64） |
| 历史 | `get_session_output_history` invoke 全量拉取 | `ws_subscribe_session(sid, startSeq)` 增量订阅 |
| 去重/续传 | `lastReplayedIndex` + `advanceWatermark` + `pendingLiveEvents` | `lastIndex`/`lastEndIndex` + 2MB 环形 buffer + `hasGap` |
| 缺口处理 | 无显式处理（回放/实时靠水位过滤） | `minSeq > startSeq` → 清空全量重播 |
| 写入管线 | rAF 合并 + DEC2026 + 分块 → xterm | writeCoalescer（同款）→ xterm |

五处重复，语义还不完全一致。

**关键前提（已有基础）**：事件 index 已是全局统一递增（`next_output_index()`，pty_output.rs 注释"桌面端 + 移动端统一计数"）——统一模型的坐标底座已存在，无需发明新坐标系。

## 2. 核心模型：一条流 + 单调游标（消费者同构）

```
每个 PTY 会话 = 一条 OutputStream（单一真源）
  ├── 事件窗口（环形，允许丢最早）     ← 现有 UnifiedOutputQueue 演进
  ├── 全局单调游标（事件号 or 字节偏移） ← 决策点，见 §6
  └── 快照点（可选增强）：最后一次 \x1b[2J 的游标位置

消费者（桌面端/移动端/未来任何端）只做两件事：
  1. 持有一个游标（上次消费到的位置）
  2. 游标与流的关系只有三种：在窗口内 → 增量；已失效 → 重置；无历史 → 从头收
```

**核心原则：**
- 单一真源是 PTY 输出流，不是任何一端；消费者之间零共享状态、无需对齐
- 一切"数据可用性"判断由服务端基于真源裁决（应答里带 min/max），消费者只做单边比对
- 传输各异、语义统一：桌面端 event / 移动端 WS，不强制同一传输
- 允许丢最早（环形语义）；丢的粒度由服务端窗口裁决，不追求字节级零丢失

## 3. 统一服务端契约（Rust 侧一个实现，两个出口）

演进 `GlobalOutputManager`，对本地转发器（FrontendOutputHandler）和 WS 共用同一订阅逻辑：

```
subscribe(session, cursor)
  → Subscription {
      min_cursor, max_cursor,
      mode: Incremental | Reset | Snapshot,   // 由服务端裁决
      backfill: 自 cursor（或自快照点）起的完整事件段,
    }
实时推送：{ cursor, data, is_waiting }        // cursor 连续校验由消费者做
游标失效：mode=Reset 时消费者清屏 → 从 backfill 起点重放
```

消费者规则收敛为一条：**收到的 cursor 不连续 → 重新 subscribe**（幂等，服务端给正确答案）。
`hasGap`/`pendingLiveEvents`/`minSeq>startSeq` 猜测逻辑全部消失——重置决策从"客户端猜"变为"服务端告知"。

## 4. 前端统一抽象：一个 composable + 两个 adapter

```
useTerminalOutputStream(sessionId, adapter)
  ├── write / replay / reset          // 内部：游标校验 → rAF 合并 → DEC2026 → xterm
  ├── cursor（消费进度，内存态）
  └── adapter 接口：{ subscribe(cursor), onData(cb), unsubscribe() }

LocalEventAdapter   // 桌面端：listen pty-output + invoke get_history
RemoteWsAdapter     // 移动端：ws_subscribe + ws_output + ws_leave
```

迁移路径：
- 桌面端 `TerminalPreview` 的 writeQueue/flush/writeReplay/pendingLiveEvents → 下沉进 composable
- 移动端 writeCoalescer/useTerminalBuffer 的写入口 → adapter 化（buffer 去重逻辑被 composable 的游标校验取代）
- 两端共享同一份写入管线、回放逻辑、滚动联动——差异只剩 adapter 传输细节

## 5. 分阶段路线图

| 阶段 | 内容 | 兼容性 |
|------|------|--------|
| **P0**（已完成） | 桌面端链路字节化（base64→Uint8Array）、服务器常驻（自启动永久开启、UI 入口移除）、性能监控默认关闭 | — |
| **P1** | Rust 统一订阅契约：subscribe 应答语义化（min/max/mode/backfill），event 与 WS 共用；**WS 线上字段不动** | 全兼容 |
| **P2** | 前端统一 composable + LocalEventAdapter，桌面端先迁移验证契约 | 桌面端内部重构 |
| **P3** | RemoteWsAdapter，移动端迁移；`hasGap`/本地环形 buffer 降级为纯回放窗口 | 协议演进点，移动端同步发版 |
| **P4**（可选） | 字节偏移游标 + 清屏快照点；`useGlobalTerminal` 第二份历史移除 | 增强，可后置 |

## 6. 三个待决策点

1. **游标用事件号还是字节偏移**
   - 事件号：现状已有、原子、无拆分问题，移动端 `end_index` 合并语义已等价于游标 → **推荐先做**
   - 字节偏移：更细粒度（支持任意字节断点续传），但要给每个事件加 offset 字段，收益在事件不拆分场景下不明显 → 放 P4
2. **本地第二份历史（useGlobalTerminal / 移动端 buffer）去留**
   - 契约成熟后，历史永远在服务端，消费者按游标拉 → 本地缓存可整体移除（内存减 2MB+5MB）
   - 前提：P1 的 backfill 应答稳定；期间保留缓存做兜底
3. **清屏快照点做不做**
   - 价值：Reset 时从"残缺窗口"变"完整一帧"，对全屏 TUI（opencode）语义正确
   - 成本：服务端约 20 行状态机（扫描 `\x1b[2J`）；可 P4 独立交付

## 7. 非目标（明确不做）

- 不强制桌面端改走 WS（传输各异原则；服务器常驻后本地 WS 仅是可选项，非必需）
- 不做跨消费者进度同步（本来就无需同步）
- 不追求"字节级零丢失"——保持"允许丢最早"语义，丢的粒度由服务端窗口裁决
