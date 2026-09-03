# 11 — 前端 NotificationAPI + 扩展点收尾

Type: task
Status: resolved
Blocked by: —

## 问题

移动端前端插件缺通知能力（宿主已依赖 `@tauri-apps/plugin-notification`）。
国际化（I18nAPI）已具备，补齐通知 + 统一导出收尾。

## 任务

1. SDK `src/types.ts`：
   - `NotificationAPI { notify(title: string, body?: string): Promise<void> }`
   - `PluginContext` 增加 `readonly notifications: NotificationAPI`
2. 宿主 `src/plugin/context.ts` 实现：调 `@tauri-apps/plugin-notification` 的 `sendNotification`。
3. `index.ts` 导出新类型；确认 `types.ts` / `runtime.ts` / `vite-plugin.ts` 导出完备（与桌面端同构）。
4. 宿主 `src/plugin/types.ts` re-export 保持同步。

## Answer

## Answer

1. SDK types.ts 新增 NotificationAPI（notify），PluginContext 增加 notifications。
2. context.ts 实现：@tauri-apps/plugin-notification 动态导入 + 权限检查后 sendNotification。
3. index.ts / 宿主 types.ts re-export 同步。
4. vue-tsc 类型检查通过。

## 验收

- `npm run test:run` 通过。
- 插件可发系统通知。
