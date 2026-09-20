# A0-3 宿主 async 化 · 前置实施方案

> 状态：**已实施（P1-P6 全部通过，2026-09-21）**——探针见
> `bedcode-desktop/src-tauri/src/plugin/manager/wasm_runtime/tests/a03_probe.rs`（7 用例），
> 产出与实测贴档见同目录 `report.md`；P2 结论与 P3 红线已落 auth-center-spec.md A0-3 备注 + AGENTS §7。
> 日期 2026-09-21。
> 上游：`.scratch/2026-09-18-devices-plugin-scope/auth-center-spec.md` A0-3（宿主运行时
> async 化）+ A0-1/A0-4（构建链）；`.scratch/2026-09-21-host-task-concurrency/spec.md` §14
> （协作式线程中期路径，本方案是其前置）。
> 关联红线：AGENTS §7（双端偏离 / ABI 批次）、ADR 0019（wasmtime 双端分叉）、ADR 0022。

---

## 1. 定位与划界

A0-3 全貌（auth-center-spec.md §A0-3 原文）：「宿主运行时 async 化：component-model-async
启用 + `Store::new_async` + `call_async` + p3 async linker 替换 `p2::add_to_linker_sync`；
既有 host_impl 适配（sync host fn 在 async store 下兼容性验证；燃料续费/ResourceLimiter
async 语义）。验收：wasip3 fixture 闭环 + **既有 unknown-unknown 插件零回归**」。

本方案把 A0-3 切为 **前置**（本方案范围，现在执行）与 **主体**（后续票，见 §5 移交）。

**前置目标**：不改宿主调用主体（`Store::new`、`invoke_command` 等 13 个 sync 入口签名、
host_impl 的 20 组原语），以**探针 + 调查**消除 A0-3 主体的不确定面，产出：
1. async store 下既有路径兼容性证据（sync host fn / block_on_async 桥 / 双路径调用）；
2. 资源限制（燃料 / ResourceLimiter）async 语义事实；
3. 同实例串行红线的落条款（防竞态硬约束）；
4. 13 个调用入口的 async 化影响面清单；
5. 测试基建适配计划 + 性能基线；
6. 主体实施票的移交输入。

**前置不动**：`component.rs` 的 `Store::new`（779 行）、13 个 sync 入口（1048-1307）、
`host.rs:100` 的 `Arc<Mutex<LoadedWasmPlugin>>` 串行模型、`block_on_async` 桥、20 组
host_impl 函数签名。

---

## 2. 现状盘点（2026-09-21 实测）

### 2.1 已完成（A0-3 前置地基）

| 项 | 证据 | 状态 |
| --- | --- | --- |
| CM_ASYNC 引擎级异步 | `wasm_runtime.rs:627` `config.wasm_component_model_async(true)`；注释：wasmtime 48 中 async_support() 已废弃为 no-op | ✅ |
| linker async 化 | `component.rs:709` `p2::add_to_linker_async`（替代 sync adapter，规避 async store 下 block_on 重入 panic）；`component.rs:715` `p3::add_to_linker`（func_wrap_async 注册 wasi0.3） | ✅ |
| 实例化走 async 入口 | `component.rs:760` `instantiate_async` 经 `block_on_async` 驱动；sync/async 组件双兼容（/tmp/wasip3-probe 场景 4 实证） | ✅ |
| wasmtime-wasi p3 依赖 | `Cargo.toml:141` `wasmtime-wasi = { version = "48", features = ["p3"] }` | ✅ |
| wasip3 构建链 | `scripts/plugin-wasm-config.mjs`（WASM_TARGET=wasm32-wasip3，pinned nightly-2026-09-16）；spike 验证零代码改动编译 | ✅ |
| 燃料/内存限额基础 | `with_config` consume_fuel + memory_reservation + limiter | ✅ |

### 2.2 差距（A0-3 主体）

| 项 | 现状 | 主体要做 |
| --- | --- | --- |
| Store 形态 | `component.rs:779` `Store::new`（sync） | `Store::new_async` + async store 语义验证 |
| 调用路径 | 13 个 sync 入口（invoke_command 1048 / on_terminal_* / on_message* / on_ws_frame / on_session_lifecycle / on_input_submitted / on_process_done / activate / deactivate / on_startup / on_shutdown / get_manifest / raw_store）+ `wasm_runtime.rs:996 call_plugin_api_host` | 入口 async 化（call_async / 组件导出 async 调用） |
| host_impl 适配 | 20 组原语 = sync fn + block_on_async 桥 | 兼容性验证（可能无需改）；如需 async 化则列入主体 |
| 产物 | resources/plugins 仍 unknown-unknown（sync） | wasip3 重建 4 个桌面插件 |
| wasi2 残留 | p2 async adapter 仍在用（wasip2 插件兼容） | A0-5 清理（wasip3 全覆盖后） |

