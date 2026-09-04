# Spec：插件生命周期真实上报（移动端）— 让状态与日志反映插件真实加载结果

Status: ready-for-execution（2026-09-04 审计：§3.1–3.3 + §3.5 全部 checklist 已落地，详见 issues/01–02；§3.4 移动端不适用，详见 issues/03；§3.6 P2 留 pending，详见 issues/04）
Date: 2026-09-04
Owner: mobile plugin system
Related: `bedcode-mobile/src-tauri/src/plugin/`、`packages/plugin-sdk-mobile/rust/`
Based-on: `.scratch/plugin-lifecycle-truthful-state/spec.md`（桌面端已落地，本 spec 为移动端对位改造）

## 0. 审计摘要（2026-09-04）

按桌面端 `.scratch/plugin-lifecycle-truthful-state/issues/` 4 张对位拆分，本 spec 的实施项已落入：

- `issues/01-sdk-contract-host-state-machine.md` — **done**（WIT 签名 + SDK 宏骨架 + `PluginState::Degraded/Activating` + `manager.rs` 状态机 phase 1b + dispatch 解耦）
- `issues/02-frontend-degraded-badge-gating.md` — **done**（TS 联合类型 + loader 门禁 + 徽章样式 + i18n）
- `issues/03-static-plugins-builtin-semantics.md` — **N/A**（移动端无 inventory 静态注册，见 spec §1.4）
- `issues/04-frontend-load-diagnostics-to-tracing.md` — **pending**（spec §3.6 P2 范围外可拆票）

未实施残留（待补验证 + 非阻塞改进）：

- `cargo test` / `npm run test:run` 全量验证尚未在 2026-09-04 当日跑（按 issue 01 末尾「验证」清单执行）
- `loader.rs:239-244` 汇总日志可加 `degraded_count` / `error_count` 分计数（非 spec 阻塞项）

---

## 1. 背景与问题

移动端与桌面端存在同构的缺陷：插件「激活成功」的判定与日志**不反映插件内部初始化的真实结果**。证据链如下：

### 1.1 契约层：on_startup 结果在 ABI 上就被丢弃

`packages/plugin-sdk-mobile/rust/wit/bedcode.wit`（L159-164）：

```wit
interface lifecycle {
    activate: func() -> result<_, string>;
    deactivate: func() -> result<_, string>;
    on-startup: func();        // ← 无返回值，失败无法上抛
    on-shutdown: func();       // ← 同上
}
```

SDK 宏 `wasm_entry!`（`wasm.rs:202-208`）进一步用 `let _ =` 显式吞掉用户实现的结果：

```rust
fn on_startup() {
    let _ = <$plugin_type as $crate::wasm::WasmPlugin>::on_startup();
}
```

而 auto-task / scheduler / file-transfer / ocr 等插件的**真正初始化**（hooks 安装、DB 建表、listener 注册、定时器）恰恰在 `on_startup` 里做。

> 注：与桌面端不同，移动端 SDK 的 `WasmPlugin` trait（`wasm.rs:44-45`）的 `on_startup`/`on_shutdown` **已经返回 `anyhow::Result<()>`**，默认 `Ok(())`。因此 SDK trait 层无需改动，只需补全 WIT 签名 + 修正 `wasm_entry!` 宏展开骨架。

### 1.2 宿主层：失败被降级为 warn，状态照常 Activated

`manager.rs`：

- `activate()`（L411-554）失败 → 置 `Error` ✓（正确）；成功 → 置 `Activated`，**不调用 on_startup**
- `on_startup` 只在 `load_all()` 末尾经 `dispatch_lifecycle_event(AppStartup)`（L341）统一分发，且 `dispatch_lifecycle_event`（L782-836）只快照 **`Activated`** 的 WASM 插件；失败仅 `warn!`（L824-831），状态不回写
- 汇总日志（loader.rs:239-244）`Scanned N dir(s), loaded M plugin(s)` 中插件一律计入 loaded_count

### 1.3 自检通道与生命周期脱节

插件运行期自检失败走 `host_mark_plugin_error` → `status_reporter`（wasm_runtime.rs:189-192，manager.rs:132-170）：置 `Error` + 持久化未启用 + 前端通知。这是有意的局部故障设计（保留），但它意味着插件内部唯一的结果上报通道对状态机完全不可见——与 on_startup 的「启动初始化成败」是两回事。

