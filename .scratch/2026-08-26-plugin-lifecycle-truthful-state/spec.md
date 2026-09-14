# Spec：插件生命周期真实上报 — 让状态与日志反映插件真实加载结果

Status: ready-for-agent
Date: 2026-08-25
Owner: desktop plugin system
Related: `bedcode-desktop/src-tauri/src/plugin/`、`packages/plugin-sdk-desktop/rust/`

---

## 1. 背景与问题

当前桌面端插件「激活成功」的判定与日志**不反映插件内部初始化的真实结果**，证据链如下：

### 1.1 契约层：on_startup 结果在 ABI 上就被丢弃

`packages/plugin-sdk-desktop/rust/wit/bedcode.wit`（L240-246）：

```wit
interface lifecycle {
    activate: func() -> result<_, string>;
    deactivate: func() -> result<_, string>;
    on-startup: func();        // ← 无返回值，失败无法上抛
    on-shutdown: func();       // ← 同上
}
```

SDK 宏 `wasm_entry!`（`wasm.rs:256-262`）进一步用 `let _ =` 显式吞掉用户实现的结果：

```rust
fn on_startup() {
    let _ = <$plugin_type as WasmPlugin>::on_startup();
}
```

而 auto-task / scheduler 等插件的**真正初始化**（hooks 安装、DB 建表、listener 注册、定时器）恰恰在 `on_startup` 里做。

### 1.2 宿主层：失败被降级为 warn，状态照常 Activated

`src-tauri/src/plugin/host.rs:616-639`：

- `activate()` 失败 → 置 Error ✓（正确）
- `on_startup()` 失败 → **仅 `warn!`**，随后 phase 3 无条件置 `Activated`
- 汇总日志 `Initialization complete: N total, X wasm, Y activated` 中该插件继续计入 activated_count

### 1.3 自检通道与生命周期脱节

插件运行期自检失败走 `mark_plugin_error`（`host/services.rs:101-119`），注释明确：「仅通知前端弹窗提示：不置 Error、不持久化」。这是有意的局部故障设计（保留），但它意味着插件内部唯一的结果上报通道对状态机完全不可见。

### 1.4 静态注册路径自相矛盾

- inventory 静态插件插入时即 `Loaded`（host.rs:153-168），但 `auto_activate_from_persisted_state` 过滤掉 StaticRegistry（host.rs:1010），无任何启动路径激活它们
- 后果：其 `on_startup` 永不执行（`notify_startup` 检查 `is_activated`）、`invoke_rust_command` 直接拒绝（commands.rs:30）——而日志却打印 `"Static plugin loaded"`
- 当前 inventory 为 0，属潜伏问题；一旦使用 `submit_plugin!` 即触发

### 1.5 结论

```
现状：持久化(意图) → 尝试 activate() → 同步返回 Ok → 打印 "activated"
                      ↓ on_startup 失败 → warn，照常 Activated
                      ↓ 插件自检失败 → toast，状态不变
```

日志反映的是「宿主调用链没抛错」，不是「插件确认就绪」。

---

## 2. 设计原则（定调）

1. **不加特殊的上报 ABI** —— 不引入 `report_status(kind, msg)` 之类的 plugin→host 推送通道。状态判定完全由**既有生命周期导出的返回值**驱动。
2. **固化流程放 SDK 默认实现** —— 生命周期处理的标准骨架（记录开始/结果、错误上抛、日志规范）写在 SDK 的宏展开代码里；插件只覆盖 `WasmPlugin` trait 对应方法即可扩展，不覆盖走默认。
3. **宿主固定监听生命周期** —— PluginHost 继续按固定时序驱动 `instantiate → activate → on_startup → … → on_shutdown → deactivate`，只升级它对这些返回值的**解释语义**（状态机），不改调用结构。

唯一的契约修正是：让既有 `on-startup` / `on-shutdown` 导出**携带结果**（补全原契约的残缺签名），不是新增接口。所有插件在 monorepo 内一起重编，不存在第三方兼容负担。

---

## 3. 详细设计

### 3.1 WIT 契约修正（abi 单一事实来源）

`packages/plugin-sdk-desktop/rust/wit/bedcode.wit` lifecycle 接口改为：

```wit
interface lifecycle {
    activate: func() -> result<_, string>;
    deactivate: func() -> result<_, string>;
    on-startup: func() -> result<_, string>;    // 补全：携带启动初始化结果
    on-shutdown: func() -> result<_, string>;   // 补全：携带清理结果（可观测性）
}
```

- `activate` / `deactivate` 不动
- 这是对生命周期契约的补全，非新上报通道；旧组件若带旧签名加载，wasmtime 导出校验失败会落入现有 `Failed to load WASM` 错误路径（Error 态展示），行为可控

### 3.2 SDK 固化流程（模板方法）

`packages/plugin-sdk-desktop/rust/src/wasm.rs`：

