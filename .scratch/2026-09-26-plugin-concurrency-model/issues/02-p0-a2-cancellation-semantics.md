# 02 — P0-A2 取消语义：`call_concurrent` task 在三条恢复路径下的语义

**Type:** research
**Spec:** `../spec.md`（§11 前置待办 2 + §3.2 I3 / §7 风险 A2）；CM-async spec §5.3 A2 / §2 F8·F9
**Blocked by:** None — can start immediately
**Status:** ✅ done（2026-09-26）——结论见下；**附带一条对 P1 决定性结论的更正**（§5），需票 01 复核

---

## 0. TL;DR（给票 06 的三句话）

1. **trap 语义不变**：属主模型下一个 task trap 仍然污染**整个 store**（实测：之后所有调用报
   `cannot enter component instance`），所以「trap → 整体重载」是唯一恢复，**不能**做「只丢那个 task」
   （wasmtime 拿不到 task 句柄，`#11833` 未实现）。I3 因此可以定义成「语义与今天逐字等价 + 显性化」。
2. **取消的唯一手段是丢 store**（实测：丢 store 干净、无 panic、挂起中的宿主 future 会被 drop，
   但 **guest 侧等待之后的代码不会执行**——没有 guest 取消清理）。三条路径里有**两条必须显式丢 store**
   （停用、应用退出），一条是顺带丢（trap→重载）。
3. ⚠ **新发现，直接威胁 spec 的 I2/G1**：一个 task 真的停在 Pending 的宿主 future 上时，
   它所在的 component 实例保持 `do_not_enter`，此后**对同一实例的新调用被无限期推迟**（直到那个
   挂起任务跑完）。这与「慢调用不再堵死该插件」的目标直接冲突，必须由票 01（官方 async 测试程序）
   复核后才能给 I2 下定义。

---

## 1. 探针实测（A2，CM-async 探针新增 `probe/src/a2.rs`）

复现（同 CM-async 探针，共享 target 目录）：

```bash
cd .scratch/2026-09-26-wasmtime-cm-async-eval/probe
~/.cargo/bin/cargo run --target-dir ../../../bedcode-desktop/src-tauri/target
~/.cargo/bin/cargo test --target-dir ../../../bedcode-desktop/src-tauri/target   # 全绿
```

guest 为手写 async 组件（与原探针同形状 + 一个会 trap 的导出 + 等待之后调 `mark` 的探针）。
宿主侧可观测面：`slow`（async import）进入次数 / await 之后完成次数 / **挂起途中被 drop 次数**
（`PendingGuard`）/ 同步 `mark` 次数。

| ID | 断言 | 实测结论 |
| --- | --- | --- |
| **A2.1** | 任务挂在 Pending 的宿主 future 上时 `drop(store)` | ① 作用域退出时任务**仍在状态表内**（size=5）——F9 的「停滞而非取消」；② `drop(store)` **无 panic**（debug_assertions 开启）、进程存活；③ 挂起中的**宿主 future 被 drop**（`slow_aborted=1`）；④ **guest 等待之后的代码未执行**（`mark_calls=0`）；⑤ 丢 store 后同 engine 的新 store 可正常调用 → **重载恢复成立** |
| **A2.2** | 请求方放弃（drop `call_concurrent` 的 future）后任务会怎样 | 任务**继续跑**：在后续作用域内放行 ⇒ 宿主 future 完成、guest 等待之后代码执行、状态归空。**必须在 `run_concurrent` 作用域内等待**才有推进（实测把等待写到作用域外 → 永不推进） |
| **A2.3** | 一个 task trap 是否污染整实例（**I3 决定性**） | trap 以 `run_concurrent` 自身返回 `Err`（`wasm trap: unreachable`）出现；此后同 store 任何调用被拒：`wasm trap: cannot enter component instance` ⇒ **与经典模型完全一致：一个 task trap = 整实例不可用，唯一恢复是丢 store**。trap 后状态表仍有残留（size=6），直到 store 被丢 |
| **A2.4** | 挂起任务是否阻塞同实例的新调用（**威胁 I2**） | **阻塞**。挂起任务让实例保持 `do_not_enter`，新调用进 `pending` 队列被推迟；放行挂起任务后，被推迟的调用补上完成（`v=9`）⇒ 是「推迟」不是「死锁」。机制见 `concurrent.rs`：`GuestCall::is_ready` 的 `do_not_enter` 分支 + `enter_instance`/`exit_instance`/`partition_pending` |

