# 票 18 · host_api 实现层共享（无 WIT 依赖域「实现层 + 各端 adapter」两层化）

Status: **实施完成（2026-10-09 批次 3+4 收口 + 主库收归）：storage/bus/database/log/events 五域上移共享核；config/fs/http 判定不抽（§10）；主库由 wasm-core 管理、不给插件直接调用（`host-database` 双端退役，A1/A3 锁更新至 v19，§11）；全链路门禁绿**
专项: `.scratch/2026-10-07-mobile-wasm-core-refactor`（阶段 4 第二票）
专项: `.scratch/2026-10-07-mobile-wasm-core-refactor`（阶段 4 第二票）
依据: spec §5 票 18 + ADR 0040（选项 C 第二步：fork 面收缩到「WIT 绑定 + host_api 移动域」）+ 票 17 §7（范式统一留本票）+ ADR 0035/0037（能力域/机制核抽取先例）。
依赖: **票 17 批次 2b**（宿主切换后移动 host_impl 形态稳定）· **桌面 P5**（能力域脱绑收尾，`packages/bedcode-wasm-core` 在途）。

---

## 0. 一句话目标

把双端 host_api 中**不依赖 WIT bindgen trait / 桌面独有类型的机制实现**（storage / db / events / http / fs / config / log / bus 的实现层）抽为**共享实现核**（并入 `bedcode-host-kit` 或新建 `bedcode-host-api-core`），每域拆「实现层（机制语义）」+「各端 adapter（端口接入）」两层；桌面/移动 wasm-core 各自保留 WIT 绑定与 adapter，机制修复一次生效。

## 1. 现状实测（2026-10-08 工作区，双端形态对比）

### 1.1 桌面侧（`packages/bedcode-wasm-core/src/host_api/`，抽取阻力**小**）

- 各域文件**不含** `impl ...Host for`（bindgen trait impl 全在 `manager/runtime/component.rs` 接线）——host_api 域已是**纯逻辑层**；
- 依赖面：`crate::permission::PERMISSION_*`（机制词汇）+ `crate::host_api::sqlite_ports::SqlitePorts`（端口）+ `crate::db`（机制真源）+ `HostContextRegistry`（`context.rs` 2 处 impl）——**均属机制核/端口范畴，无 WIT 类型泄漏**；
- 桌面独有域（pty / task / crypto / auth_center / status / process / app / timer / unit_executor / sqlite_scaffold / api / platform / connection / peer / mdns / ws）不在此列（移动不跟演）。

### 1.2 移动侧（`bedcode-mobile/packages/bedcode-wasm-core/src/manager/runtime/host_impl/`，抽取阻力**大**）

- 16 域为**自由函数**（`pub(crate) fn auth_xxx(state: &WasmPluginState, ...)`）——**无 trait 形态**；
- **`WasmPluginState` 直依赖**（`state.granted_permissions` / `state.host_ctx.storage` / `state.plugin_id`）+ `super::support::guarded_host_call` + `tokio::task::block_in_place` + `state.runtime_handle`；
- 移动范式与桌面端口形态**异构**——票 17 §7 明示「范式统一留票 18」：先解耦成「纯逻辑层 + 端口」，才能与桌面实现层对齐抽取。

### 1.3 双端共享锚点

- `bedcode-host-kit`（1,110 行，module/registry/state/ports/limits/metrics）已是唯一共享机制核——新实现核的落点候选（扩展）或新建 `bedcode-host-api-core`（同族）。

## 2. 抽取边界清单（无 WIT 依赖域判定，逐域）

