# 04: 宿主胶水剥离（AppConfig / auth_center / session_gateway / HostBusPort / 注册表 / PeerCtxProvider 端口）

**What to build:** 见 spec §6（行内）与对应 spec 章节。

**Status:** done（与 02 合并执行——编译闭包不可分割，见 issues/01 gate 裁定）

## Comments

- 2026-10-06 实施全部六项：
  1. **AppConfig（M7，D6 定案随迁）**：`system/config.rs` 入 crate，lib `system.rs` 垫片；`server/host_port.rs` / `ports_impl.rs` / `commands.rs` 消费经垫片零改动。
  2. **auth_center / session_gateway / test_tokens（M8-M10）**：入 crate `utils/`；lib `utils.rs` / `utils/auth.rs` 改 `pub use` 垫片。
  3. **HostBusPort（M11）**：从 lib `server/ports_impl.rs` 抽出入 crate `bus.rs`；lib `ports_impl.rs` 改 `pub use bedcode_wasm_core::bus::HostBusPort;` 垫片，`assemble()` 本体留 lib。
  4. **AppContext → 宿主上下文注册表**（`crate::host_context_registry`，`OnceLock<Weak<WasmHostContext>>`）：装配点在 `install_capability_domain_ports`（单入口纪律）；boot/errors 改 `self.wasm_host_ctx().app_handle()`；mdns adapter 改注册表取 ctx。
  5. **PeerCtxProvider 端口（§3.3）**：`WasmHostContext` 加 `peer_ctx_provider` 字段（两阶段注入同 `set_task_engine`）；`PluginHost::new` 增第 5 参 `Option<Arc<PeerCtxProvider>>`（lib.rs 传 `Some(peer_net_cmds::peer_ctx)`，全部夹具传 `None` → HEADLESS_UNAVAILABLE 语义不变）；mdns / peer / activation 三个消费点改走端口。
  6. **测试 harness（§4.5）**：`crate::host_harness::start_http_server`（`#[cfg(test)]`），ws_e2e / ws_output_perf 5+1 处改用它。
- **watcher.rs 特例**（spec §2.3 未列的第四消费点）：`PluginDevWatcher::start` 增 `Weak<PluginHost>` 参（lib bootstrap 注入 `Arc::downgrade(&plugin_host)`），不再经 `AppContext::global()`。
- **可见性修正**（spec §3.1 只列 runtime_util，实测还需 4 处，均已改 `pub`）：`runtime_util::ambient_handle`、`host_api::pty::{kill_all_registered, live_count}` + `mod pty/pty_output`、`WasmHostContext::{message_bus 字段, net_auth 方法}`。
- **test_tokens 可见性**：crate 内须**常编译**（不带 `#[cfg(test)]`）——依赖 crate 的 cfg(test) 项对 lib 集成测试不可见；`test_seed_plugin_secret` 同样去 cfg(test)。
- **锁扫描面**：l2_gating L2_SCAN_ROOTS 增 `../../src-tauri/src`（宿主壳留 lib）；api_bridge / wasm_flow_test 的 `server_lib_src_roots()` 改走 crate 侧 `crate_boundary_lock`（镜像表，票 05 收口）；各结构锁 `src/wasm_core/...` → `src/...` 批量修正。

## Blocked by
- 01（已完成）

## 验证
- 无头测试语义逐字不变：crate `cargo test --lib` 784 passed / 2 failed（两个失败均为**既有基线**：`test_session_task_domain_closed_loop` 9-30 起红 + `perf_p2_guest_ring_fetch_batch_curve` 墙钟阈值 flake）；lib `cargo test --lib` 76 passed / 0 failed
- `PluginHost::new` 签名扩展已同步：lib.rs + crate 内 6 处 + 集成测试 5 处 + cross-end-tests desktop_ctx.rs