补充事实（源码核对，非推断）：

- `Func::call_concurrent` 的文档原文：取消「only possible by dropping the store」，
  「not possible to remove just one task from a store」；drop 宿主侧 future 只是「放弃得知结果的
  能力」，任务会继续推进并继续回调宿主（`concurrent/func.rs:54-70`）。
- `JoinHandle::abort` 确实存在且是 `pub`（`concurrent/abort.rs`），但它服务的是
  **guest→host 的 async import future**（`HostTaskState::CalleeRunning`）与 `Accessor::spawn` 的宿主
  accessor 任务，**不暴露给 embedder 去取消 `call_concurrent` 建的 guest task**。
- embedder 也没有「枚举 store 里还剩几个 task」的正路：`Store::async_call_stack` 只给当前调用栈，
  `concurrent_state_table_size` / `assert_concurrent_state_empty` 都是 `#[doc(hidden)]`／仅供上游自测
  （后者还是 `assert!`——中途调用会 panic，见探针开发记录）。
- 源码层面的机制解释（A2.4）：宿主→guest 调用的 work item 闭包先 `enter_instance`（置
  `do_not_enter`，`concurrent.rs:2652`），guest fiber 在宿主 future 上挂起时该闭包**尚未返回**，
  `exit_instance`（`concurrent.rs:2662`）不会执行 ⇒ 实例一直「已被进入」，同实例的新 `StartImplicit`
  只能进 `pending`（判定 `concurrent.rs:757-772`，`enter/exit/partition_pending`
  `concurrent.rs:2004-2055`）。这是 wasmtime 48 事件循环/fiber 模型的性质，不是探针写法问题。

---

## 2. 三条恢复路径：属主模型下的语义 + 丢 store 接线点

现状基线（今天）：`wasm_plugins: HashMap<plugin_id, Arc<Mutex<LoadedWasmPlugin>>>`（`host.rs:60`），
Store 住在 `LoadedWasmPlugin` 内（`runtime/component.rs:1034-1051`），调用 = `run_guest_call` 抢锁 +
`block_on_async`（`commands.rs:137`）。注意两个现状事实（接线时必须知道）：

- **停用不丢 store**：`deactivate_plugin_inner`（`activation.rs:649`）只回收 pty/ws/http/mdns/task 并调
  guest `on_shutdown`/`deactivate`，**不删 `wasm_plugins` 条目**；只有 `rebuild_wasm_instance`
  （`wasm.rs:90`）替换条目、卸载时 `install.rs:198` 移除条目才真正 drop。
- **trap 恢复靠换 map 条目**：`reload_wasm_plugin`（`wasm.rs:142`）= deactivate → rebuild（insert 新
  Arc，旧 Arc 在无在飞调用时随最后一次引用释放）→ register → activate；限频 30s
  （`PLUGIN_AUTO_RELOAD_MIN_INTERVAL_SECS`，`host.rs:32`）。

### 2.1 路径 A：trap → 重载

| 项 | 属主模型下的结论 |
| --- | --- |
| 发生什么 | task trap 从**属主循环的 `run_concurrent`** 返回 `Err`（不是从某个请求 future 返回），store 被污染（`may_enter` 失败）。此后该实例任何调用都失败，与今天 `CannotEnterComponent` 同构 |
| 是否丢 store | **是（唯一恢复）**，与今天同一条路：`rebuild_wasm_instance` 换 map 条目 |
| 孤儿 task 风险 | 无：所有在飞 task 随 store 一起被丢，其中挂起的宿主 future 会被 drop（A2.1④）。但 **guest 侧清理不会跑**（A2.1③），所以宿主侧回收（pty/ws/http/task 注册表）必须照旧全跑一遍，不依赖 guest 自己收尾 |
| 新增接线点 | ① trap 错误从属主循环冒出来时，**必须显式结算**所有正在等待的请求（返回 `AppError::Plugin` + 走 `notify_plugin_runtime_error(plugin_id, "trap", …)`），否则它们会静默挂到 reload 之后或永久挂起；② 属主循环退出后 map 条目里的旧 store 还被属主任务持有 → **重载前必须先停/丢属主**，否则「重载」只是换了个 Arc，旧实例仍在跑（今天靠 `Arc` 引用计数自然收敛，属主模型下会破） |

