# 07: 设备派生与认证记录归插件

**What to build:** 让插件从宿主提供的原始连接事实自行派生设备在线视图，并自行维护连接认证记录；宿主只提供通用连接清单和脱敏上下文，不再提供设备业务 DTO、在线判定或记录 touch/close 副作用。

**Blocked by:** 02 — 连接上下文与脱敏认证身份

**Status:** done（2026-09-25，commit 见下）

- [x] 插件可从通用连接清单和连接上下文建立自己的设备派生视图。
- [x] client-connect/client-disconnect 触发插件自己的认证记录 touch/close。
- [x] 同一设备多连接、断线重连、插件停用和服务器停机时派生状态正确。
- [x] 宿主连接上下文不泄露凭据，插件不能跨属主读取连接。
- [x] 宿主删除设备连接事件、产品 `DeviceConnectionInfo` 和 session_count 等派生字段。
- [x] 设备列表、在线状态和连接历史的正反例测试通过。

## Comments
- 2026-09-25：宿主删除设备事件/DTO 面（`conn.rs` 不再 emit device-connected/disconnected、不再 touch/close；`connection_types.rs` 删除；`get_connected_devices` 命令与 `session_count` 删除；`system/constants.rs` 宿主 DEVICE_* 常量删除；注册表 `is_device_online` / `event_connection_count` / `terminal_connection_count` 随离线判定一并删除）。
- 2026-09-25：插件侧 `devices_events.rs` 订阅 `<owner>::ws:client-connect|client-disconnect`，经 `connection-context` 取脱敏身份 → 私有库 auth_records touch/close + emit `device:connected|disconnected`；**接入期记忆身份**（断开事件与注册表摘除存在 bus 异步竞态，断开期免查 connection-context，规避 "client not found"）。插件停用 `clear_connected` 清空记忆。
- 2026-09-25：同批连接生命周期加固（宿主侧）：有界帧队列 + 背压 1013、升级前客户端上限预留、注册 fail-visible、声明端点 `ws:server` 权限门。
- 2026-09-25：正反例测试——devices_events 单测 7 项（身份解析/载荷凭据零泄漏/事件名常量锁/记忆消费一次性/结构锁）；e2e（`test_ws_device_events_and_auth_records_closed_loop`）：真 Actix server + 真 WS 客户端 + 真 JWT，配对指纹 touch（connectCount 0→1 + lastSeen）/ 断开 close（disconnectedAt 回填）/ 未配对指纹零行更新反例。
- 2026-09-25：验证——桌面 `cargo test --lib` 针对性过滤全绿（registry 59 / server::websocket 93 / host_api::ws 25 / ws_e2e 6 / session_e2e 12 / 新增 e2e 1）；终端会话插件 `cargo test --lib` 326 全绿；SDK 常量测试 + manifest-validate vitest 30 全绿；前端 drift fixture 14 全绿；`cargo check --all-targets` 0 error。移动端不纳入（专项口径）。
