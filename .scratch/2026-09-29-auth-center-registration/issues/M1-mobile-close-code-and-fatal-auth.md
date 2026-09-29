# M1: 移动端保留 close code + 4001 致命不自杀（放大器修复）

**What to build:** 移动端丢弃 close code 导致认证拒绝被当网络断连自愈——这是本次日志/toast 风暴的**直接放大器**，必须同批修，不能只修桌面。

**Blocked by:** 无（可与 03 并行）

**Status:** todo

- [ ] `connection/ws_client.rs`：`ServerClosed { code, reason }` 保留 code（不再只取 reason 字符串）
- [ ] `connection/manager.rs::spawn_connection_monitor`：认证类 code（4001/4003）判**致命** → 不进 `reconnect()`、只发**一次** toast、提示需重新配对/重连
- [ ] `useMobileConnection.ts:389-404`：`ws_unexpected_disconnect` 补去重（同一 code+reason 在窗口内只弹一次）
- [ ] 错误文案走 i18n 业务码（AGENTS §6：禁止硬编码中文字符串）
- [ ] 单测：4001 → 不调 `reconnect()`；网络类 code → 仍自愈

## 关键实现事实

- 现状：`ws_client.rs:265-273` 只取 `reason.to_string()`，code 在 `CloseFrame` 里被丢掉
- 桌面 `channel/plugin.rs:43` `CLOSE_AUTH_FAILED = 4001`；认证超时也是 4001 → 移动端按 code 分流即可，不需新协商
