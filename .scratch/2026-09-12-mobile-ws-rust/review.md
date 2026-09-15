# Review：移动端 WS→Rust + 桌面 TB v3（.scratch/mobile-ws-rust 方案核对）

> 审查人：pty-byte-history 会话代理（桌面端 TB v3 原实施方，现已移交）
> 日期：2026-09-12
> 范围：spec.md 全文、tickets 01-05、已落地代码（session_output.rs / config.rs / forward.rs / Cargo.toml / 上下文文件）
> 方法：逐条核对 wire 契约一致性（AGENTS.md §9 两端同步）、边界/竞态/资源审查、实测编译与单测

---

## 0. 实测现状（审查时点）

- `cargo check --lib` 全绿（3 个 warning 均为既有代码：pty_process.rs unused import / http_filter.rs App / peer_transfer.rs node）
- `cargo test --lib`：session_output 30 ✓、terminal_ws（forward/control_frame）31 ✓ —— **含其新增 batch 双速用例**
  （batch_accumulates_by_bytes / batch_to_realtime_flushes_immediately / mode_switch_flushes_residual）
- 双速 forward_loop 主体逻辑核对**正确**：batch 纯批次无时间窗、realtime 时间窗+字节窗、模式翻转即时 flush 残留、
  Err(空闲) 分支已按模式区分（batch 不因时间窗 flush）✓
- queue 淘汰/半块 slice/字节连续性核对**正确** ✓

---

## 1. 缺陷／风险清单（按优先级）

### 🔴 高 — 移动端链路（ticket 03/04 必须处理）

**R1. 移动端「HTTP 历史 + WS 实时」拼接边界竞态未定义**
- 位置：spec M1"M1 历史走 HTTP、实时走 WS"；M2"拼接完历史才通知前端消费"
- 问题：HTTP 拉 `[from, snapshot_http)` 与 WS 实时 `[snapshot_ws, …)` 的**两个快照点不是同一时刻**。若
  subscribe_ok(snapshot_offset) 与 HTTP 响应到达顺序不定，`[min(snap_ws, snap_http), max(...)]` 区间可能
  **既出现在 HTTP 又出现在 WS（重复）或两边都不出现（空洞）**——缓存拼接用字节区间取并集才能正确。
- 建议：契约明确为——先 subscribe 拿 `subscribe_ok.snapshot_offset`；HTTP 只拉 `[from, snapshot_offset]`；
  WS 帧按 `start_offset >= snapshot_offset` 入实时缓存、低于该值的丢弃（或按区间与历史段做并集去重）；
  M1 缓存数据结构需支持**区间重叠合并**（append-only 区间列表），tests 必须有跨 snapshot 边界的用例。

**R2. 首次 subscribe 的 from_offset 语义缺失 → 与 HTTP 历史重复消费**
- 位置：spec M1"重订阅带 from_offset"，未定义**首次订阅**参数
- 问题：若首次 subscribe 不带 from_offset，服务端从 `min_offset` 全量重播历史到 WS —— 与 HTTP 历史**双份**
  （双倍带宽 + 双倍缓存写 + 拼接去重复杂度）。
- 建议：首次订阅 from_offset = HTTP 拉取完成后的缓存游标（即 HTTP 先行，subscribe 后行）；或 subscribe 先行但
  移动端 Rust 丢弃历史段、只取 history_end 之后的实时段。两条路选一写进契约，测试覆盖。

**R3. HTTP 一次性全量与移动端缓存上限矛盾（可能全量拉取被淘汰浪费）**
- 位置：spec §3 M1"一次 HTTP 拉取" + "缓存 16MB LRU"；桌面 max_total_bytes=50MB
- 问题：from=0（或游标落后）时 HTTP 返回最多 ~50MB → base64 ~67MB 单响应；移动端缓存仅 16MB →
  **拉了 2/3 被 LRU 淘汰丢弃**，且 67MB base64 单次解析对移动端内存/网络压力大。
- 建议：HTTP `from` = 移动端缓存游标做**增量拉取**（缓存淘汰后游标前移、HTTP 只拉缺口）；
  服务端可设单次响应上限（如 4MB/chunk 级分页）防御；client 侧提示"缓存淘汰后重进需全量"由截断逻辑兜底。

### 🟡 中