### 1.4 静态注册路径

移动端**无 inventory 静态注册**（`builtin_manifests()` 返回空 Vec，内置插件走 APK assets 解压 + 正常激活流程）。故桌面 spec §1.4 / §3.4 的静态插件矛盾在移动端不适用，**无需处理**。

### 1.5 结论

```
现状：持久化(意图) → 尝试 activate() → 同步返回 Ok → 打印 "activated"
                      ↓ on_startup 失败 → warn，照常 Activated
                      ↓ 插件自检失败 → Error + 禁用（通道独立，与状态机脱节）
```

日志反映的是「宿主调用链没抛错」，不是「插件确认就绪」。on_startup 的启动初始化失败被完全静默——**本次改造的核心**。

---

## 2. 设计原则（定调）

与桌面端一致：

1. **不加特殊的上报 ABI** —— 不引入 `report_status(kind, msg)` 之类的 plugin→host 推送通道。状态判定完全由**既有生命周期导出的返回值**驱动。
2. **固化流程放 SDK 默认实现** —— 生命周期处理的标准骨架（记录开始/结果、错误上抛、日志规范）写在 SDK 的宏展开代码里；插件只覆盖 `WasmPlugin` trait 对应方法即可扩展，不覆盖走默认。
3. **宿主固定监听生命周期** —— PluginManager 继续按固定时序驱动 `instantiate → activate → on_startup → … → on_shutdown → deactivate`，只升级它对这些返回值的**解释语义**（状态机），不改调用结构。

唯一的契约修正是：让既有 `on-startup` / `on-shutdown` 导出**携带结果**（补全原契约的残缺签名），不是新增接口。所有插件在 monorepo 内一起重编，不存在第三方兼容负担。

移动端差异：SDK trait 已返回 Result，故设计原则 2 的落点是**宏骨架**（wasm_entry!），trait 面 API 保持不动（插件零改动升级）。

---

## 3. 详细设计

### 3.1 WIT 契约修正（abi 单一事实来源）

`packages/plugin-sdk-mobile/rust/wit/bedcode.wit` lifecycle 接口改为：

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

### 3.2 SDK 固化流程（宏骨架，trait 不动）

`packages/plugin-sdk-mobile/rust/src/wasm.rs`：

**`WasmPlugin` trait 用户面 API 保持不变**（`on_startup()` 已返回 `anyhow::Result<()>`，插件零改动升级）。

`wasm_entry!` 生成的 lifecycle `Guest` impl 成为固定骨架：

```rust
impl $crate::wasm::exports::bedcode::plugin::lifecycle::Guest for $plugin_type {
    fn activate() -> Result<(), String> { /* 现状保留：结果日志 + Err 上抛 */ }

    fn on_startup() -> Result<(), String> {          // 签名随 WIT 变更
        let host = $crate::wasm_host::WasmHost;
        match <$plugin_type as $crate::wasm::WasmPlugin>::on_startup() {
            Ok(()) => {
                $crate::host::HostLog::log_info(&host, "Plugin startup init completed");
                Ok(())
            }
            Err(e) => {
                $crate::host::HostLog::log_error(&host, &format!("on_startup failed: {}", e));
                Err(e.to_string())
            }
            // panic 由宿主 catch_unwind 兜底（现有机制），无需在此处理
        }
    }

    fn on_shutdown() -> Result<(), String> { /* 同理不再吞错 */ }
}
```

### 3.3 宿主状态机升级

`packages/plugin-sdk-mobile/rust/src/types.rs` 的 `PluginState` 增加（**保持现有 serde 形态：`tag = "state", rename_all = "camelCase"`，struct 变体带 content**）：

```rust
#[serde(tag = "state", rename_all = "camelCase")]
pub enum PluginState {
    Loaded,
    /// 激活进行中（auto-activation / 手动激活期间），列表可见的中间态
    Activating,
    Activated,
    /// activate 成功但 on_startup 失败：实例可用、启动初始化未完成
    Degraded { error: String },
    NeedsApproval,
    Deactivated,
    Error { error: String },
}
```

状态转移（manager.rs `activate()` 重构）：