| 域 | 机制实现（可抽） | WIT/桌面依赖（留各端 adapter） | 判定 |
| --- | --- | --- | --- |
| `storage` | 键值语义、属主分区、系统空间纵深守卫（`SYSTEM_PLUGIN_ID`）、`plugin_storage` 表接入 | 权限词汇常量（各端 permission re-export）· 能力路由（core-plugin-manager 桌面独有，移动无） | ✅ 抽 |
| `database` / `plugin-database` | SQL 执行封装、表名前缀纵深、属主分区、护栏 | 权限位、SQLite 端口（桌面 `SqlitePorts` vs 移动 `host_ctx`） | ✅ 抽（端口分叉面） |
| `events` | 事件载荷封装、`<owner>::` 属主 topic 构造、`events-binary` 可选导出探测 | 投递通道（各端 bus/message_bus） | ✅ 抽（topic 语义单点） |
| `http` | 请求管线（egress 前判定点、redirect 重校验、JWT 注入点）、错误归一 | **egress 决策 = 宿主安全闸门**（端口注入，D5）· reqwest 形态（rustls vs native-tls） | ⚠️ 抽管线骨架，egress 端口化 |
| `fs` | 路径归一、SAF/平台桥判空、fs_auth 三层校验调用 | **fs_auth = 宿主/机制安全面**（双端形状不一致，票 17 已裁决宿主自持经 `FsAuthGate` 端口） | ⚠️ 抽纯路径逻辑，fs_auth 留端口 |
| `config` | 键值读、类型转换、默认值表 | 存储端口 | ✅ 抽 |
| `log` | 插件日志 callsite 构造、级别过滤、`plugin_id` 前缀 | 宿主 tracing 注入（端口） | ✅ 抽 |
| `bus` | 消息投递、属主 topic、订阅簿 | 各端队列/背压实现（桌面 bounded queue vs 移动 DeliveryJob） | ⚠️ 抽语义，队列留各端 |

**判据**：实现层**不得引用** `bedcode_plugin_api(_mobile)::` 类型与 `crate::plugin` 宿主胶水；只允许机制词汇（permission 常量经参数传入）、端口 trait、机制真源（db/storage 表）。逐域落地时按此判据核对。

## 3. 两层化设计（每域范式）

```text
实现层（共享核）：  pub fn storage_get(ctx: &mut dyn StoragePort, plugin_id, key) -> Result<...>
                          —— 纯机制语义：权限判定（词汇入参）、属主分区、纵深守卫、真源接入
各端 adapter（wasm-core 内）：  impl 端口 trait ← 桌面 SqlitePorts / HostContextRegistry
                                      ← 移动 host_ctx / granted_permissions / runtime_handle
WIT 绑定层（各自）：  component.rs 的 bindgen trait impl 调各端 adapter
```

- **端口 trait** 随 `bedcode-host-kit` 的 ports 先例（`HostEnginePorts` 30 方法形态已在 17b 落盘——`host_api/ports.rs`，移动侧可复用同一 trait 家族）；
- **迁移顺序**（每域一步、可回退）：桌面域 → 实现层抽出到共享核 + 桌面 adapter → 桌面零回归（ABI/WIT/world 零字节变动，同 ADR 0037 口径）→ 移动 17b 稳定后同域抽取 + 移动 adapter → 双端同域测试对齐。

## 4. 共享核落点（待用户裁决，推荐 + 备选）

- **选项 A（推荐）**：新建 `packages/bedcode-host-api-core`（与 host-kit 同族）——实现层独立 crate，双端 wasm-core 以 path 依赖引用；不动 host-kit 既有面（零回归面最小）。
- **选项 B**：扩展 `bedcode-host-kit`——少一个 crate，但 host-kit 是「机制锚点」最小面，塞入 host_api 实现层会模糊边界。
- 裁决随实施窗口一并问用户（§6）。

## 5. 并行约束（**现在禁动手的清单**）

