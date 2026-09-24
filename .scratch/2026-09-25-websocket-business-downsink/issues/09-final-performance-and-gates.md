# 09: 终态性能与全量门禁

**What to build:** 对硬切后的通用宿主 WS + 插件终端链路做最终验收，证明业务已下沉、性能和资源回收可接受、文档与门禁完整。

**Blocked by:** 08 — 宿主业务硬切与旧协议删除

**Status:** ready-for-agent

- [ ] 插件 ring-fetch + 二进制 WS 输出覆盖常态与压力输出，记录 CPU、内存、队列和截断指标。
- [ ] 宿主通用 transport、插件端点、真实 PTY、旧路由 404 和旧 ABI 拒绝均有集成证据。
- [ ] 插件停用、端点注销、服务器停机、连接断开不会留下任务、句柄或监听端口。
- [ ] desktop Rust、terminal-session、desktop SDK 与受影响前端测试全量通过。
- [ ] 根目录 eslint 0 error，Rust 格式与 clippy 自查完成，测试后无后台残留进程。
- [ ] code-map、ADR、插件检查清单、CHANGELOG 和本专项文档与终态一致。
- [ ] 明确记录移动端与旧版本不在兼容范围，不宣称双端或旧端可用。
- [ ] `lens_diagnostics mode=all` 无 blocker。
