# 票 03 · mDNS 单守护收敛（阶段 1 第一票）

> 状态：**已完成（2026-10-07）**。门禁结果见 §5；真机双端互连项未跑（原因见 §5）。

## 1. 目标与本票实际范围

spec 票 03：「`mdns/advertiser.rs` + `discovery.rs` 双实例 → 复用根
`packages/bedcode-discovery-engine` 单 `ServiceDaemon`（消灭双 daemon 同绑 5353
病灶）；`host_impl/mdns.rs` 对齐桌面属主定向事件」。

**核验修正（实测）**：
- `host_impl/mdns.rs` 的「与桌面同构单守护 + 双句柄表 + 属主定向事件
  （`<owner>::mdns:found|lost`，payload 增量 serviceType/browserId）」已在 spec v2
  ticket 06 **落地**（本票无动作，仅验证）。
- 真正的病灶 = 宿主命令面 `mdns/discovery.rs` + `mdns/advertiser.rs`（前端设备发现/
  广播用），每次 `start` 都 `ServiceDaemon::new()`——三个守护面并存（宿主发现 +
  宿主广播 + 插件面单例）。
- 「复用根 discovery-engine」的正确落点：discovery-engine 是**能力域 crate**（含桌面
  WIT bindgen + inventory），移动端整 crate 拉不动（ADR 0018/0037）——复用它的
  **引擎机制**（单守护模型）。本票把守护提升为移动端引擎资产
  `crate::mdns::engine`（与桌面 engine.rs 同构，票 17 抽取共享核时两端对齐）。

## 2. 改动清单（6 文件）

| 文件 | 改动 |
| --- | --- |
| `src/mdns/engine.rs`（**新建**） | 全局唯一 `ServiceDaemon`（OnceLock）+ init（含 Android 多播锁 fire-and-forget）+ `daemon()`/`daemon_if_initialized()`/`shared_daemon()` + REANNOUNCE_INTERVAL。注释写明「全仓守护创建点只有此一处」红线 |
| `src/mdns.rs` | 入口加 `pub(crate) mod engine;` |
| `src/plugin/wasm_runtime/host_impl/mdns.rs` | 删本地 DAEMON/init/daemon/daemon_if_initialized/shared_daemon/REANNOUNCE_INTERVAL 定义，改 `use crate::mdns::engine::{...}`；模块头注释更新 |
| `src/mdns/discovery.rs` | `MdnsDiscovery` 不再持有/新建守护：`start_inner` 用 `engine::daemon().browse()`；`stop` 只 `stop_browse` 不 `shutdown`；删 `daemon` 字段 |
| `src/mdns/advertiser.rs` | `MdnsAdvertiser` 同上：`start` 用 `engine::daemon().register()`；`stop` 只 `unregister` 不 `shutdown`；删 `daemon` 字段 |
| `src/peer_net.rs` | `shared_daemon()` 引用从 `host_impl` 改 `crate::mdns::engine`（顺带消除宿主引擎对插件面模块的跨层引用）；`register_host_service`/`stop_host_service` 留 host_impl（句柄表属插件面） |

**行为不变**：前端契约（5 命令 + `mdns_service_found/resolved/removed` 事件形状）零改动；
插件面 host-mdns 原语语义逐字保留。

## 3. 为什么是 `crate::mdns::engine` 而非直接引根包

- 根 `packages/bedcode-discovery-engine` = 引擎机制 + **桌面 WIT 绑定**（bindgen + inventory
  自报 + HostModule）。移动端依赖它会拉进桌面 WIT 类型（ADR 0037 D1 阻塞）。
- 本票把守护提升为移动端**引擎资产**（位置正确：引擎机制不在插件面模块里），
  为票 17「无 WIT 依赖域抽共享核」（engine.rs 机制层抽根共享）铺好位置——届时
  `mdns/engine.rs` 与桌面 `discovery-engine/engine.rs` 对齐后抽共享，移动端不再自持。

## 4. 防回接锁（§5.1.4）

- 注释红线：engine.rs 模块头「全仓共享守护创建点只有 init_daemon 一处，切勿另建」。
- 结构证据：`mdns/discovery.rs` + `advertiser.rs` + `host_impl/mdns.rs` 三处均无
  `ServiceDaemon::new()`（rg 验证，见 §6）。

## 5. 门禁结果

| 门禁 | 结果 |
| --- | --- |
| 移动端 `cargo check` | ✅ 0 error 0 warning（6.25s 增量） |
| mdns 针对性单测（`cargo test --lib mdns`） | ✅ **11 passed / 0 failed**（含权限门、属主仲裁、purge、跨插件隔离、host/插件共存、自播过滤、表有界） |
| peer_net 回归（`cargo test --lib peer_net`） | ✅ 8 passed / 0 failed |
| `cargo fmt --check`（改动文件） | ✅ 干净（全仓 diff 仅 auth/manager.rs——**并行会话在途改动，未碰**） |
| `cargo clippy --lib` | ✅ 无本任务新增警告（剩余为既有基线：new_without_default×2 / map_identity / while_let_loop，最小改动原则不动） |
| **双端 mDNS 行为等价（对等网络集成测试）** | ⚠️ **未跑**：需要双端真实互连（桌面 + 移动真机/模拟器同网段），本机无移动端环境；单守护模型行为由插件面 11 测试 + peer-net 回归覆盖，真机验证列入收尾全量验收（票 20） |

## 6. 结构锁证据

```
$ rg -n 'ServiceDaemon::new' src/mdns src/plugin/wasm_runtime/host_impl/mdns.rs src/peer_net.rs
（无输出——全移动端守护创建点唯一：src/mdns/engine.rs）
```
