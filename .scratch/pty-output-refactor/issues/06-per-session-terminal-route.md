# 06 — 每会话终端路由 + TB v2 二进制帧（远程通道）

**What to build:** 新增 `GET /ws/terminal/session/{session_id}`：连接创建即绑定会话（`TerminalWs::new_for_session`），首消息 JWT 认证（02 的 gate），不存在会话回 `error(SESSION_NOT_FOUND)` 并关闭；actor 删 subscribed_sessions 多路复用（订阅即连接）。forward_loop 远程通道改 TB v2 二进制帧（16B 帧头：`magic "TB" + version=2 + flags(0x01 is_waiting) + seq(8 LE) + len(4 LE)`），删除 base64 JSON 形态；OutputBuffer 删 offset、保留合并（30ms/64KB）。会话消息（subscribe/history_end/auth/input/session_stopped）为 JSON 控制帧，无 message_id/expect_response。旧 `/ws/terminal` 路由不动（D2 compat）。

**Spec:** §5.1、§5.3、§5.4、§5.5

**Blocked by:** 02, 05

**Status:** ready-for-agent

- [ ] 新路由注册 + per-session actor + 会话不存在错误流
- [ ] 订阅/认证控制帧协议（JSON）+ 二进制输出帧编码（forward.rs）
- [ ] 远程通道合并策略适配 TB v2；删除 base64 文本帧
- [ ] 每会话 mpsc 容量 32768（D7）；try_send 背压丢弃保留
- [ ] 测试：TB v2 编解码、控制帧状态机、输出帧二进制校验、路由鉴权

## Comments