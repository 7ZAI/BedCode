# 02 — 前端消费新状态：降级徽章与加载门禁

**What to build:** 插件管理页对 Degraded 插件显示明确的降级标识而非「已启用」；前端模块加载门禁放行 Degraded（后端实例在运行、入口应可见），并在控制台标注降级原因。

设计依据见同目录 `../spec.md` §3.6、§5.1（开放问题裁决：功能门禁先放行 + UI 标识）。

**Blocked by:** 01 — SDK 契约补全 + 宿主状态机

**Status:** resolved

- [x] 前端两侧 `PluginState` 联合类型扩展 `Degraded` / `Activating`，线协议形状与 Rust serde 一一对应
- [x] 启动加载门禁：Degraded 加载前端模块并 warn 降级原因；Activating 及其余状态维持现有跳过行为
- [x] 插件列表/详情页状态徽章区分展示降级态；所有 `state === 'Activated'` 相等性判定点逐一核对归类（徽章展示 vs 功能门禁）
- [x] i18n 新 key 同步出现在 zh-CN 与 en
- [x] 测试 fixtures 覆盖新状态；加载门禁分支有组件级测试
- [x] `npm run test:run` 全绿

## Comments（实施记录 2026-08-26）

**类型**：`src/plugin/types.ts` + `packages/plugin-sdk-desktop/src/types.ts` 双写副本同步扩展；SDK 副本顺带补上此前缺失的 `NeedsApproval`，两侧均与 Rust `PluginState`（tag="state", content="error"）7 变体一一对应。

**判定点归类**（全部核对完毕）：

| 判定点 | 归类 | 处理 |
|--------|------|------|
| `loader.loadAll` 门禁 | 功能门禁 | Degraded 放行 + `console.warn` 标注降级原因（spec §3.6） |
| `PluginsView` ENABLED/DISABLED 分区、开关 ON 态 | 展示归类 | 改用新增 `isRunning()`（Activated+Degraded），降级行保持停用语义不误入未启用分区 |
| `PluginDetailView` 启停按钮样式/方向 | 展示归类 | 同上改 `isRunning()`；绿色圆点仅 Activated |
| `hasConfiguration` 配置入口 | 功能门禁 | 放行 Degraded（§5.1 裁决：用户可能正是要改配置修复启动失败） |
| `PluginConfigView.isActivatedState` → 重命名 `canConfigure` | 功能门禁 | 放行 Degraded（同上） |
| 徽章文案/配色 | 徽章展示 | `getStateKey` 映射新 key；详情页琥珀色徽章 + `degradedReason` 原因行；列表页琥珀色「已降级」chip（title 悬停见原始错误） |

**i18n**：`desktop.plugin.activating` / `degraded` / `degradedReason`（zh-CN 与 en 同步）。注意后端 `is_activated()` API 门禁仍严格 Activated，前端 `isRunning` 只用于展示/入口归类，语义边界已在 JSDoc 注明。

**测试**：fixtures 新增 `makeDegradedPluginInfo` / `makeActivatingPluginInfo`；新文件 `src/__tests__/integration/plugin-loader-gating.test.ts` 以 mark_error 为「已尝试加载」探针断言全状态矩阵门禁行为 + warn 含原因；plugin-flow.test.ts 增组件级场景（降级进已启用分区、徽章可见、开关停用语义）。

**验证**：`npm run test:run` 58 文件 517 例全绿；`vue-tsc --noEmit` 干净；eslint 0 error。⚠️ 勿用 `--pool=forks --singleFork` 手动覆盖跑全量——单 fork 共享进程下 Button.test 会报 `window is not defined`（环境固有问题，与本改动无关）。
