# 03 — P0-A3 架构量审计：`run_guest_call` → 事件循环属主的改造面

**Type:** task（审计 + 设计）
**Spec:** `../spec.md`（§11 前置待办 3 + §3「宿主侧架构改造」；CM-async spec §5.3 A3）
**Blocked by:** None — can start immediately
**Status:** done（2026-09-26）——审计 + 设计定稿见 `## Conclusion`；零生产改动

**What to build:** 摸清「每插件实例一把锁 + `spawn_blocking` + `block_on_async`」的全部调用面与接线点，产出属主化改造设计（P1 的施工图）。**本票只审计与设计，不改生产代码。**

现状调用链（已确认路径）：

```text
webview/IPC --plugin_invoke--> manager/host/commands.rs:166 with_wasm_plugin_call
                              └─ :137 run_guest_call
                                   ├─ tokio::task::spawn_blocking
                                   ├─ block_on_ambient(wasm_plugin.lock())   ← 实例锁
                                   └─ std::panic::catch_unwind(call(&mut guard))  ← 单次调用独立
```

**审计清单：**

1. **`run_guest_call` 全部调用点**（逐个列出调用方 + 是否可重入 + 语义）：
   - `manager/host/commands.rs`：`invoke_command`（前台命令面）；
   - `manager/host/activation.rs`：`activate` / `on_startup` / `on_shutdown` / `deactivate`（生命周期，注释注明「WASI 需无 handle 线程」）；
   - `manager/host/services.rs`：`dispatch_process_done` / `dispatch_task_event`（宿主事件回调插件）；
   - 其余入口（总线/bus、events、timer 派发、HTTP 端点、WS 端点、api_registry 互调、`manager/task.rs` 等）——rg 全仓 `run_guest_call|with_wasm_plugin_call|block_on_async` 找全，逐个登记。
2. **状态与校验面**：trap 恢复（`schedule_plugin_reload_after_trap` + `wasm_reload_throttle` 限频）、fuel/指标（`track_call` / `refill_call_fuel` 口径，spec §3.3「task 级计量」）、`wasm_core/config`（StoreLimits / tunables，`runtime.rs:307 wasm_component_model_async(true)`）——这些在属主模型下各自怎么改（P1 的中断影响面）。
3. **灰度接线**：`call_model` 开关（`mutex`（现状） / `event-loop`（新））放哪（候选：`wasm_core/config` 或 store 配置）、默认 `mutex`、P1 验收后切默认并保留一版回退窗口、开关如何同时管住「每实例走哪条模型」。
4. **设计输出**（写给票 06）：属主任务的消息枚举（入队命令/生命周期/停止信号）、oneshot 结果回传、同步立即完成 task 与挂起 task 的调度、Store 独占不变式 I1 的结构保证（消除第二入口编译期/结构锁）、I4 入队顺序 = 启动顺序的队列语义、`run_concurrent` 持续 poll（F9）。

**Out of scope:**

- 不改任何生产代码、不写实现（P1 的事）。
- 不做取消语义设计（票 02）、peer 分类（票 05）、SDK（票 07）。

## Conclusion（2026-09-26，审计 + 设计定稿，供票 06 直接施工）

**Status: done**（零生产改动；本文只含审计事实与设计，不含实现）。

### 1. 现状链路的两个关键事实（审计先决）

**F1（决定性）：今天**每次** guest 调用已经走 CM-async task 机制**——只是作用域是「每次调用一个」。

- `runtime.rs:307` 开 `wasm_component_model_async(true)`；`Config::concurrency_support` 默认 **true**（上游 `config.rs:3350`）。
- `LoadedWasmPlugin` 的每个导出方法（`component.rs:1398-…`）形如
  `block_on_async(async { typed.call_xxx(&mut self.store, …).await })`，`TypedFunc::call_async`
  在 `concurrency_support` 下**走 `call_async_concurrent`**（上游 `func/typed.rs:177-180`）：
  `start_call_concurrent` + `run_concurrent_trap_on_idle(finish)`（上游 `concurrent/func.rs:266-277`）。