```
Loaded/Error(e)/Degraded(e)          // Degraded/Error 可重试激活
  │ phase 0: 审批门禁（现有逻辑不变；内置 ApkAsset/FrontendOnly 放行）
  ├─→ Activating
  │
  │ phase 1: WASM activate() 导出
  │   ├─ Ok(0) ──→ 继续
  │   ├─ Ok(code)/Err(e)/panic ──→ Error(e)              [现有行为]
  │
  │ phase 1b: on_startup() 导出   ★ 新增：激活成功后立即调用，不再延后到 load_all 统一分发
  │   ├─ Ok(()) ──→ phase 2
  │   ├─ Err(e) ──→ Degraded { error: e }   ★ 不再静默；error! 日志
  │   └─ panic ──→ Error(panic)
  │
  └ phase 2: 置终态（Activated / Degraded）+ 汇总计数
```

要点：

- **on_startup 调用点从 load_all 的 dispatch_lifecycle_event 前置到 activate() 内部**。原设计在 `dispatch_lifecycle_event(AppStartup)` 里对所有 Activated 插件统一发 `on_startup`；本改造将其作为激活流程的 phase 1b 执行，使结果能驱动状态机。**AppStartup 事件仍需保留**（前端 TS 插件监听 `plugin:lifecycle:appStartup` 依赖它），只是 WASM 侧的 on_startup 不再经由它分发——两者解耦，避免重复调用（防止 on_startup 被执行两次）。
- **phase 2 终态与扩展点注册的关系**：移动端 WASM 插件扩展点（view/command/settings 等）注册发生在**前端 TS 侧**（`loader.loadFrontend` 的 `module.activate(context)`），不在宿主 Rust 侧。宿主 Rust 侧没有类似桌面 phase 3 的 api_registry/message_bus 注册动作（移动端 message_bus 订阅由插件自己在 host 调用里发起）。故 **Degraded 插件是否完成扩展点注册取决于前端 loader 门禁**（见 §3.5）：若前端对 Degraded 放行并调用 `module.activate`，则扩展点照常注册——Degraded 语义 = 「实例活着、前端照常挂载，但宿主 on_startup 报告启动初始化未完成」。
- `deactivate()`（L559-604）对 Degraded 插件正常工作：停用时 Degraded → Deactivated
- `is_activated()`（L635-641）语义保持严格 `Activated`（API 门禁不放宽）；如需放宽给 Degraded，另立 ticket 讨论
- 持久化语义不变：persisted enabled 表存**用户意图**。auto-activation 失败进 Degraded/Error 不回写 false——下次启动仍重试（意图 ≠ 健康快照）
- `dispatch_lifecycle_event` 的 WASM 目标筛选：从「只发 Activated」改为「发 Activated + Degraded」——Degraded 插件的实例是活的，其运行期事件回调（auth/disconnect/session 等）照常应能收到。**on_startup 除外**（已前置到 activate，须从 AppStartup 事件路径剔除，避免二次调用）

### 3.4 静态插件路径修复

移动端无 inventory 静态注册，**本节不适用**。`builtin_manifests()` 返回空，APK assets 内置插件走正常激活流程（auto-activate 已覆盖），无「内置却不可 invoke」矛盾。

### 3.5 前端配合

- `packages/plugin-sdk-mobile/src/types.ts` 与 `src/plugin/types.ts` 的 `PluginState` 联合类型扩展 `{ state: 'Activating' }` / `{ state: 'Degraded'; error: string }`（serde tag 格式与 Rust 侧一致）
- `loader.loadAll` gating（src/plugin/loader.ts:80-103）：
  - 现状以 `pluginIsEnabled`（持久化意图）门禁，**不区分状态**（注释明确这是为规避后端异步激活竞态）。改造后建议：`isEnabled` 门禁保留（意图优先，避免重启丢扩展点），但**加载后若 state 为 Degraded 打印 console.warn 标注降级原因**；对 Activating 中间态跳过（等最终态再补载，或维持现状交由轮询兜底）
  - 核心：**Degraded 放行前端加载**（后端实例在运行、命令可用，UI 入口应可见），与桌面端 loadAll gating 语义对齐
