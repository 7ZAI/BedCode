# 02: 桌面端双速传播 + HTTP 一次性历史

Type: task
Status: claimed（桌面另一 agent 在途；代码已就位、我读码核对过——提交由其负责）
Blocked by: 01

## 范围
- `forward_loop` 双速模式：订阅者 mode（realtime=时间窗+字节窗合并 / batch=满 batch_bytes 才发，
  默认 64KB 可配置）；`control_frame` 新增 `{"type":"mode","mode":"realtime"|"batch"}`；
  terminal_ws 存储每订阅者 mode AtomicU8，切换即时生效（切换 realtime 时 flush 残留）
- HTTP 历史接口：`GET /api/sessions/{id}/history?from=<u64>`（JWT）→
  `{min_offset, snapshot_offset, history_bytes, data_base64}`（[from, snapshot_offset) 字节，
  chunk 级跳过 + 半块 slice）——移动端历史首选路径（不再走 WS 重播）
- app.rs 路由 + 新 controller（可并入 session_controller）
- subscribe_ok 携带 `protocol: 3`（老客户端忽略）

## 验收
- `cargo test`（forward 双速 / mode 切换 / history controller / queue.range）
- 手工冒烟说明：端到端需移动端配合，桌面侧以单测 + 集成测试覆盖