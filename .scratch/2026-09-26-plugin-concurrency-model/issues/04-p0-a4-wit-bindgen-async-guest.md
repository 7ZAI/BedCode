# 04 — P0-A4 工具链：`wit-bindgen` `async: true` 在本仓库工具链下能否稳定出组件

**Type:** prototype
**Spec:** `../spec.md`（§11 前置待办 4 + §5 W5；CM-async spec §5.3 A4 / F7）
**Blocked by:** None — can start immediately（交叉验证既有的 `.scratch/2026-09-25-wasip3-host-api-optimization/issues/01` 探针结论）
**Status:** done（2026-09-26）——**No-Go**（WIT 层 async 在 0.60 + 48.0.3 组合下入口 abort）；旧「debug-assertions 哨兵」根因假设**已证伪**，实测槽值为非零脏值；结论与后续收敛步骤见 `## Conclusion`

**What to build:** 用真实 SDK 工具链验证 guest 侧 async 绑定生成（`wit-bindgen` `feature = "async"` / `async_support`：`block_on` / `yield_async` / futures / streams），并解除或确认一个已知阻塞。

**关键已知阻塞（必须处理，非另起炉灶）**：`.scratch/2026-09-25-wasip3-host-api-optimization/issues/01` §5 已实测两条死路，本票在其上继续：

1. **同步导出 + async import** → guest 经 `block_on` 等待 async import → wasm trap `cannot block a synchronous task before returning`。即 async-lowered import 必须由 async-lifted 导出调用（wasmtime 判据 `may_block(task) = async_function || returned_or_cancelled()`）。
2. **async 导出 + async import** → guest 一进入 async-lifted 导出即 abort：`wit-bindgen 0.60.0/src/rt/async_support.rs:560` 的 `assert!(context_get().is_null())` 失败。
   - **可疑根因（未证实，不下结论）**：wasmtime 48 在 `set_thread` 里当 `debug_assertions` 打开时把 context slot 写成 `[u32::MAX; N]` 哨兵（`runtime/component/concurrent.rs`），与 wit-bindgen 0.60「导出入口 slot 为 0」的假设冲突。
   - **验证方式（本票必做）**：`cargo test --release` 或给 wasmtime 关 debug-assertions 后复跑同一用例，看第 2 条是否消失。（代价：整棵依赖树 release 重编，用共享 `bedcode-desktop/src-tauri/target` 控制。）

**工作内容：**

1. 复现 `.scratch/2026-09-25-wasip3-host-api-optimization/issues/01` §5 第 2 条的 abort（或直接在其探针 crates 上继续），完成根因验证（debug_assertions 假设成立 / 不成立）。
2. release 或关 debug-assertions 下，用**真实 SDK 形状**（`packages/plugin-sdk-desktop/rust`，`wit-bindgen = "=0.60.0"` 锁版，nightly-2026-09-16 + `wasm32-wasip3`，`scripts/wasip3-toolchain.sh`）生成一个含 async import 的 guest 组件并跑通「async 导出 → async import 挂起 → 恢复返回」。
3. 若第 2 条是 debug_assertions 假象：给出 release/CI 下可用的配置（如 CI 用 release 测、或 wasmtime 关 debug 的精确 feature/配置），并验证 `cargo test`（debug）未来可直接用（或给出必须绕的坑）。
4. 若第 2 条真实存在（release 也 abort）：记录完整证据（版本、配置、最小复现），判定 P2（票 07）是否要改走「手写 async lower + 手写组件」的替代路径，或等上游修复；不得把探针接口塞进生产 WIT 兜底（探针纪律）。
5. 记录精确 feature 组合与版本（wasmtime / wasmtime-wasi 48.0.x、Rust nightly、wit-bindgen 0.60.0、target）——作为本票结论的单一事实来源。

## Conclusion（2026-09-26，实测；**No-Go** + 旧根因假设被证伪）

**Status: done**（判定完成；探针资产留在 `.scratch/2026-09-26-plugin-concurrency-model/probe/`，零生产改动）。

### 1. 测量环境（单一事实来源）

| 项 | 值 | 出处 |
| --- | --- | --- |
| wasmtime / wasmtime-wasi | **48.0.3**（探针 Cargo.toml 钉死 `=48.0.3`） | 探针 `Cargo.toml`；与宿主 `Cargo.lock` 一致 |
| wit-bindgen（guest） | **0.60.0**（`=0.60.0`，与生产 SDK 同版） | `probe/guest-async/Cargo.toml` |
| Rust（guest） | **nightly-2026-09-16**（rustc 1.100.0-nightly）+ `wasm32-wasip3` | `scripts/wasip3-toolchain.sh` |
| Rust（宿主探针） | stable | 仓库默认 |
| Engine 配置 | `Config::wasm_component_model_async(true)`（`concurrency_support` 默认 true） | `probe/host-probe/src/main.rs` |
| guest 形状 | 接口形态 import/export 双 `async func`（对齐官方 `round-trip` world）；另有同步诊断导出 `ctx-read` / `install-hook` | `probe/guest-async/wit/async-export-probe.wit` |

构建/运行命令（可复现）：

