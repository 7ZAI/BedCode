# TB v3 桌面端旧方案遗留审计（2026-09-12）

> 范围：桌面端 pty 输出链路（pty_reader / session_output / forward / control_frame /
> terminal_ws / config / 桌面 WebView 前端）v3 化后，v2 时代（事件 index / seq /
> base64 JSON 输出帧）遗留的无效代码与注释残留。
> 状态：**清理完成（2026-09-12）**——A 级全部删除、B 级全部清理、C 级注释/命名
> 更新；验证：cargo test --lib 608 + 全部集成测试 + vitest 614 + eslint 0 error 全绿。
> 本文件保留为清理记录；剩「附注」中 spec §6/§8 文档表述更新待 ticket。

## 结论概要

- 旧路由（Message 协议 actor）**本身仍活跃**（桌面 WS 环回 `useTerminalOutputStream`
  走 `/ws/terminal/local`），不是死代码；其输出帧已统一为 TB v3 二进制，仅部分
  wire 字段/序列化件是 v2 残留。
- 真正可安全删除的死代码：3 处（A1/A2/A3）+ 1 条疑似死链（A4，需确认旧移动端
  JoinSession 已全部下线）。其余为恒值死字段、兼容签名残留、注释/命名过时。

---

## 🔴 A 级：死代码（零引用 / 可安全删除）

### A1. `OutputEventSerialized` + `to_serialized()` + `data_base64()`（session_output.rs:64-101）
v2 时代 base64 JSON 输出帧的序列化辅助。v3 二进制帧后**全仓（含内联测试）零调用**。
`session.rs` 的 re-export 中亦无（未导出）。

### A2. `channels.global_queue_capacity`（config.rs:441，默认 100_000）
注释自认：*"历史遗留：事件数上限已被字节块队列的 max_chunks 取代，保留为兼容字段"*。
全仓（含 settings 读写行 851-852、props 校验行 122）之外零消费。连带删除：
定义 + 默认值 + props table 行 + serialization 行。

### A3. `channels.pty_subscription_capacity`（config.rs:433，默认 1024）
旧 broadcast 订阅通道容量；订阅已全部改为 mpsc（8192/16384/32768 固定），
**全仓零消费**。

### A4. JoinSession JSON 输出帧路径（session_control.rs:185-270 + 382-440）
`OutputBuffer`（自维护 `start_index/end_index/next_index` 合成事件序号 + base64
JSON 输出帧 `Message::output_from_base64`）仅被 `SessionControlAction::JoinSession`
分支使用。现状：
- JoinSession wire action **服务端不构造、前端 `src/` 零调用、集成测试零覆盖**
- 移动端 v3（mobile-ws-rust）走后，该路径无已知生产消费者
- 其输出格式（index/end_index + base64 JSON）与 v3 字节语义**不一致**
- 连带：`enums/control.rs` `TerminalAction::Output` 的 `index`/`end_index` 字段
  （v2 序号字段，仅此路径序列化）
⚠️ 删除前确认旧移动端 v2 客户端已全部下线（AGENTS §9 双端同步约束）。

---

## 🟡 B 级：恒值死字段 / 兼容签名残留（无害，建议清理）

### B1. `SubscribeResponse.mode / min_offset / max_offset`（enums/control.rs:139-145）
`Handler<SubscribeResult>`（terminal_ws.rs:1473-1485）注释自认恒值：
`mode 恒 SubscribeMode::Reset、min_offset/max_offset 恒 0`；
前端 `useTerminalOutputStream.handleControl` 不消费这三字段。
连带：`SubscribeMode::Incremental` 变体 wire 永不可达（07 已弃用裁决三态）。

### B2. 旧订阅 wire `start_seq`（enums/control.rs:132）
`handle_subscribe` 注释：*"05 快照协议忽略 wire start_seq（恒全量重播）"*；
桌面环回 `buildSubscribe` 不带、新路由用 `from_offset` → 遗留字段，仅留日志透传。

