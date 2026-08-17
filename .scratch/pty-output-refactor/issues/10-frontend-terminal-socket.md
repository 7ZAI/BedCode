# 10 — 移动端前端直连终端 WS：useTerminalSocket + 快照拼接

**What to build:** 新 composable `useTerminalSocket`：每会话终端 WS（`binaryType='arraybuffer'`），连接 → 首消息 JWT 认证（token 经 09 的 `get_terminal_ws_info`）→ subscribe → 快照拼接 → 重连退避（500ms→8s 封顶）→ seq 去重。`stores/terminalBuffer.ts` 重写：删 cursor/offset/连续性校验/指数退避自愈；新增 `last_rendered_seq`/`snapshot_seq`/实时缓冲/历史缓存（16MB LRU，D5）/pending（保留：handler 注册前缓冲）。状态机：`HISTORY`（seq ≤ snapshot_seq 写 xterm + 入缓存；> snapshot_seq 入实时缓冲）→ `history_end` → FLUSH → LIVE（直写 + 入缓存；seq 缺口 → 重发 subscribe，跳过 ≤ last_rendered_seq；min_seq > last_rendered_seq+1 → 截断 toast + 清屏重播）。重连恢复：跳过 ≤ last_rendered_seq；页面重进：本地缓存回放 + 服务端帧按 seq 跳过。`TerminalView.vue`/`TerminalInputBar.vue` 输入改经 socket JSON input 帧。

**Spec:** §6.2、§6.3、§6.4（验收 1/2/3/4/5）

**Blocked by:** 09, 03

**Status:** ready-for-agent

- [ ] useTerminalSocket（连接/认证/订阅/拼接/重连/去重/退避）
- [ ] terminalBuffer store 重写（状态机 + 双缓存 + pending）
- [ ] useTerminalBuffer/useMobileCommands 适配；输入迁移
- [ ] TerminalView 接线 + 预加载（prepareSession）语义适配
- [ ] 测试：vitest（拼接/缺口/重连/去重/退避/缓存截断）+ 真机弱网回归

## Comments