- ⇒ 现状模型 = 「每次调用开一个**短命** `run_concurrent` 作用域」；`concurrency_support` 关掉的
  经典路径（`on_fiber`）已不生效。属主化不是「引入新机制」，而是**把短命作用域换成常驻作用域**
  并把调用对象从 `&mut Store` 换成 `&Accessor`。风险面比预想小。
- 附：`Config::async_support()` 在 48 已废弃为 no-op（既有注释已记），无需额外开关。

**F2：属主模型要用的 API 全部是公开的**（无需 `#[doc(hidden)]` 逃生口）：

| API | 上游位置 | 用途 |
| --- | --- | --- |
| `Store::run_concurrent(async \|accessor\| …)` | `concurrent.rs:1186` | 常驻属主作用域 |
| `StoreContextMut::run_concurrent_trap_on_idle` | `concurrent.rs:1197`（`pub(super)`） | 仅 wasmtime 内部；**宿主不可用**，故「单调用作用域」的 trap-on-idle 语义不会被继承 |
| `TypedFunc::start_call_concurrent(store, params)` | `concurrent/func.rs:361` | 同步启动（不阻塞），返回 `TypedFuncCallConcurrent<T, Params, Return>` |
| `TypedFunc::finish_call_concurrent(accessor, call)` | `concurrent/func.rs:149` 附近（async） | 在作用域内 await 完成 |
| `Func::call_concurrent / start_call_concurrent` | `concurrent/func.rs:114/131` | 动态 `Val` 版（本设计不需要，见 §5 闭集 op） |
| `Accessor::with(\|store\| …)` | `concurrent.rs` | 取 `&mut StoreContextMut`（start / 读 store data 用） |

**F3（改造面最大的既存耦合）：能力转发是「宿主 import 处理器内部再调另一个实例」**。

`capability.rs:339-389` 的 `forward_storage_{get,set,delete}` 在**插件 A 的 guest 调用栈内**
（host import 处理器里）`block_on_async(instance_B.lock())` + `call_capability_export` 直调 B。
`host-api-call`（互调）虽走 bus + oneshot（`host_api/api.rs:126-192`，有 timeout），但同样是在
A 的 guest 调用栈内**同步等待**。⇒ 属主模型下这两条都变成「A 的属主在等 B 的属主」：

- 不是新问题（今天 A 的实例锁同样被占满整段等待，行为等价），**但必须防「B 无属主→永久挂起」**：
  B 已停用/未激活时今天立即报错（实例缺失），属主模型下若只是「channel 无人收」会变成静默挂起。
  设计上必须以 `OwnerHandle::is_alive` 前置判定 + channel `try_send` 失败即显性错误（§5.4）。
- 这一条**不在 P1 解决**（彻底解决要 host-storage / host-api-call async 化 = P4 候选 ⑤），
  但它决定了 I2 的表述边界：**I2 是「不阻塞属主循环」**，而不是「不阻塞任何等待」——
  嵌套等待（能力转发 / 互调）在 P1 后仍会占住调用方属主，与今天逐字等价（I5）。

### 2. 全调用点枚举表（无遗漏入口）

走 `run_guest_call` / `with_wasm_plugin_call` 的共 **10 处**（`rg '\.run_guest_call\(|\.with_wasm_plugin_call\('` 全仓仅 3 文件命中，`manager/host/{commands,activation,services}.rs`）；另有 **1 处绕过封装的直锁** 与 **3 处能力转发直锁**，一并登记：

