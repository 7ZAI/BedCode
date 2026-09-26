# Spec：wasmtime CM-async（组件模型异步）能力探索 —— 探针与落地判定

- **日期**：2026-09-26
- **状态**：**进行中**（探针已设计，证据待回填 §5）
- **来源**：用户指令「异步与并发的新模型……`func_wrap_concurrent`……为什么不行？」→「按建议做探针，把这个探索 wasmtime 异步实现的任务写成 spec」
- **范围**：**不改仓库任何生产代码**。探针为独立 crate（`.scratch/2026-09-26-wasmtime-cm-async-eval/probe/`），共享 `bedcode-desktop/src-tauri/target` 以复用已编译的 wasmtime。
- **与 `.scratch/2026-09-26-output-ack-backpressure/spec.md` 的关系**：那个 spec 的 P2（唤醒推送）与本文的「CM-async 化」是**同一问题的两条解法**（见 §7 决策树）。本文只回答「CM-async 这条路在本项目可行吗、代价多少」，不做实施。

---

## 1. 问题：为什么问这个

`output-ack-backpressure` spec 反复撞到同一堵墙：宿主对每个插件实例**一把实例锁**（`manager/host/commands.rs:149` `wasm_plugin.lock()`），任何跨越 `await` 的 guest 调用都会占着它。已确认的三处现实后果：

| 现象 | 出处 | 现状判断 |
| --- | --- | --- |
| 前端必须定时轮询拉输出（洞 ① 无发布者） | `TerminalPreview.vue` `pullTick` | 已知妥协 |
| 非流式 `host-http.fetch` 在 import 内等网络**全程持锁** → 慢 HTTP 堵住该插件所有命令 | `wasm_core/host_api/http.rs:222` | **未处理的尖角** |
| 「guest 挂起等输出、同时处理输入」在经典模型下不可行 | 同上 | 只能靠启发式（F4 输入即时拉取） |

用户指出 wasmtime 48 引入了 `func_wrap_concurrent` 这类细粒度并发控制，可能推翻上述限制。**本文的任务：用探针实测它在本项目的 wasmtime 48 / 组件产物上到底成不成立。**

---

## 2. 已核实事实（本地源码逐条核对，非记忆）

> 全部在 `~/.cargo/registry/src/index.crates.io-*/wasmtime-48.0.3/` 内核对；探针只验证 §3 的行为性问题。

