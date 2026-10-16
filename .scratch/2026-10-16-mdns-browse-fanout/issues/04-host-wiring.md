# 票 04 — 宿主接线：双端铸造订阅 + 移动旧链路迁中枢

Status: resolved
Type: task
Blocked by: 02, 03

## 目标

- **`packages/bedcode-server-peer-net/src/lib.rs`（spawn 处 `:1011-1033` 一带）**：铸造订阅——
  `ctx.ports.mdns.subscribe_type(SERVICE_TYPE)` → 转 `bedcode_peer_net::PeerMdnsBrowse { receiver, guard, request_requery: 绑 ctx.ports.mdns.request_type_requery(SERVICE_TYPE) }` → 传新 spawn 签名；`request_discovery_refresh`（`:1459`）requery 转发随 `DiscoveryDaemon::request_requery` 语义自动生效（核对注释同步）
- **桌面**：`bedcode-desktop/src-tauri/src/server/ports_impl.rs`（`HostMdnsPort` `:318` 一带）实现两个新端口方法——实现 = `bedcode_discovery_engine::subscribe_type` / `request_type_requery` 的转换（`into_parts` 形态，receiver 与守卫不得脱钩）
- **移动**：`bedcode-mobile/src-tauri/src/peer/ports.rs`（`MobileMdnsPort` `:226` 一带）同扩；`src/peer/net.rs` 注释同步
- **移动旧链路浏览迁中枢**（spec §6 D5）：`bedcode-mobile/src-tauri/src/mdns/discovery.rs:54-58,196`——
  - start 改 `subscribe_type` 并持有订阅句柄（`MdnsDiscovery` 增持守卫字段）
  - stop 改 drop 句柄（删 `daemon.stop_browse`）
  - 事件循环 `recv_timeout` 适配 flume 同签名；`RECV_TIMEOUT_SECS=1`（`system/constants/mdns.rs:6`）不变
- **既有锁保持绿**：`retired_mobile_mdns_advertise_lock`（`MdnsDiscovery` 薄壳与三命令在场）/ `peer_engine_consolidation_lock`（共享守护复用锚点在 `src/peer/ports.rs` 在场）
- 移动 `plugin/host_domains/mdns.rs:16` 红线注释复核（spec §8）

## 门禁

- `packages/bedcode-server-peer-net`、桌面 `src-tauri`、移动 `src-tauri` 各自 `cargo test` 全绿
- `cd cross-end-tests && cargo test --test mdns_health_flow`——覆盖「桌面广播 → 移动旧链路 browse（`_bedcode._tcp.local.`）发现 + stop 清列表」，正是 D5 迁移面的集成回归；两端 wasm 产物已在位（已确认）

## Comments

### 改动摘要

- **`packages/bedcode-server-peer-net/src/lib.rs`**：`start_locked` 在 `spawn_peer_mdns_daemon` 前铸造订阅——
  `ctx.ports.mdns.subscribe_type(SERVICE_TYPE)` → `into_parts()` → `PeerMdnsBrowse { receiver, guard,
  request_requery: Arc::new(move || requery_ports.request_type_requery(SERVICE_TYPE)) }`，receiver 与守卫**整份**
  移交（守卫随 `DiscoveryDaemon` 存续期持有，stop 释放即退订）；spawn 改传 5 参。`request_discovery_refresh`
  的 doc 与内联注释同步为「requery 经中枢统一重发唯一 browse + 缓存重放，两路订阅者同时收到」。
- **`packages/bedcode-server-peer-net/src/engine_ports.rs`**：端口表 `MdnsPort` 行补「浏览类型中枢订阅」说明。
- **桌面 `bedcode-desktop/src-tauri/src/server/ports_impl.rs`**：`HostMdnsPort` 实现 `subscribe_type`
  （`bedcode_discovery_engine::subscribe_type` → `into_parts` → `MdnsTypeSubscription::new`）与
  `request_type_requery`（直转引擎出口）；`use` 补 `MdnsTypeSubscription`；结构体 doc 补中枢归属说明。
- **移动 `bedcode-mobile/src-tauri/src/peer/ports.rs`**：`MobileMdnsPort` 同扩两方法（与桌面同源实现）。
- **移动 `bedcode-mobile/src-tauri/src/mdns/discovery.rs`**：旧链路浏览迁中枢——`start_inner` 改
  `subscribe_type`，新增 `browse_guard: Mutex<Option<Box<dyn Any + Send + Sync>>>` 字段托管退订守卫
  （`stop` 取走并 drop）；删除 `daemon.stop_browse(SERVICE_TYPE)` 直调（那会摘掉**整类型** listener，
  连 peer-net / 插件订阅一起杀）；模块头补中枢不变量说明。`RECV_TIMEOUT_SECS=1` 与事件循环
  `recv_timeout` 语义不变（flume 同签名）。