**R4. history_end 帧在移动端新链路的处置未定义**
- 位置：桌面 WS 路由仍发 `history_end {snapshot_offset}`（subscribe 成功后）；M1 帧解析只列了去重/缺口/截断
- 问题：移动端 Rust WS 解析器收到 history_end 应转什么事件/忽略？契约空白 → 实现歧义。
- 建议：M1 明确"history_end 到达 → 若此前 HTTP 已交付历史、只需把实时段 gate 打开；否则把 history_end 的
  snapshot_offset 用做 HTTP 拉取边界"（与 R1 合流）。

**R5. 移动端重启游标丢失 → 重复渲染**
- 位置：M1"意外断开 → 重连保留游标"（内存态）
- 问题：移动端应用进程被杀 → 游标归零 → 重进 `from=0` 全量重拉 → xterm 从零重放整个历史（与已渲染内容重复）。
- 建议：lastRenderedOffset / 缓存游标持久化（SQLite 或 storage），重启后 from=持久化游标；历史缓存头部被淘汰
  的用 truncated 语义。

**R6. "v2 兼容"仅覆盖 ack 方向——旧移动端输出会断**
- 位置：control_frame.rs ack 解析接受 version 2|3（正确）；但**输出帧已是 v3**（version=3），旧移动端解析器
  不识 → 断流
- 建议：文档明示"过渡期兼容=仅 ack 方向"，旧客户端必须随两端同步升级（AGENTS §9）；不要向用户表述为
  "v2 客户端可用"。

**R7. HTTP history 接口鉴权粒度与配额未定义**
- 位置：spec D2 `GET /api/sessions/{id}/history`（JWT）
- 建议：确认走既有 session 访问控制（移动端只能查自己认证的会话）；加响应大小上限与频率限制防滥用。

### 🟢 低（信息级 / 优化）

**R8. snapshot_from 回放事件 session_id=""、timestamp=0**
- 影响：消费端（forward）只用 data/start_offset/is_waiting，不受影响；但历史事件元数据丢失，若未来插件管道
  （process_through_plugins 以 session_id 路由）接历史路径会拿到空值。chunk 结构未存 session_id——要支持需
  OutputChunk 增字段或 snapshot_from 收 session_id 参数。

**R9. Bytes 拷贝可省一次**
- `push` 用 `Bytes::copy_from_slice(&event.data)`；PtyReader 的 raw_bytes 是新建 Vec，
  可 `Bytes::from(vec)`（搬移零拷贝）——需事件侧把 data 作为 Bytes 载体或 push 收 Vec 后转入。

**R10. start_offset 双重分配冗余**
- on_output 预分配 `event.start_offset = queue.max_offset()` + push 内"防御性重赋"——同一写锁临界区值恒一致，
  建议删 push 内重赋（保留 on_output 一侧），注释注明不变量。

**R11. 50MB 默认值变更（64→50MB）是产品行为变化**
- config description 已注明；属用户最初提议（审查对话中曾列待确认），此处确认 OK。

---

## 2. 未完成项核对（ticket 归属）

| 项 | ticket | 状态（审查时点） |
| --- | --- | --- |
| WebView 前端 `useTerminalOutputStreamChannel.ts` v3 + offset 游标 + 跨帧裁剪 + 单测 | 01 | ❌ 未动——**桌面本地环回通道仍按 v2 解析，环回会断**，需 01 收尾 |
| `ClientFrame::SetMode` / mode AtomicU8 接线（终端 WS 两路调用点） | 02 | ❌ 控制帧未实现（forward 双速已就绪） |
| HTTP history controller / 路由 | 02 | ❌ 未实现（`SnapshotOutputManager.snapshot_bytes` 已备好，可直连） |
| subscribe_ok 携带 `protocol: 3` | 02 | ❌ |
| 移动端 terminal_link.rs / 前端接线 | 03/04 | ❌ 未开始 |
| 全量验证 / CHANGELOG | 05 | ❌ |

---

## 3. 对我方（pty-byte-history 实施）遗留的提醒

- `tests/ws_session_route.rs` 集成测试已随本次迁移改为 TB v3 断言（check_tb_v3_frame + subscribe_ok 三件套），
  config 补齐后需全量 `cargo test`（含 tests/ 集成）确认——审查时点 `cargo check --tests` 曾因 config 字段
  缺失失败，config 补齐后未重跑集成测试。
- `.scratch/pty-byte-history/spec.md` §7（移动端契约）与 mobile-ws-rust spec 已演进（HTTP 历史替代 WS 重播），
  两文档需在 ticket 05 对齐，避免契约漂移。