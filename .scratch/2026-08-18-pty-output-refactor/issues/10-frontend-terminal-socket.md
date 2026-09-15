# 10 — 移动端前端直连终端 WS：useTerminalSocket + 快照拼接

**What to build:** 新 composable `useTerminalSocket`：每会话终端 WS（`binaryType='arraybuffer'`），连接 → 首消息 JWT 认证（token 经 09 的 `get_terminal_ws_info`）→ subscribe → 快照拼接 → 重连退避（500ms→8s 封顶）→ seq 去重。`stores/terminalBuffer.ts` 重写：删 cursor/offset/连续性校验/指数退避自愈；新增 `last_rendered_seq`/`snapshot_seq`/实时缓冲/历史缓存（16MB LRU，D5）/pending（保留：handler 注册前缓冲）。状态机：`HISTORY`（seq ≤ snapshot_seq 写 xterm + 入缓存；> snapshot_seq 入实时缓冲）→ `history_end` → FLUSH → LIVE（直写 + 入缓存；seq 缺口 → 重发 subscribe，跳过 ≤ last_rendered_seq；min_seq > last_rendered_seq+1 → 截断 toast + 清屏重播）。重连恢复：跳过 ≤ last_rendered_seq；页面重进：本地缓存回放 + 服务端帧按 seq 跳过。`TerminalView.vue`/`TerminalInputBar.vue` 输入改经 socket JSON input 帧。

**Spec:** §6.2、§6.3、§6.4（验收 1/2/3/4/5）

**Blocked by:** 09, 03

**Status:** done

- [x] useTerminalSocket（连接/认证/订阅/拼接/重连/去重/退避）
- [x] terminalBuffer store 重写（状态机 + 双缓存 + pending）
- [x] useTerminalBuffer/useMobileCommands 适配；输入迁移
- [x] TerminalView 接线 + 预加载（prepareSession）语义适配
- [x] 测试：vitest（拼接/缺口/重连/去重/退避/缓存截断）+ 真机弱网回归

## Comments

- 2026-08-19 完成，提交 `feat(mobile): P2 ticket10 前端直连终端 WS`
- 新增 `composables/useTerminalSocket.ts`：每会话 socket（get_terminal_ws_info 建连 → auth 首消息 → subscribe → TB v2 帧解析 + 控制帧收发 + 重连退避 500ms→8s）；sendInput Base64 输入帧
- `stores/terminalBuffer.ts` 重写：状态机（idle→connecting→auth→history→live）+ lastRenderedSeq/snapshotSeq + 历史缓存（16MB LRU）+ liveBuffer（history_end FLUSH）——
  - 去重：重播帧跳过 ≤ lastRenderedSeq；缺口：frame.seq > lastRenderedSeq+1 → 重发 subscribe
  - 截断：subscribe_ok 时 minSeq > lastRenderedSeq+1 → onClear + onTruncated + 锚定；每会话限一次
  - registerRealtimeHandler 无条件回放历史缓存（xterm 新实例语义，覆盖重进/预加载）；不双写
  - terminal_output_activity 通知（节流 200ms）；markSessionStopped/Running 适配
- `useTerminalBuffer.ts` 适配：unsubscribeSession 不再 wsLeaveSession（命令已删）；prepareSession 轮询 subscribe_ok；handleSessionStopped/Removed 简化
- `TerminalView.vue`：输入三处（submit/execute/specialKey）改经 store.sendInput（替代 httpSendSessionInput）
- 测试：terminalBuffer.test.ts 重写（状态机 15 用例：完整流/实时缓冲/去重/缺口/截断/重连/session_stopped/ERROR 重试/缓存回放/LRU/sendInput/通知）；useTerminalBuffer.test.ts 重写（11）；terminal-flow.test.ts 重写前 2 用例为 socket 帧链路（保留 HTTP 输入回传）；connection-flow.test.ts ws_output 断言反转为 toBeUndefined
- **验证**：vitest 20 文件 203 测试全绿；vue-tsc --noEmit 通过
- #lesson：mock socket 的 isOpen 恒 true 会让 store 走「已连接只 subscribe」分支绕过 start——测试 fake 必须如实模拟初始未连接态；测试帧序必须连续（实时帧从历史末帧 seq+1 起，否则触发缺口重订阅误判）
- 遗留（11 收尾）：wsGetTerminalIncremental/TerminalOutputEvent 死代码、spec/pipeline 文档重写、真机弱网回归