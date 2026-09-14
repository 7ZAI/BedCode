# 02 — 前端消费新状态：降级徽章与加载门禁（移动端）

**What to build:** 移动端插件管理页对 Degraded 插件显示明确的降级标识而非「已启用」；前端模块加载门禁放行 Degraded（后端实例在运行、入口应可见），并在控制台标注降级原因。

设计依据见同目录 `../spec.md` §3.5、§5.1（开放问题裁决：功能门禁先放行 + UI 标识）。移动端相对桌面端的差异：

- 桌面端用 `isRunning()`（Activated+Degraded）做展示归类；移动端条件更克制——`PluginView` 的徽章/分区已有现成 `stateBadgeClass` / `getStateKey`，补 Degraded 分支即可，无需抽 `isRunning()`
- 移动端无配置入口（`PluginConfigView`）类似桌面端的 `canConfigure` 提早放行问题——直接按状态徽章 + `console.warn` 即可

**Blocked by:** 01 — SDK 契约补全 + 宿主状态机

**Status:** done（2026-09-04 审计确认，spec §3.5 全部 checklist 已落地；与桌面端 issue 02 对位）

- [x] 前端两侧 `PluginState` 联合类型扩展 `Degraded` / `Activating`：`bedcode-mobile/packages/plugin-sdk-mobile/src/types.ts:20-22` 与 `bedcode-mobile/src/plugin/types.ts` 双写副本同步
- [x] 启动加载门禁（`src/plugin/loader.ts:101-111`）：`Activating` 中间态跳过本次前端加载（`console.log` 标注，等最终态轮询兜底）；`Degraded` 放行前端加载并 `console.warn` 标注降级原因
- [x] 插件列表/详情页状态徽章区分展示降级态：`src/views/PluginView.vue:704-716` `stateBadgeClass` 增 `Degraded` 分支（琥珀色 `mobile-warning` 背景 + 文字色）；`getStateKey`（`PluginView.vue:719-727`）映射 `mobile.plugin.stateDegraded` / `stateActivating` 文案 key
- [x] i18n 新 key 同步出现在 zh-CN（`src/locales/zh-CN/mobile.ts:443,445`）与 en（`src/locales/en/mobile.ts:444,446`）：`stateActivating: '激活中' / 'Activating'`、`stateDegraded: '降级' / 'Degraded'`
- [x] 后端 `is_activated()`（`manager.rs:761-767`）门禁语义保持严格 `Activated`（spec §3.3 末段，§5.1 开放问题不归入本票）
- [x] `loader.activate` / `deactivate`（`loader.ts:117-166`）：对 Degraded 插件可正常 re-activate（后端 `activate` 入口 `Activated | Degraded` 判幂等早返回，重复 activate 走完整 phase 1b 重试 on_startup）

## 实现记录（2026-09-04 审计）

- **类型双写**：`bedcode-mobile/packages/plugin-sdk-mobile/src/types.ts:20-22` 与 `bedcode-mobile/src/plugin/types.ts` 同步扩展 `Activating` / `Degraded { error: string }`，与 Rust serde tag 形状一致
- **loader 门禁语义**（`src/plugin/loader.ts:88-114`）：
  - `isEnabled`（持久化意图）门禁保留：意图优先，避免重启丢扩展点（spec §3.5 论证）
  - `Activating` 跳过本次加载（中间态）：`console.log("Plugin X activating, deferring frontend load")`
  - `Degraded` 放行加载：`console.warn("Plugin X loaded but degraded: <error>")` 标注降级原因，前端模块照常挂载
- **徽章**（`src/views/PluginView.vue:704-727`）：
  - `stateBadgeClass`：Error → danger 背景；`NeedsApproval` / `Degraded` → amber（`mobile-warning` 15% 透明背景 + 主色文字）；`Activated` → success 绿；`Activating` / `Loaded` / `Deactivated` → 灰（默认 muted）
  - `getStateKey` 显式分支：`Error`/`NeedsApproval`/`Degraded`/`Activated`/`Activating`/`Deactivated`/`Loaded` 全部覆盖
- **i18n**：`mobile.plugin.stateActivating` 与 `stateDegraded` 已在 zh-CN/en 同步（`mobile.ts:441-446` 区间）
- **activate 路径**（`loader.ts:117-134`）：`pluginCmds.pluginActivate` → `loadFrontend`；后端 `activate` 对 Degraded 重复进入走完整 phase 1b（on_startup 重试），无需前端特殊处理
- **deactivate 路径**（`loader.ts:137-166`）：Deactivated 终态由后端保证（manager.rs:696 守卫 + 696 行后状态写），前端只需清理 disposable + 事件监听

## 验证（2026-09-04 已跑全绿）

- [x] `pnpm run test:run`（mobile，AGENTS.md 强制 pnpm；`pnpm run test` = vitest watch 模式挂起，禁止使用）—— 36 文件 / 304 例全绿（2026-09-04 实际跑，含 09-04 补的 1 例 + 修的 splash.ts JSDoc 解析 bug）
- [x] **2026-09-04 已补**：loader gating 状态矩阵单测 1 主 case，落在 `bedcode-mobile/src/__tests__/integration/plugin-loader-gating.test.ts`，覆盖 8 个插件（Activated / Degraded / Activating / Loaded / Error / NeedsApproval / Deactivated / `pluginType: 'rust'`）：
  - 6 个状态放行 → 触发 `plugin_mark_error`（asset:// 动态 import 失败是 spec 内失败行为，不是 loader bug）：Activated / Degraded / Loaded / Error / NeedsApproval / Deactivated
  - 2 个跳过：Activating（中间态 defer）+ `pluginType: 'rust'`（结构跳过，前端模块本就不存在）
  - Degraded 路径断言 `console.warn` 含 `'DEGRADED'` + 插件 id + 原始 error 串
  - 套用桌面同款模式：mock `@tauri-apps/api/core` invoke、`pluginCmds` namespace import、markErrorCount 探针
  - **issue 04 诊断失败 case 跳过**：spec §3.6 P2 移动端未实施，loader.ts 无 `plugin_frontend_load_report` 命令，第二个 case 不写
- [x] **附修 bug**：`src/config/splash.ts:9` JSDoc 注释里的 `locales/*/mobile.ts` 字面 `*/` 把 `/**` 块注释提前关闭，导致 5 个集成测试文件（connection-flow / terminal-flow / pairing-flow / session-flow 等）transform 失败；转义为 `` `locales/<locale>/mobile.ts` `` 模板字符串形式解决。本 bug 与本 spec 改动无关，是文档命令字眼未随工具链迁移同步的预存问题；本轮顺手修
- [ ] dev run：4 个内置插件启动后徽章显示正常；手动注入 Degraded（如临时把 file-transfer 的 `on_startup` 改为 Err）验证「降级」徽章 + 控制台 warn
