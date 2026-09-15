# Spec：插件动态 UI 严格跟随启用状态 — 修复多次启用/禁用后工具箱入口丢失

Status: implemented（2026-09-06，cargo test 328 / vitest 325 全绿；Android 真机验收待办见 issues 01/05/06）
Date: 2026-09-06
Owner: mobile plugin system
Related: `bedcode-mobile/src/plugin/{loader,registry,events,context}.ts`、`bedcode-mobile/src/views/{ToolboxView,PluginView}.vue`、`bedcode-mobile/src-tauri/src/plugin/wasm_runtime/host_impl/mdns.rs`、`bedcode-mobile/plugins/file-transfer/rust/src/device_bridge.rs`
Based-on: `.scratch/mobile-plugin-lifecycle-truthful-state/spec.md`（移动端 Degraded 状态机已落地，本 spec 在其之上收敛 UI 生命周期闭环）

---

## 1. 背景与问题

用户报告：移动端插件管理中，文件传输（file-transfer）插件经历多次「启用 / 停用」切换后，其**工具箱入口永久消失，重新启用也不再显示**。且当用户停留在插件的二级页时，切到 `/mobile/plugins` 停用该插件，再返回原入口，**二级页组件仍残留展示**，与插件已停用的状态不符。

代码侦察确认了两组根因，均落在「插件动态 UI 的加载 / 卸载是否闭环」这一焦点：

### 1.1 前端加载/卸载链存在两处不对称（可致入口永久消失）

**A. `deactivate` 早退不通知后端**（`loader.ts:138-139`）：
```ts
async deactivate(pluginId) {
  const plugin = this.plugins.get(pluginId)
  if (!plugin) return   // ← 前端模块未加载时直接返回
  ...
}
```
若上次启用中**前端模块加载失败**（import/activate 抛错，`plugins` Map 无记录）但**后端 `plugin_activate` 已成功**（后端保持 Activated，WASM 实例存活并持有 mDNS browse 句柄），此时用户停用 → `deactivate` 早退 → `pluginCmds.pluginDeactivate` 永不调用 → **后端 WASM 实例泄漏，mDNS 浏览继续运行**。注册表入口被清理，但后端半活。

**B. `activate` 失败路径只 markError、不拆解后端**（`loader.ts:128-133`）：`plugin_activate` 成功后若 `loadFrontend` 抛错，仅 `pluginMarkError` 置 Error + `clearPlugin` 清前端注册表；**后端 WASM 实例仍存活**（`wasm_plugins` 未摘除），持久化 enabled 保持 true（`manager.mark_error` 只置 Error、不落盘 disabled，`manager.rs:817-822`）。重启用 → `plugin_activate` 遇 Error 状态**重试激活**（Error 不在幂等早退集，`manager.rs:607-611` 仅对 Activated/Degraded 早退）→ 若根因（如 mDNS 重入 panic）是确定性的 → 再次失败 → 再次 Error。**每次重启用都在同一处失败，工具箱入口永不出现**；且因缺陷 A（`deactivate` 早退）无法对称拆解后端，缺一条干净恢复路径。

另有 `report_ready` 自愈（Error→Activated，`manager.rs:829-840`）可把后端状态拉回 Activated 而前端模块仍未挂载 → 重启用命中幂等早退（跳过后端重激活）→ 前端入口同样永久缺失。

**C. `PluginView.handlePluginToggle` catch 只回退 UI 开关、不回退后端运行时**（`PluginView.vue:569-574`）：`pluginEnabledStates` 回退到 `!enabled`，但后端可能已 Activated（WASM 存活）或 Error。UI 显示「已停用」而后端仍在跑，UI 状态 ⇄ 后端状态漂移。

### 1.2 Android mDNS 多播锁重入可致激活失败（用户点名要查的 panic）

`host_impl/mdns.rs:43`：
```rust
#[cfg(target_os = "android")]
if let Err(e) = tauri::async_runtime::block_on(multicast_lock_acquire()) {
```
项目自身文档约束（`manager.rs:152`）明确：**「禁止在运行时内使用 block_on（会 panic）」**。而 `mdns_browse` 恰是 WASM 宿主函数：它被 `manager.rs::activate` 内的 wasm `activate()` 调用，而 `activate` 又由 `plugin_activate` 等 **Tauri async command（跑在 tokio worker）** 驱动；`tauri::async_runtime::block_on`（`tauri-2.11.5 async_runtime.rs:272`）对全局 runtime 执行 `block_on`。从 runtime 上下文内再 block_on → tokio panic → 宿主函数恐慌被 wasmtime 转为 trap → `loaded.activate()` Err → `manager.rs:669` 置 `PluginState::Error`。

