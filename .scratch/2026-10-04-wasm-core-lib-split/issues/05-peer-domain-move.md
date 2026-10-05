# 05: peer 域迁入既有对等网络 crate，并解除对宿主组装面的耦合

**What to build:** 对等网络能力（19 条原语）搬进 `bedcode-server-peer-net`（传输引擎约 4,171 行已在；更底层的节点身份/证书/传输在仓库根 `peer-net` 约 9,919 行，也已在）。

本票与 04 的差别在于：除了搬家，还要**解除一个反向耦合** —— 该域的宿主绑定层目前有 20 处靠「从宿主组装面取引擎上下文」来拿到引擎状态。本票把它改为注入式端口，使该绑定层对宿主组装面的引用降到 0。

搬完后该域也成为一个通过机制内核装配进来的能力域。

**Blocked by:** 04

**Status:** done（2026-10-05 实施，见下方实施记录；「插件 import 未注册接口的实例化期点名」由 wasmtime 的 import 缺失错误天然满足，宿主侧未新增专门文案——与票 04 同形）

- [x] peer 19 条原语绑定层完整迁入 `bedcode-server-peer-net`，逐字保留签名 / 返回值 / 错误串 / 结构化日志字段
- [x] 引擎上下文的获取改为注入式端口；该域绑定层对宿主组装面的引用 **20 → 0**
- [x] 只改绑定层的取法（新增端口 trait + 在组装处实现），**不动对等网络引擎自身**
- [x] 引擎归属维持现状：绑定层在 `bedcode-server-peer-net`，更底层引擎留仓库根 `peer-net`（本轮不搬引擎）
- [x] 传输任务 / 接收 / 远端四类引擎状态的装配与生命周期不变；插件停用时其传输与句柄回收语义不变
- [x] 插件若 import 未注册接口，实例化期点名（沿用 04 建立的通道）
- [x] `bedcode-server-peer-net` 自身测试 + 桌面全量 + 对等传输编排退役锁 + crate 边界锁全绿
- [x] `cargo fmt` / `cargo clippy` 干净（**部分**：三个 crate / 宿主改动的文件干净；宿主 `cargo clippy --lib` 因磁盘见下方记账未跑；peer crate 引擎侧 3 条既有 warning 见记账）

## Comments

### 实施记录（2026-10-05）

#### 落点与形态

| 项 | 结果 |
| --- | --- |
| 能力域实现 | `bedcode-server-peer-net/src/plugin_binding.rs`（**单文件逐字搬迁**，727 → 约 720 行，含接线段）+ `plugin_binding/ports.rs`（端口边界）+ `plugin_binding/tests/{scaffold,gates,handle_table,payload_contract}.rs` |
| 宿主残留 | `wasm_core/host_api/peer.rs` 112 行（`HostPeerPorts` + `install` + 白名单常量），原 727 行 |
| crate 依赖新增 | `bedcode-host-kit` / `wit-bindgen =0.60.0` / `wasmtime 48` / `inventory`（与 ws 域同款） |
| ABI / WIT | **零变更**（19 条原语签名、权限位 `peer`、`abi_min = 31` 与既有一致） |

#### 端口面（3 个方法）

| 方法 | 为什么必须经端口 |
| --- | --- |
| `check_permission` | 权限门属宿主安全闸门（AGENTS §5.1.3 四类薄壳之二），复用既有 `host_api::check_permission`（同一 PermissionManager + 同一条 warn 路径） |
| **`peer_ctx`** | **本票的核心交付**：迁移前 20 处 `crate::server::peer_net_cmds::peer_ctx(&app)`，现在要「已装配好的 `PeerCtx`」。装配动作（从 Tauri managed state 取四个引擎句柄）留宿主 adapter，本 crate 对宿主组装面的引用为 **0** |
| `block_on_any` | 同步↔异步桥**必须复用宿主那份**（含 actix `current_thread` 自锁规避与 ambient runtime）；域内复制第二份即是埋雷 |

`HEADLESS_UNAVAILABLE`（「peer-net unavailable in headless context (no app_handle)」）
定义为能力域常量、宿主 adapter 引用它：这条 wire 文案逐字保留且单一事实源在域侧。

#### 两处「判定顺序」是行为契约，逐字保留并加锁

1. **属主判定先于取引擎上下文**：`peer_close` 里非属主拿到的必须是属主拒绝，
   而不是先撞上无头错误（否则越权探测结果随机化）。原有用例
   `peer_close_by_non_owner_is_denied_before_app_check` 迁入后补了句柄仍在册的断言。
