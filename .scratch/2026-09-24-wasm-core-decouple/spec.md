# wasm_core 解耦重构（依赖单向化）

Status: **draft**（工单 01–08 已拆分下发，待逐票执行）
Date: 2026-09-24
范围: **仅桌面端**（`bedcode-desktop/src-tauri/src/wasm_core/`）；不涉 WIT / ABI / 权限词汇 / 跨端协议，无移动端影响
关联: `docs/adr/0022-plugin-host-interface-primitive-boundary.md`（裁剪线）、`wasm_core.rs` facade 注释（「模块间协作只经本 facade 再导出或 trait 注入，禁止新增横向耦合」）
任务路由: AGENTS.md §4「改 Rust 后端」+ §5「架构红线上内聚低耦合」

---

## 1. 动机与目标

### 1.1 诊断结论

`wasm_core/`（28,300 行 / 72 个 rs 文件）当前存在模块间**双向依赖环**，违反 facade 注释声明的「禁止新增横向耦合」纪律：

```
manager/（生命周期/运行时）                  host_api/（宿主能力实现）
  ├─ component.rs: Host trait 绑定 → 调 host_api::*          ←─
  ├─ loader.rs: host_api::pty::register_quota                 ←─
  ├─ task.rs: execute_unit 直调 host_api::{fs,http,process}    ←─
  └─ activation.rs: 停用 purge 硬编码调 mdns/ws/pty 三域       ←─
                                             │
host_api/*.rs: 21 处 WasmHostContext（定义在 manager::runtime）、
               14 处 block_on_async（manager::runtime）、
               manager::registry/types/capability/storage/task 引用 ──→
```

具体耦合点（按严重度）：

| # | 耦合点 | 现状 |
|---|--------|------|
| C1 | `WasmHostContext` 定义在 `manager::runtime`，14 字段上帝对象 | host_api 依赖 manager（**反向**） |
| C2 | `block_on_async`/`AMBIENT_RT`/`ambient_handle` 基础设施在 `manager::runtime`，36 个使用方（含 bus/security/host_api） | host_api/security 依赖 manager（**反向**） |
| C3 | `host_api/task.rs` 反依赖 `manager::task`（core_task） | host_api 依赖 manager（**反向环闭合**） |
| C4 | `manager/task.rs::execute_unit` 字符串 match 直调 host_api 域函数 | manager 依赖 host_api 具体实现（无接口） |
| C5 | `host_api/api_bridge.rs` 依赖 `manager::host::PluginHost` + `manager::registry` | host_api 依赖 manager（**反向**） |
| C6 | `security/{framework,approval,fs_auth}` 依赖 `manager::storage::PluginStorage` + `manager::runtime::block_on_async` | security 依赖 manager（**反向**，隐藏环：host_api/fs → monitor → task → host_api） |
| C7 | `monitor.rs::snapshot` 内联调用 `manager::task::task_metrics_snapshot` | monitor 依赖 manager（隐藏环） |

### 1.2 目标架构

```
wasm_core/
├── 中立层（不依赖任何 wasm_core 兄弟模块）：
│   ├── runtime_util（block_on_async / AMBIENT_RT / ambient_handle）   [票 01]
│   ├── storage（PluginStorage）                                      [票 03]
│   ├── monitor（抢 count；task 快照经 MetricsSource 注入）             [票 02]
│   ├── permission / config / bus（既有，不动）
├── security/（依赖中立层 + permission，零 manager）                    [票 01/03 后达成]
├── host_api/
│   ├── context.rs（WasmHostContext + PluginServices + ProcessRegistry + kill_process_group）[票 04]
│   ├── 角色接口（DbScope / PermissionScope / BusScope / …）            [票 05]
│   ├── task.rs（TaskEngine trait 消费方，零 manager::task 依赖）       [票 07]
│   └── 22 个能力域
└── manager/（依赖 host_api + 中立层，唯一组合方向）                    [票 06 后 host_api 零 manager]
```

**单向依赖硬判据（本 spec 的完成定义）**：

1. `cargo test` 全量满绿 lib 测试 + 8 个集成 target（含 fixture wasm 构建链路，§3 测试纪律）
2. `rg "crate::wasm_core::manager" host_api/` **零命中**（不含注释/文档）
3. `rg "crate::wasm_core::manager" security/` **零命中**（不含注释/文档）
4. `monitor.rs` 不再内联引用 `manager::task`（经注入的快照源）
5. 无 WIT / ABI / 权限词汇 / wire 协议变更（跨端兼容性零影响）
6. 专项测试覆盖：host_api 各域针对性测试 + task_e2e / session_e2e / ws_e2e 等集成用例