| # | 事实 | 出处 |
| --- | --- | --- |
| F1 | `call` / `call_async` **"require exclusive access to the store until the completion of the call"**；`call_concurrent` "may run **concurrently with other calls to the same instance**" | `runtime/component/concurrent/func.rs:24-31` |
| F2 | `func_wrap_concurrent` 的宿主函数拿到 `&Accessor<T>`：**"a store is not available to `f` across `await` points but it is temporarily available while actively being polled"**；未立即 resolve 时 **guest 侧调用立即返回**（前提：guest 以 `async` 方式 lower） | `runtime/component/linker.rs:583-600` |
| F3 | guest 也可以**同步** lower 一个 concurrent 宿主函数——此时 wasmtime 会**阻塞 guest 直到宿主闭包完成** | 同上（"Wasmtime will manage blocking the guest"） |
| F4 | 配套 API：`call_concurrent` / `start_call_concurrent` / `finish_call_concurrent`、`StoreContextMut::run_concurrent`（**每 Store 一个事件循环**）、`Accessor::spawn`、`FutureReader` / `StreamReader` | `runtime/component/concurrent.rs:1-50` |
| F5 | **能力已在本项目构建内**：`component-model-async` 在 wasmtime `default` features；`tunables.concurrency_support` 默认 `true`（`= cfg!(feature = "component-model-async")`）；宿主已设 `wasm_component_model_async(true)` | `wasmtime-48.0.3/Cargo.toml:default`；`wasmtime-environ-48.0.3/src/tunables.rs:278`；`wasmtime/src/config.rs:2658`；`bedcode-desktop/src-tauri/src/wasm_core/manager/runtime.rs:307` |
| F6 | **`bindgen!` 不生成 `func_wrap_concurrent`**：`async \| store` 模式生成的是 `HostWithStore` trait + `Access`（`Access` 内部就是 `StoreContextMut`），注册仍走 `func_wrap_async` → **跨 await 依旧持 store** | `crates/component-macro/tests/expanded/direct-import_concurrent.rs:181`；`wasmtime/src/runtime/component/concurrent.rs:216`（`Access` 定义） |
| F7 | guest 侧 **WIT→绑定支持 async ABI**：`wit-bindgen` 的 `async: true`（feature `async`）+ `async_support`（`block_on` / `yield_async` / futures / streams） | `wit-bindgen-0.60.0/src/lib.rs:846-902` |
| F8 | **取消语义缺失**：`call_concurrent` 创建的 task "only possible by **dropping the store**"（#11833 未实现） | `runtime/component/concurrent/func.rs:54-62` |
| F9 | **推进条件**：task 只在 `run_concurrent` 作用域内推进；没有活跃 `run_concurrent` 时 task 停滞 | `runtime/component/concurrent/func.rs:35-50` |
| F10 | **成熟度**：官方文档对 `wasm_component_model_async` 自评 "Wasmtime's support for this feature is **_very incomplete_**"；且 `wasmtime-wasi` 48 **自己没用** concurrent 路径（`p3::add_to_linker` 是同步版） | `wasmtime/src/config.rs:1280`；`wasmtime-wasi-48.0.3/src/p3/mod.rs:170` |
| F11 | 上游有可复用的**手写 async 组件文本**（`tests/misc_testsuite/component-model/async/*.wast`，含 `cancel-host` / `drop-host` / `backpressure-deadlock` 等边界）与 40+ 个 async 测试程序（`crates/test-programs/src/bin/async_*`） | wasmtime v48.0.3 GitHub tree |
| F12 | async lower 的状态机：lowered 异步调用返回 `u32 = subtask_id << 4 \| status`，`1 = STARTED`、`4 = RETURN_CANCELLED`；等待用 `waitable-set.new/join/wait/drop` + `thread.yield` | `/tmp/cmasync-fixtures/cancel-host.wast:30-45,180-215` |
| **F13**（探针实测） | linker 把「WIT 的 async 标注」与「注册方式」**强制配对**：① 同步 WIT import + `func_wrap_concurrent` → 拒（"only for `async func`-typed imports"）；② async WIT import + `func_wrap`/`func_wrap_async` → 拒（"despite the name, these implement a *sync*-WIT-typed function via blocking host code"） | 探针 P0.2 / P0.4 |
| **F14**（探针实测） | 两种宿主函数签名的差异就是「能否跨 await 持 store」：`func_wrap_concurrent` 要 `Pin<Box<dyn Future + Send + '_>>`（HRTB，**不能**标 `+'static`）；`func_wrap_async` 要 **不 pin** 的 `Box<dyn Future>`（全程持 `StoreContextMut`） | 探针编译期报错（先写 `+'static` 被拒） |
| **F15**（探针实测） | 手写 async 组件的工程细节：组件层导出名必须 **kebab-case**；async import 的 lowered core 签名 = `(param args..., retptr) (result i32)`（返回状态字）；等待需 `waitable-set.new/join/wait/drop`，**漏 `subtask.drop` 会 trap `resource has children`** | 探针迭代过程（三个真实报错） |

**F13 合起来的含义（对本项目的直接推论）**：本项目 WIT 里**没有任何 `async` 标注的 import**，`add_to_linker_async` 只是把**同步** import 的宿主实现写成阻塞友好形式。于是任何「要等」的宿主逻辑（如 `host_api/http.rs:222` 在 import 栈内 `block_on_async` 等网络）都独占 `&mut Store`——这正是 §1 三个现象的同一个根因，也说明**要拿到并发必须改 WIT**（把那个 import 声明为 `async`），不是只改宿主实现。