- i18n：`mobile.plugin.stateActivating` / `stateDegraded`（zh-CN 与 en 同步，Done When 要求，见 mobile.ts:441-446）
- `PluginView.vue` 状态徽章（L231/283/693/703）：`stateBadgeClass` / `getStateKey` 增加 Degraded 分支（黄色 warning 徽章 + `stateDegraded` 文案）；Activating 走灰（复用默认 branch）
- `loader.activate`（L106-122）与 `deactivate`（L125-166）：对 Degraded 插件可正常 re-activate（后端 `activate` 已允许 Degraded → Activated 重试）

### 3.6 P2（诊断补全，范围外可拆票）

前端 TS 模块加载成败目前只有 console.log，宿主 `tracing` 不可见。可在移动端 api 层增加宿主内部诊断命令（仅写 tracing，不入状态机）。注意这是**宿主自身诊断**，不是插件协议，不受「不加上报 ABI」约束。与桌面 spec §3.7 同源。

---

## 4. 测试计划

| 层 | 内容 |
|----|------|
| SDK rust | `wasm_entry!` 宏测试：on_startup Err 传播到导出返回值；默认实现 Ok |
| 测试插件 | `packages/plugin-component-test` 增加 on_startup-fail 用例（feature 开关），覆盖宿主 Degraded 路径 |
| 宿主 manager.rs tests | activate on_startup 失败 → Degraded；Degraded 可重试激活 → Activated；deactivate(Degraded) → Deactivated；AppStartup 不再触发二次 on_startup；dispatch_lifecycle_event 对 Degraded 插件仍投递运行期事件 |
| component.rs tests | `on_startup()` 导出失败传播到 `crate::Result`（WIT result 映射） |
| 前端 | PluginState fixtures 扩展新状态；loadAll 对 Degraded 放行 + warn、对 Activating 跳过 |
| 全量 | `cargo test`（workspace）+ `pnpm run test:run`（mobile，AGENTS.md 强制 pnpm 且禁 watch 模式）全绿；4 个内置插件（ai-chatbox / auto-task / file-transfer / ocr）重新构建通过 |

## 5. 开放问题

1. **Degraded 的功能门禁**：`invoke_command` / 命令面板 / 视图挂载是否放行 Degraded 插件？（倾向：放行——实例活着且命令可用；但 auto-task 这类 on_startup 即失败的插件，命令执行大概率也会失败，放行只是把错误推迟到调用点。建议先放行 + UI 降级标识，观察实际插件表现再收紧）——与桌面 spec §5.1 对齐
2. **on_startup 超时看门狗**：guest 调用跑在 `spawn_blocking`，sync wasmtime 无法安全中断卡死的导出。本 spec 不做（另立 ticket，需 wasmtime epoch interruption 方案）
3. **`mark_plugin_error` 是否需要 severity 参数**：维持现状（纯通知通道），待出现真实需求再加

## 6. Non-goals

- 不新增任何 plugin→host 的状态推送/心跳接口
- 不改 mark_plugin_error 的「置 Error + 持久化未启用」语义
- 不改持久化格式（仍是 id→bool 意图表）
- 不处理 TS-only 插件前端加载超时策略（已有 5s timeout + pluginMarkError，路径基本诚实）
- 不改动桌面端已落地的实现（本 spec 仅对位移动端）

## 7. 涉及文件清单

| 文件 | 改动 |
|------|------|
| `packages/plugin-sdk-mobile/rust/wit/bedcode.wit` | on-startup/on-shutdown 加 result |
| `packages/plugin-sdk-mobile/rust/src/wasm.rs` | wasm_entry! lifecycle Guest 骨架：吞错 → 上抛 + 日志（trait 不动） |
| `packages/plugin-sdk-mobile/rust/src/types.rs` | PluginState 增加 Activating / Degraded { error: String } |
| `bedcode-mobile/src-tauri/src/plugin/manager.rs` | activate() 状态机重构（phase 1b on_startup + Degraded 终态）；dispatch_lifecycle_event 目标扩为 Activated+Degraded 并剔除 on_startup |
| `bedcode-mobile/src-tauri/src/plugin/loader.rs` | 无（Loaded 初态不变；汇总日志可加 Degraded/Error 分计数） |
| `bedcode-mobile/src-tauri/src/plugin/wasm_runtime/component.rs` | on_startu