- **移动 `bedcode-mobile/src-tauri/peer/net.rs`**：模块头 mDNS 复用段落同步「共享守护 + 类型中枢出口」。
- **移动 `plugin/host_domains/mdns.rs`**：新增 `pub fn register_process_ports()`（进程级端口装配，幂等）+
  模块头补第二层不变量（浏览面中枢）说明；`plugin/host_domains/mod.rs` 的 `mod mdns` 可见性
  `pub(crate)` → `pub`（开机期与无头探针走同一生产入口）。
- **移动 `src/lib.rs`**：setup 的 mDNS 钩子处调用 `register_process_ports()`（早于任何 `subscribe_type`）。
- **移动 `tests/shared_artifact_probe.rs`**：`mobile_runtime()` 内调用 `register_process_ports()`
  （无头形态走生产入口，非自造替身）。
- **`bedcode-desktop/wasm-apps/terminal-session/rust/src/d10_contract_test.rs`**：`PilotMdns` 补齐 T1 扩
  trait 后的两个方法（`subscribe_type` / `request_type_requery`，同 `panic!` 桩）+ import（编译修复）。
- **`Cargo.lock`（peer-net / cross-end-tests）**：cargo 自动补 `flume` 条目（T1 给 server-base 加 flume 后
  这两个 crate 的锁未同步，属既有隐患的补齐；非手工编辑）。

### 门禁结果（全部实跑）

| # | 命令（各自 crate 根） | 结果 |
| --- | --- | --- |
| 1 | `packages/bedcode-server-peer-net`：`CARGO_BUILD_JOBS=2 cargo test` | ✅ 56 passed / 0 failed（+ doc-tests 0） |
| 2 | 桌面 `bedcode-desktop/src-tauri`：`CARGO_BUILD_JOBS=2 cargo test --no-fail-fast` | ⚠️ 37 个测试二进制全绿，仅 1 失败：`capability_crates_unit_tests_only::governed_crates_have_no_crate_level_integration_test_surfaces`（报 `bedcode-host-api-core/tests/boundary_lock.rs` 存在）——**base 既有问题**，与本改动无关：该文件由 HEAD 祖先提交 `2d6ae198f` 引入，主仓同存同败 |
| 3 | 移动 `bedcode-mobile/src-tauri`：`CARGO_BUILD_JOBS=2 cargo test --no-fail-fast` | ✅ 37 个测试二进制全绿，395 passed / 0 failed（含 `retired_mobile_mdns_advertise_lock`、`peer_engine_consolidation_lock`、`shared_artifact_probe`） |
| 4 | `cross-end-tests`：`CARGO_BUILD_JOBS=2 cargo test --test mdns_health_flow` | ✅ 1 passed / 0 failed（M-001 桌面广播→移动旧链路发现 / M-002 /api/health 探测 / M-003 stop 清列表） |

**额外跑的 crate（非本票门禁，因本票改了其一个文件）**：`bedcode-desktop/wasm-apps/terminal-session/rust`
`CARGO_BUILD_JOBS=2 cargo test --no-fail-fast` → ❌ **编译失败（EXIT=101）**。定位结论：**base 既有问题，与本改动无关**。
取证方式——把该文件临时还原成 `HEAD` 版本后 `cargo test --no-run`，同样 3 个错误（`EXIT=101`）：

- `error[E0046]`：`PilotMdns` 缺 `subscribe_type` / `request_type_requery`（T1 扩 trait 后漏补）——**本票已修**；
- `error[E0308]` ×2（`:381` HTTP `registry::register`、`:1131` WS `endpoint::register`）：实参是
  `bedcode_plugin_api::EndpointAuth`，形参要 `bedcode_server_http::wire::EndpointAuth` /
  `bedcode_server_websocket::wire::EndpointAuth`——两枚同名枚举各自成对（由
  `packages/bedcode-server-http/src/wire/drift_lock.rs` 锁漂移），测试文件用了 SDK 那枚而没转换。
  该文件与两个 `wire.rs` 的最后一次改动（`4c5958a18` / `fb519bc0c`）都是本分支 base 的祖先提交，
  即 **HEAD 上该 crate 本就编译不过**（dev 分支不触发 CI，所以无人看守）。

