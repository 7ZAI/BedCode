# 06 — ToolboxView 二级页严格跟随启用状态（验收 + 路由级加固）

**What to build:** 验证并加固「插件停用后，其二级页在返回原入口时隐藏」的不变量。既有三道闸已实现（`refreshTick` + `activePluginView` 引用相等性 + 空态占位）；本票补齐 `onActivated` 时机竞态（切回时先渲染一帧旧组件再消失的「闪一下」假象），并产出验收用例。

设计依据见同目录 `../spec.md` §4 D7。

**Type:** task
**Status:** resolved
**Blocked by:** 03（停用后注册表清理依赖对称拆解先落地）

- [x] 宿主 `<router-view>` 增加 `:key="route.fullPath"`，使 KeepAlive 缓存以路由切换为准，消除「闪一下再消失」
- [x] 保持 `refreshTick` / `activePluginView` 引用相等性检查不动
- [x] vitest：模拟 `activePluginView` 指向旧注册对象，断言停用后 getter 返回 null、入口列表呈现
- [x] 真机验收用例（写入 checklist）：file-transfer 二级页 → `/mobile/plugins` 停用 → 返回 toolbox，断言二级页隐藏、入口消失；再启用断言入口复现

## 验收
- vitest 二级页守卫用例绿。
- 真机：上述路径严格跟随，无闪烁残留。

## Comments

实现（2026-09-06）：
- `MobileLayout.vue` 宿主 router-view 的 keep-alive 分支补 `:key="route.fullPath"`（终端分支原本已有）：KeepAlive 缓存以路由切换为准，杜绝同名宿主组件跨路由复用实例的「先渲染旧帧再消失」边角竞态；`refreshTick` / `activePluginView` 引用相等性检查未动。
- vitest：`toolboxKeepAlive.test.ts` 新增「二级页严格跟随停用」用例——activePluginView 暂存旧注册对象，停用（仅 clearPlugin 不重注册）后 getter 返回 null、空态占位入口列表呈现。2/2 通过。
- 真机验收用例（checklist，待人工执行）见下。

### 真机验收用例（checklist）

前置：Android 真机安装 debug 包，file-transfer 插件已启用。

1. **二级页严格隐藏**：进入工具箱 → 点入 file-transfer 二级页 → 底部导航/设置进入「插件管理」→ 停用 file-transfer → 返回工具箱入口（页面级返回）。
   - 断言：二级页不残留（不出现「文件传输」标题页），呈现入口列表/空态占位。
   - 断言：无「先闪一帧旧二级页再消失」。
2. **入口消失**：停用后工具箱列表中 file-transfer 入口立即消失（空态占位出现）。
3. **入口复现**：重新启用 file-transfer → 返回工具箱，入口出现，二级页可正常进入。
4. **多次 toggle**：连续启用/停用 10+ 次，每次断言入口严格跟随、无闪烁残留、无重复入口。
5. **停用后端真停**：停用后 adb logcat 无 file-transfer mDNS browse 活动（`mdns browse loop exited` 日志出现）。