### 1.3 设计模式

| 票 | 模式 | 用途 |
|----|------|------|
| 01/03 | 依职责重归属（Facade 拆分） | 基础设施函数/类型下沉中立层，消灭反向依赖 |
| 02 | 观察者/注册器（Observer + Registry） | monitor 的 task 快照源改注入，解隐藏环 |
| 04 | 依赖倒置（DIP）：消费方定义接口 | `PluginServices` trait 迁 host_api，PluginHost（manager）实现；capability 经 `CapabilityProvider` trait 引用 |
| 05 | 接口隔离（ISP）：角色接口 | `WasmHostContext` 实现多个窄 trait，各域函数签名只取 `&dyn` 子接口 |
| 07 | 策略（Strategy）+ 注册表（Registry）+ DIP | `UnitExecutor`/`TaskEngine` trait：core-task 只依赖接口，不依赖具体域函数 |

---

## 2. 工单总览

| # | 标题 | 依赖 |
|---|------|------|
| 01 | 异步基础设施中立化（block_on_async 三件套） | — |
| 02 | monitor 去环（task 指标快照注册制） | —（可并行） |
| 03 | PluginStorage 中立化 | —（可并行） |
| 04 | host_api/context.rs 立项（WasmHostContext 迁入 + capability trait 化） | 01, 03 |
| 05 | 接口隔离：host_api 域签名角色接口化 | 04 |
| 06 | api_bridge 迁出 host_api → manager/host | 04 |
| 07 | C3 策略化：host_api/task.rs 反向依赖破除 | 04, 06 |
| 08 | 收尾：全量回归 + 文档记账 | 01–07 |

---

## 3. 关键既有事实（执行者必读）

### 3.1 WasmHostContext（14 字段，`manager/runtime.rs:433`）

外部系统性文档注释完备，字段使用分布（source of truth 以代码为准）：

| 字段 | 消费者 |
|------|--------|
| db / plugin_dbs / plugin_db_root | auth / config / database / runtime(Self::set_plugin_db_root) |
| storage | host_api/storage |
| secrets_cache | auth |
| app_handle | events / http / mdns / activation |
| permission | check_permission（host_api.rs 顶层，约 30 处调用点）/ task |
| fs_auth | fs / task / component |
| message_bus | api / bus / pty / ws / activation |
| plugin_services（两阶段注入 Arc<RwLock<Option<Arc<dyn PluginServices>>>>） | app / process / status / timer / task |
| process_registry（ProcessRegistry） | process |
| api_registry（security::api_registry） | bus / activation |
| security（SecurityFramework） | bus / fs / host / activation |
| capabilities（manager::capability::CapabilityRegistry） | storage / runtime / component |

### 3.2 PluginServices trait（`manager/runtime.rs:361`）

- 定义在 manager，消费方在 host_api（app/process/status/timer/task）→ **造环**
- 已实现「trait 对象 + 两阶段注入」解 PluginHost ↔ WasmHostContext 类型互引的先例（本 spec 票 07 复用此模式）
- 方法：mark_plugin_error / register_plugin_timer / dispatch_process_done / dispatch_task_event / …（以代码为准）

### 3.3 component.rs 的 Host 绑定

- `impl bedcode::plugin::host_*::Host for WasmPluginState` 共 22 域，每方法都是 `域函数(&WasmHostContext / self.host_ctx, …)` 的样板转发（职责合理，**本批不改**）
- `WasmPluginState` 定义在 `manager/runtime.rs:240`（不是 component.rs）
- `package/plugin-sdk-desktop/rust/wit/bedcode.wit` 是 ABI 单一事实源，**不得触碰**

### 3.4 测试设施

- `host_api.rs::tests::build_host_ctx()`：全内存无头上下文构造器（pub(crate)，跨模块测试共用 ~7 处引用）
- 依赖 fixture 的集成测试会经 rustup shim 构建 wasm（单一事实源 `scripts/wasip3-toolchain.sh`），跑测试**必须**用 `~/.cargo/bin/cargo`（AGENTS §3 实测教训）
- 每步验证用针对性过滤命令（§3 两段式），全量回归留票 08

### 3.5 其他

- `api_bridge` 外部消费面：`wasm_core.rs` facade `pub use host_api::api_bridge;` + `commands.rs:385 pub use crate::wasm_core::api_bridge::*`（迁移后 facade 再导出保持）
- `security/` 对 manager 的 3 处引用全部在 01/03 覆盖范围内
- 修改必须遵守 §11 提交纪律（conventional commits）；共享 worktree 无在途改动（2026-09-24 实测工作区干净）