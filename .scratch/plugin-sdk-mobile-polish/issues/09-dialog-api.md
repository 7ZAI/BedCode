# 09 — 前端 DialogAPI（弹窗扩展性）

Type: task
Status: resolved
Blocked by: —

## 问题

用户要求前端提供完备扩展性，包括弹窗。当前移动端 SDK 前端无 DialogAPI，
插件无法向用户展示确认框、输入框、自定义内容弹窗。

## 任务

1. SDK `src/types.ts`：
   - `DialogOptions { title?, message?, confirmText?, cancelText? }`
   - `DialogAPI { showDialog(opts): Promise<DialogResult>; showConfirm(opts): Promise<boolean>; showPrompt(opts): Promise<string | null>; showToast(message, type?): void }`
   - `PluginContext` 增加 `readonly dialogs: DialogAPI`
2. 宿主 `src/plugin/context.ts` 实现 `dialogs`：
   - 通过 `window.__BEDCODE_SHARED__` 暴露的 dialog 宿主服务调用（fire 事件或直接调宿主 composable）。
3. 宿主实现一个可复用的 dialog 渲染宿主（Vue 组件 + 队列），移动端样式（遵循 frontend-styles）。
4. `index.ts` 导出 DialogAPI 类型。

## Answer

## Answer

1. SDK types.ts 新增 DialogOptions / DialogResult / DialogAPI，PluginContext 增加 dialogs。
2. 宿主 src/plugin/dialog-host.ts：模块级响应式队列 + Promise 关联（FIFO 多弹窗），
   showDialog / showConfirm / showPrompt / resolveTop。
3. 宿主 src/plugin/PluginDialogHost.vue：Teleport 渲染宿主移动端样式弹窗（复用 --mobile-* 变量），
   App.vue 挂载。
4. shared-runtime 暴露 dialogs 到 window.__BEDCODE_SHARED__.dialogs；context.ts 实现 dialogs。
5. i18n：plugin.dialog.confirm / plugin.dialog.cancel（zh-CN + en）。
6. dialogHost.test.ts 4 个单测通过。

## 验收

- `npm run test:run` 通过。
- 插件内 `context.dialogs.showConfirm(...)` 弹出移动端样式确认框，返回用户选择。
- i18n：SDK 无中文硬编码（zh/en 由宿主统一管理）。
