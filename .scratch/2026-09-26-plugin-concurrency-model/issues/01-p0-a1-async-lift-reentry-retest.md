# 01 — P0-A1 复测：async-lifted 导出 task 体是否重入

**Type:** research
**Spec:** `../spec.md`（§11 前置待办 1；CM-async spec §5.3 A1）
**Blocked by:** None — can start immediately
**Status:** done（2026-09-26，票 01 结论已写回下方 ## Conclusion；A2' 复核结论见 Conclusion 末节）

**What to build:** 判定 CM-async 探针发现的「async-lifted 导出 task 体重入」异常是**手写组件构造问题**还是 **wasmtime 缺陷**，并给出落地前的处置结论。

探针现场（`.scratch/2026-09-26-wasmtime-cm-async-eval/probe/` D5 判别实验）：

- 手写 **stackless** async 组件里，**async-lifted 导出**的 task 体会被重入一次（guest 的一处同步 import 被调 **2** 次）；
- 同样形状改为**同步导出**则只调 1 次。

即：行为差异与「导出是否 async-lifted」强相关，但探针用的是手写组件文本（模板取自上游 `cancel-host.wast`），未配 `thread.resume-later` / stackful 时可能存在构造污染。**未判定前不得写任何 async 原语**（spec §7 风险 A1）。

**工作内容：**

1. 用上游官方测试程序复测，排除手写组件构造因素：
   - wasmtime v48.0.3 的 `crates/test-programs/src/bin/async_*`（40+ 个组件级 async 测试）；
   - `tests/misc_testsuite/component-model/async/*.wast`（`cancel-host` / `drop-host` / `backpressure-deadlock` 等边界）。
   - 首选路径：在这些官方程序/文本里找「async 导出 + 同步 import + 计数」形状的用例复现重入；或把探针 D5 的形状改写为官方测试程序同款工具链产物（wit-bindgen + 官方 async traits）后重跑。
2. 判定两分支：
   - **构造问题** → 记录正确的 guest 构造方式（stackful / `resume-later` 要求）写进 ADR；
   - **wasmtime 行为/缺陷** → 记录最小复现（可附上游 issue 链接），并评估对 P2/P3 的影响（若重入只出现在特定构造下，给出结构性规避；若影响所有 async 原语，则 P2/P3 需重新评估）。
3. 结论写回：本票 `## Conclusion` + spec §11 第 1 项更新。
4. **（票 02 追加，同一批官方程序里一并测）A2'：挂起任务是否阻塞同实例新调用**。
   票 02 在手写 guest 上实测到（探针 `probe/src/a2.rs` A2.4，`cargo test` 可复现）：
   一个 task 真停在 Pending 的宿主 future 上时，所在 component 实例保持 `do_not_enter`
   （`concurrent.rs:2652` 的 `enter_instance` 已执行、`2662` 的 `exit_instance` 因挂起未执行），
   同实例后续 `call_concurrent` 进 `pending` 队列被**无限期推迟**；放行挂起任务后才补上完成。
   这与「挂起 task 不独占 store、其它命令能同期处理」的目标**直接冲突**，必须用官方产物复核：
   - 若官方 stackless 产物同样如此 ⇒ 属主模型的可得性需重新表述（“不阻塞属主” ≠ “不阻塞同实例其它请求”），
     并按票 02 §5.1 草稿提上游 issue（是否发布待用户确认）；
   - 若官方产物不复现 ⇒ 说明是手写组件构造差异，记下正确构造要求写进 ADR。

**Out of scope:**

- 不改任何生产代码、不写任何生产 async 原语、不动 WIT/ABI。
- 不升级 wasmtime（ADR 0019 锁 48）。
- 不做取消语义（那是票 02 的 A2，已结）；但 A2'（挂起任务阻塞同实例新调用）因同样需要官方程序，在本票一并复核。

**Acceptance:**

- [x] 用官方 async 测试程序/文本复测（或与官方工具链产物对照），给出「重入是否可复现于官方产物」的结论。
- [x] 判定分支明确：构造问题（带正确写法）或 wasmtime 缺陷（带最小复现 + upstream 链接）。
- [x] 结论写回本票 + spec §11 第 1 项。
- [x] **A2' 复核**（票 02 追加）：挂起任务是否阻塞同实例新调用，用官方 async 产物给出结论。
- [x] 探针/复现代码零生产改动（`cargo test` 基线全绿）。

## Conclusion（2026-09-26，票 01 实测）

**判定：A1 重入现象不可复现 —— 原始记录疑为「函数体固有多次调用」的误读，非 wasmtime 缺陷，也非需要特殊构造的组件问题。**

### 证据链（探针 `.scratch/2026-09-26-wasmtime-cm-async-eval/probe/src/a1.rs`，wasmtime 48.0.3，`cargo test` 15 passed）

判别矩阵全部实测：

