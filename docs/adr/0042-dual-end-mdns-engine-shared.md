# 双端 mDNS 引擎共享：discovery-engine 去 WIT、移动端收窄薄转发、topic wire 统一

## 状态

**已实施（2026-10-09，`.scratch/2026-10-08-dual-end-shared-libs/spec.md` M1–M4）**。
用户方向指令①「mdns 也应该使用同一个 lib；如果当前的 lib 不满足双端同时引用应该修改至满足」、
指令②「mDNS 应该解绑桌面端」。移动端 ABI **不变**（17，host-mdns 函数签名零变化）；
变化仅为**事件 topic wire 字符串**（移动端向桌面终态单向对齐，ADR 0018「契约独立」维护约束
不阻止 wire 对齐——双端共有接口的增量一致原则，ADR 0019）。

## 背景

移动端 `host-mdns` 是桌面 `packages/bedcode-discovery-engine` 的**同构复刻**：
共享守护（`OnceLock<ServiceDaemon>`）/ BROWSERS+ADVERTISERS 双句柄表 / 属主仲裁 / 事件定向
投递 / purge，两端各持一份（桌面 ~1200 行引擎 + 组件绑定，移动端 fork crate `host_impl/mdns.rs`
821 行 + 宿主 `mdns/engine.rs` 62 行）。同构双份带来三个结构性问题：

1. **双守护真源**：桌面 `discovery-engine::DAEMON` vs 移动端 `crate::mdns::engine::DAEMON`。
   两套实现同步演进成本高，任何一端漂移即产生「同绑 5353 互抢多播包」类病灶（历史真机实证）。
2. **事件 wire 分叉**：桌面 `<owner>::mdns:found|lost`（`owned_topic` 属主命名空间，wasm-core
   `host_api/bus.rs` 明确 legacy 形态 retired），移动端仍 `mdns:found.<owner>` 旧点分格式——
   同一原语、两种 wire，跨端插件/工具无法通用订阅。
3. **绑定层隔离**：discovery-engine 原绑定桌面 WIT（`bindgen!`/host-kit/inventory），
   移动端整 crate 拉不动（ADR 0037 D1 / ADR 0040 面对面约束）。

能力域脱绑（ADR 0035 对表，`.scratch/2026-10-08-capability-crates-unbind-desktop/spec.md`
P2 票）恰好为共享铺路：discovery-engine 已 feature-gate 掉桌面 WIT 绑定层，
默认形态 = 纯引擎 + 端口 trait（零 WIT），任何宿主可直接引用。

## 决策

### D1 · 共享形态：改造 discovery-engine + `desktop-host` feature（不新建 crate）

`packages/bedcode-discovery-engine` 保持**一个 crate 双端同引**：
默认（无 feature）= 纯引擎机制（engine/advertiser/types/wire/ports），WIT 依赖全部 optional；
`desktop-host` feature = `bindgen!` / `HostModule` / `inventory::submit!` / `impl Host` 装配
（桌面 Cargo.toml 已带 feature）。不新建 `bedcode-mdns-engine`：治理锁（SPLIT_CRATES /
crate_boundary_lock）零扰动、桌面零结构变化、「解绑」肉眼可验（feature 边界）。

### D2 · 引擎机制归属共享 crate；移动端收窄为薄转发 + 端口适配

移动端 fork crate `host_impl/mdns.rs`（821 行）重写为**薄转发**（~230 行）：
5 条原语（`mdns_browse/stop_browse/advertise/stop_advertise/is_advertising`）与
`register_host_service/stop_host_service/purge_for_plugin` **函数签名不变**（component.rs 绑定 /
host_impl 聚合入口 / peer_net 调用方零改动），body 改为调引擎域函数；句柄表 / 事件循环 /
自播过滤 / re-announce 全部进共享引擎。移动端差异面收敛为
[`MobileDiscoveryPorts`]（实现 `DiscoveryPorts` 9 方法）：
- 权限门 = 插件 manifest `granted_permissions`（构造时结算，与既有 `check_permission(state)` 同语义）；
- `publish` = `state.host_ctx.message_bus.publish(topic, "host", payload)`；
- `local_node_id` = `host_ctx.ports.current_node_id`（现有自播回显过滤逻辑搬入适配器）；
- `spawn` = `tauri::async_runtime::spawn`（顺带消灭历史 `std::thread` + 阻塞 `recv()` 范式）；
- `forward_mdns_*` 恒 `None`（移动端无能力路由；语义 = 无提供者走引擎，与桌面逐字一致）。

`purge` / 无 state 上下文走 `minimal` 构造（bus/app 缺省，不发布、不拦回显）。

### D3 · 守护单例真源统一 + Android 多播锁平台钩子

移动端宿主 `mdns/engine.rs` **删除**——守护真源唯一化到
`discovery-engine::engine::DAEMON`（`shared_daemon()` / `daemon_if_initialized()` 公开）。
平台初始化差异走引擎新增的 `set_daemon_init_hook`（进程级一次装配，默认空）：
桌面零注册（`disable_virtual_interfaces` 仍在引擎 `init_daemon` 内执行）；
移动端 src-tauri setup 注册 Android 多播锁钩子（fire-and-forget spawn，行为与原
`mdns/engine.rs::init_daemon` 一致）。命令面 `mdns/discovery.rs`、`mdns/advertiser.rs`、
`peer_net.rs` 节点发现全部改引引擎 `shared_daemon()`。