**`WasmPlugin` trait 用户面 API 保持不变**（`activate()` / `on_startup()` 等方法签名不动，插件零改动升级）。

`wasm_entry!` 生成的 lifecycle `Guest` impl 成为固定骨架：

```rust
impl lifecycle::Guest for $plugin_type {
    fn activate() -> Result<(), String> {
        // 固化流程：结果日志 + Err 上抛（现状保留）
    }

    fn on_startup() -> Result<(), String> {          // 签名随 WIT 变更
        match <$plugin_type>::on_startup() {
            Ok(()) => { log_info("Plugin startup init completed"); Ok(()) }
            Err(e) => { log_error(&format!("on_startup failed: {e}")); Err(e.to_string()) }
            // panic 由宿主 catch_unwind 兜底（现有机制），无需在此处理
        }
    }

    fn on_shutdown() -> Result<(), String> { /* 同理不再吞错 */ }
}
```

静态注册 trait `BedcodePlugin`（`traits.rs`）同步：

- `on_startup()` 返回类型 `Pin<Box<dyn Future<Output = ()>>>` → `Pin<Box<dyn Future<Output = anyhow::Result<()>>>>`
- 默认实现返回 `Ok(())`；`BedcodePluginEntry` fn pointer 类型同步修改
- 该路径当前 0 使用者，破坏性变更可接受

### 3.3 宿主状态机升级

`bedcode-plugin-api/types.rs` 的 `PluginState` 增加：

```rust
pub enum PluginState {
    Loaded,
    /// 激活进行中（auto-activation / 手动激活期间），列表可见的中间态
    Activating,
    Activated,
    NeedsApproval,          // 不变
    /// activate 成功但 on_startup 失败：实例可用、启动初始化未完成
    Degraded(String),
    Error(String),
    Deactivated,
}
```

状态转移（host.rs `activate_plugin` 重构）：

```
Loaded/Error(e)/Degraded(e)          // Degraded/Error 可重试激活（复用现有 Error 重激逻辑）
  │ phase 1: 读 plan + 重新授权
  ├─→ Activating
  │
  │ phase 2 (WASM): activate()
  │   ├─ Ok ──→ 继续
  │   ├─ Err(e)/panic ──→ Error(e)                    [现有行为]
  │
  │ phase 2b (WASM): on_startup()
  │   ├─ Ok ──→ phase 3
  │   ├─ Err(e) ──→ Degraded(e)   ★ 不再静默；error! 日志 + 计入汇总
  │   └─ panic ──→ Error(panic)                     [trap 重载机制覆盖]
  │
  └ phase 3: api_registry/message_bus/订阅注册（顺序不变，Degraded 也完成注册：
             实例本身是活的，只是启动初始化部分失败）
```

要点：

- **phase 3 注册动作对 Degraded 照常执行**：message_bus 订阅、api_registry 登记等以「activate 成功」为前提，与 on_startup 解耦
- `deactivate_plugin` 对 Degraded 插件正常工作（on_shutdown + deactivate 已有容错分支）；停用时清除 Degraded → Deactivated
- `is_activated()` 语义保持严格 `Activated`（API 门禁不放宽）；如需放宽给 Degraded，另立 ticket 讨论
- 持久化语义不变：persisted map 存**用户意图**（enabled/disabled bool）。auto-activation 失败进 Degraded/Error 不回写 false——下次启动仍重试（意图 ≠ 健康快照）

### 3.4 静态插件路径修复（消除 1.4 矛盾）

方案 A（采纳）：`PluginHost::new` 中静态插件直接置 `Activated`（builtin 常驻语义——随二进制分发、无独立启停），使 `notify_startup` 回调与 `invoke_rust_command` 门禁自然生效；日志如实打印 `Static plugin activated (builtin)`。

备选 B（否决理由）：保持 Loaded 并从 notify_startup 删静态分支——治标，「内置却不可 invoke」的行为矛盾仍在。

### 3.5 日志规范（固化在宿主 + SDK 双侧）