| 在途面 | 属主 | 本票约束 |
| --- | --- | --- |
| `bedcode-mobile/packages/bedcode-wasm-core/src/manager/runtime/**` | 票 17 batch 1b/2b 并行会话（`component.rs` 2026-10-08 18:12 活跃修改） | **禁碰**——移动 host_impl 形态在宿主切换后才会稳定，现在抽取必返工 |
| `packages/bedcode-wasm-core/src/host_api/**`（桌面） | 能力域脱绑 P5（在途） | 禁碰——P5 落地前桌面 host_api 依赖面未冻结 |
| 宿主 `plugin/wasm_runtime/host_impl/` | 票 17 batch 2b（宿主切换） | 禁碰——切换后 16 域真源迁 fork crate |
| 本票可立即做 | `.scratch/` 文档（本文件）+ 双端 host_api 形态盘点 | ✅ 已落 |

## 6. 门禁（实施窗口内）

- 桌面：`packages/bedcode-wasm-core` ABI/WIT/world **零字节变动** + 桌面 `cargo test` 全量（676+1 既有 perf 红除外）+ `--no-default-features` 无头态编译过
- 移动：17b 宿主切换后 `cargo test` 全量 + 插件三方案例回归 + fork_boundary_lock / 12 退役锁回归
- 双端同域测试对齐：每域「实现层单测（共享核内）+ 各端 adapter 测试（wasm-core 内）」双份
- 新锁：共享核 crate 边界锁（实现层不得回引 wasm-core / 不得引用 WIT SDK 类型）+ 双端对照锁（票 19 Part B 联动）
- 裁决点：共享核落点（§4）+ 每域抽取顺序（§3 迁移顺序）需用户拍板

## 7. 风险

| 风险 | 吸收 |
| --- | --- |
| 移动 17b 未定形即抽 → 返工 | 严格窗口约束（§5）：17b 全绿后才动移动面 |
| 桌面抽取破坏零回归 | 每域独立一步 + ABI/WIT 零变动门禁 + crate 边界锁先行 |
| http/fs/bus 域端口分叉面大（egress/fs_auth/队列） | 这些域降级为「抽骨架 + 端口」形态，不追求逐行同构 |
| 共享核与 host-kit 边界模糊 | §4 裁决 + 文档边界清单（§2 判据）钉住 |

## 8. 批次 1 实施记录（2026-10-09，storage 域）

**裁决落定**：§4 共享核落点 = **选项 A**——新建 `packages/bedcode-host-api-core`（与 host-kit 同族不合并：host-kit 保持装配期机制锚点最小面，新 crate 承载 host_api 运行期实现层）；用户「继续实施」拍板（2026-10-09），并补记 ADR 0040 Comments。迁移顺序按 §3 桌面先行。

**新增 `packages/bedcode-host-api-core`**（机制级最小依赖 serde_json + tracing；零 tauri / tokio / SDK）：

1. `src/storage.rs`——host-storage 实现层：权限门（`PermissionGate`，权限词汇经参数传入）→ 系统空间纵深守卫 → 能力路由（`forward_kv_*` 端口默认 `None`；桌面 adapter 委托 `forward_storage_*`，移动端默认直通）→ 键值原语（serde_json 规范形，`kv_get/set/delete`）。`SYSTEM_PLUGIN_ID` 真源随实现层上移本模块（原双端各一份），双端 `storage.rs` 经 re-export 保既有路径（ADR 0037 垫片先例）。6 个实现层单测（mock 端口：门禁与词汇透传 / 往返隔离幂等 / 纵深守卫 / 路由短路两向隔离 / 非 JSON 降级 / 错误包装与转发透传）。
2. `tests/boundary_lock.rs`——crate 边界锁（§6 门禁）：needle `plugin_api` / `wasm_core` / `tauri` / `tokio` 扫描（跳注释行）+ 域文件在场断言。**变异自检 2/2**：代码行注入禁词 → 红 / 还原 → 绿；required 清单注入幽灵路径 → 红 / 还原 → 绿；注释豁免由 lib.rs 文档注释天然实证（其大量提及 tauri/tokio 而锁绿）。

