# wasm-core 组件引擎合一（common / bare 双形态共用一份机制）

> 立项：2026-10-10。用户裁决「wasm-core 核心应该只有一份，差异在宿主端装配，不该在最基础的 crate 里区分」。

## 1. 问题陈述

单一 crate 双形态（ADR 0045 / ADR 0047）落地后，形态壳里残留两份**与 WIT 无关**的组件引擎机制：

| 机制 | `common.rs` | `bare.rs` / `bare/component.rs` |
| --- | --- | --- |
| `aot_cache_key` | 137 | `bare.rs:116` |
| `compile_component_from_file`（AOT 缓存） | 577 | `bare/component.rs:525` |
| Store 装配（`Store::new` + limiter + 燃料注入） | `common/component.rs:636-644` | `bare/component.rs:627-635` |
| 实例化（含 sync/async 分叉） | `common/component.rs:650` | `bare/component.rs:637` |
| 燃料续费（每次导出调用前） | `common/component.rs:1021` | `bare/component.rs:737-743` |
| 实例创建 / 死亡日志 | `common.rs:722` / `component.rs:611` | `bare/component.rs:651` / `688` |

两侧仅三处**真差异**：实例化 sync/async、桌面多一层 WASI 预打开、导出绑定 WIT 面（`abi.form()` 与额外可选导出探测）。

## 2. 目标与非目标

**目标**：把上表全部机制抽成 `manager/runtime/engine.rs` **一份**（与 WIT 无关 ⇒ 不含 `bindgen!`、不含任何 interface 路径），两形态壳只保留 WIT 绑定与形态接线。

**第二批（同批追加，用户裁决）**：

- **导出调用的结果判定合一份**（用户裁决「可以用泛型」）：trap 记账 + error 日志 + 双层结果映射的文案与分流收敛到 `engine::{log_trap, trapped, settle, settle_guest, settle_guest_event, start_call_timer}`；形态壳只留绑定获取、具体调用（sync/async）与生命周期记账的事件选择。
- **Engine 构建参数合一**（用户裁决「桌面端是真源码」）：`build_engine_config` 等 7 个纯函数（配置面 → wasmtime 原语的唯一映射点）搬进 `engine.rs`，移动侧 `WasmRuntime::new` 改为消费 `engine::build_engine_config(&CoreConfig::default(), plugin_debug_mode())`，不再硬编码字面量。

**形态绑定层为什么仍不可合**：见 §3 末尾「不可合并的三条硬约束」。

**非目标（留后续票）**：

- Engine 自定义钩子（`EngineSetup` / `EngineCustomizer` / `with_setup`）形态化——桌面经 `app_handle` 取配置文件与 AOT 缓存目录，移动无此面；钩子本身是形态接线不是机制。
- `test_support/bare.rs` 里的 `test_engine()` 仍是手工 `Config::new()`——`bare-host` 形态的 crate 内单测 HEAD 即不可编译，改了无法验证，故不动（禁改不可验证的代码）。
- 形态壳 `add_to_linker` 改 `inventory` 自报——**撤回，见下**。

### 撤回：「两形态装配口径不一致」（开工前的判断有误，已核实）

开工时判断「bare 用 host-kit `ModuleRegistry` 自报、common 用逐接口硬编码，口径不一致」。逐行核实后**该判断不成立**：两形态遵循**同一条规则**——

|interface 归属 | 桌面 `common/component.rs` | 移动 `bare/component.rs` |
| --- | --- | --- |
| 住core 内（实现与接线同 crate） | 8 组逐行 `add_to_linker` | 12 组逐行 `add_to_linker` |
| 已迁出 core（外部 crate / 宿主 adapter） | ws / peer / http 三个能力域经 `ModuleRegistry` 自报 | 移动独有 5 域经 `ModuleRegistry` 自报 |

即「in-crate 直接列表 / out-of-crate 自报注册」两边完全一致；差别只在**哪些域住在core**（移动独有域按 ADR 0047 已落移动宿主，桌面域按 ADR 0022/0036 留在核心里），那是归属裁决不是机制分叉。把 in-core 域改inventory 不会带来任何收益（它们不迁出，真迁出时随该批处理），却要动白名单锁 + 三处测试 + code-map，且与 ADR 0036 D4 记录冲突。**故撤回该子项。**

