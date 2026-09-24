# 09b: WS 动作词表迁移 · migrate（存量词表逐批改走声明式路由）

**What to build:** 承继 09a 的 expand，把现有宿主硬编码的 WS 动作（会话启动/停止/删除/调整大小/订阅/列表等）逐批迁移到插件声明式路由下，每批保持 CI 绿（旧 switch 并存兜底）。按动作域分批（会话控制、终端输入/订阅、同步/队列推送）。迁移接线由声明对应插件（terminal-session）承接实际业务，宿主只做转发与鉴权帧分派。

**Blocked by:** 09a

**Status: ✅ done（2026-09-24，与 09c 同批落地）**

- [x] 会话控制五域（list/start/stop/remove/resize）词表解释迁插件 `ws_control`（互调 api `session-ws-control` + `events-ws.on-client-message` 直连帧协议），宿主 `/ws/event` 改走声明闸门 + 转发 + 回包信封
- [x] 声明端点 `session-control`（auth=jwt）直连 e2e：jwt 认证成功 + list 回包 + 未知动作 error 帧 + 二进制忽略
- [x] 行为不回退：`pty_session_chain` 集成测试经转发层全绿（移动端 wire 逐字不变）
- [x] 帧时序/字段语义与旧路径一致：信封 message_id / session_id（新会话 id）与原 `handle_control` 逐字一致