**桌面 `packages/bedcode-wasm-core`**（零 WIT / ABI / world 变动；`component.rs` 绑定层与 `tests/kv.rs` 调用面零改动）：

- `host_api/storage.rs` → adapter（`SqlitePorts` → 共享核 `StoragePorts` 端口），域函数签名不变；权限拒绝文案（`permission denied`）与 `storage error: {}` 包装逐字保留——`PermissionGate.deny_error` 双端自持（桌面 / 移动文案不同是既有行为面，强行统一属行为变更，另走裁决）。

**移动 fork crate `bedcode-mobile/packages/bedcode-wasm-core`**（3 文件）：

- `manager/runtime/host_impl/storage.rs` → adapter（`WasmPluginState` → 共享核端口：granted 集权限判定 + `block_in_place` 驱动 + `guarded_host_call` panic 守卫，fallback 文本逐字一致）+ 3 个 adapter 单测（往返 wire 形 / 权限文案逐字 / 纵深守卫生效）；逻辑层签名零改动（`component.rs` 不动）。
- **行为对齐**：共享核系统空间纵深守卫自本批起对移动端生效（此前移动缺该守卫——双份漂移税实例，桌面一直有）；set 的 JSON 解析从「权限门后」移到「权限门前」（共享层签名为规范形，与桌面 component.rs 既有形态一致）——未授权 + 非法 JSON 的边缘入参错误文本由权限文案变为解析文案，授权路径零变化。

**门禁**：

| 项 | 结果 |
| --- | --- |
| 共享核 `cargo test` | 6 实现层 + 2 锁全绿 |
| 桌面 wasm-core `cargo test` 全量 | **677 绿 + 1 既有 perf 红基线**（`terminal_output_perf::perf_p2`，§6 明文豁免） |
| 桌面 wasm-core `--no-default-features` 无头编译 | 通过（3 warnings 均在 server-* 依赖 crate，非本改动） |
| 桌面 ABI / WIT / world | 零字节变动（git diff 仅 storage 面；`Cargo.lock` 经包管理器变更） |
| 移动 fork crate / 移动宿主 `cargo test` | **暂挂**——并行会话「双端共享 lib M3」（mdns 域迁 `bedcode-discovery-engine`，`.scratch/2026-10-08-dual-end-shared-libs/`）在途使 fork crate 处中间态（Cargo.toml 的 mdns-sd 已换 discovery-engine、`ports.rs`/`test_support.rs` 已改、`host_impl/mdns.rs` 未改完，编译红全在其面）；本批移动侧 3 文件在该基线下名字解析零报错，门禁待 M3 落地后补跑 |
| 变异自检 | 边界锁 2/2（见上） |

**后续批次**（本票剩余域，按 §3 顺序）：database → config / events / log / fs / http 骨架；票 19 Part B 双端对照锁联动（批次 2 的 topic 形态机制已产生「共享核 + 桌面 SDK」两份拷贝，对照锁锁定三方逐字一致）。

## 9. 批次 2 实施记录（2026-10-09，bus 域语义）

**抽取面**（§2「⚠️ 抽语义，队列留各端」落地）：topic 形态机制（`TOPIC_NS_SEP` / `API_TOPIC_PREFIX` / `REPLY_TOPIC_PREFIX` / `owned_topic` / `topic_owner` / `is_reply_topic` / `is_legacy_owner_suffix`——自桌面 SDK `host/bus.rs` 上移，**宿主侧单点**，guest 侧拷贝留双端 SDK）+ 三道门禁（命名空间 / 订阅面：回复道 + legacy / 互调门）+ 发布判定链（权限位 → 严格 JSON → 命名空间 → 互调门 → 投递）。队列与订阅簿（背压 / 派发形状）留各端。

**两端策略分叉由端口承载**（`BusPorts`）：