后果：Android 真机上 file-transfer 激活期一旦走此路径即 panic → 插件状态 Error → 前端 `loadFrontend` 失败清注册表。**若根因确定（同一上下文下每次触发），此后每次启用都在同一点失败，工具箱入口再也不会出现**，与「多次启用/禁用后入口丢失且不恢复」吻合。

**准确度说明**：panic 是否每次必现取决于 tauri 全局 `RUNTIME` 与 app 运行时的同一性等上下文细节，代码静态分析无法 100% 判定触发条件；但「宿主函数内对全局 runtime 同步 block_on」**违反项目自设约束**（`manager.rs:152`）是确定的，且宿主函数阻塞 worker 本身即反模式。ticket 01 需在设备上实证并消除该模式，而非赌它不触发。

### 1.3 次要但真实的残留

- `device_bridge::stop_browse`（`device_bridge.rs:71-78`）只清 `sessions`，**`endpoints` memo 永不清理**；百次 toggle 后进程内静态表持续增长。
- `events.clearPluginEvents`（`events.ts:74-81`）直接 `delete(set)`，**未触发每条 disposable 的 `tauriUnlisten?.()`**，Tauri listener 句柄泄漏。主路径由 `context._disposables` 兜底清理，此为次级防线。
- **两条错误路径持久化语义分叉**（行为记录，非缺陷）：`status_reporter`（`manager.rs:188-228`，插件自报错误）会**落盘 enabled=false**（下次启动不重试）；`mark_error`（`manager.rs:817-822`，前端 loader 上报加载失败）**只置 Error、不动 enabled**。二者看似不一致，实为有意设计：运行时自报失败 → 禁用；前端加载失败 → 保留 intent、下次启动重试（自愈）。本 spec 不改变此语义。

---

## 2. 解决方案

让「插件动态 UI」严格跟随「启用状态」这条不变量成立，并消除 Android 激活必现失败：

1. **后端 mDNS 多播锁不再重入**：`mdns_browse` 内不再 `block_on`，改为 fire-and-forget 异步获取或跳过降级，消除 Android 激活 panic。
2. **前端加载/卸载链对称闭环**：`deactivate` 对「前端模块未加载」也能拆解后端；`activate`/`loadFrontend` 失败时对称停用后端，保证重启用从干净状态重启。
3. **Toggle 失败回退一致**：UI 开关、后端运行时、注册表三者任一失败路径都收敛到同一终态。
4. **ToolboxView 二级页严格跟随**：验证并加固「停用后返回入口时二级页隐藏」的三道闸机制。
5. **清理残留**：`endpoints` memo 对称清理、`clearPluginEvents` 真正 dispose。

持久化语义核对实际代码（非仅既有 spec）：`status_reporter`（插件自报错误）落盘 `enabled=false`；`mark_error`（前端 loader 失败）保留 intent 供下次启动重试。本 spec 不改动这两条语义，只保证运行时与 UI 严格收敛到一致终态。

---

## 3. 用户故事

1. As a mobile user, I want the file-transfer toolbox entry to appear the moment I enable the plugin, so that enabled functionality is immediately reachable.
2. As a mobile user, I want the file-transfer toolbox entry to disappear the moment I disable the plugin, so that disabled functionality is never visible anywhere in the app shell.
3. As a mobile user, I want the toolbox page to reflect my latest toggles when I return from the plugin manager, so that I do not need to restart the app to see the change.
4. As a mobile user, I want my in-progress second-level plugin page to hide itself when the plugin is disabled from the plugin manager, so that I never stare at a view backed by a deactivated plugin.
5. As a mobile user, I want repeated enable/disable cycles to never permanently lose a plugin entry, so that the toolbox stays trustworthy.
6. As a mobile user, I want enabling the plugin to either fully succeed (entry + component working) or fully tear down (no zombie backend, error surfaced), so that re-enabling always starts from a clean state.
7. As a mobile user on Android, I want enabling the file-transfer plugin to never crash from an mDNS/multicast-lock reentrancy, so that the plugin can actually activate on device.
8. As a mobile user, I want toggling a plugin many times in a row to not leak memory or listeners, so that the app stays responsive over time.
9. As a plugin developer, I want my plugin's `deactivate()` to be invoked even when its frontend module previously failed to load, so that no backend process outlives the UI teardown.
10. As a plugin developer, I want `deactivate()` and `clearPlugin` to be idempotent, so that a partial failure on either path never leaves the registry half-dirty.
11. As a plugin developer, I want my plugin's registered UI to be removed when it is disabled while its second-level page is still mounted, so that the page does not reference stale context.
12. As a maintainer, I want the loader's failure path to tear down the backend instance symmetrically, so that a later re-enable does not hit backend idempotent-skip with a broken frontend module.
13. As a maintainer, I want the mDNS browse loop to survive repeated start/stop cycles, so that host-mdns handles are never leaked across toggles.
14. As a maintainer, I want Tauri event listeners owned by a plugin to be truly unregistered on teardown, so that the event bus does not accumulate zombies across toggle cycles.
15. As a mobile user, I want the entry to reappear after a transient failure once I re-toggle the plugin, so that transient errors are recoverable without app restart.

