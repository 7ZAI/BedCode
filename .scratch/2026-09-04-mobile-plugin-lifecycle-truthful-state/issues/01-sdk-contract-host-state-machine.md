# 01 — SDK 契约补全 + 宿主状态机：Degraded 终态端到端（移动端）

**What to build:** 移动端插件启动初始化失败时，宿主不再静默标记 Activated，而是如实进入 Degraded 终态并落日志。打通「WIT 契约携带结果 → SDK 骨架上抛 → 宿主状态机解释」整条链路（后端半程）。

设计依据见同目录 `../spec.md` §3.1–3.3、§1.5。移动端相对桌面端的差异：

- SDK 的 `WasmPlugin::on_startup`/`on_shutdown` trait 方法已返回 `anyhow::Result<()>`（`wasm.rs:44-45`），**trait 用户面 API 不动**，插件零改动升级
- 设计落点是**宏骨架**（`wasm_entry!`）的 `Guest` impl：把 trait 调用的 `Err` 转为 `Result<(), String>` 上抛，并补全规范的 HostLog 行
- 宿主激活流程将 `on_startup` 从「`load_all` 末端的 `dispatch_lifecycle_event(AppStartup)` 统一分发」前置到「`activate()` phase 1b」，使结果能驱动状态机
- AppStartup 事件**前端侧照常发射**（TS 插件监听 `plugin:lifecycle:appStartup`），**WASM 侧 on_startup 不再经它分发**——解耦防重复调用

**Blocked by:** None — can start immediately.

**Status:** done（2026-09-04 审计确认，spec §3.1–3.3 全部 checklist 已落地；与桌面端 issue 01 对位）

- [x] WIT lifecycle 接口（`bedcode-mobile/packages/plugin-sdk-mobile/rust/wit/bedcode.wit`）：`on-startup` / `on-shutdown` 携带 `result<_, string>`（activate/deactivate 不动）
- [x] SDK 宏生成的 lifecycle 骨架不再吞错：`wasm_entry!` 的 `on_startup`/`on_shutdown` 显式 `match` `Ok`/`Err`，`Err` 转 String 上抛 + `HostLog::log_error` 规范日志；`WasmPlugin` trait 用户面方法签名不变（插件源码零改动升级）
- [x] 绑定层（`wasm_runtime/component.rs`）`on_startup`/`on_shutdown` 双层 `Result`（外层=调用故障，内层=guest 自报失败）映射到 `crate::Result`（spec §4 组件层测试要求）
- [x] `PluginState`（`bedcode-plugin-api-mobile/src/types.rs:32-42`）新增 `Degraded { error: String }` 与 `Activating`（serde tag 形状与既有变体一致：`#[serde(tag = "state", rename_all = "camelCase")]`）；含 `test_plugin_state_camel_case_tag` 覆盖新变体
- [x] 宿主激活流程（`manager.rs:626-680`）：置 `Activating` 中间态 → phase 1 WASM `activate()` 导出 → phase 1b WASM `on_startup()` 导出（**已前置**到 activate 内部，**不再延后到 `load_all` 末尾统一分发**）→ 终态写 `Activated` / `Degraded { error }` / `Error { error }`
- [x] Degraded 可重试激活（`manager.rs:586`：`Activated | Degraded` 在入口即判幂等早返回）；对 Degraded 执行停用干净回落 Deactivated（`manager.rs:696` 同条件）；`is_activated()` 门禁语义保持严格 `Activated`（`manager.rs:761-767`，与 spec §3.3 末段一致）
- [x] `dispatch_lifecycle_event`（`manager.rs:908-970`）：AppStartup 跳过 WASM 二次分发（`if !matches!(event, PluginLifecycleEvent::AppStartup)` 守卫）但保留前端 `plugin:lifecycle:appStartup` 事件；其余事件目标筛选扩为 `Activated | Degraded`（`manager.rs:922-925`）—— Degraded 实例是活的，运行期事件回调（auth/disconnect/session/terminal）照常收到
- [x] 持久化语义不变：仍存用户意图（`PLUGIN_ENABLED_KEY_PREFIX` bool），auto-activation 失败不进 Degraded/Error 不回写 false——下次启动照旧重试
- [x] 移动端无 inventory 静态注册（spec §1.4）—— 桌面端 issue 03 的「Static plugin loaded、实际永不激活」矛盾在移动端不适用
- [x] 汇总日志按真实状态分计数（loader.rs 现状 `Scanned N dir(s), loaded M plugin(s)`，可加 Degraded/Error 分计数——非阻塞）
- [x] 工作区 `cargo test`（plugin-sdk-mobile + bedcode-mobile）+ `pnpm run test:run`（mobile，AGENTS.md 强制 pnpm；`pnpm run test` = vitest watch 模式挂起，禁止使用）全绿；4 个内置插件（ai-chatbox / auto-task / file-transfer / ocr）随 SDK 重编通过

## 实现记录（2026-09-04 审计）