**F5 + F6 合起来的含义**：运行时能力已在，但**代码生成层没有对应开关**——要真用上 `func_wrap_concurrent`，那个 import 必须**手写 linker 注册**（绕开生成的绑定），guest 侧也要改用 wit-bindgen async 模式。这是本路线的真实工程量所在。

---

## 3. 探针设计（判定门）

三个阶段，逐级加难；**任一阶段红即停并按 §6 决策**。全部为独立 crate，零生产改动。

### P0 · 基线与「经典模型现状」机械确认（不依赖 async guest）

| ID | 断言 | 期望 | 意义 |
| --- | --- | --- | --- |
| P0.1 | `Config::wasm_component_model_async(true)` 后 `get_concurrency_support()` | `true` | 印证 F5：能力默认在 |
| P0.2 | 注册一个 `func_wrap_concurrent` 宿主函数（同步 lower 的 guest 调用它，宿主 future 挂起） | **guest 侧被阻塞**，宿主拿不回控制权 | 印证 F3：**没有 async guest 就没有并发** |
| P0.3 | 经典 `call_async` 在 async import（`func_wrap_async` 返回 Pending）挂起期间，另一个 `call_async` 是否能推进 | **不能**（需 `&mut Store`） | 机械确认本项目今天的状态 quo |

### P1 · 决定性测试（async guest + concurrent import）

guest 用 F11 的手写组件文本改写（无额外工具链，`Component::new(&engine, <wast 文本>)` 直接吃文本）：

- host import `slow(v: u32) -> u32`：**`async` lower**，宿主用 `func_wrap_concurrent` + `tokio::sync::Notify` 实现（模拟「PTY 写入后叫醒」）
- host import `poke(v: u32) -> u32`：**同步** `func_wrap`（模拟 `session.input` 这类短命令）
- guest 导出（**async-lifted**）：`run_slow`（启动 `slow` → `waitable-set` 等它完成）、`run_poke`（立即调 `poke`）

| ID | 断言 | 判定 |
| --- | --- | --- |
| P1.1 | 同一 `run_concurrent` 作用域内 `select(fut_slow, fut_poke)` | **必须 `Either::Right`（poke 先完成）** = 挂起的 task 不再独占 store |
| P1.2 | `notify.notify_waiters()` 后 `fut_slow` 完成并返回预期值 | 唤醒链路通 |
| P1.3 | `run_slow` 未完成时宿主 drop 掉整个 store | 观察是否有 trap / 泄漏（对应 F8 的取消缺口） |

**P1.1 是全部决策的唯一硬指标。**

### P2 · 项目形态复现（把 P1 的形状换成终端场景）

- 挂起方 = 「等 PTY 输出」的 concurrent import（`Notify` 由模拟的 ring 写入触发）
- 并发方 = 第二个 guest task 调**同步** import（= `session.input`）
- 附加观测：同一 store 上 `set_fuel` / `get_fuel`、`Store` 级指标（`call_concurrent` 期间）在多 task 下是否仍准确（对齐本项目 `refill_call_fuel` / `track_call`）

---

## 4. 探针工程约束