---

## 4. 实施决策

### D1 — 后端 mDNS 多播锁去重入（激活失败根因消除）

`mdns_browse` 内移除 `tauri::async_runtime::block_on(multicast_lock_acquire())` 的同步等待语义，改为 fire-and-forget 异步获取：
- 以 `tauri::async_runtime::spawn(async move { … multicast_lock_acquire().await … })` 调起：不阻塞当前线程、不进入全局 runtime block_on、不依赖「当前是否已在 runtime」；
- 多播锁获取降级为 best-effort：失败只 warn、不阻断 browse（缺锁仅退化收包，与现状同语义）；
- 同一条 browse 订阅生命周期内锁获取至多触发一次（幂等守卫），避免重复唤醒。

接口面：`mdns_browse` 签名与返回不变，`device_bridge::start_browse` 无需改动。验证方式：ticket 01 在**已进入 tokio runtime 上下文**的用例中调用 `mdns_browse`，断言不 panic（改动前该场景可触发 panic）。

### D2 — `deactivate` 对称拆解：前端模块未加载也停用后端

`loader.deactivate` 移除「`plugins` 无记录即早退」的不对称。改为：
- 无论前端模块是否在 `plugins` Map，只要后端命令可达，即调用 `pluginCmds.pluginDeactivate`；
- 前端侧（disposables / clearPluginEvents / clearPlugin / module.deactivate）在存在时执行、缺失时跳过；
- 全程幂等：重复调用、或前端模块未加载过，都不产生副作用；
- 停用操作对后端 `pluginDeactivate` 的幂等（`manager.rs:717-719` 已保证）依赖保持。

### D3 — `activate`/`loadFrontend` 失败路径对称停用后端

`loadFrontend` 失败时（import 超时 / activate 抛错 / 超时），除既有 `clearPlugin` + `pluginMarkError` 外，追加：
- 若该插件此前已成功 `plugin_activate`（WASM 实例已创建，**不论当前状态 Activated/Degraded/Error** —— markError 只置 Error、实例仍在 `wasm_plugins`），则调用 `pluginDeactivate` 拆解后端 WASM 实例；
- 保证「前端注册表为空 ⇔ 后端非存活」，重启用从干净状态重启，不命中幂等早退。

### D4 — `PluginView.handlePluginToggle` 失败回退三态一致

catch 路径当前只回退 `pluginEnabledStates`（UI 镜像）。注意 `pluginSetEnabled(enabled)` 已在激活**之前**执行（`PluginView.vue:557`），失败后 enabled 持久化保持 true —— 这是自愈意图（下次启动 auto-activate 重试），不回写。追加的收敛动作：
- 调用 `pluginLoader.deactivate`（幂等）确保后端运行时拆解，即使前端模块未加载；
- 运行时与 UI 都必须收敛到「停用」终态（后端 WASM 拆解 + 前端开关回退）；enabled 持久化保持用户意图；
- 超时兜底路径（`TOGGLE_TIMEOUT_MS`）同步拆解，避免 15s 后开关回退但后端仍活。

### D5 — `clearPluginEvents` 真正 dispose

`events.ts` 增加「pluginId → disposable 列表」反向索引（或在 `on()` 注册时登记）；`clearPluginEvents` 遍历触发 `dispose()`（含 `tauriUnlisten?.()`）后再删除内存 Set，消除 Tauri listener 泄漏。`context._disposables` 主清理路径不动。

### D6 — `device_bridge::stop_browse` 对称清 memo

`stop_browse` 在清 `sessions` 的同时清空 `endpoints` memo。**取舍注明**：endpoint memo 是为跨激活重连复用而保留（`device_bridge.rs:70` 注释），全清会丢失该复用；本决策采用「全清」以收敛内存不变量，若回归测试证明重连体验受损，改为「无活跃 session 时再清」的折中。

### D7 — ToolboxView 二级页严格跟随启用状态（验收 + 加固）

