# 06 — 每会话终端路由 + TB v2 二进制帧（远程通道）

**What to build:** 新增 `GET /ws/terminal/session/{session_id}`：连接创建即绑定会话（`TerminalWs::new_for_session`），首消息 JWT 认证（02 的 gate），不存在会话回 `error(SESSION_NOT_FOUND)` 并关闭；actor 删 subscribed_sessions 多路复用（订阅即连接）。forward_loop 远程通道改 TB v2 二进制帧（16B 帧头：`magic "TB" + version=2 + flags(0x01 is_waiting) + seq(8 LE) + len(4 LE)`），删除 base64 JSON 形态；OutputBuffer 删 offset、保留合并（30ms/64KB）。会话消息（subscribe/history_end/auth/input/session_stopped）为 JSON 控制帧，无 message_id/expect_response。旧 `/ws/terminal` 路由不动（D2 compat）。

**Spec:** §5.1、§5.3、§5.4、§5.5

**Blocked by:** 02, 05

**Status:** done（commit `FIXME`）

- [x] 新路由注册 + per-session actor + 会话不存在错误流
- [x] 订阅/认证控制帧协议（JSON）+ 二进制输出帧编码（forward.rs）
- [x] 远程通道合并策略适配 TB v2；删除 base64 文本帧
- [x] 每会话 mpsc 容量 32768（D7）；try_send 背压丢弃保留
- [x] 测试：TB v2 编解码、控制帧状态机、输出帧二进制校验、路由鉴权

## Comments

### 协议决策（P2 前端与 07 本地通道必须遵循）

1. **TB v2 帧 seq = 帧内首事件 index；flags 高 7 位编码「事件数 - 1」（1..=128，超限拆帧）**。16B 帧头只有单条 seq（8B）+ len（4B）：合并帧若只带首 seq，消费端无法推导帧末 seq——seq 缺口检测（丢帧自愈）与重播去重（跳过 ≤ last_rendered_seq）都会在合并批次边界误判。约定：帧末 seq = `seq + event_count - 1`；单事件帧高 7 位 = 0，帧头与 spec 原义逐字节一致。`V2_FRAME_MAX_EVENTS=128` 超限拆分 flush（合并语义不变）。
2. **SubscribeMode / start_seq / mode / min_offset / max_offset wire 字段未删**（handoff 提的「协议层收敛」与 spec §7「旧路由消息结构不变」冲突，按 spec 执行）：旧 `/ws/terminal` 路由 + Message 枚举结构不动，v2.0.0 移动端 compat；协议收敛落在新路由控制帧（天然无这些字段）。07/09 迁移后若删旧路由字段需双端确认。
3. **会话停止通知**：新路由连接经 `SessionManager::subscribe_status()`（status broadcast）监听绑定会话 Stopped → 推送 `{"type":"session_stopped"}` 帧，不断开连接（前端自行决定交互）。
4. **旧路由 forward_loop** 改用 `OutputFormat` 三态枚举（LocalV1 20B / RemoteLegacy base64 / RemoteV2 16B），行为与改造前一致（单测全绿佐证）。

### 验证

- 单测：`server::ws::terminal_ws` 28 通过（TB v2 编解码/合并事件数上限/flags、控制帧解析、forward_loop 合并时序回归）
- 集成：`tests/ws_session_route.rs` 全流程 1 通过（AUTH_REQUIRED 拒绝对称 / SESSION_NOT_FOUND / 无效 token AUTH_FAILED / auth_ok→subscribe_ok→history_end→TB v2 实时帧校验 / session_stopped）
- 全量：lib 559 + 8 集成二进制全绿（含 ws_pairing_auth/pty_session_chain 回归）