```bash
# guest（2s）
cd .scratch/2026-09-26-plugin-concurrency-model/probe/guest-async
CARGO_TARGET_DIR=<repo>/bedcode-desktop/target/fixtures RUSTUP_TOOLCHAIN=nightly-2026-09-16 \
  ~/.cargo/bin/cargo build --target wasm32-wasip3 --release

# 宿主探针（首次约 3-6 min，仅依赖 wasmtime + wasmtime-wasi）
cd ../host-probe
CARGO_TARGET_DIR=<repo>/bedcode-desktop/target/concurrency-probe ~/.cargo/bin/cargo build

# 运行（mode: both | concurrent | call_async | poke）
<repo>/bedcode-desktop/target/concurrency-probe/debug/concurrency-owner-probe \
  <repo>/bedcode-desktop/target/fixtures/wasm32-wasip3/release/bedcode_async_export_probe.wasm poke
```

### 2. 判定：WIT 层 async 在当前工具链组合下**不可用**（No-Go）

复现结论与 2026-09-25 票 01 §5 第 2 条**一致**（同一断言、同一位置），且**排除了三个候选解释**：

| 实验 | 结果 |
| --- | --- |
| **E1** `concurrent`（官方形态：`run_concurrent` + `TypedFunc::call_concurrent`） | guest 入口 abort：`wit-bindgen-0.60.0/src/rt/async_support.rs:560 assertion failed: context_get().is_null()` |
| **E2** `call_async`（= 生产 `LoadedWasmPlugin` 现行形态） | 同上 abort |
| **E3** `poke`（**async 导出但不含任何 await**） | **同样 abort** ⇒ 与「async import 挂起」无关，问题在 **async-lift 入口本身** |
| **E4** wasmtime `debug-assertions = false`（`cargo build --config 'profile.dev.package.wasmtime.debug-assertions=false'`，产物与默认档二进制不同：`libwasmtime-07a0…` 156.9MB vs `-81ccd…` 160.1MB） | **仍然 abort** ⇒ **旧根因假设（`set_thread` 的 `[u32::MAX;N]` 哨兵）被证伪**；且 `src-tauri/Cargo.toml` 的 `[profile.test.package.wasmtime] debug-assertions=false`（2026-09-25 起就有）说明旧的 `cargo test` 现场本来也没有哨兵 |
| **E5** guest 特征集对齐官方（`features = ['default','async-spawn','inter-task-wakeup']`，官方 test-programs 用同集合） | 仍然 abort ⇒ 与特征集无关 |
| **E6** 包名去版本（官方 `package local:local;` 无版本；我们原为 `@0.1.0`） | 仍然 abort ⇒ 与包版本无关 |

### 3. 新证据：槽值是「非零脏值」而非哨兵（`ctx-read` 诊断）

在 guest 侧加两个**同步**诊断导出（`ctx-read` 直读 `[context-get-0]`，全程不碰 WASI；`install-hook` 装 panic 钩子打印槽值）：

```text
[poke-only] ① 入口前 ctx[0]=0x100000          ← 全新实例、未调用任何导出前已非 0（同步线程视角）
[poke-only] ② install-hook：OK（钩子已装）      ← 该导出自身写 stderr（触发 async WASI 宿主调用）
[poke-only] ③ WASI 调用后 ctx[0]=0x100000      ← 未变 ⇒ 单纯 async WASI 宿主调用不是污染源
[probe-guest] panic 现场 context slot 0 = 0xffcf0   ← async-lift 入口（另一线程）看到的槽值
```

事实：① **全新实例、零导出调用时槽就已经非 0**（`0x100000` = 1 MiB，形似线性内存地址/堆基址）；
② 同步导出与 async-lift 入口看到的是**不同线程的槽**（`0x100000` vs `0xffcf0`）；
③ 两个值都像**指针而非标记位**。⇒ 在 wit-bindgen 0.60 + wasmtime 48.0.3 的组合下，**async-lift 入口运行时所处线程的组件 context slot 不是 wit-bindgen 期望的 null**，而 wit-bindgen 的契约（`start_task` 必须拿到 null 槽）无从满足 ⇒ 该路径不可用。**这不是「构造错误」也不是「debug 断言」**，而是两条工具链在 context 槽初始化/归属上的不匹配。

**未能收敛到的部分（如实记录，不猜）**：上游官方 `crates/test-programs/src/bin/async_round_trip_stackless*.rs` 在 wasmtime CI 天天绿，说明存在一个「官方环境与我们的差异」尚未定位。已排除的差异见 §2；**未排除**的候选只剩两个方向：
（a）官方 guest 的 **world 完全不含 WASI import**（`local:local` 纯自定义接口），而我们的 guest 是 `wasm32-wasip3 + std`（必然导入 wasi 0.3）——`0x100000` 这个「实例刚建好就非 0」的槽值使这条最可疑；
（b）官方 harness 调用 export 的方式与 `Func::call_concurrent` 之外还有别的初始化（如先跑一个 sync 导出/初始化步骤）。
**继续收敛的具体步骤已写成后续任务（见 §6），不阻塞本票结论。**

