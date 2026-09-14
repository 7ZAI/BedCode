# Mobile WS → Rust + 桌面字节连续 跟进地图

## Notes
- 用户指令（2026-09-12，优先级最高，见 spec §0）：订阅时机改为会话启动即订阅（Rust 管理、前端触发）；
  移动端数据真源 = Rust 缓存；桌面端字节连续（TB v3）+ 两套缓存 + HTTP 一次性历史 + 双速传播。
- 桌面端 `.scratch/pty-byte-history/spec.md`（TB v3）为基准；其桌面端实施已在途（session_output.rs 改了
  一半，当前编译不过）——按 AGENTS §11 合并续做，禁止整文件回滚。
- 在途改动确认：`bedcode-desktop/src-tauri/src/session/session_output.rs`
  （OutputEvent.index→start_offset、HistoryEnd 改字节三件套字段，主体未迁移）。

## Decisions
- D1：桌面 TB v3 收尾（spec D1）——队列改字节块（Bytes 半块切片）、from_offset 订阅、字节三件套、
  acked_offset、双速 forward、mode 控制帧。ticket 01（另一 agent 完成）。
- D2：HTTP 一次性历史 `GET /api/sessions/{id}/history?from=<offset>`。ticket 02（另一 agent 完成，路由/控制器已就位）。
- D3：移动端 Rust 终端链路（每会话一 WS、缓存、ack、重连、事件、命令）。ticket 03（已落地）。
- D4：移动端前端接线（订阅时机/模式切换/历史拼接/输入路由）。ticket 04（已落地）。
- D5：文档同步 + 全量验证收尾。ticket 05（open）。
- 结论去向：实施过程中在对应 ticket 的 Answer 段记录。

## 09-12 晚续做（ticket 03/04 尾收 + 验证）
- ticket 03 补 11 单测 + 修两个真实运行时 bug（见 ticket 03 Answer）：invoke 返回值 camelCase 键、
  HTTP 历史回退信封（code!=0 + snake→camel）。
- ticket 04 修 4 处前端问题（见 ticket 04 落地核对），vitest 10 fail+3 error → 372 全绿。
- 验证：移动端 cargo test 282 全绿 / vitest 372 全绿 / vue-tsc 0 / eslint 0 error。
- 遗留（ticket 05 处理）：链路加密 ws-terminal 协商；spec §7 与本文档对齐；桌面端全量验证由在途桌面 agent收尾；
  TerminalPreview.vue（桌面，另一 agent 在途）的 oxlint 既有告警不属本任务。CHANGELOG 按用户裁决不改（已发布日志）。

## Frontier
01 → 02 → 03 → 04 → 05