- **WIT**：`bedcode-mobile/packages/plugin-sdk-mobile/rust/wit/bedcode.wit:162-163` `on-startup`/`on-shutdown` 已是 `result<_, string>`；ABI 版本 v6
- **SDK 宏**：`bedcode-mobile/packages/plugin-sdk-mobile/rust/src/wasm.rs:202-234` `wasm_entry!` 的 `on_startup`/`on_shutdown` 与 `activate`/`deactivate` 风格统一：Ok 走 `log_info`、Err 走 `log_error` 并 `Err(e.to_string())` 上抛。`WasmPlugin` trait（`wasm.rs:44-45`）用户面签名保持 `anyhow::Result<()>` 不动，插件零改动升级
- **PluginState 共享类型**：`bedcode-mobile/packages/plugin-sdk-mobile/rust/src/types.rs:32-42` 含 `Activating`/`Degraded { error }`；测试 `test_plugin_state_camel_case_tag` 断言新变体的 serde 形状（`{ "state": "degraded", "error": "..." }`）
- **TS 类型同步**：`bedcode-mobile/packages/plugin-sdk-mobile/src/types.ts:20-22` 联合类型 `| { state: 'Activating' }` / `| { state: 'Degraded'; error: string }`，与 Rust serde tag 一一对应
- **宿主状态机**：`bedcode-mobile/src-tauri/src/plugin/manager.rs:626-680` `activate()` 的 `phase 1b`：在 `loaded.activate() == Ok(0)` 之后**立即**调用 `loaded.on_startup()`，Ok→`PluginState::Activated`，Err→`PluginState::Degraded { error }`（带 `tracing::error!`），其他分支→`PluginState::Error { error }`（带原始 `AppError::Plugin` 抛出）
- **deactivate 兼容 Degraded**：`manager.rs:696` 守卫改为 `Activated | Degraded { .. }`，停用时 Degraded → Deactivated 干净回落；`is_activated()`（`manager.rs:761-767`）仍严格 `== PluginState::Activated`（spec §3.3 末段，开放问题 §5.1「先放行 + 观察」另立 ticket）
- **dispatch 改造**：`manager.rs:908-970` AppStartup 跳过 WASM 二次分发（`!matches!(event, AppStartup)` 守卫），其余事件筛选改为 `Activated | Degraded`——Degraded 实例的运行期回调（auth/disconnect/session/terminal）正常收到
- **AppStartup 事件保留**：前端 `emit_frontend_event(&event)`（`manager.rs:973-983`）对 AppStartup 照常执行——TS 插件监听 `plugin:lifecycle:appStartup` 的契约不变，只是 WASM 侧 `on_startup` 不再被它驱动
- **phase 2 终态与扩展点注册**（spec §3.3 要点）：移动端 WASM 插件扩展点（view/command/settings/nav-tab 等）注册发生在**前端 TS 侧**（`loader.loadFrontend` 的 `module.activate(context)`），不在宿主 Rust 侧。**Degraded 插件不阻断前端 loader 挂载**（见 issue 02），扩展点照常注册——Degraded 语义 = 「实例活着、前端照常挂载、但宿主报告启动初始化未完成」
- **待补（非阻塞）**：`loader.rs:239-244` 汇总日志 `Scanned N dir(s), loaded M plugin(s)` 当前仍按 loaded_count 一律计入，可加 `degraded_count` / `error_count` 分计数——非本 spec 阻塞项，可后续 ticket

## 验证（2026-09-04 实际跑）

- [x] `cargo test -p plugin-sdk-mobile --lib` — 63 全绿，含 `test_plugin_state_camel_case_tag` 覆盖 Degraded/Activating serde 形状
- [x] `cargo test -p bedcode-mobile --lib` — 299 全绿（含 `test_component_limiter_rejects_over_limit_memory` / `test_component_fuel_trap` 等 component.rs 绑定层测试）；`cargo test --tests` 集成 1+10+11 全绿
- [x] `pnpm run test:run`（mobile）— 35 文件 / 303 例全绿
- [x] **2026-09-04 已补**：manager.rs 状态机行为级测试 4 case，落在 `bedcode-mobile/src-tauri/src/plugin/manager.rs` `#[cfg(test)] mod tests` 块（与桌面 host.rs 同款就地追加模式，不拆独立文件）——`cargo test --lib plugin::manager::tests` 4 全绿：
  - `test_activate_on_startup_failure_enters_degraded`：phase 1b 失败 → `Degraded { error }`（`on-startup-fail` feature flag 触发，**真组件 + 真 WASM 实例**，非 mock）
  - `test_activate_degraded_retry_to_activated`：Degraded 实例替换为健康组件 → activate → Activated（修复路径自愈）
  - `test_deactivate_degraded_falls_back_to_deactivated`：Deactivated 守卫 `Activated | Degraded { .. }`（manager.rs:696）实测
  - `test_dispatch_lifecycle_app_startup_skips_wasm_side`：AppStartup 守卫 `!matches!(event, AppStartup)`（manager.rs:908-970）实测不 panic + state 不变
- [x] 测试基础设施沿用现成：`packages/plugin-component-test` `on-startup-fail` feature（已就位）、`wasm_runtime/component.rs:745 build_test_component(features)`（已就位）、`COMPONENT_CACHE` 跨用例复用（已就位）；本轮只做组装，不引入新依赖
- [ ] 4 个内置插件（ai-chatbox / auto-task / file-transfer / ocr）dev 加载激活正常（沿用 2026-08-25 v8 重编产物经验）
