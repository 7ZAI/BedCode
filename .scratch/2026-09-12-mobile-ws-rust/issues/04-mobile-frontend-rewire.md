# 04: 移动端前端接线（订阅时机 / 模式切换 / 历史拼接 / 输入路由）

Type: task
Status: resolved（2026-09-12 晚收尾：测试基建修复 + 验收全绿）
Blocked by: 03

## Answer（补充）
接线与验收完成：useTerminalSocket 退役、store 改 Rust 命令驱动 + 事件消费；启动即订阅/停止取消/断开重建/页面进出模式切换全链路就位；历史拼接「拼完才消费」；输入走 terminal_send_input。
修复：测试基建 3 处（vi.mock 孤立 fn、旧 v2 socket 集成测试整文件重写、markSessionStopped 竞态代数推进）+ useMobileConnection JSON 类型收窄。
验证：vitest 372 全绿 · vue-tsc 0 · eslint 0 error。

## 范围
- `useTerminalSocket.ts` 退役（终端 WS 全部进入 Rust）；`terminalBuffer` store 改为驱动 Rust 命令 +
  消费 `terminal-*` 事件 + 维护 lastRenderedOffset
- 历史拼接：进入终端页 → `terminal-history-ready` 写入 xterm 完成后才开始消费 `terminal-frame`
  （「拼接完历史才通知前端消费」）；跨帧裁剪（overlap = cursor - startOffset）
- 订阅时机：useMobileConnection startSession 成功 → terminal_subscribe；会话 Stopped → unsubscribe；
  意外断开（terminal-state reconnect）→ Rust 自动处理；手动断开 → 全量 unsubscribe
- 模式切换：TerminalView 挂载 → set_mode(realtime)；卸载（会话未停）→ set_mode(batch)
- 输入：sendInput 改走 `terminal_send_input`（Rust → WS → 桌面 PTY）；resize 保持既有路径
- 退出终端页但会话存活期间：实时事件照常入 Rust 缓存，前端不再渲染
- 测试：mockInvoke + 事件模拟适配；跨帧裁剪用例

## 验收
- `cd bedcode-mobile && pnpm run test:run` 全绿；根目录 `pnpm exec eslint .` 0 error
- DEV mock 会话路径不回归（mockTerminal 直出，不依赖 Rust）

## 落地核对（09-12 晚续做）
- useTerminalSocket.ts 已删、terminalBuffer store 重写为 Rust 命令驱动 + terminal-* 事件消费 + lastRenderedOffset；
  历史拼接改走 `terminalGetHistory` invoke（缓存优先，“terminal-history-ready”事件改为命令返回值实现，偏差记录于 ticket 03）
- TerminalView 挂载/卸载 → register/unregisterRealtimeHandler + markPageEntered/Left（realtime/batch 双速）✓
- startSession 成功即订阅 ✓；Stopped → markSessionStopped ✓；重连 onPaired 重建订阅 ✓；断开全量取消 ✓
- 输入走 terminal_send_input ✓（resize 保持既有路径）
- 修复：① useTerminalBuffer.test.ts vi.mock 孤立 fn 导致事件 handler 从未注册（8 fail）→ 共享引用修正；
  ② terminal-flow.test.ts 旧 v2 socket 架构 stale（lastRenderedSeq/onHistoryEnd）→ 整文件重写 Rust 驱动；
  ③ markSessionStopped 缺 replayGeneration 推进（在途 splice 重写游标）→ 补代数推进；
  ④ useMobileConnection autoReconnect JSON 未收窄（vue-tsc error）→ typeof 收窄、保 0=不等待语义
- 验证：vitest 372 全绿；vue-tsc 0；eslint 0 error（60 warning 既有）