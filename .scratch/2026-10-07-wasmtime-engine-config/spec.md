# spec：wasm-core 开放 wasmtime 引擎定制面（A + B）

- 日期：2026-10-07（dev）
- 范围：`bedcode-desktop/packages/bedcode-wasm-core`（配置面 + 引擎面），**不动 ABI / WIT / 协议 / 移动端**
- 决策：用户终裁 **A+B**（A = 强类型 `EngineTuning` 进 `CoreConfig`；B = `EngineCustomizer` 逃生舱）
- 边界：§5.1.3 四类薄壳① 引擎实现（不新增业务类型/状态/存储/路由/业务默认值，B1-B6 零命中）

## 1. 问题

Engine 只在 `manager/runtime.rs:186-240`（`WasmRuntime::with_config`）构建一次，8 个 wasmtime knob 中
**3 个写死、1 个借道 store 字段、4 个可配**：

| knob | 现状 | 位置 |
| --- | --- | --- |
| `wasm_component_model_async(true)` | **写死** | runtime.rs:190 |
| `consume_fuel` | 可配 `engine.consume_fuel` | :192 |
| `wasm_backtrace_max_frames` | 可配 | :198 |
| `wasm_backtrace_details(Environment)` | **写死** | :212 |
| `memory_reservation` | 可配 | :218 |
| `memory_may_move(false)` | **写死** | :222 |
| `max_wasm_stack` | 借道 `store.max_wasm_stack_bytes` | :225 |
| `cache` | 可配 `engine.compile_cache`(bool) | :230 |

**从未触碰、走 wasmtime 默认**的还有十余项：`cranelift_opt_level`（默认 Speed）、`parallel_compilation`
（默认开）、`debug_info`（**默认关**）、`native_unwind_info`、`profiler`、`epoch_interruption`、
`wasm_component_model_error_context`、`generate_address_map`、`memory_guard_size`、`target` 与整排 proposal
开关（threads/simd/bulk_memory/multi_memory/tail_call/reference_types/memory64）。

ADR 0037 之后 wasm-core 的定位是「任何 Tauri 宿主可直接 path 依赖」，但对「换目标 / 换编译档 / 要诊断
信息」零出口——第三方宿主拿到的 Engine 参数不可调、不可复现。

**顺带发现的一处机制缺口**：`runtime.rs:204-211` 的注释与
`engine_limits.rs::test_debug_mode_trap_includes_line_info` 冒烟测试都声称插件调试模式能拿到
`file:line` 行号，但 **`debug_info` 从未开启**（wasmtime 默认 false → 无 DWARF），该冒烟测试若真在
`BEDCODE_PLUGIN_DEBUG=1` 下跑必然红。A 面把 `debug_info` 纳管后此意图才成立。

## 2. 决策

### D1 · A 面：`CoreConfig.engine.tuning: EngineTuning`（serde + validate）

分两组语义：

- **显式钉死组**（今天写死的字面量，默认值逐字等于今天行为）：`component_model_async`（true）、
  `backtrace_details`（environment）、`memory_may_move`（false）。恒调用 API，**不继承上游默认**。
- **跟随默认组**（今天不触碰的 knob）：全部 `Option<T>`，**`None` = 不调用该 API**，逐字继承
  wasmtime 默认 → 上游升级不引入行为变更，零漂移面。字段：`debug_info` / `native_unwind_info` /
  `parallel_compilation` / `opt_level` / `profiling` / `epoch_interruption` /
  `component_model_error_context` / `wasm_threads` / `wasm_simd` / `wasm_bulk_memory` /
  `wasm_multi_memory` / `wasm_tail_call` / `wasm_reference_types` / `wasm_memory64`。

**不开 `target` / `allocation_strategy(pooling)`**：两者改地址空间布局与 target 假设，不属机制中立项，
不进 JSON 面（宿主仍可用 B 面逃生舱自己承担）。

### D2 · B 面：`EngineCustomizer` 逃生舱 + `EngineSetup` 装配结构

```rust
pub type EngineCustomizer = Arc<dyn Fn(&mut wasmtime::Config) -> crate::Result<()> + Send + Sync>;
pub struct EngineSetup { pub config, pub first_party_dirs, pub customizer }
```

钩子在 wasm-core 自身设置**之后**、`Engine::new` **之前**执行 → 「宿主定制」语义上是最后一道。
`WasmRuntime::with_setup(storage, app_handle, setup)` 为新入口；`new` / `with_config` 签名**不动**
（内部委托 `with_setup`），现有调用点与测试夹具零改动。

`lib.rs` 增 `pub use wasmtime;`：宿主写闭包时不需自己加 wasmtime 依赖（避免版本漂移，
ADR 0019 双端锁版的第三方版本）。

### D3 · 优先级链（写进代码注释 + code-map）

```
编译期默认 < wasm-core.json（Engine 参数构建期固化，重启生效） < 传入的 CoreConfig（with_setup）
  < EngineCustomizer（仅构建期，最后一道；但锁定项随后被重申）
```

### D4 · 锁定 4 项：钩子不能改安全闸门