| 策略 | 桌面 | 移动 |
| --- | --- | --- |
| 权限位 | **无**（审计票 05：topic 形态即 ACL）→ 发布/订阅函数收 `Option<&PermissionGate>`（None） | 有（`PERMISSION_BUS` granted 集）→ `Some(gate)`，deny 文案逐字保留 |
| 互调门 | core-security 授权框架（ADR 0017 层 1） | 恒放行（WIT v17 无 host-api-call，= 既有行为） |
| 投递 | `Handle::spawn` 异步投递（无运行时上下文显性拒绝） | `block_in_place` + `guarded_host_call` |

**共享核 `bus.rs`**（+ `gate.rs`：`PermissionGate` 自 storage 提升为独立模块，两域共用）：7 实现层单测（形态闭环 / 发布门禁链逐段文本 / 无权限位跳过 / 二进制同门禁+字节透传 / 订阅面门 / 退订仅命名空间门）；边界锁 required 清单 + `src/bus.rs`。

**桌面 adapter**（`host_api/bus.rs` 重写，`component.rs` 绑定层与测试调用面零改动；`api_gate_target_owner` 留桌面——其消费者 `host_api/api.rs`（host-api-call 桌面独有域）不跟抽）：测试块经拼接保真（**与 HEAD 逐字一致已验证**），52 bus 相关用例全绿。

**移动 adapter**（`host_impl/bus.rs` 重写，逻辑层签名零改动）+ 5 个 adapter 单测。**行为对齐（移动端自本批起与桌面同文同语义，此前移动无任何总线门禁——双份漂移税实例）**：

1. 命名空间门：`<owner>::<name>` 只有属主（与宿主）可发布/订阅；
2. 订阅面门：回复道对 WASM 关闭 + legacy 定向形态显式拒绝并回带新形态；退订只过命名空间门（清理幂等）；
3. 互调门：恒放行（无 host-api-call 域，既有行为不变）；
4. JSON 严格解析：发布载荷非法 JSON 由「降级原始串 + warn」改为显性拒绝（SDK 侧收 `serde_json::Value` 序列化恒合法，实测 file-transfer 唯一 bus 消费面走公开道 `peer:discovery-refresh`——既有插件零回归）。

**门禁**：

| 项 | 结果 |
| --- | --- |
| 共享核 `cargo test` | 13 实现层（storage 6 + bus 7）+ 2 锁全绿 |
| 桌面 wasm-core `cargo test` 全量 | **677 绿 + 1 既有 perf 红基线**（另 1 例 `engine_config::incoherent_tuning` 单跑即绿——与 M3 会话并行编译争抢 CPU 的负载型 flaky，非本改动） |
| 桌面 `--no-default-features` 无头编译 | 通过 |
| 桌面 ABI / WIT / world | 零字节变动 |
| 移动 fork crate | **lib 编译通过**（adapter 面完整）；`cargo test` 暂挂——M3 会话夹具面（`component.rs` 引用其已删的 `build_test_component` 等测试构建器）编译红，门禁待其收口补跑 |
| 移动宿主 `cargo test` | 同上暂挂（同一编译图） |

**遗留与联动**：① 移动测试门禁 + 插件三方案例回归待 M3 收口后补跑（批次 1+2 一并）；② 行为对齐项（移动命名空间/订阅面门 + JSON 严格化）建议真机复验后随批转默认；③ 票 19 Part B 对照锁需覆盖 topic 形态机制三方（共享核 / 桌面 SDK / 移动 SDK 无助手——只锁共享核 ↔ 桌面 SDK）。

## 10. 批次 3 + 4 实施记录（2026-10-09，database / log / events + config-fs-http 不抽裁决）

**方向裁决（2026-10-09 用户指令）**：「以目前 wasm-core 具有的机制为准 重构移动端」——
共享核承载**桌面完整机制**（非双端共性子集），移动端 adapter 接入后自动补齐缺失机制；
前两批「行为对齐」先例（移动获得系统空间守卫 / 总线门禁）推广到全部剩余域。