| # | 入口 | 位置 | 现状形态 | 语义 | 属主化后形态 |
| --- | --- | --- | --- | --- | --- |
| 1 | `invoke_wasm_command`（前端命令面 / HTTP `_http_endpoint` / 插件定时器 tick） | `commands.rs:213-262`（锁 `:242`） | `with_wasm_plugin_call` | 前台命令，返回值回调用方 | 入队 + oneshot **等结果** |
| 2 | `dispatch_to_wasm`（bus 消息） | `services.rs:308-327`（`:315`） | `block_on_async` + `with_wasm_plugin_call`（**同步 trait**） | 消息投递，失败仅记日志 | 入队 + oneshot 等结果（调用方是 bus 投递任务，非属主；等结果以保留错误可观测性） |
| 3 | `dispatch_ws_frame`（WS 帧） | `services.rs:333-349`（`:340`） | 同上 | 投递并返回「是否被接收」bool | 入队 + oneshot 等 `bool`（`AtomicBool` 旁路可去） |
| 4 | `dispatch_process_done`（host-process 完成回调） | `services.rs:102-133`（`:120`）；调用源 `host_api/process.rs:194` | 同上（`block_on_async` 包住） | 事件回调，尽力而为 | 入队 + oneshot（失败路径保持「记日志不扩散」） |
| 5 | `dispatch_task_event`（host-task 事件） | `services.rs:135-175`（`:155`）；调用源 `manager/task.rs:1000` | 同上 | 同上（`Ok(false)`=旧产物未导出） | 入队 + oneshot（保留三态：投递/未导出/失败） |
| 6 | `activate` | `activation.rs:427` | `run_guest_call` 直调 | 生命周期（置态前置） | 入队 + oneshot 等结果；失败仍走 `mark_error` |
| 7 | `on_startup` | `activation.rs:467` | 同上 | 生命周期（决定 Activated/Degraded） | 同上 |
| 8 | `on_shutdown` | `activation.rs:686` | 同上 | 生命周期（**受 I3③ 约束**） | 同上，但停用顺序改为「先停属主」⇒ 见 §5.6 的取舍 |
| 9 | `deactivate` | `activation.rs:714` | 同上 | 生命周期 | 同上 |
| 10 | （空位）`reload`/`rebuild` 不直接调 guest，经 8/9 组合 | `wasm.rs:142-172` | — | 停用→重建→注册→激活 | 停属主插在步骤 1 与 2 之间 |
| 11 | **`call_plugin_capability_export`（绕过封装直锁！）** | `host.rs:305-334`（`:326` `instance.lock().await`） | 直接 `Arc<Mutex<LoadedWasmPlugin>>` + `call_capability_export` | 宿主中间件（auth-policy）取策略 | 入队 + oneshot 等结果；**这是唯一不经过 trap 恢复封装的调用点**，属主化后必须并入同一门面 |
| 12 | `auth_center_candidates` | `host.rs:342-365`（`:355` 锁读 `exported_capabilities`） | 锁读元数据 | 只读发现 | **不进属主**：元数据外提（§5.2） |
| 13 | `register_system_capabilities` | `activation.rs:594-627`（`:606` 锁读） | 锁读元数据 + 注册句柄 | 装配 | 元数据外提 + 注册新句柄类型 |
| 14 | `forward_storage_{get,set,delete}`（能力转发，**guest 栈内嵌套调用**） | `capability.rs:339-389`（3 处 `block_on_async` + `lock`） | 直锁 B 实例并同步等 | 系统组件接管 host-storage | 走 B 的属主门面；无属主即显性错误（§5.4） |
| 15 | `activate_plugin` 预打开目录漂移检测 | `activation.rs:384-400`（`:391` 锁读 `preopened_dirs`） | 锁读元数据 | 只读 | 元数据外提 |
| 16 | 装配表写入点：启动扫描 / zip 安装 / 卸载 / 重建 | `host.rs:195-197`、`install.rs:43-47`、`install.rs:198`、`wasm.rs:108-118` | map 插入/移除 | 生命周期 | 插入/移除前先停属主（§5.6） |
| 17 | 测试脚手架（`raw_store()` / 直接 `wasm_plugins` 读写） | `runtime/tests/{a03_probe,engine_limits}.rs`、`host/tests/{wasm_flow,system_component}_test.rs` | 直锁/直搬 store | 测试专用 | 属主模型下 `raw_store` 不可表达 ⇒ 测试按模型分派（§6.4） |

**同步调用源（谁在同步上下文里发起 guest 调用）**：`bus` 投递任务（`bus.rs:350`）、
`host_api/process.rs:194`、`manager/task.rs:1000`、`capability.rs` 三处转发 —— 这四处**必须保留同步门面**
（`block_on_async(oneshot)`），属主门面因此需要「同步 API + 异步 API」两层。

### 3. 状态与校验面：trap / fuel / config 改造点