| 项 | 决定 | 理由 |
| --- | --- | --- |
| 位置 | `.scratch/2026-09-26-wasmtime-cm-async-eval/probe/`（独立 crate，`[workspace]` 空表隔离） | 可评审、可复现；不入任何生产构建 |
| target | `--target-dir bedcode-desktop/src-tauri/target` | 复用已编译的 wasmtime（该目录已 12 G，本机仅 6 G 可用内存，冷编译有 OOM 风险） |
| 依赖 | `wasmtime = "48"`（**default features**，与桌面端一致以最大化复用）、`tokio`、`futures`、`anyhow` | 不引入新 wasmtime 版本（ADR 0019 双端锁死） |
| guest | 手写组件 `.wat` 文本（模板来自上游 `cancel-host.wast` / `drop-host.wast`） | 无需 wit-bindgen nightly 工具链；且 F7 的 guest async 支持在探针里不是被测项 |
| 禁止 | 改动 `bedcode-desktop/**` 任何生产文件；`cargo fmt`/`clippy` 不作用于生产树 | AGENTS §0 最小改动、§11 不夹带 |

---

## 5. 证据（探针实测，2026-09-26）

**探针**：`.scratch/2026-09-26-wasmtime-cm-async-eval/probe/`（独立 crate，`wasmtime = "48"` default features；构建复用 `bedcode-desktop/src-tauri/target`）。guest 为手写组件文本（async ABI，模板取自上游 `tests/misc_testsuite/component-model/async/cancel-host.wast`）；`slow` 挂起式 `func_wrap_concurrent` 模拟「等 PTY 输出」，`mark` 同步 `func_wrap` 模拟 `session.input`，两个 guest 导出均 async-lifted。

复现：
```bash
cd .scratch/2026-09-26-wasmtime-cm-async-eval/probe
~/.cargo/bin/cargo run   --target-dir ../../../bedcode-desktop/src-tauri/target   # 打印结论
~/.cargo/bin/cargo test  --target-dir ../../../bedcode-desktop/src-tauri/target   # 5 passed
```

### 5.1 实测输出

```text
[P0.1] concurrency_support=true 可用；=false 时注册被拒 ✓
[P0.2] 类型系统拒绝：同步 import 只能用 func_wrap/func_wrap_async ✓
       └ …this import's WIT type is a plain (non-`async`) function, but was satisfied with
         `func_new_concurrent`/`func_wrap_concurrent`, which is only for `async func`-typed imports…
[P0.4] async-WIT import 只能由 func_wrap_concurrent 满足 ✓
       └ …this import is declared `async func` in WIT, but was satisfied with a sync-style host function
         (`func_new`/`func_wrap`, or `func_new_async`/`func_wrap_async` — despite the name, these implement
         a *sync*-WIT-typed function via blocking host code, not an `async func` import)…
[P1] 挂起 task 不独占 store：poke=9（先完成） / slow=Some(1007)（后完成）✓
[P2] run_concurrent 退出后 concurrent state 为空 ✓
[P3] 多 task 下 fuel 可观测：10000000 → 9999961（消耗 39）
=== 全部通过：CM-async 在本项目 wasmtime 48 上可用（P1 为决定性指标） ===
```

`cargo test`：5 passed。

### 5.1.1 A2 批次输出（2026-09-26 增补，`probe/src/a2.rs`）

```text
-- A2 取消 / trap 语义 --
[A2.1] 作用域退出后任务仍在表内（size=5，停滞未取消）✓
[A2.1] drop store：进程存活、挂起宿主 future 被 drop ✓
[A2.1] guest 等待之后的代码未执行（取消不做 guest 侧清理）✓
[A2.1] 丢 store 后新 store 可正常调用（重载恢复成立）✓
[A2.2] 作用域内放行 → 脱离句柄的任务继续推进（宿主 future 完成 + guest 等待后代码执行）✓
[A2.2] 任务跑完后 concurrent state 归空 ✓
[A2.3] trap 观测：[run_concurrent 自身返回 Err] error while executing at wasm backtrace:
    0:    0x196 - m!<wasm function 10>: wasm trap: wasm `unreachable` instruction executed
[A2.3] trap 后同 store 调用被拒：wasm trap: cannot enter component instance ⇒ 仍按经典模型污染整实例
[A2.3] trap 后状态表 size=6
[A2.4] 挂起任务阻塞了同实例的新调用（do_not_enter 推迟）⇒ I2 在同实例内不成立 ⚠
[A2.4] 放行后被推迟的调用补上完成（v=9）⇒ 是「推迟」不是「死锁」
```

