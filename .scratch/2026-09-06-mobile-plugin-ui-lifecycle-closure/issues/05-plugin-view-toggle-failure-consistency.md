# 05 — PluginView toggle 失败回退三态一致（UI/后端/注册表收敛）

**What to build:** `handlePluginToggle` catch 路径当前只回退 `pluginEnabledStates`（UI 镜像），后端可能已 Activated（WASM 存活）或 Error，UI 状态 ⇄ 后端状态漂移。失败时收敛到「停用」终态：调用幂等 `pluginLoader.deactivate` 拆解后端运行时；保持持久化 enabled = 用户意图不回写。

设计依据见同目录 `../spec.md` §1.1 C、§4 D4。

**Type:** task
**Status:** resolved
**Blocked by:** 03（依赖 `deactivate` 对「前端模块未加载」也能拆解后端的能力）

- [x] catch 路径追加幂等 `pluginLoader.deactivate(pluginId)`（即使前端模块未加载）
- [x] 超时兜底路径（`TOGGLE_TIMEOUT_MS` 15s 后）同步拆解，避免开关回退但后端仍活
- [x] 持久化 enabled 不回写（用户意图保留），UI 开关回退到停用
- [x] `loadPlugins()` 重拉后 `pluginEnabledStates` 与后端一致

## 验收
- vitest（spec §5 主 seam）：mock `pluginLoader.deactivate` 抛错场景，断言开关回退 + 后端命令被调 + 持久化不变。
- 真机：启用 file-transfer 使其失败（或 mock），toggle 后 UI 开关回到停用、后端无残留。

## Comments

实现（2026-09-06）：
- catch 路径：幂等 `pluginLoader.deactivate(pluginId)`（依赖 issue 03 的「前端模块未加载也能停用后端」）→ `loadPlugins()` 重拉刷新状态徽章 → UI 开关跟随运行时终态（拆解成功→停用；拆解失败→回退用户原方向取反）。持久化 enabled 不回写。
- 超时兜底路径（TOGGLE_TIMEOUT_MS）：回退开关 + fire-and-forget `pluginLoader.deactivate`（拆解失败仅记日志，不阻塞收尾）。
- vitest 新增 `pluginToggleConvergence.test.ts` 2 例全绿：启用失败 → pluginSetEnabled 仅一次启用方向写入（持久化不回写）+ deactivate 被调 + 开关回退；拆解也失败 → 开关仍回退、错误仅记日志。
- 待办：真机启用失败场景人工验收（本机无设备）。