## 3. 设计

`engine.rs` 的每个函数都**不知道 WIT 的存在**：

- `aot_cache_key(path, source_len) -> u64`
- `compile_component_from_file(engine, cache_dir, path) -> Result<Component>`——AOT 缓存键 / 新鲜度 / 原子写回 / 失败降级唯一一份
- `new_store(engine, state) -> Result<Store<WasmPluginState>>`——`Store::new` + `limiter` 接线 + 首调用燃料注入
- `InstantiateMode::{Sync, Async}` + `instantiate(store, linker, component, mode) -> Result<Instance, wasmtime::Error>`——async 分叉是**形态参数**而非两份实现；错误串由调用方按形态语境构造（桌面要拼旧产物重建指引）
- `inject_call_fuel(store)` / `refill_call_fuel(store)`——注入点（ABI 协商）与续费点（每次导出调用）分离
- `log_instance_created(plugin_id)` / `log_instance_dropped(plugin_id, lifetime)`——成对的生命周期观测
- `build_engine_config(core_config, debug_mode)` + `reassert_locked_knobs` + `warn_incoherent_tuning` + `log_effective_engine_config` + 三个配置→wasmtime 原语映射——**配置面唯一映射点**（桌面 `CoreConfig` 为真源）
- `log_trap` / `trapped` / `settle` / `settle_guest` / `settle_guest_event` / `start_call_timer`——**导出调用的结果判定**（trap 记账 + error 日志 + 双层结果映射）

形态壳保留：Engine 自定义钩子、`bindgen!`、`impl … Host`、`add_to_linker`、`verify_abi`（含 form 校验与可选导出探测）、`Loaded*Plugin` 实例类型与「取绑定 → 调用 → 分派到共享判定」的薄壳。

### 不可合并的三条硬约束（绑定层，即 WIT 面的分叉不可消除）

1. `bindgen!` 生成模块名 = WIT package 名（`bedcode`），同 crate 两次 `bindgen!` 必冲突（`lib.rs` 的 `compile_error!` 即守卫）。
2. 插件产物静态 import **本端 world 全集**，宿主必须按本端 world 生成绑定并全量注册；一份绑定服务两个 world ⇒ 同一 interface 要对同一 `WasmPluginState` 重复 impl。
3. ADR 0018：移动 WIT/ABI 独立演进，不跟演桌面 ABI 破坏性变更。

## 4. 行为变更（必须显式记录）

第一批：

1. **移动形态的燃料预算改为走 `StoreLimits::fuel_budget()`**：原 `bare` 侧三处硬用 `FUEL_PER_CALL` 常量，**debug 构建（`BEDCODE_PLUGIN_DEBUG=1`）不套 32× 放大倍率**，与桌面形态不一致（潜在过早 trap）。统一后两形态同口径。生产 release 构建两者数值相同 ⇒ 无行为变更。
2. **移动形态获得燃料消耗记账**：`refill_call_fuel` 把上一区间消耗记进 `PluginMetrics`（桌面已有）。
3. **AOT 反序列化统一走 `Component::deserialize_file`**：桌面原为「读全文件字节 + `deserialize`」，与移动侧的 `deserialize_file` 等价，去掉一次全文件拷贝。

第二批：

4. **移动形态 Engine 参数改由桌面真源构建**：`CoreConfig::default()` 的字面量与移动侧原硬编码**逐值相等**（`consume_fuel=true` / 预留 `MAX_PLUGIN_MEMORY_BYTES` / 栈 `MAX_WASM_STACK_BYTES` / `memory_may_move=false` / `compile_cache=true`），故上述五项**无变化**；新增三项：backtrace 帧数 20→**32**（与桌面一致，trap 错误串更完整）、`component_model_async=true` 显式化、「Engine 生效参数」结构化 info 日志与无效调组合 warn（`warn_incoherent_tuning`）。生产 release 无差异。
5. **移动形态获得 trap 记账与 error 日志**：`engine::trapped` 统一记账 `LifecycleEvent::Trap` 并打 `plugin_id` / `export` / `trap_detail`（Debug 全链）——移动侧此前完全没有（`e8` 类 trap 只在错误串里）。
6. **移动形态 activate/deactivate 获得生命周期记账**：`ActivateOk` / `Deactivate`（原仅桌面有）。
7. **`get_manifest` 的错误文案**由 `WASM get_manifest() call failed` 统一为 `WASM manifest() call failed`（历史文案本就写着 `manifest()`，日志字段随之对齐）。
8. **`common/component.rs` 的测试引擎改走生产构建函数**（原手工 `Config::new()` 摆 fuel + backtrace）：测试与生产 Engine 配置从此同源；lock 变异时当场抓到这处第二真源。