| 面 | 现状 | 属主模型下的改造 |
| --- | --- | --- |
| **trap 恢复** | 每次调用 `catch_unwind` → `with_wasm_plugin_call` 捕获 → `notify_plugin_runtime_error("trap"/"panic")` → `schedule_plugin_reload_after_trap`（限频 30 s） | ① **`catch_unwind` 仍然要在**（宿主函数 panic 如嵌套 `block_in_place` 依旧存在，`run_guest_call:150`），但要包在**属主任务外层**：属主任务 panic = 实例不可用；② trap 从「某个请求的 future」变成「`finish_call_concurrent` 的 Err / 属主 `run_concurrent` 的 Err」：前者只结算该请求（但**实例已被污染**，故同时触发实例级失败）；后者=属主退出；③ 两条路都收敛到同一函数 `on_owner_instance_failed(plugin_id, reason)`：**先**逐条显式失败所有在等请求（`AppError::Plugin` + plugin_id + trap 原因）**再** `schedule_plugin_reload_after_trap`（票 02 §2.1 fail-visible 形态）；④ 禁止「排队等重载完再试」。 |
| **fuel / 指标** | `exports()` / `call_capability_export` 每次调用前 `refill_call_fuel()`（`component.rs:1299-1315`）：结算上一区间消耗 + `set_fuel(budget)`；`track_call()` RAII 计时器按「调用」计数 | ① 口径从「调用」变「**task**」：`refill` 仍在 start 前（同步、在 accessor 内）执行；`track_call` 的 RAII 计时器要绑到 **start→finish** 的 task 生命周期（用一个 `TaskTimer` 放进 inflight 槽，完成时落账）；② **多 task 并发时 `set_fuel` 是全局量而非 per-task**（探针 P3 实测可观测：10000000 → 9999961）⇒ 必须记文档：燃料仍是**实例级**预算，不是 per-task 累加；`fuel_per_call` 语义 = 「两次续费之间的预算」，并发 task 共享。此差异写入 ADR（票 10）+ 监测面注释，禁止假装 per-task。③ `record_fuel_consumed` 的差值结算在并发下会互相干扰 ⇒ 改为「仅在没有在飞 task 时结算」，或按「续费点差值之和」记（设计选后者：每次续费前结算，累加即可，天然无干扰）。 |
| **config** | `CoreConfig{engine, store}`（`wasm_core/config.rs:99-104`），`WasmRuntime::set_config` 只影响新建 Store | 新增 `CoreConfig.call_model: CallModel`（`mutex`(默认) / `event-loop`），**实例级快照**（建实例时读一次，存进装配表条目）。语义：切换只影响**此后重建/新激活**的实例；已运行实例不变，直到下一次 reload/rebuild。理由：Store 与实例绑定，热切会破坏 I1（第二入口）。 |
| **`is_activated` / `PluginServices::is_activated`** | 读 `plugins` 表（非实例） | 不变（不碰实例） |
| **host-task 域（`manager/task.rs`）** | 独立引擎 + `purge_for_plugin` 回收在册任务 | 不变；但属主模型新增一类停用资源 **guest task**（票 02 §关联证据已点名）⇒ `purge_for_plugin` 之后必须再停属主（顺序见 §5.6） |

### 4. `call_model` 灰度开关：接线与回退窗口

```text
CoreConfig.call_model: "mutex" | "event-loop"   （默认 mutex；文件 wasm-core.json + set_config 运行时覆盖）
        │  实例创建/重建时读取一次（快照进装配表条目）
        ▼
InstanceSlot::Mutex(Arc<tokio::sync::Mutex<LoadedWasmPlugin>>)     ← 现存代码原样保留
InstanceSlot::Owner(OwnerHandle)                                    ← 新属主任务
        │
        ▼  统一门面（唯一入口，两者同构）
PluginHost::call_guest(plugin_id, GuestOp) -> crate::Result<GuestReply>          （异步）
PluginHost::call_guest_blocking(plugin_id, GuestOp) -> crate::Result<GuestReply> （同步桥，供 bus/process/task/转发四处）
```

- **回退窗口**：开关双向可切；`mutex` 分支 = 今天逐字节代码（`run_guest_call` 保留），
  回退后行为与今天一致（spec §8 A4 可验证）。P1 验收通过后默认切 `event-loop`，保留一版回退窗口（spec §6）。
