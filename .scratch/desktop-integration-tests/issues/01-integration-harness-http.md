# 01 — 集成测试基建 + HTTP 契约

**What to build:** 桌面端集成测试的基建与第一组可验证行为：在测试进程内真实启动局域网服务器（分配独立端口，不与真实运行实例冲突），并验证 HTTP 层契约——健康检查端点可访问，受保护 API 在未携带/携带非法 JWT 时被拒（401），携带合法 JWT 时放行。基建部分包括跨测试的串行化（全局单例只允许一个测试同时操作服务器）、优雅停机与端口释放。

**Blocked by:** None — can start immediately

**Status:** resolved

- [x] `cargo test --test server_integration` 存在真实测试目标并全绿（非恒真断言，每场景至少一个真实往返断言）
- [x] 测试内服务器使用 OS 分配/随机端口，不硬编码默认端口，与并行跑的 lib 测试（538 个）互不干扰
- [x] 健康检查场景：HTTP 200 + 预期响应体
- [x] 鉴权契约场景：无 token / 非法 token → 401；合法 token → 放行（真实中间件在真实路由上的行为）
- [x] 每个测试结束服务器句柄优雅停机，端口可复用；重复运行（连续两次 `cargo test`）稳定
- [x] 全局单例（服务器管理器、会话注册表、应用上下文）不会因并行测试相互污染——验证方式：同一测试二进制内多个场景串行执行

## Answer

实现于 f575970ae（2026-08-16）。`tests/server_integration.rs` 单 `#[tokio::test]` + 场景子步骤（串行防单例污染），端口 `TcpListener::bind(0)` 探测分配；三场景：健康检查 200+port 动态比对、JWT 四档 A/B（无/乱串/错钥→401，合法→404 观测点）、优雅停机+同端口重启复用。`cargo test --test server_integration` 连跑两次绿；`cargo test --lib` 538 无回归。

**遗留（影响 02+）**：AppContext.app_handle 硬编码 `Arc<AppHandle>`（Wry），与 `mock_app()` 的 MockRuntime 不兼容；受保护 handler 首行 `AppContext::global()` 未初始化即 panic——后续票跑真实 handler 前需小改生产代码（泛型化或弱化 app_handle）。
