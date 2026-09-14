# 01: 桌面端 TB v3 字节连续收尾（在途迁移续做）

Type: task
Status: resolved
Blocked by: —

## 范围
完成 `.scratch/pty-byte-history/spec.md` §5 桌面端清单中尚未落地的部分（session_output.rs 已在途，
其余文件未动）。本次会让桌面端恢复编译 + 全量测试通过。

- `session/session_output.rs`：`UnifiedOutputQueue` 重写为字节块队列
  （`VecDeque<OutputChunk{start_offset, bytes: Bytes, end_is_waiting}>`，max_offset/min_offset/
  total_bytes/max_total_bytes(50MB)/max_chunks(65536)）；on_output 分配 start_offset；unacked_fifo
  改 (end_offset, bytes)；on_ack(acked_offset)；subscribe(…, from_offset) 字节锚点回放 + 半块 slice；
  `SubscribeResponse{snapshot_offset,min_offset,history_bytes}`；内联测试全量适配
- `pty/pty_reader.rs`：去掉 index 占位（on_output 分配 offset）
- `server/ws/terminal_ws/forward.rs`：`encode_output_frame_v3`（start_offset 8LE + len 4LE，无 count）；
  OutputBuffer 记 start_offset；HistoryEnd 字节三件套；测试适配
- `server/ws/terminal_ws/control_frame.rs`：Subscribe{from_offset}、ServerFrame 字节三件套、
  ack 解析 v3（acked_offset）+ v2 兼容
- `server/ws/terminal_ws.rs`：subscribe/ack/protocol 版本接线
- `system/config.rs`：global_queue_max_bytes 64MB→50MB；新增 global_queue_max_chunks；
  新增 terminal.batch_bytes（双速模式用，64KB）
- 旧路由最小适配：`server/ws/message.rs` / `server/services/session_control.rs` 的 `event.index` →
  start_offset
- WebView 前端 `composables/useTerminalOutputStreamChannel.ts`：TB v3 解析 + start/end offset 游标 +
  跨帧裁剪；测试适配
- 桌面端 Cargo.toml 显式声明 `bytes` 依赖

## 验收
- `cd bedcode-desktop/src-tauri && cargo test` 全绿
- `cd bedcode-desktop && pnpm run test:run` 全绿；根目录 `pnpm exec eslint .` 0 error
## Answer
桌面 TB v3 全部落地：session_output 字节块队列（50MB/65536）/from_offset 快照/字节三件套/acked_offset；forward v3 + 双速模式（realtime/batch + batch_bytes）；control_frame SetMode；config 新增 global_queue_max_chunks/batch_bytes；桌面前端两 composable + 测试 v3 化（含跨帧裁剪）；cargo test 611+集成全绿、vitest 614 全绿。桌面 Rust+前端 0 lint error。