- **谁决定模型**：装配表条目持有 `call_model` 快照；`rebuild_wasm_instance` 按当前 config 重建（=
  reload 即切换的语义）。测试可直接按模型构造实例，无需全局开关（并行安全）。
- **I1 结构保证**：`InstanceSlot` 是**唯一**存放 Store 的地方；`LoadedWasmPlugin` 的字段可见性收紧为
  `owner` 模块内可见（`pub(in crate::wasm_core::manager::host)`），装配表只暴露 `InstanceSlot`。
  结构锁（票 06 落地）：`wasm_plugins: HashMap<String, Arc<WasmInstanceEntry>>` 且 `WasmInstanceEntry`
  无公开 `store`/`instance` 访问器；`LoadedWasmPlugin::new` 之外的构造点归零。

### 5. 属主任务设计（票 06 施工图）

#### 5.1 任务拓扑

```text
PluginHost
  └─ wasm_plugins: HashMap<plugin_id, Arc<WasmInstanceEntry>>
       WasmInstanceEntry {
         meta: InstanceMeta { exported_capabilities, preopened_dirs, created_at, call_model },  // 不可变，宿主侧直读
         slot: InstanceSlot,                                                                    // Mutex | Owner
       }

Owner 任务（每个 event-loop 实例一个）：
  tokio::spawn(owner_loop(store, instance, typed_funcs, rx, stats))
    └─ store.run_concurrent(async |accessor| { loop { … } }).await   ← 常驻，整个实例生命周期只进一次
```

**属主任务必须长驻 `run_concurrent` 作用域**（票 02 §6.1：作用域退出→在飞 task 停滞）。
归属与回收：`WasmInstanceEntry` 被 drop（map 移除）⇒ `OwnerHandle` drop ⇒ 先发 `Shutdown`
（属主自退）再 `abort`（兜底），最后 store 随属主任务 drop（票 02 A2.1 证丢 store 干净）。

#### 5.2 消息与结算

```rust
enum OwnerMsg {
    Call { op: GuestOp, reply: oneshot::Sender<crate::Result<GuestReply>> },
    Shutdown { reply: oneshot::Sender<ShutdownReport> },
}

/// 闭集 op（**不用泛型/闭包**：类型擦除会丢 bindgen 的 typed 通道，见 §5.3）
enum GuestOp {                                   // Params/Return 类型逐条对应现导出
    InvokeCommand { name: String, args_json: String },        // (String,String)->String
    Activate, OnStartup, OnShutdown, Deactivate,              // ()->Result<(),String> / ()->()（逐条核 WIT）
    OnMessage { topic: String, sender: String, payload_json: String },
    OnMessageBinary { topic: String, sender: String, payload: Vec<u8> },
    OnProcessDone { event_json: String },
    OnTaskEvent { event_json: String },
    WsFrame { /* client | server 两态，字段同现 WIT */ },
    CapStorageGet { key: String }, CapStorageSet { key: String, value: String },
    CapStorageDelete { key: String }, CapAuthVerifyDeviceToken { token: String },
}
enum GuestReply { Unit, Code(i32), Str(String), Bool(bool), StartupResult(Result<(),String>) }
```

- **闭集而非泛型**的依据：`TypedFunc::start_call_concurrent` 的 `Params/Return` 是编译期类型，
  泛型 erase 会退化成动态 `Val` 编解码（多一层手工 canonical ABI，且丢掉类型检查）。
  现存泛型出口只有 **4 个具体实例化**（`call_plugin_capability_export`：storage get/set/delete、
  auth-policy verify-device-token，见 `auth_center.rs:163` 与 `capability.rs:348/367/384`），
  闭集枚举完全覆盖，且与 `PROBE_CAPABILITIES` 的闭表设计同构。**新增能力路由时必须同步加 op**
  （票 06 加结构锁：`ROUTABLE_CAPABILITIES` 与 `GuestOp` 的 Cap* 分支一一对应）。
- **oneshot 结算**：每个请求一个 `oneshot::Sender`；属主在 start 成功即挂 `inflight`，
  finish 完成后 `send`。**请求方放弃等待（drop rx）不取消任务**（票 02 A2.2/I3④）：
  属主忽略 `send` 的 Err + `detached_task_completed` 计数 + `debug!`。
