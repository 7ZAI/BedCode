# 01: 修正三处文档失真

**What to build:** 仓库里有三处注释/文档描述与代码实际状态不符。本票把它们改对，让后续任何人（包括 agent）按文档行动时不会走错。纯注释与文档改动，**不改变任何运行时行为**。

三处失真（取证见 spec §10）：

1. **WIT `host-auth` 注释**：仍写「真源是内核表 `pairings` / `connection_history`」「v18 记录面 `trusted-devices-*`」。实际这两张表已随 ABI v31/v32/v34 退役，主库现存 5 张表；该 interface 实际是 10 条原语 —— secret-store 四条 + auth-setting + link-identity-parts + 认证中心注册/注销两条 + methods-list/invoke 两条，即**编排桥接面**而非记录面。
2. **peer-net 发现模块头注释**：自称「两套发现互不感知、可同进程共存」，但同文件内实际走的是共享守护（经 `Ports` 的 shared-daemon 注入）。注释描述的是被消灭前的状态。
3. **SDK 测试里的 inventory 调用形态待核实**：写法与宿主锁定的 inventory 0.3.x API 形态不一致（该版本 `iter::<T>` 是枚举变体而非函数）。需核实是真编译破口还是 SDK 独立 lock 解析到了别的版本。

**Blocked by:** None (can start immediately)

**Status:** done

- [x] 失真 1 的注释改写为反映实际 10 条原语与实际真源表；已退役表名从注释中彻底移除
- [x] 失真 2 的注释改为说明服务类型错开的原因 + 守护经端口层注入共享，并指向端口 trait 的定义处
- [x] 失真 3 有明确结论（编译破口 / 版本差异），结论写进本票 Comments；若确为破口则修正调用形态
- [x] 改动后逐条用检索复核「注释描述」与「代码实际」一致
- [x] 若失真 3 需改代码，则跑对应 crate 的自身测试；纯注释改动无需跑测试
- [x] 不触碰任何逻辑代码；`git diff` 仅含注释/文档行

## Comments

- 2026-10-04 实施完毕。改动仅两个文件：`bedcode-desktop/packages/plugin-sdk-desktop/rust/wit/bedcode.wit`（+37/-14）与 `packages/peer-net/src/discovery.rs`（+12/-2），`git diff --stat` 零逻辑行。
- **失真 1 处置**：`host-auth` 头注释从「secret-store + 认证记录面」改写为「secret-store + 认证编排桥接面」，逐条列出 10 条原语的真源归属（secret-store 4 条 → 主库 `plugin_secrets`；`auth-setting-set` → 主库 `settings`；`link-identity-parts` → `link_crypto`；中心注册/注销 2 条 + methods-list/invoke 2 条 → 编排桥接，执行在认证中心插件）。原注释里「真源是内核表 `pairings` / `connection_history`」已删除，并新增「已退役、不得回接」小节点名退役原语。
  - 残留说明（有意保留）：`pairings` / `connection_history` 二词仍出现在本 interface 内的 **v24 历史条目**（记录退役裁定）与 **v18 ABI 变更条目**（记录当次追加了什么）。这两处是审计轨迹，不是「现行真源」描述；删掉它们反而丢失退役证据。
  - 移动端 WIT 无 `host-auth`（desktop 独有），无需同步。
- **失真 2 处置**：模块头「两套发现互不感知、可同进程共存」改为「两套发现靠**服务类型**隔离，但**共用宿主全局共享守护**」，并写明守护句柄由宿主壳经 `MdnsPort` 端口层注入、本模块不再自建 `ServiceDaemon`（与文件内 `spawn_peer_mdns_daemon` 的收敛注释一致）；新增一条工程约束「不得依赖多实例共存，新增发现服务必须走同一共享守护」（对应 spec 票 03 的验收点：共享守护创建点仍只有一处）。同时点明本模块内仍自建守护的是 `spawn_peer_mdns_advertiser`（仅广告、全仓无生产调用方）。
- **失真 3 结论：不是编译破口，spec §10 F3 的判断本身需要更正。** 实测 inventory 0.3.24 的 `iter::<T>` 不是「枚举变体」，而是 `impl Deref<Target = fn() -> Iter<T>>` 的静态（`~/.cargo/registry/src/*/inventory-0.3.24/src/lib.rs:351-355`），因此**括号调用形态 `inventory::iter::<T>()` 与 `inventory::iter::<T>.into_iter()` 两种都合法且等价**。SDK 与宿主解析到同一版本（SDK `Cargo.lock:85-88` = 0.3.24；宿主 `Cargo.toml:127` = "0.3"）。实证：`cd bedcode-desktop/packages/plugin-sdk-desktop/rust && cargo test inventory` → `test traits::tests::test_inventory_collects_registered_plugin ... ok`。故**不改代码**。
  - 顺带确认：`peer-net` 的 `cargo doc --no-deps` 通过，本次新增的两个 intra-doc link 均解析成功（余下 8 条 warning 均为既有、与本票无关）。
- 下一步：票 02（认领 `wasm_core` 在途改动）。