| 时点 | 级别 | 内容 |
|------|------|------|
| auto-activation 开始 | info | `[PluginHost] Auto-activating plugin: {id}` （现有） |
| activate 导出返回 | info/error | `{id} activate ok/failed: {e}` |
| on_startup 导出返回 | info/**error** | `{id} on_startup ok/failed: {e}`（★ 本次核心变化） |
| 终态 | info | `{id} final state: Activated / Degraded({e}) / Error({e})` |
| 汇总 | info | 分状态计数：`total={} activated={} degraded={} error={} loading={}` |

### 3.6 前端配合

- `src/plugin/types.ts` 与 `packages/plugin-sdk-desktop/src/types.ts` 的 `PluginState` 联合类型扩展 `'Activating'` / `{ state: 'Degraded'; error: string }`（serde tag 格式与 Rust 侧一致）
- `loader.loadAll` gating（src/plugin/loader.ts:57）：
  - `Activated` → 加载前端模块（现状）
  - `Degraded` → **也加载前端模块**（后端实例在运行、命令可用，UI 入口应可见），console.warn 标注降级原因
  - 其余（Activating/Loaded/Error/NeedsApproval/Deactivated）→ 跳过
- i18n：`desktop.plugin.stateActivating` / `stateDegraded`（zh-CN 与 en 同步，Done When 要求）
- `contributionKinds.ts` 等 `state.state === 'Activated'` 判定点逐一核对：状态徽章区分展示 Degraded；功能门禁是否放行 Degraded 见 §5 开放问题

### 3.7 P2（诊断补全，范围外可拆票）

前端 TS 模块加载成败目前只有 console.log，`runtime.*.log` 不可见（本次分析发现的缺口之一）。可在 api_bridge 增加 host 内部诊断命令（仅写 tracing，不入状态机）。注意这是**宿主自身诊断**，不是插件协议，不受「不加上报 ABI」约束。

---

## 4. 测试计划

| 层 | 内容 |
|----|------|
| SDK rust | `wasm_entry!` 宏测试：on_startup Err 传播到导出返回值；默认实现 Ok |
| 测试插件 | `packages/plugin-test` 增加 on_startup-fail 用例（或现有组件加开关），覆盖宿主 Degraded 路径 |
| 宿主 host.rs tests | auto-activate on_startup 失败 → Degraded；Degraded 可重试激活 → Activated；deactivate(Degraded) → Deactivated；汇总分状态计数断言；静态插件置 Activated 后 notify_startup 回调被调 |
| 前端 | plugin-flow.test.ts fixtures 扩展新状态；loadAll 对 Degraded 加载、对 Activating 跳过 |
| 全量 | `cargo test`（workspace）+ `npm run test:run`（desktop）全绿；4 个内置插件重新构建通过 |

## 5. 开放问题

1. **Degraded 的功能门禁**：`plugin_invoke` / 命令面板 / 视图挂载是否放行 Degraded 插件？（倾向：放行——实例活着且 phase 3 注册已完成；但 auto-task 这类 on_startup 即失败的插件，命令执行大概率也会失败，放行只是把错误推迟到调用点。建议先放行 + UI 降级标识，观察实际插件表现再收紧）
2. **on_startup 超时看门狗**：guest 调用跑在 `spawn_blocking`，sync wasmtime 无法安全中断卡死的导出。本 spec 不做（另立 ticket，需 wasmtime epoch interruption 方案）
3. **`mark_plugin_error` 是否需要 severity 参数**：维持现状（纯通知通道），待出现真实需求再加

## 6. Non-goals

- 不新增任何 plugin→host 的状态推送/心跳接口
- 不改 mark_plugin_error 的「不改状态」语义
- 不改持久化格式（仍是 id→bool 意图表）
- 不处理 TS-only 插件前端加载超时策略（已有 5s timeout + pluginMarkError，路径基本诚实）

## 7. 涉及文件清单

| 文件 | 改动 |
|------|------|
| `packages/plugin-sdk-desktop/rust/wit/bedcode.wit` | on-startup/on-shutdown 加 result |
| `packages/plugin-sdk-desktop/rust/src/wasm.rs` | wasm_entry! lifecycle Guest 骨架：吞错 → 上抛 + 日志 |
| `packages/plugin-sdk-desktop/rust/src/traits.rs` | BedcodePlugin::on_startup/on_shutdown 返回 Result；Entry 类型同步 |
| `packages/plugin-sdk-desktop/rust/src/types.rs` | PluginState 增加 Activating / Degraded(String) |
| `bedcode-desktop/src-tauri/src/plugin/host.rs` | activate_plugin 状态机重构；静态插件置 Activated；汇总分状态计数；notify_startup 相应调整 |
| `bedcode-desktop/src-tauri/src/plugin/loader.rs` | 无（Loaded 初态不变） |
| `bedcode-desktop/src/plugin/types.ts`、`packages/plugin-sdk-desktop/src/types.ts` | PluginState 联合类型扩展 |
| `bedcode-desktop/src/plugin/loader.ts` | loadAll gating 增加 Degraded 分支 |
| `bedcode-desktop/src/plugin/contributionKinds.ts` 及各状态判定点 | Degraded 徽章/文案 |
| `bedcode-desktop/src/locales/{zh-CN,en}/desktop.ts` | 新状态 i18n key |
| `packages/plugin-test` | on_startup-fail 测试用例 |
| 相关测试文件 | 见 §4 |

## 8. 迁移与发布

1. SDK WIT/trait 变更与宿主状态机变更**同一 PR**（abi.rs 为单一事实来源，漂移由编译期暴露）
2. 4 个内置插件（ai-chatbox / auto-task / file-transfer / scheduler）随 monorepo 构建链重编，源码预计零改动（除非存在被吞掉的 on_startup 错误——那正是要暴露的）
3. 升级后首次启动：此前「假活」的插件将显式进入 Degraded/Error，属预期行为校正，release note 注明