- **队列**：`mpsc::channel(OWNER_QUEUE_CAP)`（建议 64，常量进 `config`）；**入队满 = 显性错误**
  （`AppError::Plugin("plugin call queue is full")`），**禁止**无限缓冲（内存放大面）与静默丢弃（fail-visible）。

#### 5.3 调度循环（I2 / I4）

```rust
store.run_concurrent(async |accessor| {
    let mut inflight: FuturesUnordered<InFlight> = FuturesUnordered::new();
    loop {
        tokio::select! {
            biased;                                     // I4：先收请求再结算——启动顺序严格 = 入队顺序
            maybe = rx.recv() => match maybe {
                Some(OwnerMsg::Call { op, reply }) => {
                    match start_call(accessor, op) {     // 同步：refill fuel + start_call_concurrent
                        Ok(task) => inflight.push(task), // 立即完成的 task 也在下一轮 select 结算
                        Err(e)   => { let _ = reply.send(Err(e)); }
                    }
                }
                Some(OwnerMsg::Shutdown { reply }) => { /* 结算在飞 → reply → break */ }
                None => break,                           // 通道关闭（entry 被 drop）
            },
            Some(done) = inflight.next(), if !inflight.is_empty() => { /* send + 计时落账 */ }
        }
    }
})
```

- **I2 表述（按票 01/02 的 A2' 复核结论修正）**：任何单次 guest 调用**不阻塞属主循环**
  ——start 是同步不等待，慢调用挂起为 task，属主继续收 `OwnerMsg`。但 wasmtime 48 的实例级
  `do_not_enter` 语义意味着**同实例后续 task 会被推迟到挂起 task 放行**（票 01 A2'：官方产物同样如此）
  ⇒ **不承诺「同实例并发执行」**，只承诺「属主不被占死、请求不丢、有界失败」。
  ADR（票 10）必须按此写，禁止宣传「插件内真并发」。
- **I4**：`biased` + 单点 start（只在 `rx.recv()` 分支里 start）⇒ 启动顺序 = 入队顺序，
  天然满足；测试用「同步 op 连发 N 条 → 完成序 = 入队序」断言。
- **`select!` 公平性**：`biased` 会饿死结算分支（大量入队时）⇒ 加「每轮最多 start 1 个」即可，
  实际入队速率远低于结算速率；若压测暴露饿死，改为 `select!` 非 biased + 显式 FIFO 队列。
- **完成回调的「先完成后入队」顺序**：`inflight.next()` 分支与 `rx.recv()` 分支同轮时 `biased` 优先入队
  ——语义 = 「新请求先入队再结算旧结果」，对观测面友好（避免结果先于启动）。

#### 5.4 门面与嵌套调用（F3 的处置）

```rust
// 异步门面（Tauri command / 生命周期 / 转发 / 中间件）
async fn call_guest(&self, plugin_id: &str, op: GuestOp) -> crate::Result<GuestReply>
// 同步门面（bus 投递 / process done / task event 三处 + 能力转发）
fn call_guest_blocking(&self, plugin_id: &str, op: GuestOp) -> crate::Result<GuestReply>
          = block_on_async(call_guest(...))
```

- `Mutex` 分支：两个门面都直接落到今天的 `run_guest_call` 实现（I5 by construction）。
- `Owner` 分支：`try_send` + `oneshot`；**属主已停/通道关闭 → 立即 Err**（不静默挂起）；
  队列满 → 立即 Err；属主任务 panic → `JoinHandle` 已结束 ⇒ 后续 `try_send` 失败 ⇒ 同上。
- **嵌套调用（guest 栈内 → 另一实例）**：转发/互调在 P1 保持同步等待（I5），但：
  ① 目标无属主必须立即失败（今天「实例缺失」等价）；② 目标 trap 时的错误传播路径与今天一致；
  ③ **环依赖（A→B→A）今天就会死锁**（A 的实例锁被自身占住），P1 后同样死锁 ⇒ 记 ADR 已知边界 +
  超时兜底记待立项（`api_call` 已有 timeout；能力转发没有——**本票要求 P1 给转发加超时**，把
  「永久死锁」降级为「有界失败」，这是 P1 相对今天的净收益，改动点：`capability.rs` 三处转发包
  `tokio::time::timeout`）。

