# 票 18 · host_api 实现层共享（无 WIT 依赖域「实现层 + 各端 adapter」两层化）

Status: **计划已定稿（2026-10-08）；实施窗口 = 票 17 批次 2b（宿主切换）+ 桌面 P5 落地后**——抽取对象双端均在并行在途，禁现在动手术
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