**fail-visible 形态**：trap 后排队中的请求逐条收到显式错误（`AppError::Plugin`，文案含 plugin_id 与
trap 原因），前端仍走既有 `runtime_error` 事件；**禁止**让请求排队等「重载完再试」。

### 2.2 路径 B：停用 purge

| 项 | 属主模型下的结论 |
| --- | --- |
| 发生什么 | 停用要回收 pty/ws/http/mdns/task + guest `on_shutdown`/`deactivate`。但**挂起的 guest task 会挡住 `on_shutdown`**（A2.4：新调用进 pending，直到挂起任务跑完）——若不先丢 store，停用会卡在「等一个永远不会完成的 task」 |
| 是否丢 store | **是，且必须排在最前**：`停属主 → 丢 store → 再做宿主管回收 → 最后按既有顺序尝试 on_shutdown` |
| 孤儿 task 风险 | 无（随 store 丢）。但顺序变了：**`on_shutdown` 只在「属主已停且 store 尚可调用」时才可能跑**——实际实现上更可能是「先丢 store ⇒ 根本没法再调 `on_shutdown`」。这是与今天的**行为差异**，必须显性化 |
| 新增接线点 | ① `deactivate_plugin_inner`（`activation.rs:649`）最前面插入「停属主 + 丢 store」一步；② 停用后再重装/再激活时走 `rebuild_wasm_instance`（今天 reload 已经这么干，停用不激活的路径需要票 06 决定是否也重建实例）；③ 卸载路径 `install.rs:198` 移除条目时同样要停属主 |

**fail-visible 形态**：停用时若因「有在飞 task」而放弃调用 guest `on_shutdown`/`deactivate`，
必须 `warn!` + 计数（`deactivate_skipped_inflight`）+ 前端可见提示，**禁止**静默跳过
（guest 可能留了临时文件/锁；宿主不能替它假装收尾成功）。这是本票要求的「新旧语义不得静默变化」的
唯一实质差异点。

### 2.3 路径 C：会话销毁 / 应用退出

| 项 | 属主模型下的结论 |
| --- | --- |
| 发生什么 | 退出走 `deactivate_all`（`activation.rs:34`，lifecycle 优先级 20；PTY 引擎全量回收在优先级 10，`system/lifecycle.rs:259`）。今天逐插件串行 deactivate，guest `on_shutdown` 同步跑完 |
| 是否丢 store | **是**：属主模型下「退出 `run_concurrent` 作用域」不足以收尾（A2.1①：任务留在表里停滞），**必须 drop store** 才真正回收（A2.1② 证明这一路径干净） |
| 孤儿 task 风险 | 无，但顺序有讲究：PTY 引擎在优先级 10 已经先杀，挂在 PTY future 上的 task 会被以错误唤醒或直接随 store 被丢——都安全，但要保证属主任务在进程退出前被 abort（否则 `tokio::spawn` 的属主可能活过 shutdown 钩子） |
| 新增接线点 | ① `deactivate_all` 内先「停全部属主 + 丢全部 store」再跑现在的 deactivate 列表（停用会变成「store 已丢 ⇒ 跳过 guest on_shutdown」，按 2.2 的 fail-visible 记）；② 属主任务的 `JoinHandle` 要被 join/abort，且 drop 顺序保证 store 在属主 future 之前不可达 |

**fail-visible 形态**：退出时每个被放弃的 guest `on_shutdown` 记 `warn!`；进程退出码/日志里能看到
「N 个插件因在飞 task 未收到 on_shutdown」。

### 2.4 顺带结论：请求方放弃 ≠ 取消（A2.2）

属主模型下前端超时/断连会 drop 等待 future，**任务继续跑并继续产生副作用**（写 PTY、发事件）。
这与今天的 `spawn_blocking` 无法取消是同构的（今天也继续跑），但要显式定义结算口径：
结果无人接收 ⇒ 记 `detached_task_completed` 计数 + `debug!`，**副作用照常生效**
（PTY 写入不能因为「没人等结果」而丢），只有返回值被丢弃。