### 2.3 串行模型现状（前置红线的事实基础）

`host.rs:100`：`wasm_plugins: Arc<RwLock<HashMap<String, Arc<Mutex<LoadedWasmPlugin>>>>>`——
**std::sync::Mutex 每插件一把**，13 个入口全部先 `lock()`（阻塞）再同步调用。插件静态状态
（session 插件：配对码 `CURRENT_CODE`、QR `qr_manager`、挑战注册表 `CHALLENGES`、config
缓存）依赖此串行保证，**无内部并发保护**（std Mutex 在单线程下不阻塞）。

---

## 3. 前置实施步骤（本方案范围）

### P1 · async store 兼容性探针（核心，决定主体风险）

建探针于 `src-tauri/tests/`（或复用 `/tmp/wasip3-probe` 模式，落仓库 `src-tauri/tests/a03_probe.rs`），
三场景，全部需贴出实际输出：

- **P1-a sync host_impl 在 async store 下**：`Store::new_async` + 现有 sync 注册的
  bedcode host 接口（`bedcode::plugin::host_*::add_to_linker`，func_wrap 注册）被
  wasip3 组件调用——验证：sync host fn 在 async store 下可被调用、`block_on_async`
  桥（三路径：多线程 block_in_place / current_thread spawn / ambient 重入检测）不 panic。
  **风险点**：host_impl 内部 `block_on_async` 在 fiber 执行线程（tokio worker）上是否
  触发"runtime within runtime"——先例：p2 sync adapter 正是栽在这（component.rs:705 注释），
  但 host_impl 的 `block_on_async` 有重入检测（wasm_runtime.rs:139-143 spawn 新线程路径），
  需实证是否安全。
- **P1-b wasip3 组件完整闭环**：现有 session 插件 wasip3 产物（已编译存在）在 async
  store 下 `activate → invoke_command(_http_endpoint) → host 原语回调（fs/process/auth）→
  终端 hooks → deactivate` 全链路。
- **P1-c 既有 unknown-unknown 产物零回归**：现行 4 个插件产物（resources/plugins/）在
  async store 下行为逐字节不变（A0-3 硬门槛预演；gateway/闭环测试的既有断言直接复用）。

产物：P1 报告（三场景 pass/fail + 失败时现象与栈）。**P1-a fail 即主体设计方向改变**
（host_impl async 化或保留 sync store 双轨），按 §6 回退。

### P2 · 资源限制 async 语义调查

- 燃料：`store.set_fuel` 在 async store / fiber 下的生效语义（guest 指令计数是否跨
  suspend/resume 累计；wasmtime 48 是否有 async 语义变化——A0-3 原文标注需复核）。
- 内存：`ResourceLimiter` 在 async store 下 memory_growing 调用线程与限制语义。
- 产物：调查结论段（写回 auth-center-spec.md A0-3 备注）+ 探针断言。

### P3 · 同实例串行红线落条款（本方案最重要的设计输出）

**红线（写入 auth-center-spec.md A0-3 与 AGENTS §7）**：A0-3 主体实施后，**每插件实例
同一时刻仍只允许一个 guest 调用在执行**；async 化只改变「宿主线程在等待时让出」，
不引入「同实例并发进入 guest」。

理由：插件静态状态（配对码 / QR / 挑战注册表 / config 缓存 / 私有库连接）无内部并发
保护；同 Store 并发（`call_concurrent` / `run_concurrent`）是 §14 协作式线程的语义边界
（guest 自 spawn 逻辑线程交错），**不是宿主主动并发进入 guest 的理由**。

实施形态（主体阶段）：`host.rs:100` 的 `Arc<Mutex<LoadedWasmPlugin>>` 在 async 化时
**改为 tokio `Mutex`（await 持锁，不因等待释放）**——锁语义与现在等价（串行），只是
等待期间的宿主线程让出；**禁止**改成「await 点释放锁」的细粒度锁（那会让第二个调用
进入 guest 与第一个调用交错 → 插件静态状态竞态 + wasmtime Store 重入 panic）。

### P4 · 13 个调用入口 async 化影响面清单