`cargo test`：9 passed（P0.1 / P0.2 / P0.4 / P1 / P3 + A2.1 / A2.2 / A2.3 / A2.4）。

**探针开发记录（三条真实的坑，写下来免得重踩）**：

1. **`env_logger` + `RUST_LOG=trace` 是定位事件循环行为的唯一手段**（wasmtime 内部走 `log`，
   不接 logger 就只能黑盒猜）。看 `GuestCall::is_ready` / `enter_instance` / `partition_pending`
   三条 trace 就能判定「调用为什么没跑」。
2. **linker 闭包与 store 必须共用同一份宿主状态**（`Host` 里是 `Arc` 计数器）。分开建两份时，
   断言读到的是另一份计数器，表现为「宿主函数明明被调了，计数却是 0」——A2 首版踩了一次。
3. **等待必须写在 `run_concurrent` 作用域内**。A2.2 首版把等待写到作用域外（只为了简化写法），
   任务永不推进，看起来像 wasmtime 的 bug；实际就是 F9。同时注意
   `assert_concurrent_state_empty()` 是上游**自测专用**（`#[doc(hidden)]` + `assert!`），
   任务尚在飞行中调用它会 panic，不能当中途探针用。

### 5.2 P1（决定性）到底证明了什么

探针的 P1 不是「两个 future 择先完成」这种弱断言，而是**构造上**保证了交错：

1. `slow` 挂在 `Semaphore(0)` 上（permit 是存量，不会丢唤醒——早期用 `Notify` 时因「notify 早于 wait 注册」导致假死，改 semaphore 后消失）；
2. 显式等待 `slow_calls == 1`（宿主函数确已进入并挂起）**之后**才放行 permit；
3. 断言 `slow_calls == 1`（挂起真实发生过）+ `poke_value == 9`（短任务在慢任务挂起期间完成）+ `slow_value == 1007`（叫醒后拿到结果）+ `assert_concurrent_state_empty()`。

即：**一个挂在宿主 future 上的 guest task，不再阻塞同一 store 上的其他 guest task**——这正是「guest 自己 await 等输出、同时还能处理输入/resize」所需的那条性质。

### 5.3 异常与未覆盖（必须随结论一起读）

| ID | 现象 | 现状处置 |
| --- | --- | --- |
| **A1** | 手写的 **stackless** async 组件里，**async-lifted 导出**的 task 体会重入一次（guest 的一处同步 import 被调 **2** 次）；同样形状改为**同步导出**则只调 1 次（探针 D5 判别实验） | **已判定（2026-09-26，票 01）：不可复现**。判别矩阵 D1-D8 全不重入（含 async import + waitable 等待的现场完整形状，slow 进入 1 次 / mark 固有 2 次）；官方产物（`async_round_trip_stackless_sync_import`）断言精确匹配亦不重入；原始记录最可能把「函数体固有 2 处同步 import 调用」误判为重入。非 wasmtime 缺陷、非构造问题，无 P2/P3 影响。探针 `probe/src/a1.rs` |
| **A2** | 取消语义（F8） | ✅ **已实测**（2026-09-26，`probe/src/a2.rs`，A2.1-A2.4）：① 退出 `run_concurrent` 作用域不取消任务（任务留在表内停滞）；② **丢 store 是唯一取消手段**，干净无 panic，挂起中的**宿主 future 会被 drop**；③ 但 **guest 等待之后的代码不执行**（无 guest 取消清理）；④ **task trap 仍污染整 store**（之后所有调用 `cannot enter component instance`）⇒ 与经典模型一致，无「只丢一个 task」的可能。结论与 I3 定义见 `../2026-09-26-plugin-concurrency-model/issues/02-p0-a2-cancellation-semantics.md` |
| **A2'**（新，2026-09-26） | **一个 task 真停在 Pending 的宿主 future 上时，所在 component 实例保持 `do_not_enter`，同实例后续调用被无限期推迟**（放行挂起任务后才补上完成）。机制：`GuestCall::is_ready` 的 `do_not_enter` 分支 + 挂起时 `exit_instance` 尚未执行 | ⚠ **直接威胁「挂起 task 不独占 store」的结论**。见下方「§5.4 P1 证据更正」与 A2' 处置 |
| A3 | 宿主调用架构改造量未评估：探针只证明运行时能力，不证明 `run_guest_call`（锁 + `spawn_blocking` + `block_on_async`）能无痛改成「每 store 事件循环属主」 | 属立项阶段的工作量估计，本文不做结论 |
| A4 | guest 侧只验证了手写组件；`wit-bindgen` 的 `async: true` 路径（F7）未实测 | 立项时需用真实 SDK 验证（需 nightly + `build-std`） |