既有三道闸已实现：`refreshTick`（KeepAlive reactivate 强制重渲染）+ `activePluginView` 引用相等性检查（停用后旧对象不在新数组中 → 二级页隐藏）+ 空态占位。本 spec 将其作为**验收不变量**；此外补一次**防御性**路由级 `key` 加固：`<router-view :key="route.fullPath">` 使 KeepAlive 缓存以路由切换为准，杜绝「onActivated 钩子与渲染时序存在边角竞态时先渲染旧组件帧再消失」的假象（当前代码在多数时序下无此问题，加固为双保险）。

---

## 5. 测试决策

### 主 seam：前端 loader + registry 的启用/停用闭环测试（vitest）

- **测试什么（外部行为）**：`pluginLoader.activate` / `pluginLoader.deactivate` 连续交替 N 次后，断言：
  - `registry.toolboxViews` 内该插件视图出现次数 = 0 或 1（不残留旧引用、不重复）；
  - 停用后 `registry.toolboxViews` 不含该插件；重启用后重新包含；
  - 前端模块加载失败时，`pluginCmds.pluginDeactivate` 被调用（后端拆解），且 `plugins` Map 无残留；
  - 二级页守卫：模拟 `activePluginView` 指向的旧注册对象，断言停用后 getter 返回 null。
- **仅测外部行为**：mock `pluginCmds`（`pluginGetInfo`/`pluginActivate`/`pluginDeactivate`/`pluginMarkError`/`pluginSetEnabled`）与动态 `import()`，不触及 Vue 渲染细节。
- **先例**：`.scratch/mobile-plugin-lifecycle-truthful-state` 的 loader.ts gating 矩阵已用 vitest 覆盖（pnpm 36 文件 304/304）；`bedcode-mobile/vitest.config.ts` 已就绪。

### 次 seam：后端 mdns 多播锁守卫（cargo test）

- 把「是否应在当前上下文同步获取多播锁」抽为可测判据；用例覆盖 runtime 内外两种上下文，断言不 panic、不嵌套 block_on。
- 同一测试模块加「browse → stop_browse 连续多次」用例，断言 `BROWSERS` 表不增长、stop_browse 幂等返回 false。
- 先例：`device_bridge.rs` 已有 `#[cfg(test)]` 静态态隔离单测；`mobile-plugin-lifecycle-truthful-state` cargo test 4/4 全绿。

### 验收（人工/真机，纳入 ticket）

- Android 真机：file-transfer 启用 → 二级页 → 插件管理停用 → 返回 toolbox，断言二级页隐藏、入口消失；再启用断言入口复现。
- 连续 10+ 次 toggle，断言入口严格跟随、无闪烁残留。

---

## 6. 范围外

- 桌面端同名生命周期问题（本次仅移动端）。
- 插件并发加载调度优化（多次快速 toggle 的节流/去抖 UI 层处理）。
- 新增前端状态机或重构 plugin lifecycle 模型（以最小侵入修复为主，不复刻既有 spec 的 Degraded 机制）。
- mdns-sd crate 自身多次 start/stop 的资源回收行为（依赖宿主防护层兜底）。
- 文件传输插件的传输/设备发现业务逻辑本身。

---

## 7. 补充说明

- **持久化语义（核对代码后更新）**：`.scratch/mobile-plugin-lifecycle-truthful-state/spec.md` §3.5 桌面语义是「失败不回写 false」，但移动端实际代码**分叉为两条**——`status_reporter`（插件自报错误）落盘 `enabled=false`（`manager.rs:208`），`mark_error`（前端 loader 失败）保留 intent（`manager.rs:817-822`）。本 spec 不改动这两条语义，只在它们之上恢复「前端注册表为空 ⇔ 后端非存活」与「重启用从干净状态重启」。
- D2/D3 完成后，`manager.rs` 的 `plugin_activate` 幂等早退（Activated/Degraded → Ok）与 Error 重试两条路径都不会再产生「前端模块缺失但后端存活」的夹生组合——要么前后端都活着，要么都停用；`report_ready` 自愈（Error→Activated）也不能再让「后端 Activated、前端入口缺失」长期停留。
- D6 的 endpoint memo 全清会失去跨激活重连复用，属刻意的内存不变量取舍；如回归证明体验受损，回退为「无活跃 session 时清」。
- 事件泄漏（D5）是次级防线：主路径 `context._disposables` 已随 `deactivate` 逐条 dispose；本 spec 补齐的是「前端模块未加载但已有 Tauri listener」的边角。
- mdns panic（§1.2）的精确触发条件依赖 tauri/tokio 运行时上下文细节，静态分析无法 100% 定论；ticket 01 的测试需在**已进入 runtime 上下文**的场景实证（改动前可触发、改动后不触发），并同时消除「宿主函数阻塞 worker」这一反模式本身。
