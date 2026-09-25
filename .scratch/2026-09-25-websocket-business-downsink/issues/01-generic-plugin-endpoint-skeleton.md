# 01: 通用插件端点骨架

**What to build:** 让插件能够注册自己的 WebSocket 服务端端点，完成握手认证、属主隔离、限流和原始 text/binary 帧收发；宿主只做传输与安全裁决，不解析任何业务 payload。

**Blocked by:** None (can start immediately)

**Status:** ready-for-agent

- [x] 插件端点支持声明式注册、激活期登记和停用回收。
- [x] `auth: none` 与 `auth: jwt` 两种端点策略均能完成握手，超时/失败按通用关闭码退出。
- [x] text/binary 帧原样收发，同一连接保持帧顺序，发送队列有界且队列满时显式失败。
- [x] 跨插件句柄、端点和客户端寻址全部拒绝，插件停用只回收属主资源。
- [x] 连接注册、client-connect、client-disconnect 的时序和恰好一次语义有行为测试。
- [x] 通用端点路径不出现会话、终端、设备、任务或同步业务类型。

## Comments
- 2026-09-25：完成声明式端点登记/回收、none/jwt 认证、通用 text/binary 帧转发、有界队列、属主隔离与连接时序测试。
- 2026-09-25：`cd bedcode-desktop/src-tauri && cargo test --lib` 通过（1054 passed）；根目录 `pnpm exec eslint .` 通过（0 errors，120 warnings）。
- 2026-09-25：`cargo test --test broadcast_shutdown` 仍失败，原因是票据05已将 session lifecycle 事件改为插件私有 bus/前端 emit，而该旧集成测试仍期待宿主 `SyncData` 广播；本票据未修改该业务范围。