---

## 3. I3 的建议定义（已写回 `../spec.md` §3.2）

| ID | 定义（建议原文） |
| --- | --- |
| **I3** | ① **trap / panic = 整实例不可用**（与今天逐字等价：A2.3 证明 task trap 同样污染 store）：唯一的恢复是「停属主 → 丢 store → 重新实例化」，仍由 `schedule_plugin_reload_after_trap` 限频调度；**禁止**「只丢那个 task」——wasmtime 不提供句柄（`#11833`）。② trap 发生时**所有在等该实例的请求必须逐条显式失败**（不排队、不静默重试）。③ 停用 / 退出一律**先丢 store 再做资源回收**；因在飞 task 而放弃 guest `on_shutdown` / `deactivate` 时**必须显性告警 + 计数**。④ 请求方放弃等待**不取消**任务，副作用照常生效，返回值丢弃并计数。 |

## 4. 上游缺口判定

| 缺口 | 处置 | 理由 |
| --- | --- | --- |
| 取消单个 guest task（`#11833`） | **不重复提 issue**，按 ADR 偏离记录 | 已有上游 issue 在跟踪；我们要的是「取消整个 store」，现成手段（丢 store）已够用 |
| embedder 看不到 store 里还剩几个 task | 记 ADR 偏离（可观测性受限） | `concurrent_state_table_size` / `assert_concurrent_state_empty` 都是 `#[doc(hidden)]`，测试期可当断言用（中途调用会 panic），生产只能自持计数 |
| **A2.4：挂起 task 阻塞同实例新调用** | **提一个带最小复现的上游 issue（草稿见 §5.1，待用户确认后发）** | 这不是 #11833 的子集：它不是「取消不了」，而是「一个长挂起 task 让同实例**完全**无法再被调用」，直接决定 embedder 能不能做「guest 挂起 + 同期处理命令」的设计；且我们有可复现的最小探针 |
| 退出 `run_concurrent` 作用域时挂起任务静默停滞、无提示 | 并入 §5.1 issue（同一次上报） | embedder 无法察觉「我留下了一个还在跑的任务」，容易写出「退出即回收」的错觉代码 |

**ADR 偏离项**（票 10 收口时写进 ADR 正文）：取消粒度 = store；trap 粒度 = 实例；停用粒度 = 先丢 store
再回收；放弃等待 ≠ 取消；挂起任务对同实例新调用的阻塞（待票 01 复核后定级）。

## 5. 对既有结论的更正 / 新增风险（必须随票 01 一起看）

### 5.1 上游 issue 草稿（待确认后发布到 bytecodealliance/wasmtime）

**标题**：`Concurrent guest task parked on a host future blocks new calls to the same component instance`

**正文要点**（配 `probe/src/a2.rs` 的 A2.4 作为最小复现）：

1. 复现：async 导入的宿主 future 保持 `Pending` ⇒ guest task 停在 `SuspendReason::Waiting`。
2. 观察：同一 component 实例的后续 `call_concurrent` 一直不进（`GuestCall::is_ready` 因
   `do_not_enter == false` 判否，进 `pending`），直到挂起任务跑完、`exit_instance` 触发
   `partition_pending` 才补上。
3. 影响：embedder 想要的「guest 挂起等事件、同时处理其它命令」在同一实例内不可得；且这个
   `pending` 状态对 embedder **完全不可见**（`concurrent_state_table_size` 是 `#[doc(hidden)]`），
   症状表现为「调用永不返回」而不是报错。
4. 期望（任一即可）：① 挂起（`SuspendReason::Waiting`）时释放实例的 entered 状态，使同实例其它
   task 可进入；② 或提供公开 API 让 embedder 查询/取消 store 内 task（#11833 的可观测面对应物）；
   ③ 至少在文档里写明「一个实例内挂起 task 会阻塞同实例后续调用」。

### 5.2 对 CM-async 探针 P1 的更正

原 P1 断言「挂起 task 不独占 store」在本轮复核中**证据不成立**：`select` 里 `run_poke` 先完成，
而放行 permit 的 spin 发生在其后，`run_slow` 在同一轮事件循环里就跑完了（trace 里能看到
`set event … Returned` 紧跟 `PushFuture`）——**slow 从未真正跨轮停在 Pending 上**。
真正的停车场景是 A2.1/A2.4，实测结论相反（见 §1）。已在 CM-async 探针 spec §5.3 记为 A2 结论，
并在 `../spec.md` §7 风险表加一行：**I2/G1 需票 01（官方 `async_*` 测试程序）复核**。

