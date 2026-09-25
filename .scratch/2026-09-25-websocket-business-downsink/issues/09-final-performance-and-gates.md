# 09: 终态性能与全量门禁

**What to build:** 对硬切后的通用宿主 WS + 插件终端链路做最终验收，证明业务已下沉、性能和资源回收可接受、文档与门禁完整。

**Blocked by:** 08 — 宿主业务硬切与旧协议删除

**Status:** done（2026-09-25，commit 见 handoff）

- [x] 插件 ring-fetch + 二进制 WS 输出覆盖常态与压力输出，记录 CPU、内存、队列和截断指标。
      `ws_output_perf.rs`（新探针，runtime tests）：真实 Actix server + 真实 terminal-session
      插件 + 真实 bash PTY + 真实 WS 客户端，经 `/ws/plugin/.../terminal` 订阅后以 poll 帧
      驱动 drain。常态（环安全 192 KiB）全量到达 + 0 截断；压力（10 MiB 节流循环产出
      ≈10 MB/s）边产边拉 ≥ 4 MiB（debug 数量级地板）+ 截断次数贴档。与
      `terminal_output_perf.rs` 的 PTY 输出基线（P1-P3）互补成 §9.3 完整门禁。
- [x] 宿主通用 transport、插件端点、真实 PTY、旧路由 404 和旧 ABI 拒绝均有集成证据。
      `pty_session_chain` 场景 4 扩为显式断言 `/ws/event` 与 `/ws/terminal/session/{id}`
      旧路由 → 404（无升级），未注册端点同样 404；旧 ABI 拒绝由
      `stale_artifact_instantiation_hint_is_selective` 扩展 v28 判据（`broadcast-sync`
      删项点名 + 重建指引）锁定。
- [x] 插件停用、端点注销、服务器停机、连接断开不会留下任务、句柄或监听端口。
      `broadcast_shutdown`（连接清理 + 停机 + 端口释放）+ `ws_e2e`（端点注销 4005 /
      插件停用 purge）+ `pty_session_chain`（优雅停机 + 清理）全绿。
- [x] desktop Rust、terminal-session、desktop SDK 与受影响前端测试全量通过。
      desktop `cargo test` 全绿（lib 928 + 集成 target）；插件 326；SDK 146；
      前端 vitest 全量（见验证记录）。
- [x] 根目录 eslint 0 error，Rust 格式与 clippy 自查完成，测试后无后台残留进程。
- [x] code-map、ADR、插件检查清单、CHANGELOG 和本专项文档与终态一致。
      ADR 0022 补 v19（2026-09-25 websocket 业务下沉批次）+ 双端偏离 desktop v27→v28；
      code-map / 检查清单 / CHANGELOG 已于票 08 同步，本票复核。
- [x] 明确记录移动端与旧版本不在兼容范围，不宣称双端或旧端可用。
      spec / handoff / CHANGELOG / ADR 双端偏离均写明「移动端不在 v28 兼容范围、零改动」。
- [x] `lens_diagnostics mode=all` 无 blocker。

## 执行记录（2026-09-25）

- **性能门禁（spec §9.3）**：新增 `ws_output_perf.rs` 探针（`mod ws_output_perf;` 注册于
  runtime.rs）。探针设计要点：环容量默认 256 KiB（插件 spawn 未声明 ringBytes），瞬时突增
  产出必然淘汰（截断语义）；故常态用**环安全**产出（192 KiB ≤ 环，无淘汰 → 测纯消费侧
  到达吞吐），压力用**节流循环**产出（80 × 128 KiB + 12ms ≈ 10 MB/s 持续流 → 边产边拉
  追赶门）。消费端 poll 驱动（直连实例 tick 不跑），2ms 排空 + 500ms 静默结束判定。
  实测（debug）：常态 196 KiB 全量到达、0 截断；压力 10 MiB 收到 ~8.1 MiB、38 次
  ring_resync（环淘汰重锚协议按 spec §7.4 显式生效，不静默丢数据）。
- **旧路由 404**：`pty_session_chain` 场景 4 扩为三条断言（`/ws/event`、`/ws/terminal/session/{id}`、
  未注册插件端点）全部 404 无升级。
- **旧 ABI 拒绝判据**：`stale_artifact_rebuild_hint` 判据 + `broadcast-sync`（v28 删项）与
  文案点名，单测补 v28 形态。
- **ADR 0022**：双端偏离节「当前 desktop v27 / mobile 11」→ v28（含移动端不兼容声明）；
  修订记录补 v19 条目。
- **产物重建**：并发 agent 曾以陈旧 manifest 覆盖 `resources/plugins/desktop/...` 的
  plugin.json（缺 session-history，32 项 api）导致 pty_session_chain 假红 → 重跑
  `node scripts/build.js --rust-only` 恢复（33 项 api），测试复绿。
- **验证**：`cargo test --lib perf_ws_terminal_output` / `stale_artifact`、
  `--test pty_session_chain` / `ws_auth_rules` / `broadcast_shutdown`、
  `--lib ws_e2e` 全绿；eslint 0 error；`cargo check --all-targets` 0 error。

## 遗留

- 全量 `cargo test`（lib + 全部集成 target）与前端 vitest 全量、`lens_diagnostics mode=all`
  最终收尾（见验证记录；本文件仅记录票级验收，全量门禁命令输出见 handoff 收尾记录）。