### 批次 3 · database 域（以桌面机制为完整基准）

**共享核 `src/database.rs`**（机制级依赖新增 rusqlite 0.32 hooks + regex）：

1. **权限门参数化**：`PermissionGate`（permission 位 / api / deny_error）随调用传入；
2. **表名前缀校验**（正则层，尽力而为 + 可读文案）：`validate_sql_table_prefix` 返回
   核心文案（`SQL table name … does not match …`），各端 adapter 补前缀
   （桌面/移动统一 `Plugin error: `）；`extract_table_names` 合并双端正则
   （移动版字符类支持引号连字符标识符完整提取 + 桌面版 `RENAME TO` 补丁）——
   **行为修正**：引号标识符 `my-table` 提取完整名而非旧桌面版截断的 `my`（判定结果
   不变，桌面 sql_parse 测试断言同步更新并注明）；
3. **SQLite authorizer 引擎层纵深**：`with_main_db_guards` + `authorize_main_db_action` +
   `is_schema_catalog` / `names_schema_catalog` / `strip_sql_literals_and_comments` 全套上移；
4. **语句超时护栏**：`with_statement_timeout`（progress handler 硬中断，
   `PLUGIN_DB_STATEMENT_TIMEOUT_SECS = 120`）上移；
5. **结果集护栏**（行数 10_000 / 字节 32MB 双上限）+ **批次事务**（`execute_batch_on_conn`，
   内部每句裸事务拒绝 = 双端统一为更严语义）；
6. **常量真源上移**：`PLUGIN_DB_*` 四常量 → 共享核；桌面 `bedcode-server-base::constants`
   改 re-export（保 `crate::system::constants::*` 路径零改动）；
7. **实现层单测 12 例**（绑定往返 / 行字节护栏 / 列转换 / 前缀正反例 / 裸事务 /
   批次全提交回滚与内部拒绝 / authorizer 规则 / 超时触发与卸载 / 目录表检测）。

**桌面 adapter**（`host_api/database.rs` 865→≈300 行）：域函数签名零改动（component.rs
零回归）；机制 re-export 保 tests 路径；错误文案逐字保留（`database error: {}` /
`Plugin error: ` 前缀）。

**移动 adapter**（`host_impl/db.rs` 573→≈400 行）——**机制补齐（此前缺失项）**：

| 补齐面 | 此前（双份漂移税实例） | 接入后（桌面形态） |
| --- | --- | --- |
| 权限位分离 | host-database 与 host-plugin-database 同挂 `storage` | 主库 `database:main`（SDK 新词汇 `PERMISSION_DATABASE_MAIN`，2026-10-09 加，纯增量——移动 3 插件实测零 host-database 消费者，无 manifest 迁移负担）；插件库仍 `storage` |
| 引擎层纵深 | 仅正则尽力而为校验 | SQLite authorizer 逐动作仲裁（`with_main_db_guards`） |
| 语句超时 | 无（rusqlite 未开 hooks） | progress handler 硬中断 120s（`with_statement_timeout`） |
| 错误文案 | `SQL execution failed: {e}` / `table name validation failed: …` | 桌面 `database error: {e}` / `Plugin error: …` |

`sql_guard.rs` 变 re-export 垫片（保宿主 `plugin::wasm_host` 转发符号面；返回形态
`Result<(), String>`，实际消费者已改走共享核）。移动本地常量删除（共享核常量）。

### 批次 4 · log / events 域

**log（共享核 `src/log.rs`，桌面 421 行全套上移）**：统一 callsite + 按调用点缓存的
`'static` Metadata + per-plugin 级别阈值（`BEDCODE_PLUGIN_LOG=id=level`，thread_local
缓存）+ `[plugin:xxx]` 前缀格式化。target 统一为共享核中性名
`bedcode_host_api_core::plugin_log`（原桌面 target 无过滤消费者，实测）。测试基建
`capture` 随迁共享核（桌面 `log::capture` re-export 保 engine_config/engine_limits 路径）。
**移动接入**：component.rs 内联 host-log（此前裸 `tracing::*!`）改调共享核 4 函数——
获得 callsite 缓存 + 阈值过滤全套机制。