## 5. 门禁（实测结果）

- **双形态编译**：`bedcode-desktop/src-tauri` `cargo check --lib` 0 error 0 新增 warning；`bedcode-mobile/src-tauri` `cargo check --lib` 同。
- **测试代码编译**：wasm-core crate 根 `cargo check --lib --tests` 0 error（common-host）。
- **core crate 全量单测（common-host）**：`cargo test --lib` → **595 passed / 0 failed**（含本批新增 3 例：缓存键判据、机制正向锚点、防回接锁）。
- **移动端全量**：`bedcode-mobile/src-tauri` `cargo test` → 219 lib + 全部集成套件 **0 failed**（bare-host 形态端到端）。
- **锁变异自检（三针）**：① `bare.rs` 注入 `fn build_engine_config` ⇒ 红并点名；② `common/component.rs` 注入 `"WASM invoke_command() call failed"` ⇒ 红并点名；③（第一批）`bare/component.rs` 注入 `.set_fuel(` ⇒ 红。每次还原后 sha256 与变异前逐字一致且锁回绿。
- **锁的实战价值**：扩展 needle 后当场抓到 `common/component.rs` 测试里的第二份 Engine 配置（`config.consume_fuel(`），已改为调用生产构建函数。
- **移动端全量（第一批）**：`bedcode-mobile/src-tauri` `cargo test` → 219 lib + 全部集成套件 **0 failed**。第二批（Engine 构建 + 导出判定）改动后**未复跑**：复跑时磁盘写满（`target/host-kits` 13G + 两端 target 各 12G），`cargo clean` 移动端 target 后 core 单测复跑通过；移动端第二批的验证面目前只有双形态 `cargo check --lib` 0 error。**收尾须补跑移动端全量**。
- **`bare-host` 形态的 crate 内单测在 HEAD 即不可编译**（`src/bus.rs` 的测试引用 `common-host` 专属的 `HostBusPort` / `bedcode_server_websocket`，未随形态 cfg 门控）——非本次改动引入；该形态的验证面因此是「双形态 `cargo check` + 移动端全量测试」。**记为独立缺陷**，不在本批范围。
- 未跑：桌面 `src-tauri` 的集成测试（`session_e2e` / `pty_e2e` 等）——本次改动不触碰宿主装配代码路径，行为证据已由 core crate 595 例（含组件往返 / AOT 缓存 / 燃料看门狗 / ABI 协商）覆盖；桌面集成套件另含 HEAD 即红的 `ws_e2e` / `ws_output_perf`，混跑会污染判读。
- 未跑：`cross-end-tests`（未改跨端协议）、`pnpm run test:run` / eslint（未改前端）、真机冒烟（无设备）。

## 6. 行为变更复核（实测后确认无额外漂移）

1. 移动形态燃料预算改走 `StoreLimits::fuel_budget()`：release 构建与原 `FUEL_PER_CALL` 常量**同值**（`fuel_budget_for(false)` = `fuel_per_call`）；仅 `BEDCODE_PLUGIN_DEBUG=1` 的debug 构建变为 32× —— 与桌面形态对齐。
2. 移动形态获得燃料消耗记账（`record_fuel_consumed`）：桌面原已有；纯增量埋点。
3. AOT 反序列化统一 `Component::deserialize_file`（桌面原为读全文件 + `deserialize`）：结果等价，少一次全文件拷贝。

## 7. 指针

- 形态命名与落点纪律：ADR 0047；`AGENTS.md` §5.4。
- 前序批次：`.scratch/2026-10-10-wasm-core-mobile-purity/`（票 06 收口）。