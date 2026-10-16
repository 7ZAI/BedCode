# SQLite 能力域 crate 撤销：插件面数据库机制留在 wasm 核心（host-database / host-plugin-database / host-storage）

## 状态

**已实施**（2026-10-05，桌面端；**ABI / WIT / 协议零变动**：三个 interface 的函数名、
权限位、错误文案、结构化日志字段逐字未变）。
撤销对象：`.scratch/2026-10-04-wasm-core-lib-split/spec.md` 的**票 07 / 08**，
以及本目录下 `bedcode-desktop/packages/bedcode-sqlite-engine/` 这个 crate 本身。
边界判据的单一事实源仍是 **ADR 0022**；本 ADR 只裁决**这一域的机制面归属**，
不改 B1–B6 判据，也不动 ADR 0035 的机制内核与另外四个能力域。

## 背景

票 07 / 08 把两样东西放进同一个 crate：

1. **引擎面** —— `src-tauri/src/db/`（连接管理、`schema.sql` 主库单一事实源、幂等迁移、
   设置项查询）；
2. **机制面** —— `host-database`（5）/ `host-plugin-database`（5）/ `host-storage`（3）
   共 13 条原语：权限门位置、主库表名前缀纵深（SQLite authorizer）、语句超时 / 行数 /
   字节护栏、批次事务语义、插件私有库句柄、kv 属主分区与系统空间 fail-closed 守卫、
   能力路由。

实施后复查发现归属出现**两个答案**，且第二个答案是错的：

- 这三个 interface 的**真源本来就在宿主主库里**：`plugin_auth_policies` /
  `plugin_auth_records` / `plugin_secrets` / `plugin_storage` 四张表由引擎面的
  `schema.sql` 建，宿主 13 个文件（`security/{auth_policy,approval,strategy,network_auth,
  fs_auth}`、`wasm_core/storage.rs`、`host_api/{auth,context}`、`manager/{validation,
  host/api_bridge}`、`frontend_channel.rs` 等）经 `Database` 直读直写；
- **授权判定的权威**也在宿主（`PermissionManager` + `host_api::check_permission`），
  机制面只是经端口问结果。

于是「插件的授权信息与插件信息保存在哪、谁能读写」这一组问题的答案被切成两半：
数据在宿主侧读写，判定与纵深守卫在另一个 crate。ADR 0022 的 B3（业务真源）与
「宿主只回答机制怎么做」的判据都指向同一个要求：**机制实现与机制真源同侧**。
对照另外四个已迁出的域（mdns / websocket / peer / http）——它们的真源是各自引擎里的
匿名资源（连接、信道、节点、端点），宿主不留读路径；sqlite 域不是这种形状。

另外两条实测代价（撤销的直接收益）：

- **依赖图膨胀**：能力 crate 无条件依赖 `wasmtime`(component-model) + `wit-bindgen` +
  `bedcode-host-kit` + `bedcode-plugin-api` + `inventory`，且 `plugin_binding` 无 feature
  门 ⇒ 只想要 `Database` 的消费方（宿主 17 个文件）也被迫编组件模型栈。实测 crate 图：
  撤销前 387 个 crate，引擎面单独只需 293 个，多出的 ~94 个全是
  `wasmtime* / cranelift* / wasm-tools / wit-bindgen* / wasmtime-wasi`。
- **编译单元与装配面**：改 WIT（插件 ABI）会强制重编引擎 crate；`bindgen!` 的 provider
  侧生成与宿主 guest 侧生成出**同名但不同类型**的 `Host` trait，于是宿主必须同步删掉
  自己那三组 impl 才不报 `defined twice`——一个纯 ABI 编辑被绑上一次引擎重建 + 两侧
  同步改接线。

## 决定

**D1｜机制面三 interface（13 原语）留在 `wasm_core/host_api/`**，与 ADR 0022 撤销前的
归属一致：`database.rs`（主库 5 + 插件私有库 5）、`storage.rs`（kv 3）。
`impl … Host for WasmPluginState` 三块回到 `manager/runtime/component.rs`，
`add_to_linker` 的内核本地表逐行列举这三组（与文件里其余各域同形）。

**D2｜引擎面也回到宿主**（`src/db/`），`bedcode-sqlite-engine` crate **整体撤销**。
`schema.sql` 仍是主库唯一事实源（AGENTS §9 的路径随之改回 `src-tauri/src/db/`）。
理由：引擎面只有宿主一个消费者，而它承载的正是上面那四张插件机制表的 DDL——留一半
在 crate、另一半的概念在宿主，等于把「schema 单一事实源」写成两处。

