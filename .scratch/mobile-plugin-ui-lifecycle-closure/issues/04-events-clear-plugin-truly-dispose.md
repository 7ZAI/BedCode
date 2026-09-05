# 04 — clearPluginEvents 真正 dispose Tauri listener

**What to build:** `events.clearPluginEvents` 当前直接 `delete(set)`，不触发每条 disposable 的 `tauriUnlisten?.()`，Tauri listener 句柄泄漏。补反向索引，`clearPluginEvents` 遍历触发 `dispose()` 后再删内存 Set。

设计依据见同目录 `../spec.md` §1.3、§4 D5。

**Type:** task
**Status:** resolved
**Blocked by:** None — can start immediately.

- [x] `on()` 注册时登记 pluginId → disposable 反向索引（或在 clearPluginEvents 前保留可遍历的 disposable 引用）
- [x] `clearPluginEvents` 遍历触发 `dispose()`（含 `tauriUnlisten?.()`）后再删除 Set
- [x] `context._disposables` 主清理路径不动（`loader.deactivate` 已逐条 dispose）
- [x] 重复调用幂等：已 dispose 的不再触发

## 验收
- vitest：注册一个 `on(pluginId, event, handler)`，mock `tauriUnlisten`，`clearPluginEvents` 后断言 unlisten 被调用、再次调用无副作用。

## Comments

实现（2026-09-06）：
- `events.ts` 增加 pluginId → disposable 反向索引；`on()` 注册时登记，dispose 时摘除自身。
- `clearPluginEvents` 先摘索引再快照逐条 dispose（含 `tauriUnlisten?.()`）后清内存 Set；兜底清扫无索引残留。
- `context._disposables` 主清理路径未动；双重清理经 dispose 幂等位收敛（仅触发一次 unlisten）。
- 补充竞态修复：dispose 先于 Tauri listen 建立时，listen resolve 后立即反注册（`disposed` 标志位），不留僵尸 listener。
- vitest 3 例全绿：unlisten 被调 + 幂等；dispose/listen 竞态；主路径双重清理不冲突。
