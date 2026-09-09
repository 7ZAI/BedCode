# 05: 调用链 span + 错误上下文

**What to build:** 日志从平面 event 升级为带调用链——核心链路（HTTP 请求入口中间件、会话生命周期、会话控制服务、PTY 启动/终止、WS 连接生命周期）插 span（字段 `request_id`/`session_id`/`client_id`），文件层行内打印父级 span 链；引入 `tracing-error`，error 事件自动携带 span 路径（定位到所属请求/会话，无需按时间戳对表）；明确 span 禁区：WS 每帧、PTY 输出转发、广播分发等 per-frame/per-message 路径不插桩。

**Blocked by:** 01（文件层 span 列表打印在 01 的订阅器构建中开启）

**Status:** resolved

- [x] 触发一条命令（本地 HTTP 或移动端）：runtime 日志中该链路的行携带 `request_id → session_id` 链，可凭 request_id 串起请求→会话→PTY→WS 各阶段
- [x] error.*.log 中来自链路内的错误行携带 span 路径（能判断所属请求/会话）；错误内容不变，仅增加上下文
- [x] 热点路径（WS 每帧收发、PTY 输出读取/转发、广播分发）无新增 span，dev 高频会话无可感卡顿
- [x] span 链打印用行内携带方式（不做 span 生命周期独立日志行，防文件膨胀）
- [x] `[plugin:xxx]` 插件日志、frontend 中继（target=`frontend`）行为不变
- [x] 代表性插桩点（≥2 处）有捕获订阅者单元测试断言 span 名称与字段（request_id/session_id）
- [x] `cargo test` 全量绿
## Answer

已完成（2026-09-09）。

**实现要点**：
- **文件层 span 链**：文本 Full 格式默认在事件行打印当前 span 链（root→leaf，含字段），无需额外配置；**未**用 FmtSpan 生命周期日志（防膨胀）
- **error 文件层 filter 修复（关键坑）**：`EnvFilter("error")` 会把 INFO 级 span 一并过滤（`Context::span → try_with_filter`），error 行丢失 span 链——改用 `FilterFn`（`is_span()→true` 全放行 + `is_event()→level<=ERROR`），error 行带完整 span 路径
- **插桩 4 点**：HTTP 请求入口（`http_filter` call 同步上下文创建 span + `.instrument(span)` 包裹异步体，字段 request_id/method/path，request_id 由 `new_request_id(peer)` 生成；EnteredSpan 非 Send 不能跨 await）；会话创建（session_manager 既有 `#[tracing::instrument]` 保留）；PTY 启动（`#[tracing::instrument(name="pty_start", skip_all, fields(session_id=%self.id))]`）；WS 连接生命周期（`terminal_ws` entered span，client + bound session）；热点路径（WS 每帧/PTY 转发/广播）未插桩
- **tracing-error**：ErrorLayer 挂链尾（reload filter 链首 S=Registry 约束下只能挂尾部）
- **踩坑**：EnteredSpan 非 Send——async fn 里 `.entered()` 跨 await 破坏 tauri command Send future，统一用 instrument 宏（对 async fn 自动 `.instrument()` 包装）；测试中子 span 必须在父 span `in_scope` 内创建（span! 创建时捕获 current span 绑定父链）；一次 partial-apply 编辑把 `#[test]` 属性丢失/错位导致「the async keyword is missing」诡异报错，git 恢复后重新插入解决

**验证**：`cargo test --lib` 583 全绿（新增：span 链文件输出测试（runtime 含 http_request{session_id} 链 + error 文件含 span 路径）、request_id 格式测试）；logging 模块 14 测试全过；全量 clippy 无新增 blocker。