### 5.4 P1 证据更正（2026-09-26，票 02 复核）

P1 当时的断言是「挂起 task 不独占 store：poke 先完成、slow 后完成」。复核发现该证据**不成立**：

- `select(fut_poke, fut_slow)` 里 poke 先被 poll 并完成；放行 permit 的 spin 发生在 poke 完成**之后**，
  此时事件循环在同几轮里就把 slow 跑完了（trace：`handle work item PushFuture` → `set event …
  Subtask { status: Returned }` 紧跟其后）——**slow 从未跨轮停在 Pending 上**。
- 真正的停车场景（A2.1/A2.4）实测结论相反：**停车会阻塞同实例新调用**（A2'）。
- 仍然成立的部分：挂起 task **不独占 `&mut Store`**（多个 task 能同时存在于同一 store，
  事件循环能驱动它们），这与 `call`/`call_async` 的「全程独占」不同；但**同实例新调用会被推迟**，
  而不是「并发通过」。因此「guest 挂起等输出 + 同期处理输入」在 wasmtime 48 上**尚未被证明**。
- 处置：P0 阶段必须用**上游官方 `async_*` 测试程序**（F11）复核 A2'（是否 stackless 产物同样如此），
  复核前不得按「单次调用不阻塞该实例」写实现；结果并入 A1 的复测票。

---

## 6. 判定与决策树

**判定（2026-09-26）**：**P1 绿 ⇒ CM-async 在本项目 wasmtime 48 上确实可用**（§5）。但「可用」≠「该现在就做」——代价集中在四处，且其中三处是契约层改动：

| 改动 | 内容 | 面 |
| --- | --- | --- |
| ① WIT | 把要等的 import（如「等 PTY 输出」「等外部 HTTP」）声明为 `async func` | WIT 变更 → **ABI bump** → SDK 发布 → 双端 WIT 副本同步（ADR 0019/0022） |
| ② 宿主注册 | 那个 import 手写 `Linker::func_wrap_concurrent` 注册（`bindgen!` 的 `async \| store` 生成的仍是 `func_wrap_async`，见 F6） | 手写注册，绕过生成代码 |
| ③ guest 侧 | 需要 async-lowering 的 guest（`wit-bindgen` `async: true`，F7）或自建绑定 | 插件 SDK 改造 |
| ④ 宿主调用架构 | `run_guest_call`（`Arc<Mutex<LoadedWasmPlugin>>` + `spawn_blocking` + `block_on_async` + `catch_unwind`）→ 「每 store 一个事件循环属主 + 多 task」 | 内核级，影响**所有**插件调用面 |

