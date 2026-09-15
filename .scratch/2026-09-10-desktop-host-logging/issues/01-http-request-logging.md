# 01: HTTP 请求全路径日志（成功/失败/耗时）

**What to build:** 既有 `http_request` span（`http_filter.rs:106`，已带 request_id/method/path）内补充完成事件——请求结束按结果记录：成功 `debug!`（status + duration_ms）、4xx `warn!`、5xx/异常 `error!`（含 error 文本）。从此"哪个设备在何时调了哪个 API、结果如何、耗时多少"在日志里可答，不再只有拒绝路径的 warn。

**Blocked by:** None

**Status:** done

- [x] `run_traffic_filter` 完成路径（含下游返回后）在 span 内发完成事件：`status`、`duration_ms` 结构化字段；成功 debug / 4xx warn / 5xx+异常 error（级别决策见 spec 刀 2）
- [x] 异常路径（`?` 上抛 / middleware 错误响应）不吞错误：error! 带 `error` 字段
- [x] 只记录元数据，不记录 body/payload（敏感数据保护）；不给每次请求打 info（成功走 debug，避免刷屏）
- [x] 测试：构造请求走 middleware，断言完成事件含 status/duration_ms 字段且级别符合决策；拒绝路径既有 warn 不变

---

# 02: json 模式 span 链（.with_span_list）

**What to build:** json 事件格式打开 span 列表——`logging.rs:428/443`（runtime 层 + error 层）的 `event_format(json())` 补 `.with_span_list(true)`，error.json 行即携带 request_id/session_id 等 span 上下文，与 text 模式行内 span 链（desktop-logging-overhaul 05 已实现）对称。零编译成本、不破坏既有字段。

**Blocked by:** None

**Status:** ready-for-agent

- [ ] json 分支两处（runtime + error）`.with_span_list(true)`
- [ ] 验证：构造带 span 链的 error 事件，断言 json 输出含 `"span"` 数组且含 request_id；既有 json 字段（level/time/target/line_number）不变
- [ ] text 分支与控制台格式零改动，既有 logging 测试全量通过

---

# 03: 启动早期 bootstrap 日志

**What to build:** build_logging 之前（config 加载、默认配置复制、dev reset 等启动早期路径，`lib.rs:150-168`）的日志不再只走 `eprintln!`——新增进程级 bootstrap 通道：`bootstrap.log`（应用日志目录，rolling::never + non_blocking），init_logging 后由 runtime 文件接管。release 构建启动失败证据不丢。统一错误上下文规范（"什么操作在哪失败"，禁止裸字符串）。

**Blocked by:** None

**Status:** ready-for-agent

- [ ] bootstrap writer 初始化：日志目录（与 runtime 同目录，目录创建幂等）、rolling::never + non_blocking、worker guard 进程级存活
- [ ] 启动早期路径（config 复制失败 / config 加载失败 / dev reset / bootstrap 自身初始化）改走 bootstrap 日志（eprintln! 保留双写或由 bootstrap writer 落盘 + 控制台）
- [ ] build_logging 完成后无缝接管：bootstrap.log 不重复记录，runtime.*.log 继续；容量裁剪（.log 后缀）天然覆盖 bootstrap.log
- [ ] panic hook / 日志系统自身未就绪路径行为不变（panic.log 独立保留）
- [ ] 测试：临时目录 + with_default 单测断言 bootstrap.log 创建且内容非空；init_logging 接管后不重复写入

---

# 04: 存量日志结构化字段迁移（plugin_id/session_id 等拼串 → key=value）

**What to build:** 全仓把关键关联键（plugin_id / session_id / device_id / request_id / node_id / run_id / batch_id / peer / pid / client_id / task_id / config_id 等）从消息字符串迁移为结构化字段（`key = %value`），消息只保留人类可读描述。AI 按 key grep 全链路（AGENTS.md Logging 节规范）从"仅新代码遵守"变为"全仓一致"。迁移不改变日志级别/时机/频率（"什么操作在哪失败"上下文描述保留并补齐）。

**Blocked by:** None（可独立推进；建议按模块分批：插件域 → 会话域 → 网络域 → 系统域，每批跑测试）

**Status:** ready-for-agent

- [ ] 审计基线：`rg -n 'tracing::(debug|info|warn|error|trace)!' src/ -g "*.rs" | grep -E '"[^"]*\{\}'` 183 处分类（关键 ID vs 人读描述），产出待迁移清单（写入本目录 audit 文档）
- [ ] 插件域迁移：host.rs / loader.rs / approval.rs / storage.rs / fs_auth.rs / wasm_runtime? / app_cli.rs（含 `format!("{}::{}", plugin_id, cmd.name)` 复合标识拆字段）
- [ ] 会话域迁移：commands/session.rs（含 `start_existing_session called with session_id: {}`）、session_*.rs
- [ ] 网络域迁移：server/*、peer_*、events/*（request_id / peer / device 字段化）
- [ ] 系统域迁移：system/*、lib.rs、watcher.rs / lifecycle.rs / power.rs
- [ ] Level 语义核对：迁移过程不提升/降级任何日志级别；热路径克制不放宽
- [ ] 每个迁移文件跑对应模块测试；全量 cargo test + clippy 收尾；无新增 warning