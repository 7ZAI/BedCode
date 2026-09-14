# 03 — loader activate/deactivate 对称拆解（前端模块未加载也停用后端）

**What to build:** 让前端插件加载/卸载链闭环：
- `deactivate` 移除「`plugins` Map 无记录即早退」的不对称 —— 即使前端模块从未加载成功，只要后端命令可达即调用 `pluginCmds.pluginDeactivate`，避免后端 WASM 实例泄漏、mDNS browse 继续运行。
- `loadFrontend` 失败路径除 `clearPlugin` + `pluginMarkError` 外，若该插件此前已 `plugin_activate` 成功，追加 `pluginDeactivate` 拆解后端，保证「前端注册表为空 ⇔ 后端非存活」，重启用从干净状态重启。

设计依据见同目录 `../spec.md` §1.1 A/B、§4 D2/D3。

**Type:** task
**Status:** resolved
**Blocked by:** None — can start immediately.

- [x] `deactivate`：后端命令可达即调用 `pluginDeactivate`；前端侧（disposables / clearPluginEvents / clearPlugin / module.deactivate）存在则执行、缺失则跳过；全程幂等
- [x] `loadFrontend` 失败路径：追加对称 `pluginDeactivate`（若后端已激活）
- [x] `activate` 失败路径：不重复 `clearPlugin`（`loadFrontend` 内部已做），但确保后端拆解
- [x] 保持持久化 enabled = 用户意图不回写（对齐既有 spec §3.5）

## 验收
- vitest（spec §5 主 seam）：mock `pluginCmds`，断言：
  - 前端模块加载失败时 `pluginDeactivate` 被调用；
  - `deactivate` 在 `plugins` 无记录时仍调用 `pluginDeactivate` 且幂等；
  - 连续 activate/deactivate N 次后 `registry.toolboxViews` 无残留旧引用、无重复。

## Comments

实现（2026-09-06）：
- `deactivate` 移除「plugins Map 无记录即早退」：后端命令可达即 `pluginDeactivate`；前端侧（disposables / module.deactivate / clearPluginEvents / clearPlugin）存在则执行、缺失则跳过；全程幂等。module.deactivate 提前到 clearPlugin 之前（若模块停用期间再注册，随后即被清掉）。
- `loadFrontend` 失败路径：`clearPlugin` → `pluginDeactivate`（先于 markError——后端 deactivate 仅对 Activated/Degraded 生效，顺序颠倒实例无法拆解）→ `pluginMarkError`。
- `activate` 重构为分步收口：pluginActivate 失败仅 markError（无实例可拆）；loadFrontend 失败由其内部完整拆解，不再重复 clearPlugin/markError。
- 持久化 enabled 不回写（保持用户意图，下次启动自愈重试），符合既有 spec §3.5。
- vitest 新增 `plugin-lifecycle-teardown.test.ts` 6 例全绿：连续 activate/deactivate 3 轮入口 0/1 严格跟随且无旧引用；前端模块加载失败 deactivate 先于 markError；plugins 无记录时 deactivate 仍通知后端且幂等。移动端全套 39 文件 325 测试通过。