### D4 · 事件 topic wire 统一为 `<owner>::` 属主命名空间

移动端向桌面终态单向对齐：宿主发布 / 插件订阅 / SDK 注释全部走
`owned_topic(owner, "mdns:found|lost") = "<owner>::mdns:found|lost"`。移动端总线是
精确 topic 匹配（无 legacy 解析路径），统一后零宿主门禁改动；迁移面 = fork `host_impl/mdns.rs`
（已随 D2 自动统一，publish 归引擎）+ wasm-app `file-transfer/rust/src/lib.rs` 订阅真源
（`format!("{PLUGIN_ID}::mdns:found")`）+ 双端 SDK/WIT 注释。**非 ABI 变更**（host-mdns
函数签名不变），移动端 ABI 17 保持；旧的 `mdns:found.<owner>` 订阅者收不到新事件——旧
插件需随此 wire 变更同步升级订阅字面量（本仓内唯一消费方 file-transfer 已同批迁移）。

### D5 · HostEnginePorts 退役 mdns 守护三方法

移动端 fork `HostEnginePorts::{mdns_daemon, mdns_daemon_if_initialized,
mdns_reannounce_interval}` 退役（守护真源移出端口层，端口不再承担「守护投影」职责）；
`current_node_id` 保留（薄转发端口适配器自播回显过滤仍在用）。同步删除
`test_support::MockPorts` 的 mdns 字段与 `with_mdns_daemon`，fork Cargo.toml 移除 `mdns-sd`
直接依赖（经引擎传递使用）。

### D6 · `register_host_service` 移除端口参数（NullTask 占位）

引擎 `register_host_service(ports, service_type, fullname)` → `(service_type, fullname)`：
宿主身份登记行不挂续期任务，「空任务占位」改为引擎内 `NullTask`（cancel no-op），
不再需要端口对象执行 spawn——消除移动端 peer-net 登记（无端口上下文）的适配负担，
桌面 `MdnsPort` 实现同步简化。行为不变（owner=host 登记行仍参与表内生命周期可见性）。

### D7 · peer-net 删除 `spawn_peer_mdns_advertiser`

`packages/peer-net/src/discovery.rs::spawn_peer_mdns_advertiser` + `DiscoveryAdvertiser`
删除：零生产调用方（仅 crate 自身导出）+ 自建 `ServiceDaemon` 违反单守护红线；
广告面由宿主自播（owner=host）/ 插件 `host-mdns` advertise 覆盖。
`disable_virtual_interfaces` 保留（peer-net 导出，供引擎初始化一次）。

## 影响面

- **共享引擎** `packages/bedcode-discovery-engine`：+`set_daemon_init_hook` /
  `pub daemon_if_initialized` / `NullTask`；`register_host_service` 去端口参数（桌面
  `bedcode-desktop/src-tauri/src/server/ports_impl.rs` 调用点同步）。
- **移动端**：fork crate Cargo.toml（+discovery-engine、-mdns-sd）、`host_impl/mdns.rs`
  薄转发重写、`host_api/ports.rs` 退役 3 方法、`test_support.rs`、宿主 `mdns/engine.rs`
  删除、`mdns/{discovery,advertiser}.rs` + `peer_net.rs` 改引引擎、`host_ports.rs`、
  `lib.rs` setup 注册多播锁钩子。
- **wasm-app**：移动端 `file-transfer/rust/src/lib.rs` 订阅 topic 字面量迁移。
- **SDK/WIT**：移动端 SDK `host/mdns.rs`、`abi.rs`、`wit/bedcode.wit` 注释契约同步
  （topic 描述改为 `<owner>::` 终态；ABI_VERSION 不变 17）。

## 验证

- 桌面：discovery-engine 31 tests 全绿；`bedcode-desktop/src-tauri` cargo check 零 error。
- 移动端 fork crate（`bedcode-wasm-core-mobile`）：`cargo test --features test-support --lib`
  **290 全绿**（含薄转发 3 断言：权限门 fail-closed / host 登记属主仲裁 / purge 空表）。
- 移动端宿主：`src-tauri cargo test --lib` **245 全绿**；peer_net 8 通过。
- peer-net：101 tests 全绿。
- 门禁：移动端全仓 `mdns:found.`/`mdns:lost.` 字面量归零（仅历史日志与刻意保留的
  演进说明）；移动端 `ServiceDaemon::new()` 代码层归零；wire 等价单测
  `owned_topic("plugin-a", MDNS_FOUND) == "plugin-a::mdns:found"`（engine.rs 单测）。

## 与既有 ADR 的关系

- ADR 0035（能力域 crate 化）：终态延续，共享落点即 feature-gate 后的能力域 crate。
- ADR 0037 D1 / ADR 0040：移动端不可拉桌面 WIT 的约束未被破坏（共享引擎零 WIT 形态）。
- ADR 0018（移动契约独立）：host-mdns 函数签名不变，仅 wire topic 对齐，不构成契约变更；
  移动端 fork crate 仍自持 `HostEnginePorts`（仅退役守护投影三方法）。
- 本 ADR 不触发 ABI bump（host-mdns WIT 面零变化）。