| ID | 形状 | 结果 |
| --- | --- | --- |
| D1 | async-lifted 导出 + 同步 import（函数体只写 1 处 mark），无 callback、不开 stackful | mark = 1 次 ✓ 不重入 |
| D3 | 同上 + thread.yield 一次 | mark = 1 次 ✓ 不重入 |
| D4 | D1 形状 + 开 stackful | mark = 1 次 ✓ 不重入 |
| D5 | D1 形状 + async lift 带 callback（官方 stackless 形状，结果经 task.return） | mark = 1 次 ✓ 不重入 |
| **D6** | **async-lifted 导出 + async import（waitable 等待）——探针现场完整形状**：mark(等待前) → slow → waitable 等待 → mark(等待后) | slow 进入 1 次 / mark 共 2 次（等待前 1 + 等待后 1）✓ 不重入 |
| D7 | 同一 wait 形状但**同步导出**（对照） | trap `cannot block a synchronous task before returning`（与官方 `callback-yield-then-exit.wast` 断言一致，预期行为） |
| D8 | D6 形状 + 开 stackful | slow 1 次 / mark 2 次 ✓ 不重入 |

**P1 现场复核**：原始探针 `run_poke` 函数体**本身写了 2 处** `call $mark`（thread.yield 前后各一次），实测 mark = 2 次 = 固有调用数（>2 才是重入），**P1 现场亦不重入**。

### 官方对照（静态证据，上游 v48.0.3 仓库）

- `crates/test-programs/src/bin/async_round_trip_stackless_sync_import.rs`：官方工具链（wit-bindgen async trait）产物，「async 导出 + 同步 import」形状，官方测试 `async_round_trip_stackless_sync_import` 断言字符串精确匹配（`entered host` 只出现一次）→ **官方产物不重入**。
- `tests/misc_testsuite/component-model/async/cancel-host.wast`：async lift **无 callback** 且不开 stackful 是官方合法形状（`assert_return (invoke "run")` 通过）。
- `tests/all/component_model/async.rs` `cancel_host_future`：同样无 callback + 不开 stackful 的 async lift，官方测试通过。

### 结论分支

- **不是 wasmtime 缺陷**：官方产物（含与探针同形状的 stackless async 导出 + 同步 import）在官方 CI 天天绿；探针判别矩阵 D1-D8 全部不重入。
- **不是需要特殊构造的组件问题**：无 callback / 开 stackful / 带 callback / 纯同步 import / async import + waitable 等待，六种形状都不重入，无需给「正确 guest 构造方式」约束。
- **原始 D5 记录最可能的解释**：当时把「run_poke 函数体固有 2 处同步 import 调用」误判为「task 体被重入一次」（对照组同步导出只执行一遍、固有调用数也可能被不同函数体形状混淆）；该判别实验代码未存档（git 仅一次提交），无法复核当时的组件文本，但相同形状在今天实测不重入。

### 对 P2/P3 的影响

**无影响**。重入既然不成立，P2/P3（async 原语落地）不需要为「task 体重复执行」做结构性规避；反而解锁：async-lifted 导出可以安全地多次调用同步 import（如 session.input 类短命令），宿主侧无需假设重入。

### 遗留说明（不阻塞）

- 原始 D5 实验代码未存档，无法逐字节复核；若未来官方 async 测试在其它 wasmtime 版本出现重入回归，可复用 a1.rs 判别矩阵（D1-D8）快速复测。
- A2' 风险（02 票产出）已回流本票复核：A2.3 显示 trap 后同 store 调用被拒（`cannot enter component instance`）——那是**经典模型的既有行为**（属主改造前 trap 污染 store），与 A1 重入无关，属 02 票的 I3 范畴。

### A2' 复核：挂起任务是否阻塞同实例新调用（02 票追加，2026-09-26）

**结论：官方产物同样存在「挂起任务阻塞同实例新调用」——这是 wasmtime 48 事件循环/fiber 模型的性质，不是手写组件构造差异；官方测试程序的结构本身就是该行为的间接证据。**

- **官方结构证据**（上游 v48.0.3 `crates/misc/component-async-tests/tests/scenario/round_trip.rs`）：
  官方 `test_round_trip`（call_style 0）用 `FuturesUnordered` **并发 push 3 个** `call_foo`，host 实现 `yield_times(10).await`（每个 guest 调用都会挂起在 host future 上）。在 wasmtime 事件循环下这些调用是**串行推进**的（第一个挂起 → `do_not_enter` → 后两个进 `pending` → 前一个完成触发 `partition_pending` → 依次放行），最终全部完成、断言只验证结果字符串 ⇒ 绿。**这与手写组件 A2.4 实测的「推迟非死锁」（放行后补上完成）完全一致**。
- **机制证据**（02 票已核对源码）：宿主→guest work item 闭包先 `enter_instance`（置 `do_not_enter`，`concurrent.rs:2652`），guest 在宿主 future 上挂起时闭包未返回、`exit_instance`（`concurrent.rs:2662`）不执行 ⇒ 实例持续「已被进入」，同实例新 `StartImplicit` 进 `pending`（`concurrent.rs:757-772`）直到 `partition_pending`（`concurrent.rs:2004-2055`）。**与 guest 是手写还是 wit-bindgen 产物无关。**
- **对属主模型（spec §3 P1）的影响**：I2/G1 的表述需修正——「不阻塞属主」≠「不阻塞同实例其它请求」。在属主模型下，一个实例内挂起 task 期间，对同一实例的新调用是**可完成的串行化**（推迟后补上）而非死锁，但也不具备「同期处理其它命令」的并发性；要拿到真正的并发，只能靠**多实例**（多插件/多会话各自独立 store）或等 wasmtime 上游修复（02 票 §5.1 的 issue 草稿，发布待用户确认）。
- **结论归口**：A2' 结论已写回 02 票 §1/§2/A2.4（一致），本票仅补官方复核证据。