`consume_fuel` / `max_wasm_stack` / `memory_reservation` / `memory_may_move` 在钩子执行后**重申**：

- 三项是资源看门狗本体（燃料 = 失控/死循环唯一闸门；wasm 栈深 = 防打穿真实线程栈；
  内存预留 = VA 与「预留即硬顶」搬移语义）；
- 它们在 `CoreConfig` 侧有跨字段校验（`memory_reservation_bytes >= store.max_memory_bytes`），
  而任意闭包能改 `Config` 且**不可回读校验**——不锁定则「配置面已校验」的承诺在最后一步静默失效；
- 宿主要改这 4 项只能走 `CoreConfig`（`memory_may_move` 已随 A 面进配置面，故配置面可改、钩子不可改）。

### D5 · 校验与告警（不静默）

- `validate()` 硬拒：`component_model_async == false` → 报错（BedCode 插件基线为 wasip3 + 组件模型
  异步，关掉会让随包插件全部实例化失败；灰度开关写错不得静默）。
- 构建期 warn（无效组合不至于报错，但要防「以为生效了」）：
  `epoch_interruption=true` 而内核看门狗用 fuel（无 `set_epoch_deadline`）· `debug_info=true` 且
  `backtrace_details=disable` · `debug_info=true` 且 `native_unwind_info=false`。

### D6 · 调试模式的 debug_info 推导（补 §1 缺口）

`EngineTuning::resolve(plugin_debug_mode)`：未显式配 `debug_info` 且处于插件调试模式时按
`Some(true)` 处理 —— 让既有注释与既有冒烟测试的意图真正成立（仅调试构建，release 零影响）。
显式配置永不被推导覆盖。

## 3. 测试契约（`unit-test-discipline`）

配置面（`config.rs` 单测，纯逻辑）：

1. `tuning_defaults_match_current_hardcoded_behavior` — 默认值逐字等于今天写死的三个字面量 +
   其余全 `None`（重构回归锚点）。
2. `tuning_resolve_derives_debug_info_only_for_debug_mode` — 推导规则的四个分支
   （显式值优先 / debug 推导 / 非 debug 保持 None / 显式 false 不被推导翻回）。
3. `tuning_loads_from_config_file_and_rejects_invalid_enum` — `wasm-core.json` 解析 + 非法枚举值
   报错带文件路径与操作上下文。
4. `validate_rejects_component_model_async_disabled`。

引擎面（`manager/runtime/tests/engine_config.rs`，行为断言）：

5. `customizer_overrides_unlocked_knobs_end_to_end` — 钩子关 `wasm_backtrace` / `parallel_compilation`：
   getter 直读 + 真实 trap 错误串端到端（对照：默认运行时必须仍带 backtrace，否则非恒真）。
6. `customizer_cannot_override_locked_knobs`（**锁定项主证据**）— 钩子把 4 项全改成「拔闸门」极值 →
   构建出的 Engine 上 4 项仍等于配置值。
7. `customizer_error_is_reported_with_context` — 闭包返 Err → `with_setup` 返带上下文错误。
8. `tuning_maps_onto_wasmtime_config` — ①**零漂移**：默认配置构建的 Engine 与裸
   `Config::new()` 引擎的 `get_*` 逐项相等（证明「`None` = 不调用」）；②显式配置逐字生效；
   ③反向非恒真：显式改动后特征集必须真的与上游不同。
9. `tuning_without_getters_reach_engine_build_log` — 无 getter 的三项走生效参数日志核对。
10. `incoherent_tuning_warns_instead_of_silently_doing_nothing`。

**观测手段的意外收获（推翻 §3 初稿的「覆盖缺口」）**：初稿假设锁定 4 项在 `Config` 上不可回读、
只能靠代码结构保证。实调时发现 **wasmtime 48 把 `get_*` 放在 `Engine` 上而非 `Config`**（上游注释：
「`Config` 不反映完整配置故读不出默认，`Engine` 才是完整生效配置」）——于是 4 项全部可经
`Engine::get_consume_fuel` / `get_max_wasm_stack` / `get_memory_reservation` / `get_memory_may_move`
逐字断言，**缺口关闭**。`component_model_async` / `backtrace_details` / `profiling` 无 getter，
走生效参数日志（该日志因此是产品面而非调试残留）。

**变异自检（三处独立打，不合并）**：
- 摘掉钩子调用 → `customizer_overrides_…` + `customizer_error_…` 两条红；
- 摘掉 `reassert_locked_knobs` 里的 `consume_fuel` → 锁用例红；
- 摘掉 `memory_may_move` 重申 → 锁用例红。

**教训**：第一次把两个探针合并打（钩子不执行 + 燃料重申被摘除），锁用例反而绿——钩子没执行
就没人关燃料，锁定无从检验。变异必须**独立**打，否则互相掩盖。

**另一处实测发现**：wasmtime 48 默认**几乎开启全部 proposal**（`WasmFeatures` 含 THREADS /
TAIL_CALL / MEMORY64 / GC / MULTI_MEMORY / RELAXED_SIMD / EXCEPTIONS），且
`memory_may_move` 默认 `true`。故 ①「显式开启某 proposal」看不出变化（初始测试因此失败），
非恒真对照必须**显式关掉**某项；② 默认必须钉死 `memory_may_move(false)` 才与历史行为一致（已做）。