## 6. 给票 06（P1 属主化）的输入清单

1. 属主任务必须**长驻** `run_concurrent` 作用域（不是每请求进出）——否则在飞 task 停滞（A2.2）。
2. 属主必须是 store 的**唯一持有者**；map 条目改成「请求通道 + abort handle + join handle」，
   所有 `Arc<Mutex<LoadedWasmPlugin>>` 克隆点都要改（`commands.rs:59`、`host.rs:317`、
   `activation.rs:385/413/596/679`、`services.rs:292`）——具体清单归票 03。
3. 停属主/丢 store 的**接线点**：`deactivate_plugin_inner`（`activation.rs:649`，最前）、
   `rebuild_wasm_instance`（`wasm.rs:90`，替换前）、`deactivate_all`（`activation.rs:34`，最前）、
   `install.rs:198`（卸载移除）。
4. 属主任务退出 = tokio 任务 abort（比今天的 `spawn_blocking` 不可取消是净收益：今天卡住的 guest
   调用线程会一直占着旧实例的 `Arc`）。
5. 必须有的门禁：trap 后在等请求逐条显式失败；停用/退出跳过 `on_shutdown` 时有告警+计数；
   「放弃等待 ≠ 取消」有计数；`call_concurrent` 的 task 计数（自持，不用 `#[doc(hidden)]` API）。

## 7. 遗留（不阻塞本票结论）

- A2.4 在**真实 wit-bindgen `async: true` guest**（stackless）上是否同样成立 —— 归票 01/04 复核。
- 上游 issue 是否发布 —— 待用户确认。
- trap 后状态表残留（size=6）是否在丢 store 后全部回收 —— A2.1 的 `slow_aborted=1` 表明挂起 future
  会被 drop，但「guest task 表项」的逐项释放没有单独断言；若票 06 需要，可加一条探针断言。

## Acceptance

- [x] 三条路径逐条给出属主模型语义与丢 store 接线点，并标 fail-visible 形态（旧语义不得静默变化）
- [x] 上游 issue 判定 + 处置（不重复提 #11833；就 A2.4 起草新 issue 待确认；其余记 ADR 偏离）
- [x] 结论写回本票 + spec §11 第 2 项 + §3.2 I3 + 供票 06 引用的设计输入
- [x] 附带：更正 CM-async 探针 P1 的证据强度（§5.2），并给出复核归属

## 关联证据

- 恢复路径代码：`src-tauri/src/wasm_core/manager/host/commands.rs`（`schedule_plugin_reload_after_trap:72`
  / `run_guest_call:137` / `with_wasm_plugin_call:166`）、`host/activation.rs`
  （`deactivate_all:34` / `deactivate_plugin_inner:649`）、`host/wasm.rs`（`rebuild_wasm_instance:90` /
  `reload_wasm_plugin:142`）、`host/install.rs:198`、`runtime/component.rs:1034`（Store 所在）、
  `manager/task.rs:551`（`task::purge_for_plugin`，注意：它管的是 **host-task 原语域**，
  **不是** guest wasm task —— 属主模型下 guest task 是**新增的**一类停用资源）
- 探针：`.scratch/2026-09-26-wasmtime-cm-async-eval/probe/src/a2.rs`（A2.1-A2.4）
- wasmtime 源码：`runtime/component/concurrent/func.rs:54-70`（取消缺口原文）、
  `concurrent/abort.rs`（`JoinHandle::abort` 服务对象）、`concurrent.rs:757-772`（`is_ready`）、
  `concurrent.rs:2004-2055`（`enter_instance`/`exit_instance`/`partition_pending`）、
  `concurrent.rs:2652/2662`（调用闭包里的 enter/exit，挂起时 exit 不执行）、
  `concurrent.rs` `make_call` → `Func::call_unchecked_raw`（trap 走
  `invoke_wasm_and_catch_traps` ⇒ `set_trapped`，`runtime/func.rs:1475-1479`）