**events（共享核 `src/events.rs`，语义薄模块）**：事件载荷严格 JSON 解析
（`parse_event_payload`，H-05 fail-visible，文案与桌面逐字一致）。**移动行为对齐**：
此前非法载荷宽松降级为字符串投递（断链形态）→ 拒绝投递 + warn（WIT `host-events.emit`
无错误返回，拒绝以 warn 落日志与既有失败语义一致）。notify 不抽（桌面 = 前端事件
JSON 载荷，移动 = Android 原生通知 Kotlin 桥，语义不同属各端平台能力）。

### config / fs / http —— **判定不抽**（逐域按 §2 判据核对）

| 域 | 判据核对 | 裁决 |
| --- | --- | --- |
| `config` | 实现层直接引用各端 SDK `ConfigKey` 枚举（match 穷尽各端变体）——判据「实现层不得引用 SDK 类型」不满足；每项取值是各端平台/引擎读取（桌面 ServerSupervisor/dirs/AppConfig vs 移动下载目录解析） | 不抽（留各端） |
| `fs` | 桌面 = WSL UNC 桥 / 任务单元执行器 / core-security 三层框架（777 行）；移动 = SAF 平台 / fs_auth 端口（365 行）——各端平台接入，无共享机制语义 | 不抽（留各端） |
| `http` | 桌面已抽能力域 crate（`bedcode-server-http::plugin_binding`，宿主只剩端口 + 装配）；移动 = crate 内 `http_engine` + egress 端口——出站授权闸门双端均已留各自宿主/引擎（安全闸门四类薄壳②） | 不抽（保持已抽形态） |

### 门禁（批次 3+4 全量）

| 项 | 结果 |
| --- | --- |
| 共享核 `cargo test` | **34 实现层 + 2 边界锁全绿**（storage 6 + bus 7 + database 12 + log 8 + events 2 + 锁 2） |
| 桌面 wasm-core `cargo test` 全量 | **669 绿 + 1 既有 perf 红基线**（`perf_p2`，§6 明文豁免；log 8 测试迁共享核，数字差合理；`--no-default-features` 无头编译通过） |
| 桌面 ABI / WIT / world | 零字节变动（component.rs 零改动；`bedcode-server-base` 仅常量 re-export + Cargo.toml 依赖） |
| 移动 fork crate `cargo test --features test-support` | **285 lib + fork_boundary_lock 4 + sdk_wit_contract_locks 4 全绿**（A2 权限词汇锁随 `PERMISSION_DATABASE_MAIN` 新增自动通过：fork crate glob re-export 自动可见） |
| 移动宿主 `cargo test` | **245 lib + 全部集成目标全绿**（12 退役锁 + 既有回归） |
| 变异/锁 | 无新锁（不抽裁决以文档记录替代锁；permission 词汇新增由既有 A2 锁覆盖） |

**遗留**：① 移动端 `database:main` 权限位新增为纯增量，3 内置插件零消费者——未来
插件用主库时 manifest 须声明该位（记入插件开发检查清单）；② 移动 log/events 行为
对齐（阈值过滤 / JSON 严格拒绝）建议真机复验；③ 票 19 Part B 对照锁覆盖面（topic
形态三方）已由并行会话（ADR 0043 + ABI v18）联动推进；④ 批次 5 原计划 fs/http 骨架
因判定不抽而取消——「同一个 wasm-core」机制面收口于 storage/bus/database/log/events
五域 + 不抽裁决（config/fs/http 属平台接入）。

## 11. 主库收归 wasm-core（2026-10-09 双端统一机制决策，批次 3 收缩）

