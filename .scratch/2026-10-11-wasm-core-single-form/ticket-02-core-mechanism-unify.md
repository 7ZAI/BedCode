# 票 02 · wasm-core 机制侧去分叉（零 cap 差异）

> 输入：`spec.md` §5 T-02。前置：T-01 基线绿。
> **依赖**：T-01 → **本票** → T-03（强串行；本票只动 wasm-core 机制面，不与 T-04/05 并行——它是 T-03 的底座）。
> **测试节奏**：针对性自验（wasm-core `cargo check` —— 无 feature / bare 形态 + 直接相关单测）；全量 `cargo test` 留 T-06 收尾。
> 目的：把 wasm-core 里**与契约无关的机制面**的 common/bare 分叉全部消除——这是「wasm-core 统一」的根基，不涉及绑定层（交 T-03/04/05），但先削掉纯机制差异，让后续绑定迁移时 wasm-core 已是一份。

## 1. 机制侧分叉清单（逐项处理）

| # | 位置 | 分叉内容 | 处理 | 归属 |
| --- | --- | --- | --- | --- |
| M1 | `src/storage.rs` | `tokio::sync::Mutex`（common）vs `std::sync::Mutex`（bare）；`new`/`test_storage` cfg 双写 | 合并成一份：锁形态未决时保留 `std::sync::Mutex`（bare 语义）+ `block_on` 桥（见 M5） | 留核心 |
| M2 | `src/security/auth_policy.rs` | 20 处 cfg：锁获取行 `lock().await` vs `lock().unwrap_or_else` | 统一为**同步锁 + 短临界区**（auth_policy 内无跨 await 持有的锁），消除 await 分叉 | 留核心 |
| M3 | `src/bus.rs` | `MessageDispatcher` 两 trait（bare 为 async、common 为同步）+ `is_activated` await 差异 + `HostBusPort`/`BusBoundWsPorts` 仅 common | bus 机制是否依赖异步是**调用面问题**，先收敛为一份 trait（异步兜底经 `block_on_async`），`HostBusPort` 如果是桌面装配面词汇随 T-04 迁宿主 | 部分留核心 / 部分迁移 |
| M4 | `src/manager/types.rs` | 双 SDK 类型引用（`bedcode_plugin_api` vs `_mobile`）+ `PluginLifecycleEvent` 仅 bare | **与 WIT 无关的纯数据**：从 SDK 剥离到中性处（host-kit 或 wasm-core 内自持），双端引用同一份 | 留核心 |
| M5 | `src/runtime_util.rs` | `spawn_with_error_boundary`/`_on`/`wrap_with_error_boundary` 仅 bare（242-270） | 这是**双端都需要**的公共设施，取消 cfg 门控常编译 | 留核心 |
| M6 | `src/manager/{loader,registry,validation,capability}.rs` | cfg import 差异（loader 12-14、registry 8-18 等） | 多为双 SDK 类型引用，随 M4 消解 | 留核心 |
| M7 | `src/lib.rs` | `compile_error!` 互斥守卫 + `intercall` 仅 common + `host_context_registry`/`terminal_stream_gateway` 仅 bare + `manager::host` 仅 common | 互斥守卫在 feature 收敛后删除；`intercall`（互调机制）双端都该有→常编译；`host_context_registry`（移动装配面）随 T-05 迁移动宿主；`terminal_stream_gateway` 是移动独有 interface 的机制面——若机制归核心则留（**语义边界 T-03 定**） | 分流 |
| M8 | `src/config.rs` | `ResourceOverrides` 双份（common 来自 SDK / bare 自持）+ 机制 cfg | 随 M4 消解：`ResourceOverrides` 归中性处一份 | 留核心 |
| M9 | `src/db/database.rs` | 1 处 cfg（bare 相关） | 随依赖消解 | 留核心 |
| M10 | `src/permission.rs` | 双 SDK `permission::*` 再导出 | 随 M4：权限词汇从 SDK 剥离到中性处 | 留核心 |
| M11 | `src/security/{fs_auth,network_auth,framework,approval}.rs` | 零散 cfg | 逐处消解，不留残余 | 留核心 |
| M12 | `src/host_api.rs` | 28 处 cfg：common 14 域模块 vs bare 5 模块（ports/sql_guard/bare_context） | **绑定层模块**随 T-03/04/05 迁出；本票只处理与机制相关的 re-export 消除 | 迁移为主 |
| M13 | `src/test_support.rs` + `test_support/bare.rs`（594 行） | 双套测试基建 | 交集基建合一份；形态专属（fixture 构建等）随各自壳迁宿主（T-04/05） | 分流 |

## 2. 上下文合一（B5）