本票**不改**这两处 E0308：不属 T4 面（与 mDNS 中枢无关），且修法涉及「两枚 `EndpointAuth` 谁是
真源 / 该在测试侧转换还是在 server-http 侧 re-export」的取舍，按票面「设计取舍不确定则停手」列为未决项。

`cargo fmt --check` 仅对本票新增行做了对齐（其余差异为工作区既有 rustfmt 漂移，未动）；`cargo fmt` / `cargo clippy`
按 §10 属非 CI 门禁自查项。本票无前端改动，故未跑 vitest / eslint。

上表四条门禁在**格式化对齐后的最终工作树**上重跑过一遍，结果逐字一致（peer-net 56/0；移动 395/0；
桌面同一处 base 既有失败；cross-end 1/0）。

### 偏差与理由

1. **移动端新增进程级发现端口装配 `register_process_ports()`（spec §3 未列，属 T4「宿主接线」）**——
   T2×T4 交界缺口：类型中枢的泵任务经 `DiscoveryPorts::spawn` 派生，而中枢的**进程级**出口
   `subscribe_type` 取 `ports::ports()`；移动端**从不** install 全局 discovery 端口（其 `impl Host` 是
   自持 adapter，端口按插件实例传参），故任何 `subscribe_type`（老链路 / peer-net / 插件 browse）都会在
   `type_hub.rs` → `ports.rs` 处 fail-visible panic（实测 backtrace：
   `shared_artifact_probe::mobile_vocabulary_event_dispatches_into_desktop_built_artifact` →
   `engine::browse` → `subscribe_type` → `type_hub.rs:112` → `ports.rs:135` panic）。修复 = 在移动开机期
   装一次 `StateDiscoveryPorts::minimal(HostPorts::new())`（中枢只消费 `spawn`），并把 `host_domains::mdns`
   放宽为 `pub` 使无头探针走同一生产入口（与既有 `peer_ports::install(None)` 惯例一致）。桌面无此问题
   （host-kit 域装配链已装全局端口）。
2. **`d10_contract_test.rs` 的 `PilotMdns` 补桩**——T1 给 `MdnsPort` 扩了两个方法后该桩漏补，属编译修复
   （CI `test.yml` 会跑该 crate），非重构。
3. **环境修复（不入提交）**：worktree 因 `core.symlinks=false` 把 `bedcode-desktop/resources` 物化成普通
   文件导致 a03 测试失败 → 已重建符号链接 `resources -> src-tauri/resources`；桌面测试缺 wasm fixture →
   已从主仓 `target/fixtures/wasm32-wasip3/release/*.wasm`（11 个）复制进 worktree 同名目录。两者都在
   gitignore 面内，`git status` 保持干净。

### 未决问题

1. **`bedcode-desktop/wasm-apps/terminal-session/rust` 的 2 处 base 既有 `E0308`**（见「门禁结果」末段）：
   `d10_contract_test.rs` 用 `bedcode_plugin_api::EndpointAuth` 解析后直接喂给
   `bedcode_server_http::wire::EndpointAuth` / `bedcode_server_websocket::wire::EndpointAuth` 形参。
   两枚枚举由 `packages/bedcode-server-http/src/wire/drift_lock.rs` 锁漂移、语义等价，但类型不同。
   修法二选一（测试侧改用传输面枚举解析 / server 侧 re-export 或加 `From`）——建议单开一张小票处理，
   本票不动。**该 crate 的 `cargo test` 因此在本票交付时仍非全绿（与 mDNS 中枢无关）。**
2. 真机清单（spec §7 的 a-f：双端互见 / goodbye / purge 回收 / 慢消费者 / 移动旧链路页面回归 / 日志核对）
   未跑——需双端真机环境，本票在无头 + 容器内完成，`cross-end-tests` 的 `mdns_health_flow` 是本地可及的
   最大集成回归面。
3. 是否需要在 T5 的源码锁 `mdns_browse_singleton_lock.rs` 里把「移动进程级端口装配入口」也登记为
   结构性锚点，留给 T5 决定（本票不预置）。
4. spec §3 的移动改动行只列了 `peer/ports.rs` / `peer/net.rs` / `mdns/discovery.rs`，未含
   `plugin/host_domains/mdns.rs` 的进程级装配——建议 T6 的 ADR 0054 把这条补进「装配时序」段落。