2. **`send-files` 的退役字段检测先于句柄寻址**：`concurrency` 脉冲字段（v31 退役）
   出现即显性报错点名重建——这是「传输编排下沉票 3」fail-visible 的行为级保险，
   宿主 `retired_peer_transfer_orchestration_is_not_reintroduced` 锁注释里指向的
   「另一保险」位置随之改指能力域（注释同步更新）。

#### 判据自校：逐字保留是脚本比过的

HEAD 版 `host_api/peer.rs` 与新 `plugin_binding.rs` 的 wire 文案集合比对：
**19 条 `host_peer_*` api 名完全一致（差集空）**、20 条 wire 字符串共有、
新文件**未引入任何新 wire 文案**；结构化日志两处（`plugin_id`/`handle` 的 warn、
自动重拨的 `node_id` info）逐字一致。

#### 一处必须记账的「锁教育」

本 crate 自带的 `dependency_direction_lock`（票 06 写的）对
`wasm_core::` / `tauri::` / `crate::server::` 等前缀**连注释一起禁**。首轮实现把
「自宿主 `X` 迁入」这类**记账性说明**写进了文档注释，锁直接红了 14 处。
处置是**改散文、不放宽锁**（锁的价值正在此处）：文档改说「宿主 host_api 域的 peer
适配器」「由 AppHandle 装配引擎上下文的调用」。这与票 04 在 ws 域的宽松口径不同
（ws 锁只禁 `crate::server::{http,websocket,core}` 三个子路径），属**面内锁强度差异**，
不是本票引入的偏差。

#### 测试（17 条：9 条逐条迁入 + 8 条新增）

- `handle_table`（6 条）：句柄表与属主仲裁，逐条迁入。
- `gates`（5 条）：权限门 2 条迁入 + 新增 3 条（无头文案逐字锁定、属主可达引擎侧的
  正例、非属主 close 后句柄仍在册）。
- `payload_contract`（6 条，新增）：退役 `concurrency` 字段报错、双形态载荷过校验、
  非法 JSON 文案逐字、未知句柄文案、属主判定先于载荷解析、
  **`collect-outgoing` 是唯一不取引擎上下文的原语**（无头亦可用，且权限门仍生效）。

#### 验证台账

| 套件 | 结果 |
| --- | --- |
| `bedcode-server-peer-net`（`cargo test --lib`） | **48 passed**（31 基线 + 17 迁入/新增） |
| 宿主 `cargo test --lib` | 见交付说明（876 = 票 04 账目 885 − 迁走的 9 条）✅ 逐条对齐 |
| `capability_registry_matches_whitelist` | 绿（白名单 2 → 3 个能力域，双向断言含 missing 方向） |
| `bedcode-host-kit` | 未改（本票零改动），票 04 的 11 绿沿用 |

#### 记账（需要交代的事）

1. **磁盘**：开工时根分区 100%（1.5GiB 可用），两个 `src-tauri/target` 都超 AGENTS §3
   的 15GB 阈值。清了 `bedcode-mobile/src-tauri/target`（9.3G）与桌面
   `target/debug/incremental`（8.5G）才腾出空间。**`bedcode-mobile/src-tauri/target`
   当时正有一个并发会话在跑 `cargo test --lib`**——删除动作发生在它启动之后，
   它因此从头重编（最终 366 绿）。教训：清 target 前先 `ps` 看有没有别人的
   cargo 在跑，那次是**先删后看**。
2. **未跑**：宿主 `cargo clippy --lib`（磁盘不足以全量重编）；`cross-end-tests`
   （跨端协议零变更，本票不适用）；wasm 应用完整构建（无插件侧改动）。
3. **peer crate 引擎侧 3 条既有 warning 未修**（`peer_engine_receive.rs` 死字段/死函数、
   `peer_engine_transfer.rs:395` 的 `drop(&T)` 空操作）：HEAD 即存在，且本票验收项
   明确「不动对等网络引擎自身」——`drop(state)` 那处的真实意图需要引擎侧判断，
   应当单独立票而非顺手带过。
4. **顺手消掉 peer crate 的 2 条既有 warning**（`SharedDirRoot` 只被 test 用的 import、
   `start_node_owned` 的多余 `mut`）：crate 本轮成为能力域后应自身干净，两处均为
   编译期修正，测试全绿。