**D3｜端口 trait 保留，但降级为「可测性缝」**。`sqlite_ports::SqlitePorts`
（权限门 / 主库与私有库句柄 / 唯一那份同步↔异步桥 / kv 存储与能力路由）留在
`host_api/sqlite_ports.rs`，生产实现在 `host_api/sqlite.rs`。理由：域逻辑因此能在
**不构造完整 `WasmHostContext`** 的前提下用假端口（`host_api/sqlite_scaffold.rs`）
跑护栏、表名前缀纵深、批次事务、kv 隔离与系统空间守卫——这 40 个用例（1064 行）在
crate 形态下已存在且有效，随实现同迁，不是新写的覆盖率。端口**不再是架构边界**：
- 无进程级 `OnceLock` 单例、无实例级 `domain_ports` 登记、无 `inventory` 自报、无
  强制引用行、无 `HOST_MODULES` 白名单项；
- 端口按**本次调用**的上下文现取（`sqlite::ports_for(&ctx)`，零分配、借用而非持有
  `Arc`），`plugin_db` 的懒创建 future 借用上下文（与既有
  `DbScope::get_or_create_plugin_db` 同一形状），故不再需要 crate 形态那份
  「装配期捕获 `Arc<WasmHostContext>`」的手段；
- 端口方法去掉 `'static` 约束、`plugin_db` 带生命周期参数——这是「同 crate 内借用」
  成立的必要条件。

**D4｜fail-visible 三形态不降级**（AGENTS §5.1.4）：契约字眼、权限位、错误文案、
`Host` impl 与 `add_to_linker` 行数全部回到撤销前的原样与原位；实例化期缺 import 仍是
`component imports instance … but a matching implementation was not found in the linker`
（`test_loaded_plugin_component_roundtrip` 就是这条断言的日常门禁）。

**D5｜ADR 0035 的机制内核与另外四个能力域不变**（mdns / websocket / peer / http 仍在
各自 crate）。本 ADR 只否决「sqlite 这一域的机制面出内核」这一个形态。**是否把同一
判据推广到另外四域不在本 ADR 范围**——它们与 sqlite 域的差别是「真源是否留在宿主」，
需逐域核，属独立立项。

## 后果

- **归属回到单一答案**：授权信息（`plugin_auth_*` / `plugin_secrets`）与插件信息
  （`plugin_storage`）的「存在哪 / 谁能读写」全部落在 `src/db/`（表与迁移）与
  `wasm_core/host_api/`（判定与原语）两侧同属宿主，且与 AGENTS §5.1「无业务内核」不冲突
  （这些是机制状态与安全闸门，不是产品语义）。
- **锁与白名单同步收缩**：crate 边界锁去掉 `bedcode-sqlite-engine` 的 4 项登记；
  `HOST_MODULES` 白名单去掉 `sqlite`（强制引用行 `use bedcode_sqlite_engine as _;` 一并删）；
  可路由能力闭表锁的 `host-storage` 端口来源改指 `src/wasm_core/host_api/sqlite_ports.rs`。
  锁**没有变成不检查**——它现在断言的是「sqlite 域的实现确实在宿主 `component.rs` 与
  `host_api/` 内」，能力域自报集合少一项仍会被 missing 方向捕获。
- **测试面不变**：引擎面 4 个用例（含退役表不重建）、机制面 40 个用例（5 个分组文件）
  全部随实现同迁，用 `cargo test --lib` 在宿主 crate 根跑（不再有「宿主 `cargo test`
  不覆盖该域」这条缺口）。
- **代价**：`wasm_api` 层失去了「新增能力域 = 一条依赖 + 一个白名单项」的样板收益，
  换回「机制与真源同侧」的单一归属。若日后要为这三条原语做独立的产物裁剪或跨端复用，
  正确形状是搬**引擎**（连同 schema 真源一起）而不是搬机制面。

## 验证

- `cargo check --lib --tests` 通过；`cargo clippy --lib --tests` 无新增警告（`database.rs`
  / `storage.rs` 的 `dead_code` 是这条判据的固有噪声：生产面唯一调用方是 `bindgen!`
  生成的 `Host` trait impl，链接器在运行时调用，lint 看不见——与撤销前同形）。
- `cargo test --lib`：`wasm_core::host_api::database` 32 / `host_api::storage` 4 /
  `db::` 4 / `component::tests` 20（含 roundtrip）/ `crate_boundary` 8 /
  `capability::tests` 6 / `empty_dir_lock` 4 全绿；全量仅余既有的
  `session_e2e::test_session_task_domain_closed_loop` 一红（票 06/07/08 已在 CHANGELOG
  记录的既存失败，与本 ADR 无关）。
- 集成测试：`src-tauri/tests/` 五个 target 的 `use` 改指 `bedcode_desktop_lib::db`，
  编译通过（`cargo check --tests`）。
- 未跑：`cross-end-tests`（无跨端协议改动）、wasm 应用完整构建与 `gen/android` gradlew
  （无插件 / Kotlin 改动）。