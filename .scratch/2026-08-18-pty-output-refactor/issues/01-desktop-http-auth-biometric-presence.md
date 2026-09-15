# 01 — 桌面端 HTTP 认证补全：生物认证 + 广播去重 + 在线语义（事件 WS）

**What to build:** 在已有 `/api/auth/{pairing,verify,qr-connect,reauth}`（`auth_controller.rs` 已完整）基础上补齐：① 生物认证 HTTP 化（`POST /api/auth/biometric-challenge` / `biometric-verify`，challenge 存储键控从 `SocketAddr` 改为 `device_fingerprint`，单次有效，签名验证逻辑从 WS 抽取复用）；② `WsSessionEntry` 增加 `channel_type: Event | Terminal`，`broadcast` 默认仅发 Event 通道、按 fingerprint 去重（SyncData 只发事件 WS）；③ **设备在线语义 = 常驻事件 WS 存活**（用户定稿）：指纹匹配 + channel_type=Event 且已认证即在线；最后一条事件 WS 断开 → 触发 `DEVICE_DISCONNECTED` + 连接历史 `close_open_connection_event` 回填（终端通道断开不触发离线）；④ HTTP 路由 JWT 校验中间件（pairing/verify/qr-connect 豁免）。

**Spec:** §4.1、§4.2、§4.4、§4.6（验收 1/2/3/6/7）

**Blocked by:**

**Status:** done（已提交 7112f2b1）

- [x] `auth_controller.rs` 新增 biometric-challenge / biometric-verify 两端点
- [x] `auth_service.rs` 抽取 `issue_biometric_challenge` / `verify_biometric_signature`（含 WS 旧路径复用或删除）
- [x] `AppContext::biometric_challenges()` 键控改 fingerprint + 清理时机（事件 WS 断开/设备移除）
- [x] `WsSessionEntry.channel_type` + broadcast 过滤（仅 Event）+ 按 fingerprint 去重 + 注册点补类型
- [x] 设备在线查询（fingerprint 维）+ DEVICE_DISCONNECTED/连接历史回填触发逻辑（依赖 02 的 stopping() 按类型区分）
- [x] HTTP 路由 JWT 校验中间件
- [x] 测试：biometric challenge 单次有效、广播仅事件通道/去重、在线判定（事件 WS 断开→离线、终端断开→仍在线）、HTTP JWT 中间件

## Comments

- 2026-08-17：auth_controller 四端点已核实为完整实现（含 DB 记录与 DEVICE_CONNECTED）。
- 2026-08-17 修订：**取消 HTTP presence 心跳方案**——设备在线改为常驻事件 WS 存活判定（用户定稿），presence 端点不实现。