#### 5.5 trap / panic 语义（I3 落地映射）

| 事件 | 检测点 | 动作 |
| --- | --- | --- |
| 单 task `finish` 返回 trap | inflight 结算分支 | ① `log_trap`（保持现格式：`export` + `trap_detail=?`）② 该请求 Err ③ **实例级失败**：`on_owner_instance_failed` → 剩余在等请求逐条 Err → `notify_plugin_runtime_error("trap")` → `schedule_plugin_reload_after_trap` ④ 属主退出（store 不可再用） |
| 属主任务 panic（宿主函数 panic 穿透） | `OwnerHandle::join` 监测任务 / `catch_unwind` 包属主任务体 | 同上（`"panic"` 分类） |
| `run_concurrent` 顶层 Err | 属主循环退出处 | 同上；若为 `Shutdown` 引起则走正常路径（不触发 reload） |
| 属主任务异常退出但未触发 reload | 装配表条目 `slot.owner.is_alive == false` | 后续调用 Err（fail-visible）；`handle_owner_exit` 里补 `mark_error` |

#### 5.6 停机顺序（I3③ / I6）

```text
deactivate_plugin_inner（activation.rs:649）：
  1. [新增] 停属主 + 丢 store（WasmInstanceEntry.slot → Owner 停止并置 None）
  2. 既有宿主回收（mdns / ws / http / pty / task::purge …，顺序不变）
  3. guest on_shutdown / deactivate 尝试：属主已停 ⇒ **无法调用** ⇒ 显性告警 + 计数
     （`deactivate_skipped_inflight` / `on_shutdown_skipped`），前端可见提示
```
- **这一步是与今天唯一的实质语义差异**（今天 on_shutdown 能跑完）。取舍已在票 02 §2.2 定：**宁可显性跳过也不赌一个挂起 task 永不放行**。票 06 必须落 fail-visible 计数 + i18n 提示（若需前端呈现，走宿主既有 `PLUGIN_RUNTIME_ERROR` 通道，**不新增产品语义**）。
- 备选（票 06 可选实现，需评审）：先「停属主循环入口（不再收新请求）+ 给在飞 task 一个有界宽限
  （如 500 ms，超时即丢 store）」，能跑完的 on_shutdown 照跑。**默认按前者实现**（简单、可预测）；
  宽限方案作为后续可选优化，不得默认开启。
- `deactivate_all`（`activation.rs:34`）：在每个插件 deactivate 之前无需额外动作（逐插件走上面同一条）；
  但**必须保证属主任务在进程退出前被 abort/join**（票 02 §2.3）：`deactivate_all` 末尾加一次
  「残留属主清扫」（`shutdown_all_owners()`）兜底。
- 卸载（`install.rs:198`）与重建（`wasm.rs:108-118`）：**先停属主再动 map**，禁止先替换 Arc。

### 6. 改造量估计（票 06 工时/风险输入）

| 文件 | 改动 | 规模（估） |
| --- | --- | --- |
| `manager/host/owner.rs`（新） | 属主任务 + `GuestOp`/`GuestReply`/`OwnerMsg`/`OwnerHandle`/start-finish 分派 + 结算/计数 | ~450-600 行 |
| `manager/host.rs` | 装配表类型 `WasmInstanceEntry{meta, slot}`；`call_guest` 门面；`call_plugin_capability_export` 并入；元数据外提 | ~150 行 |
| `manager/host/commands.rs` | 10 个调用点改门面；保留 `Mutex` 分支实现 | ~150 行 |
| `manager/host/activation.rs` | 停机顺序（3 处）+ 元数据读取改外提 | ~80 行 |
| `manager/host/services.rs` | 4 个 dispatcher 改同步门面 | ~60 行 |
| `manager/host/wasm.rs` / `install.rs` | 重建/安装/卸载的属主生命周期 | ~60 行 |
| `manager/capability.rs` + `host_api/context.rs` | 提供者句柄类型 + 转发超时 + 无属主显性错 | ~80 行 |
| `manager/runtime/component.rs` | `LoadedWasmPlugin` 拆出 Store/Instance + typed funcs 表；`refill_call_fuel`/`track_call` 适配 | ~150 行 |
| `wasm_core/config.rs` | `call_model` 字段 + 校验 | ~30 行 |
| `manager/task.rs` / `host_api/*` | 同步门面替换（3 处） | ~40 行 |
| 测试（新增） | I1-I6 各 1+ 用例；trap/停机/队列满/放弃等待；两模型对照 | ~600-800 行 |

