# 07 — 移动端 WS 加密接线（终端 + 事件通道）

**What to build:** `useTerminalSocket.ts` 与事件 socket（/ws/event 连接处）接入 05 的帧编解码层：pin 存在时 auth 首消息附 `crypto:{v:1, ek}`；`auth_ok` 校验 crypto echo——缺失时 strict 断连 / 非 strict 明文续跑并提示；按 spec 双 ECDH 派生 c2s/s2c 会话密钥；收发帧过编解码层（text JSON 控制帧与 binary TBv2 输出帧分路，seq 双向严格计数）；重连自动 rekey（全新临时密钥对，seq 归零）；close code 4003 映射为明确错误文案（i18n）。注意与既有 TBv2 背压 ack 环、auth 超时关闭逻辑的兼容。

**Blocked by:** 04, 05

**Status:** ready-for-agent

- [ ] vitest（mock WebSocket）：协商握手 → text/binary 加密帧双向往返 → 终端缓冲数据流顺序正确
- [ ] auth_ok 缺 crypto echo：strict 断连分支 + 非 strict 明文续跑分支均有测试
- [ ] 重连后使用全新密钥（旧 seq 状态不残留，单测断言）
- [ ] 4003 close 映射错误文案，i18n zh-CN / en 成对
- [ ] 与背压 ack 流回归共存（既有 terminal socket 测试全绿）
- [ ] `npm run test:run` 全绿

## Comments

- 2026-08-26 实现：终端 WS 握手提案/auth_ok 回执消费/双向帧编解码/Close 4003 映射 onError；重连自动重握手。事件通道在移动端为 Rust 实现（connection/event_ws.rs），TS 覆盖不到 → 已拆 issue 09（needs-triage）。