**副作用发现（测试自身的卫生问题，已修）**：初稿的日志用例用 `profiling = JitDump` 覆盖 `profiling`
映射，而 wasmtime 的 jitdump（perf-jitify）会在**进程 cwd** 落 `jit-<pid>.dump` —— 每次跑测试污染
crate 根目录 9 个文件。改为：`profiling` 映射改由**纯映射单测**（`vocabulary_maps_onto_wasmtime_primitives`，
不建 Engine）逐项钉住，日志用例只用无副作用的取值。

## 4. 落地清单

- `src/config.rs`：`EngineTuning` + `OptLevel` / `Profiling` / `BacktraceDetails`（serde kebab-case）
  + `resolve()` + `validate()` 增规则 + 4 组单测。
- `src/manager/runtime.rs`：`build_engine_config()` 抽取（纯函数）+ `warn_incoherent_tuning()`
  + `EngineSetup` / `EngineCustomizer` / `with_setup` / `reassert_locked_knobs()` + 生效值结构化日志。
- `src/manager/runtime.rs`：注册 `mod engine_config;`（tests 子模块）+ `#[cfg(test)] engine()`
  访问器（`WasmRuntime` 不实现 Debug，且 Engine 生效值只能经 Engine 读）。
- `src/lib.rs`：`pub use wasmtime;` + facade 再导出 `EngineSetup` / `EngineCustomizer`。
- 文档：`bedcode-desktop/docs/code-map.md` wasm-core 段（配置面 + 优先级链 + 锁定项）、
  根 `CHANGELOG.md` + `CHANGELOG_zh.md`、本 spec。

## 5. 验证（实测结果）

**全绿项**

- `cd packages/bedcode-wasm-core && cargo test --lib engine_config::` → **7/7 绿**
- `cd packages/bedcode-wasm-core && cargo test --lib config::` → **14/14 绿**
- `cd packages/bedcode-wasm-core && cargo test` → **672 通过 / 1 失败**（唯一失败见下）
- `cd src-tauri && cargo test --no-fail-fast` → **144 通过 / 0 失败**（25 个测试目标）
- `cargo clippy --lib --tests`：我新增的代码 **零新增告警**（逐条核对我触碰的 5 个文件的告警位置，
  全部落在既有代码上：`config.rs:326` `impl Default for CoreConfig` 可 derive、
  `runtime.rs:128` 孤儿文档注释、`runtime.rs:769~1319` 票 05b 遗留的死代码测试助手、
  `test_support.rs:170/240/244`）；`engine_config.rs` 只剩测试惯用的 expect/unwrap 提示。
- `cargo fmt`：全 crate 在 HEAD 就不是 rustfmt-clean（差异集中在未触碰的既有代码），
  故只对我新增的代码做格式化（新测试文件 rustfmt 全量格式化；其余 4 文件我新增区域 fmt 无差异）。
- `lens_diagnostics mode=full` 对该 crate **无 LSP server 配置**（5 文件 inconclusive），
  以 `cargo build/test/clippy` 为准（均已实跑）。

**既有失败（非本票引入，已实证）**

1. `terminal_output_perf::perf_p2_guest_ring_fetch_batch_curve`（wasm-core 性能探针）：
   报 16 KiB JSON 单次往返 6.3 ms，校准线 ~1.2 ms / >5 ms 判回归。
   **HEAD 对照**：把我的改动 stash 后在同一台机器独占跑 HEAD，测得 **6.08 ms，同样越线**——
   即 dev HEAD 本身就红。它测的是 guest ring-fetch 路径（本票未触碰，默认 Engine 生效配置未变，
   零漂移用例即为其证），同文件另两条性能用例通过。
2. 宿主 `system_component_test`（6 失败）与 `ws_e2e`（2 失败）：均为
   「夹具产物缺失」panic（`bedcode_plugin_system_test.wasm` / `bedcode_plugin_sdk_fixtures.ws.release.wasm`）。
   **根因**：票 05b 把用例搬到宿主集成测试后，「谁来构建这两个夹具」无人接续——
   全仓只有读取方（`test_support::system_test_artifact_bytes` / `sdk_fixture_artifact_bytes`），
   没有构建方，且产物盘上也没有。按 `fixture_build.rs` 的配方手工补建后，
   两个目标 **10/10 与 6/6 全绿**（与本票无关，属环境/构建覆盖缺口）。

**未纳入自动化门禁（手工项）**

- `target` / pooling 逃生舱能力：暂无宿主消费方，仅 API 面（未接线验证）
- `BEDCODE_PLUGIN_DEBUG=1` 下 `test_debug_mode_trap_includes_line_info` 行号冒烟：
  需 debug profile 夹具重建，未跑（本票已补上它依赖的 `debug_info`，机制上使其可成立）
- 真机 / 浏览器核验：纯 Rust 引擎面改动，无前端变更