**用户指令**：「机制完善 manifest 不再需要声明和 wasm-core 机制相关的 database:main
权限 默认拥有 但是插件或者 wasm-app 自身需要数据库能力 需要声明」「主库由 wasm-core
管理 不给任何 wasm-app 和插件直接调用的方法」「不仅仅是移动端 是对于 wasm-core
双端机制的补充」。

**终态语义**：插件**无法**直接 SQL 主库（`host-database` 接口自双端 WIT 面移除）；
插件数据库能力 = **插件私有库**（`host-plugin-database`，声明 `storage` 位）。
`database:main` 不再存在（无接口可声明；「默认拥有」自然成立——主库是 wasm-core
机制内部真源：激活状态 / 审批记录 / 授权记录 / plugin_storage）。

### 改动面（双端）

| 面 | 动作 |
| --- | --- |
| WIT（桌面 / 移动） | `host-database` 接口 + world import 删除 |
| SDK（双端） | `HostDatabase` trait + `db_*` 方法删除（保留 `HostPluginDatabase`）；`PERMISSION_DATABASE_MAIN` 词汇删除（桌面原高位 / 移动批次 3 新增位同删）；ABI 桌面 34→35 / 移动 18→19（破坏性，fail-visible：旧产物实例化期 import 缺失点名重建） |
| wasm-core（双端） | `host_database::Host` impl + `add_to_linker` 删除；`host_api/database.rs`（桌面）/ `host_impl/db.rs`（移动）收缩为纯插件库 adapter |
| 共享核 `database` | 收缩为插件库面（权限门 storage / 超时护栏 / 行字节护栏 / 批次事务）；`with_main_db_guards` / `authorize_main_db_action` / `is_schema_catalog` / `names_schema_catalog` / `validate_sql_table_prefix` / `extract_table_names` 删除（regex 依赖随之移除） |
| 前端生成物 / 元数据（桌面） | `permission-vocabulary.ts/json`、`contributionKinds.ts`、locales zh/en、`plugin-sdk-fixtures/plugin.json` 的 `database:main` 条目删除 |
| 测试 | 桌面 `db_isolation`（authorizer 纵深）/ `sql_parse`（前缀校验）整文件删除；`db_faces` 改插件库单门；`db_guards` / `db_batch` 入口改插件库（scaffold 注入 `set_plugin_db`）；夹具 `test_db` 命令删除 / `test.db-roundtrip` 主库段删除；sdk_e2e 断言主库命令面退役；移动组件夹具 `version()` 18→19 |
| 锁 | 移动 A1（接口清单 17→16 import / 23→22 interfaces / ABI 18→19）/ A3（接线表删 host-database 5 函数）更新至 v19；桌面权限三副本锁 / approval 测试随词汇同步 |

**零插件迁移负担**（双端插件生态实测零主库消费者）：桌面 agent-hub 全部用
`plugin_db_*`（私有库）；移动 3 插件零 `host-database` 引用。

### 门禁（主库收归全量）

| 项 | 结果 |
| --- | --- |
| 共享核 `cargo test` | 31 实现层 + 2 边界锁全绿（database 收缩后 8 例） |
| 桌面 wasm-core `cargo test` 全量 | 648 绿 + 1 既有 perf 红基线 |
| 移动 fork crate `--features test-support` | 284 lib + fork_boundary_lock 4 + sdk_wit_contract_locks 4 全绿（A1/A3 更新至 v19） |
| 移动宿主 `cargo test` | 245 lib + 全部集成目标全绿 |

**遗留**：① 桌面 `component.rs` 词元锁测试示例仍含 `database:main`（测匹配算法，
非权限引用，保留）；② `database:main` 历史出现于文档/注释处保留为退役记录；
③ 插件开发检查清单补「数据库能力 = 插件私有库（storage 位）声明」条目；
④ CHANGELOG 双语条目留统一提交时补（工作区与并行会话 ADR 0043/ABI v18 改动混杂）。