**总量约 1800-2200 行改动（含测试）**，涉及 11 个生产文件 + 测试。**无 WIT/ABI/SDK 改动**（I5 前提）。
风险集中在三处：① `Component`/`Instance`/`TypedFunc` 的所有权搬迁（`'static` + scope 生命周期）；
② 停机顺序（票 02 的语义差异）；③ 能力转发嵌套（F3）。

### 7. 给票 06 的施工顺序建议

1. **拆 Store/Instance 所有权**（`LoadedWasmPlugin` → `PluginCore{store, instance, typed_funcs}` + `InstanceMeta`），
   先不动调用模型（所有调用仍走 mutex 包装），跑全量测试确认零回归。
2. 新建 `owner.rs`：`GuestOp` 闭集 + start/finish 分派 + `OwnerHandle`；
   在**新测试**里直接驱动属主（不经 PluginHost），验证 I2/I4/fuel/放弃等待/trap 结算。
3. 装配表换 `WasmInstanceEntry{meta, slot}`；`call_guest` 门面双分支；10 个调用点逐个切换；
   `call_model` 开关接线；两模型对照测试（同一用例 × 两模型）。
4. 停机顺序（§5.6）+ 卸载/重建接线 + fail-visible 计数。
5. 能力转发超时 + 无属主显性错（`capability.rs`）。
6. 收尾：桌面端 `cargo test` 全量 + 4 个 wasm-app 契约测试 + 宿主 wasm_flow 集成全绿；
   切默认 `event-loop` 并验证回退窗口；观测面（task 计数/队列深度/放弃计数）接 core-monitor。

### 8. 本票未解决 / 留给后续（写在票 06 的已知边界）

- **I2 的真实边界**：同实例并发不可得（A2'）⇒ 「慢调用不堵死整插件」在 P1 后仍不成立（挂起期间同实例请求被推迟）；P1 的收益是「属主不占死、请求有界失败、trap/停用语义显性化」。**真正修复要等上游或 P3 之后的多实例化**（如 per-session 实例）——记 ADR 待评估，不在 P1 范围。
- host-storage / host-api-call 的 async 化（F3 根治）= P4 候选 ⑤，本票只加超时兜底。
- `raw_store()` 类测试辅助在 `event-loop` 模型下不可表达（属主独占），测试须按模型分派或改为黑盒断言。

**Acceptance:**

- [x] 全调用点枚举表（调用方 / 入口 / 锁行为 / 属主化后形态），无遗漏入口（10 处封装调用 + 1 处直锁 + 3 处转发 + 4 处元数据/装配点 + 测试面，全仓 `rg` 双向核对）。
- [x] trap 恢复、fuel/指标、config 三块在属主模型下的改造点清单（§3）。
- [x] `call_model` 开关接线设计与回退窗口方案（§4）。
- [x] 属主任务设计（消息、oneshot、调度、I1/I4 结构保证）写回本票，供票 06 直接施工（§5）。
- [x] spec §11 第 3 项更新（改造量估计有结论：~1800-2200 行 / 11 文件 / 无 WIT 改动）。
- [x] 零生产改动（本票只读代码 + 上游 wasmtime 源码核对，未改 `src-tauri/src`）。

## 关联证据

- `src-tauri/src/wasm_core/manager/host/commands.rs:137/166`、`manager/host/activation.rs:426-714`、`manager/host/services.rs:103-155`
- `src-tauri/src/wasm_core/manager/runtime/component.rs:1442`（`block_on_async(call_invoke)`）、`runtime.rs:307`（`wasm_component_model_async(true)`）
- `src-tauri/src/wasm_core/host_api/*.rs` 的 `block_on_async` 计数（peer 23 / database 13 / auth 12 / ws 11 / pty 7 …，spec §4.2）——注意计数高 ≠ 要 async，属主化本身不逐个改这些实现