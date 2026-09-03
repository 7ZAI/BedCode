# 04 — 背压：wire 契约 + 前端 ack 回发

**What to build:** 背压反馈环的 wire 契约与前端侧——复用 TB v2 帧 `seq` 维度（不引入新坐标系），前端在每次写入解析完成后回发 ack 帧携已渲染到的 `last_rendered_seq`；仅对已消费帧回发，未渲染部分不 ack。控制消息走现有文本协议，ack 走二进制帧。

**Blocked by:** None — can start immediately.

**Status:** resolved（实现完成，e2e 待真机）

- [x] 定义 ack 帧 wire 契约（TB v2 头 + ACK 标志位 0x02 + acked_seq(8 LE) + session_id UTF-8；与服务端→客户端输出帧的 WAITING/事件数位语义互不冲突，仅入站二进制帧按 ack 解析）（复用 TB v2 帧 seq 维度、携 `last_rendered_seq`、与普通输出帧可区分）
- [x] 前端在写入解析完成后回发 ack 帧（onWriteParsed → `useTerminalOutputStream.confirmWriteParsed()` → 携 last_rendered_seq；节流 = 64KB 字节阈值 + 250ms 空闲兜底，首次无条件建立基线），携已渲染到的 `last_rendered_seq`
- [x] 仅对已消费帧回发 ack（游标只在 deliverFrame 推进，节流窗口内未达阈值不逐帧回发），未渲染部分不 ack
- [x] 复用现有 `MockWebSocket` 测试骨架单测（sentBinary 记录 + decodeAck 助手，5 用例：seq/session_id、64KB 阈值、节流窗口、新会话水位重置、stop 后不 ack）

## Answer（wire 契约，2026-08-21）

- **Ack 帧**（客户端→服务端，同一 WS 二进制）：`magic "TB"(2) + version=2(1) + flags=0x02 ACK(1) + acked_seq(8 LE) + len(4 LE) + session_id(UTF-8)`。复用 TB v2 头；值位与输出帧的 WAITING/事件数位不冲突——服务端只对**入站二进制帧**按 ack 解析，输出帧仅服务端→客户端
- **前端触发**：`Terminal.onWriteParsed`（写解析完成）→ `confirmWriteParsed()`（useTerminalOutputStream 公开 API）→ 携 `last_rendered_seq`；节流 64KB 字节阈值 + 250ms 空闲兜底；首 ack 无条件建基线
- **服务端消费**：`TerminalWs` 的 `WsMessage::Binary` → `parse_ack_frame` → `GlobalOutputManager::ack(session_id, seq)` → `SessionOutputManager::on_ack`（unacked_fifo 按序弹 ≤ seq 的条目）
- **水位/账本**：`BACKPRESSURE_WATERMARK_BYTES=1MB`、`UNACKED_FIFO_CAP=8192`（满则冻结记账保守暂停）；`should_pause` 纯原子读，PTY 读线程轮询
- **e2e（44-46）**：见 issues 05/06

## Comments

- 2026-08-21：实现完成。修复了一个隐藏问题：测试内 70KB spread 数组触发 vitest worker OOM（`ERR_WORKER_OUT_OF_MEMORY`），已改为直接传 `Uint8Array`（MockWebSocket.binary 宽化签名），全量 436 不再 OOM：ack 时序、回发上限窗口、断线重连/重订阅后语义不变
- [x] 现有 composable 测试全绿（全量 vitest 436 通过；修复了一处测试内 70KB spread 数组触发 vitest worker OOM 的问题）