| 入口 | 位置 | 宿主调用方（async 化影响面） |
| --- | --- | --- |
| invoke_command | component.rs:1048 | host.rs 命令分派 + server 层（HTTP 网关 /api/plugin、WS 帧处理）+ 测试 |
| activate / deactivate | 1008 / 1030 | host.rs 激活流程（已 async） |
| on_startup / on_shutdown | 1099 / 1116 | 宿主启动/关闭流程（已 async） |
| on_terminal_input / output | 1063 / 1078 | terminal 链路（PTY 事件，hot path——需确认调用线程） |
| on_message / on_message_binary | 1130 / 1157 | 消息总线派发（已 async 上下文） |
| on_ws_frame | 1184 | WS 帧处理（hot path） |
| on_session_lifecycle / on_input_submitted / on_process_done | 1229 / 1251 / 1273 | 事件派发 |
| get_manifest / raw_store | 1292 / 1307 | 激活探测 / 测试 |

产出：每个入口的「调用方线程 / 是否 hot path / async 化成本」结论，供主体票排期。
hot path（on_terminal_input / on_ws_frame）优先保 sync 包装（`block_on_async` 驱动 async
入口）——热路径语义不变，收益留在 IO 等待面。

### P5 · 测试基建适配计划

- fixture 构建链：wasip3 化（`scripts/wasip3-toolchain.sh` 单一真源已就位；fixture 工程
  切 WASM_TARGET）；
- 闭环测试：`test_business_endpoints_dual_track_closed_loop` 等加「async store 下逐字节
  不变」断言；
- 性能基线：sync 调用 vs async 调用（`block_on_async` 桥 vs 原生 await）短调用开销对比
  （微基准，~10⁵ 次 host 调用耗时），写入报告。

### P6 · 风险与回退

| 风险 | 等级 | 回退 |
| --- | --- | --- |
| wasmtime-wasi p3 模块实验性（A0-3 已标注） | 中 | wasip2 + async store 过渡（A0-3 §10.6 原文） |
| P1-a fail（sync host fn 在 async store 下不兼容 / block_on_async 桥 panic） | 高 | 双轨：sync 组件走 sync store、wasip3 组件走 async store（实例级分派）；或 host_impl 渐进 async 化 |
| 热路径（PTY/WS）async 化性能回退 | 低 | hot path 保 sync 包装（P4 已留） |
| 双端分叉扩大（48 async vs 47 p2 sync） | 已接受 | ADR 0019 既有决策；双端对齐时统一 |

---

## 4. 关键设计决策（前置产出，供主体票采纳）

- **D1 · block_on_async 桥的保留与淘汰**：前置与主体初期保留（sync host fn 的驱动桥，
  三路径重入安全已实现）；主体后续若将 IO 原语 async 化（§14 协作式线程路径的前置），
  逐原语淘汰该桥。**不一次性全改**（原设计备选 A 被否的教训）。
- **D2 · 同实例串行**：见 P3 红线。std Mutex → tokio Mutex（await 持锁），串行语义不变。
- **D3 · sync host fn 与 async 组件互操作**：以 P1-a 实证为准；预期 wasmtime async store
  下 sync 注册的 host fn 可被 async 组件调用（fiber 内同步执行，不 yield），无需 20 组
  全量 async 化；若实证不支持，则主体票需引入 func_wrap_async 包装层（逐原语评估）。

---

## 5. 移交清单（主体实施票 A0-3-main，前置完成后立项）

> ✅ 前置已完成（2026-09-21）：P1 三场景探针全绿、P2 结论写回、P3 红线落稿、P4 面清单完成、
> P5 基线数据 + 适配计划、P6 风险表复核——全部见 report.md；零回退硬证据 = `--test-threads 1` 全量
> **1077/0**（48.9s，确定性；默认 16 线程并行下 wasm_runtime 集成测试族有既有时序 flake，
> 实测基线即存在且与探针有无无关，见 report.md §6）。
> 以下为 A0-3-main 立项即用清单（§7 移交输入与之一致）：

1. `Store::new` → `Store::new_async`（component.rs:779）+ 13 入口 async 化（P4 面清单）；
2. `host.rs:100` std Mutex → tokio Mutex（P3 红线实施）；
3. 资源限制 async 语义适配（P2 结论）；
4. 4 个桌面插件 wasip3 重建 + 产物切换（构建链已就绪）；
5. 双轨期：sync 组件（旧产物）与 async 组件共存验证；
6. A0-5 wasi2 清理（wasip3 全覆盖后单独票）。

## 6. 验收（前置 done 定义）

- P1 三场景探针全绿（含实际输出贴档）；
- P2 调查结论写回 auth-center-spec.md A0-3 备注；
- P3 红线条款落 auth-center-spec.md A0-3 + AGENTS §7；
- P4 面清单完成（13 入口 × 调用方/线程/成本）；
- P5 基线数据 + 测试适配计划；
- P6 风险表复核，无新增 blocker；
- 既有测试零回退（改动仅新增探针/文档，不碰生产路径）。
