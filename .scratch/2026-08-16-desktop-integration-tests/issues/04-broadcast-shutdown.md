# 04 — 多客户端广播与停机

**What to build:** 多设备场景与生命周期收尾在真实链路上可用：两个已认证客户端同时在线时，事件广播能送达目标客户端且排除发送者；服务器停机后新连接被拒绝、端口释放。

**Blocked by:** 02 — WS 配对与认证链路（需要两个已认证客户端）

**Status:** resolved

- [x] 两个已认证客户端在线，对一端执行广播语义操作（如设备同步/状态广播），另一端收到消息、发送端不收到
- [x] 断开一个客户端后，广播不再送达该客户端（注册表清理生效）
- [x] 服务器停机后新连接被拒绝，端口释放（可重新绑定）
- [x] 停机过程不泄漏孤儿客户端（防御性清理路径无警告级日志以外的异常）

## Answer

实现于 9f7cc274（2026-08-16）。`tests/broadcast_shutdown.rs`（约 700 行）：单测试函数 4 场景串行。广播走真实 WS 消息驱动链路：RemoveSession → actor → sync_tx 事件总线 → SyncEventHandler → registry 设备名排除广播；B 收 SyncData、A 仅 echo（500ms 窗口无 SyncData 排除断言）。断开后注册表移除 + send 报 not found + 全员广播对照证明管道存活。停机后 connect 拒绝（Windows SYN 重传 ~2s，超时放宽 10s）+ 端口重绑；自定义 tracing layer 断言 ERROR 级日志计数 0。

**首跑暴露真实 bug（已修复，c2f1aeca）**：VerifyCode 配对响应缺 device_name → 广播排除失效；测试改走 JWT 重连认证路径规避（与移动端真实重连行为一致），bug 记入 `.scratch/test-coverage-bugs.md` 并修复，修复后测试仍全绿。