### B3. `UnifiedOutputQueue::new(_capacity)` / `with_max_bytes(_capacity, …)`（session_output.rs:146-158）
unused `_capacity` 参数（注释"兼容旧签名保留"）；生产构造只剩 `default()` /
`with_limits()`，capacity 无意义（仅测试传真实值）。

### B4. `Message::subscribe_response` 6 参构造器（message.rs:321）
生产零调用（仅 message.rs 自身测试 1064 行使用）。

---

## 🟢 C 级：注释 / 命名残留（行为正确，文档误导）

### C1. "TB v2" 字样（实际编码已 v3-only，无 v2 编码函数）
| 位置 | 原文 |
| --- | --- |
| forward.rs:1（模块头） | "将 OutputEvent 流编码为 **TB v2** 二进制帧" |
| terminal_ws.rs:1274 | "将 OutputEvent 转为 **TB v2** 二进制帧发到 actor…仅剩 TB v2" |
| terminal_ws.rs:770-772 | "forward_loop 输出 **TB v2** 二进制帧(§5.3)"、"解析合并 message 内全部 **TB v2 帧**"、"前端 **seq** 缺口自愈" |
| app.rs:32-37 | "输出帧为 **TB v2** 二进制(§5.3)" |
| terminal_stream.rs:12（模块头） | "复用 WS 同款 **TB v2 帧** + 快照 **seq 语义**…min_seq / snapshot_seq / history_count"（实际已 v3 字节三件套） |

### C2. "min_seq" 字样（应 min_offset）
| 位置 | 原文 |
| --- | --- |
| config.rs:397 | "严格从队首（**min_seq**）起播全部保留事件" |
| session_output.rs:673 | warn 文案"回退 **min_seq** 严格回放" |
| terminal_stream.rs:20 | 注释"前端按 **seq** 去重跳过历史段"（HistoryEnd 处，实际 offset 语义） |

### C3. 前端参数名 / JSDoc / 文案（值实为 minOffset）
| 位置 | 残留 |
| --- | --- |
| useTerminalOutputStream.ts:61 / Channel:48 | `onTruncated?: (minSeq: number)` 参数名 |
| useTerminalOutputStream.ts `resubscribe` JSDoc | "按 **seq** 去重"、"[min_seq .. snapshot_seq]"、"≤ last_rendered_seq"、"**min_seq > last_rendered_seq + 1**"（+1 为 v2 事件游标判断，与实际 `minOffset > lastRenderedOffset` 不符） |
| useTerminalOutputStreamChannel.ts `resubscribe` JSDoc | "保留 **last_rendered_seq**，重播时按 **seq** 去重" |
| TerminalPreview.vue:465-475 | `onTruncated: (minSeq)` 参数名 + console 文案 "min_seq=${minSeq}" |

---

## ✅ 有意的兼容保留（勿当残留清理）

- `control_frame.rs` v2 ack 头兼容接受（`TB_FRAME_VERSION_V2`）—— spec §8 过渡兼容，仅 ack 方向
- useTerminalOutputStream 旧路由 wire 字段名 `min_seq/max_seq/history_count`
  承载字节语义值—— spec §8 增量演进（前端已映射为 minOffset/snapshotOffset/historyBytes）
- `history_start_mode=Snapshot` warn 回退分支 —— 未实现能力占位，spec §6 承认

---

## 附注（spec 与代码的张力，非代码缺陷）

spec §6 写"旧 v2 帧编码长期保留（仅过渡期兼容）"，但 forward.rs 实际已 **v3-only**
（无 `encode_output_frame_v2`）；旧 v2 客户端连接会收到 v3 帧直接断——与
`mobile-ws-rust/review.md` R6 一致。建议同步更新 spec §6/§8 表述，
并修正 C1 表内各 "TB v2" 注释（当前注释与代码互相矛盾）。