### 4. 对 P2（票 07）的 Go/No-Go

- **No-Go（维持 2026-09-25 的裁决，理由更正）**：不把 SDK 的 async 绑定生成路径建在「WIT 声明 `async func` + async-lifted 导出」之上——它在当前锁定组合下**入口即 abort**，与插件是否真的 await 无关。
- **Go（备选路径不变）**：宿主实现侧 async 化（`func_wrap_async`，WIT 保持同步签名）已在 2026-09-25 票 01 实证可用，且**不触发 WIT/ABI/SDK 变更**。插件并发模型 spec §4 的「按需异步化」在实际落地时**优先走这条路**；P3（票 08）若要 async 化 `host-http.fetch`，其 WIT 层 `async func` 方案**当前不可行**，需改判为「宿主实现侧 async（不改 WIT）」或等 §6 的收敛结论。
- **对 spec §5 W1–W6 的影响**：W1（函数级 `async func` 标注）、W2（ABI bump）在当前证据下**不应启动**；W4（手写 `func_wrap_concurrent`）在 `async func` 不可用的前提下无对象。spec 需在 §5 顶部加「前置门禁：WIT 层 async 标注在 A4 收敛前不得启用」。

### 5. 复用资产（留给后续）

| 资产 | 位置 | 用途 |
| --- | --- | --- |
| guest 探针（双 async + 同步诊断导出） | `probe/guest-async/` | 复现入口 abort；`ctx-read` / `install-hook` / panic 钩子取槽值 |
| 宿主探针（4 种调用形态 + WAT 打印） | `probe/host-probe/` | `both/concurrent/call_async/poke` 四形态；`--bin wat` 打印组件 WAT（替代未安装的 wasm-tools CLI） |
| profile 覆盖命令 | `cargo build --config 'profile.dev.package.wasmtime.debug-assertions=false'` | 免改任何 Cargo.toml 做 debug-assertions A/B |
| WAT 取证 | `--bin wat <wasm> [filter]` | 已定位：只有 async-lift 入口与 `callback` 调 `context.set`（guest 侧不存在「提前写槽」的代码路径） |

### 6. 后续任务（本票不做，供立项）

1. **差异收敛**：用官方 `async_round_trip_stackless*` 的**原始 guest**（wasmtime 仓库 `crates/test-programs`，需其构建链）在**同一个宿主探针**里跑，验证「官方产物在本宿主下是否通过」——若通过，逐项比对 guest 差异（WASI 导入 / 目标三元组 / world 形态），定位到单一变量。
2. **无 WASI 变体**：把探针 guest 改成 `#![no_std]` + `alloc`（或 `wasm32-unknown-unknown` + `wit-bindgen` 的 `realloc`），检验「WASI 0.3 导入是否就是槽被写脏的原因」；这是 §3 中未排除的（a）方向。
3. 若最终判定「WIT 层 async 不可用」为长期结论 → 写进 ADR（票 10）作为**偏离条款**，并把「插件需要等待语义」的解法统一收敛到宿主实现侧 async 化 + `host-*` 事件化。

**Acceptance:**

- [x] debug_assertions 假设验证完成（§2 E4：**证伪**；两条不同 wasmtime 构建产物均 abort）。
- [x] 真实 SDK 形状下「async 导出 + async import 挂起/恢复」有可复现判定：**入口 abort，不可用**（E1/E2/E3）。
- [x] 给出 P2（票 07）Go/No-Go 与路径建议（§4：No-Go + 改走宿主实现侧 async）。
- [x] 版本与 feature 组合记录完整（§1）；探针/验证**零生产改动**（全部资产在 `.scratch/**`，未改 `src-tauri/src`、`packages/**`、`bedcode.wit`）。

**Out of scope:**

- 不改生产 `bedcode.wit`、不动 SDK 对外 trait、不发布包。
- 不做 SDK 正式 async 绑定路径的产品化（票 07）。
- 不升级 wasmtime / wit-bindgen 版本。

**Acceptance:**

- [ ] debug_assertions 假设验证完成（release / 关 debug 复跑，结论写下）。
- [ ] 真实 SDK 形状下「async 导出 + async import 挂起/恢复」有可复现通过/判定为不可用的结论。
- [ ] 给出 P2（票 07）Go/No-Go 与路径建议（正式 async 绑定 vs 替代方案）。
- [ ] 版本与 feature 组合记录完整；探针/验证零生产改动。

## 关联证据

- `.scratch/2026-09-25-wasip3-host-api-optimization/issues/01-p3-async-host-import-probe.md` §5（两条死路 + 根因假设 + 工具链版本表）与 §7（遗留：未验证 release 下 async 导出是否可用）
- `packages/plugin-sdk-desktop/rust/Cargo.toml:30`（`wit-bindgen = "=0.60.0", features = ["macros"]`）；`packages/plugin-p3-async-host-import-test/`（既有测试专用 world + fixture 的形态样例）
- `scripts/wasip3-toolchain.sh`（`WASIP3_NIGHTLY` = nightly-2026-09-16 单一事实来源）
- CM-async spec F7（wit-bindgen async 支持）、F10（wasmtime 官方自评「very incomplete」）