- `host_api/context.rs`（桌面 `WasmHostContext`，含 tauri/AppHandle/DB scope）与 `host_api/bare_context.rs`（移动）合成一个中立 `WasmHostContext`：
  - 若差异仅是「AppHandle 类型 + 端口注入口」，做成**泛型端口槽**（`HostEnginePorts` 已存在共用）+ 可选 tauri 引用（feature 门控仅宿主持有）；
  - wasm-core 默认形态不含 tauri；桌面宿主经 feature 注入 AppHandle。
- 删除 `downcast_host` 双形态路径，`self.host_ctx()` 单一化。

## 3. feature / 依赖收敛（B5）

- `Cargo.toml`：`common-host` / `bare-host` 两特征表**退役**，`default = []`（本票先消机制面，绑定特征随 T-03/04/05 逐步退出，末了删除）。
- 双 SDK 依赖（`bedcode-plugin-api` / `bedcode-plugin-api-mobile`）收敛：**机制面不直接依赖 SDK 类型**（经 M4 剥离到 host-kit / 自持），SDK 依赖只留在绑定层（T-03+）。
- `lib.rs` 互斥守卫删除。

## 4. 语义边界（本票裁决项，必要时停下问用户）

- **锁形态**（tokio vs std Mutex）：统一到哪一极？倾向 bare 的 std + 短临界区（依赖 M5 常编译的 block_on 桥），但桌面大量 async 面可能要求 tokio 锁——**实现中若察觉性能/死锁风险，停下问用户**，不做闷头翻。
- **`intercall` 常编译**：互调机制（JSON-RPC over bus）双端语义一致，倾向常编译；若移动装配在途（0047 会话遗留）有冲突，先对齐再动。

## 5. 门禁

- wasm-core `cargo check --lib`（无 feature）**不再依赖任何形态 feature** 0 error；`cargo test --lib` 全绿（含此前未跑的移动侧用例——本票把共用测试基建合一，移动 `cargo test` 补跑纳入验收）。
- 桌面 `cargo check --lib` / 移动 `cargo check` 各自 0 error（绑定层未动，宿主应零变化）。
- 锁变异：删除互斥守卫后无残留形态词汇自检。

## 6. 交付物

- 机制面 cfg(common-host/bare-host) 计数归零（可 grep 验证：`grep -rn "cfg(feature = \"common-host\")" src/ | wc -l` → 仅剩绑定层相关，见 T-03 移交清单）。
- 上下文合一 + M1-M13 逐项处置记录 + 门禁结果附本票。
## 7. 状态：进行中（2026-10-11 中午，提交 eea06fc91 / 3c3e3fa98 / ef97024cc）

**已完成并提交**：
- M5 错误边界常编译；M1/M2 锁统一 std；M3 bus trait 单形态（`eea06fc91`，含双 SDK wasm_host 改名对齐 + 移动 dispatcher 同步化）
- 移动 test 面补跑收口 + 形态门控（`3c3e3fa98`）
- M4/M6/M8/M10 双 SDK 类型中性化（`ef97024cc`）：`src/manifest.rs` 中性超集词汇 + bus→server-base wire + permission 自持 + ResourceOverrides 单份；loader/registry/validation/host 绑定层全谱切中性
- M9 from_connection 常编译；PluginLifecycleEvent 常编译（未提交，随下一批）

**门禁实证（当前 HEAD）**：wasm-core 三态 check 0 error（common / 无 feature / bare-host）；双形态 test 全绿（common 596 / bare 307）；两端宿主 check 0 error。

**剩余（B5 及收口，规模大、与 T-03 绑定定稿耦合，建议独立会话推进）**：
- B5 上下文合一：`context::WasmHostContext`（桌面富服务：permission/fs_auth/net_auth/security/api_registry/capabilities/plugin_services/task_engine/process_registry/secrets_cache）与 `bare_context::WasmHostContext`（移动：ports/granted_permissions/status_reporter）合成中立类型——倾向桌面富结构 + 移动字段 Option/trait 槽；intercall 常编译依赖此（其引 WasmHostContext + host_api::api 均 common 门控）
- M7 收尾：intercall 门控解除（随 B5）；host_context_registry / terminal_stream_gateway（bare，归 T-05）；manager::host/task/watcher 门控归 T-04
- M11 残余：fs_auth.rs:3220 的 FsAuthGate impl（bare adapter 面，归 T-05）
- feature 表退役 + compile_error 守卫删除（等 B5 与绑定迁移到 T-03/04/05 逐步退出，末了删除）
- bus.rs HostBusPort/BusBoundWsPorts 12 处 common 门控（桌面装配面，归 T-04）
- M13 test_support 合一（common/bare 测试基建归 T-04/05 迁移）