**结论**：
- **技术路线成立**，值得**独立立项**（新 ADR，标题形如「插件并发模型升级：CM-async 事件循环属主」）。
- **不与 `output-ack-backpressure` 的 P2 混做**：P2（宿主限频 publish + 前端订阅）在经典模型下即可拿到「不用轮询」的全部用户可见收益、且不动 WIT/ABI；CM-async 是中期重构，且它同样能解掉 P2 想解的问题（guest 自己 await，宿主那次主动回调可省）。
- **先记一条低成本的即刻收益**：探针同时给 `host_api/http.rs:222`（非流式 `host-http.fetch` 在 import 栈内等网络、独占 store）指明了修法方向——把它改成 async WIT import + `func_wrap_concurrent` 即可不再阻塞交互；这条可独立成一个小的宿主改造提案。
- 立项前必做：**A1 复测**（用上游 async 测试程序，排除手写组件构造因素）、**A2 取消语义**、**A3 架构改造量估计**。

**无论哪种结果，P2 都不浪费**：P2（宿主限频唤醒 + 前端订阅）在经典模型下即可拿到「不用轮询」的全部用户可见收益；将来若切 CM-async，同一需求会退化为「guest 自己 await」，宿主那次主动回调可以撤掉。

---

## 7. 与 output-ack spec 的决策树（合并视图）

```text
想要：终端输出「有数据了」立刻到达前端
 ├─ 路线甲（经典模型，P2）：宿主限频 publish <owner>::pty:output → guest 短回调 → host-events.emit → 前端订阅
 │    改动：宿主写入路径一个出口 + 插件几行；不动 WIT/ABI；风险低
 ├─ 路线乙（CM-async）：guest task 自己 await ring-wait（concurrent import）→ 醒来直接取数
 │    改动：WIT async import + 手写 linker 注册 + guest wit-bindgen async 化 + 宿主调用架构重写；风险高
 └─ 路线丙（现状 + 启发式）：F4/F5 已把交互延迟压到 ≈0；空闲仍有 4 次/秒 invoke
```
本文只决定**路线乙是否成立**；甲随时可做，丙是现状兜底。

---

## 8. 后续待办（立项前必做）

| # | 事项 | 归属 |
| --- | --- | --- |
| 1 | ~~**A1 复测**~~ | ✅ 已结（票 01）：重入不可复现（探针 a1.rs 判别矩阵 D1-D8 + 官方产物静态对照），原始记录疑为「函数体固有 2 处调用」误读；A2' 复核结论：挂起任务阻塞同实例新调用是 wasmtime 事件循环性质 |
| 2 | ~~**A2 取消语义**~~ | ✅ 已结（票 02）；产出：I3 定稿 + 一条待复核的 A2' 风险（挂起任务阻塞同实例新调用） |
| 3 | **A3 架构量**：`run_guest_call` → 事件循环属主的改造面与回退策略 | 立项前 |
| 4 | **A4 guest 工具链**：`wit-bindgen` `async: true` 在本仓库 nightly 下能否出组件 | 立项前 |
| 5 | 小提案：非流式 `host-http.fetch` 独占 store 的修法（async WIT import + `func_wrap_concurrent`） | 可独立立项 |
| 6 | 把本文 §2 的 F13/F14/F15 写进 `docs/adr/0022` 或新的并发 ADR 作为边界依据 | 立项时 |

## 9. 风险与回退

| 风险 | 缓解 |
| --- | --- |
| 探针冷编译 OOM（本机 6 G 可用，wasmtime 峰值 8 G） | ✅ 已发生风险，实现方式为共享 desktop target 目录复用产物（探针编译秒级） |
| 手写 async 组件踩坑（语法/状态机） | ✅ 已踩三个（kebab-case 导出名 / retptr 签名 / 漏 `subtask.drop`），均记录于 F15 |
| 探针结论被当成「可以开工」 | §6 明确：本文只判定可行性，落地须独立 ADR + WIT/ABI 流程（ADR 0019/0022）+ A1–A4 前置 |
| 「P1 绿」掩盖 A1（task 重入）这类未判定项 | §5.3 与结论同页呈现，不单独埋在不注意的地方 |
