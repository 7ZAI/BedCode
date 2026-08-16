# 03 — PTY 会话链路

**What to build:** 移动端远程终端的服务端半边在真实链路上可用：已认证的 WS 客户端能创建终端会话、向会话写入命令并收到命令输出，会话可正常关闭——整条「WS → 会话管理器 → 真实 PTY 进程 → 输出回传」链路端到端打通。

**Blocked by:** 02 — WS 配对与认证链路（需要已认证客户端身份发起会话操作）

**Status:** resolved

- [x] 已认证客户端可创建会话，返回会话标识
- [x] 向会话写入简单命令（如 echo）后收到包含预期输出的输出事件
- [x] 会话关闭后状态一致（关闭事件或后续操作报已关闭）
- [x] 未认证客户端无法创建会话（与 02 的拒绝行为衔接）
- [x] Windows 平台真实 PTY 进程可用（失败时需区分测试环境问题与链路缺陷，并在报告中说明）

## Answer

实现于 e57c9132（2026-08-16）。`tests/pty_session_chain.rs`（约 565 行）：单测试函数 4 场景串行，复用 02 基建（AppContext 组装 + 配对认证 + 端口探测）。真实链路验证：StartSession（需预置会话配置）→ openpty + powershell spawn → PtyReader → GlobalOutputManager mpsc → forward_loop base64 编码 → WS `Terminal::Output` 帧。20s 轮询宽容 PTY 非确定时序；subscribe 先行（start_seq=None 队列重播 + pending 排空）兜底竞态；失败信息区分环境问题（PATH/ConPTY）与链路缺陷。连跑 2 次稳定，lib 538 无回归。
