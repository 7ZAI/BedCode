# Spec: 桌面端日志系统能力拉满 + 调用链追踪

> Status: ready-for-agent
> 范围：bedcode-desktop（主机）；不涉及移动端
> 关联：`.scratch/desktop-logging-overhaul/issues/`（ticket 由实施拆分）

---

## Problem Statement

桌面端日志系统（`system/logging` + 启动初始化 + `config` 的 LogConfig + 前端设置页）存在三类问题：

1. **性能**：runtime / error / frontend 三个文件层全部用阻塞式写盘（同步文件 I/O），PTY 输出、WS 广播等高频路径在 Tokio/actix 工作线程上直接做 syscall，造成延迟抖动与吞吐损失。
2. **可用性**：日志级别与格式是启动时静态配置（properties 文件），排查问题必须改配置重启；设置页没有任何日志入口，用户无法开 debug、无法快速定位日志文件。
3. **信息缺口**：全部日志是平面 event，无 span 调用链——一条命令从 HTTP 进来经会话控制到 PTY 再到 WS 广播，跨模块事件无法串联；error.*.log 无法回答"这条错误发生在哪个会话/请求链路上"。

另外：无磁盘容量保护（debug 级别单日可写数百 MB），panic.log 只追加不轮转。

## Solution

在不引入重型分布式追踪的前提下，把 tracing 生态现有能力拉满，并补齐调用链插桩：

- **刀 1 · 能力拉满**：三个文件层全部切换 `non_blocking` 异步写盘（有界缓冲 + worker 线程）；日志级别改运行时热调（`reload`），设置页新增「日志设置」区（级别下拉即时生效、格式/轮转/容量配置、打开日志目录按钮）；新增后台容量裁剪任务（总字节上限，panic.log 一并纳入）。
- **刀 2 · 调用链**：核心链路（HTTP 请求 → 会话生命周期 → PTY → WS 连接）插 `span`，文件层打印 span 链；引入 `tracing-error` 让 error 事件自动携带 span 路径；配置新增 JSON 结构化格式开关。

不改动：按天命名（`runtime.*.log` / `error.*.log` / `frontend.*.log`）、dev 启动重置当天日志、控制台自定义格式化器、`[plugin:xxx]` 前缀、前端 console relay 通道——现有 grep 排查习惯完全保留。

## User Stories

1. 作为桌面端用户，我希望日志写盘不阻塞终端输出与 WebSocket 广播，以便在长时间会话和高频输出下不出现卡顿与延迟抖动。
2. 作为桌面端用户，我希望在设置页直接把日志级别从 info 切到 debug，以便遇到问题时不改配置文件、不重启应用即可开启详细日志。
3. 作为桌面端用户，我希望设置页提供「打开日志目录」入口，以便一键定位 runtime/error/frontend 日志文件交给排查者。
4. 作为桌面端用户，我希望设置页能配置轮转策略、保留数量与磁盘容量上限，以便控制日志占用，长期运行不撑爆磁盘。
5. 作为桌面端用户，我希望日志目录总大小超过上限时自动清理最旧文件（当前文件除外），以便磁盘占用有硬性保护。
6. 作为桌面端用户，我希望 panic.log（崩溃记录）也纳入容量治理，以便它不会无限增长。
7. 作为桌面端用户，我希望日志文件支持 JSON 结构化输出开关，以便用 jq / 脚本按 level、target、session_id 字段过滤分析。
8. 作为桌面端 AI agent / 开发者，我希望 runtime 日志每行带出当前调用链（如 `request_id → session_id → …`），以便跨模块定位"这条事件属于哪次请求/哪个会话"。
9. 作为桌面端 AI agent / 开发者，我希望 error.*.log 的错误行直接携带 span 路径，以便只看错误文件就能判断断点所在的请求链。
10. 作为桌面端开发者，我希望每次请求/连接有可关联的唯一标识（request_id / session_id），以便移动端触发的命令在日志里可全链路串起来。
11. 作为桌面端开发者，我希望日志级别热调在 release 模式同样可用，以便现场诊断生产问题无需重启。
12. 作为桌面端开发者，我希望 non_blocking 队列满导致丢日志时能被察觉（周期告警 + 丢弃计数可读），以便不静默丢失关键日志。
13. 作为桌面端开发者，我希望现有的按天命名与「dev 启动重置当天日志」行为不变，以便既有的 grep 排查习惯零迁移成本。
14. 作为桌面端开发者，我希望前端 console 中继（target=`frontend`）与插件 `[plugin:xxx]` 前缀日志在改造后行为不变，以便 AI agent 前端排查通道照常工作。
15. 作为桌面端开发者，我希望 JSON 格式不影响按天文件名与日切轮转，以便两种格式都能用同样的保留策略管理。
16. 作为桌面端开发者，我希望 dev 构建仍然默认落盘 debug（现状），并允许用户在设置页临时调高/调低，以便开发期排查与日常低噪声可切换。
17. 作为插件开发者，我希望插件 WASM 日志（经宿主 log 桥）不因 span 改造而增加行为变化，以便插件侧的日志消费方式保持不变。
18. 作为桌面端运维/用户，我希望日志配置的保存走既有的 AppConfig 持久化链路，以便设置页重进后保持，并与配置文件手动编辑共存。

## Implementation Decisions

### D1 · 日志构建 seam（刀 1 的基础）

- 把日志系统构建抽为可测试的纯函数（输入：日志目录 + LogConfig；输出：订阅器 + 句柄集），不依赖 Tauri AppHandle。启动初始化只做目录准备与调用该函数的薄封装。
- 句柄集包含：non_blocking 的 WorkerGuard（进程内全局持有，随应用生命周期存活）、runtime/console 层级别的 reload handle、丢弃计数引用。
- 现有三个文件层（error/runtime/frontend）的过滤语义不变：error 固定 ERROR、runtime 可配置（dev 初始强制 debug）、frontend 仅 dev 且 target=`frontend`。

### D2 · 非阻塞写盘（tracing-appender 现成能力，无新依赖）

- 三个文件层全部套 `non_blocking`：各自独立的有界缓冲 + 独立 worker 线程；队列满时丢弃（默认策略）+ 丢弃计数。
- WorkerGuard 保存在进程级全局，保证退出前 flush；任何情况下不得提前 drop。
- 后台容量裁剪任务周期性读取丢弃计数，自上次检查起有新增丢弃时输出一条 warn（target 归入 runtime 日志），使丢日志可感知。

### D3 · 级别热调（tracing-subscriber `reload`，无新依赖）

- runtime 层与 console 层改用 `reload::Layer` 包装的 EnvFilter，reload handle 注册到进程级全局。
- 新命令 `set_log_level(level)`：替换 runtime 层过滤器为指定级别（debug/info/warn/error），即时生效、不落盘重启；dev 的初始 debug 语义保留但允许被热调覆盖。
- 持久化的 `file_level` 仍作为下次启动的初始值走既有 AppConfig 链路，热调只影响本次运行。

### D4 · JSON 结构化开关（配置项，不热调）

- `LogConfig` 新增 `format` 字段（`text` / `json`，默认 `text`），作用于 runtime 与 error 文件层。
- JSON 仅启动时生效（fmt layer 的 format 不可 reload），设置页 UI 注明"重启后生效"。
- 控制台输出与 frontend 文件层不受格式开关影响（保持 text，前端 relay 的人类可读性优先）。

### D5 · 容量裁剪（不引 rolling-file）

- `LogConfig` 新增 `capacity_bytes`（默认 512MB，0 = 不限制）。
- 后台任务：应用启动时 + 每 10 分钟扫描日志目录，总大小超上限时按修改时间删除最旧文件（跳过当前在写的文件），直到低于上限；`panic.log` 一并纳入。
- 明确不引入按单文件大小轮转的库：tracing-appender 的按天轮转 + max_files 已管理文件数量与命名（`runtime.*.log` glob 是既有 grep 习惯），本决策只补"总字节"这一维度，避免改变文件命名与 agent 排查方式。

### D6 · 设置页「日志设置」区

- 设置页新增日志 section（沿用现有 section 布局与「重启后生效」文案先例，如配对设置端口）：
  - 日志级别下拉（debug/info/warn/error）→ 热调命令，即时生效，不重启；
  - 格式开关（text/json）→ 保存配置，提示重启生效；
  - 保留数量、容量上限输入 → 保存配置，重启生效；
  - 「打开日志目录」按钮 → 打开系统文件管理器定位日志目录。
- 新增命令：`open_log_dir`；级别热调命令契约见 D3。保存逻辑复用既有 AppConfig 保存链路（落应用数据目录 properties），保证重启后配置保持并可被手动编辑共存。
- i18n：zh-CN 与 en 同步新增 key（沿用 `settings.log.*` 命名空间）。

### D7 · 调用链 span 插桩（刀 2，`attributes` feature 已启用，零新依赖）

- 插桩点（严格控制数量 ≤10，覆盖端到端主链路）：
  1. HTTP 请求入口中间件：每个请求 span（字段 `request_id` + method + path，request_id 为入口生成并贯穿）；
  2. 会话管理生命周期方法（会话创建/启动/停止，字段 `session_id`）；
  3. 会话控制服务方法（移动端触发的命令路径，继承会话 span）；
  4. PTY 进程启动/终止（字段 `session_id` + pid）；
  5. WS 连接生命周期（连接建立/认证/关闭，字段 `client_id`/`device_name`）。
- **span 禁区（禁止插桩，防热点开销）**：WS 每帧收发、PTY 输出读取/转发、广播分发（registry broadcast）等 per-frame/per-message 路径。
- 文件层（runtime/error）启用 `with_span_list(true)`：Full 格式下每行事件行内带父级 span 链。**不开** `FmtSpan` 生命周期独立日志（每 span 进出各一条会显著膨胀文件）。

### D8 · 错误上下文（新增唯一依赖：`tracing-error`）

- error 与 runtime 层挂 `ErrorLayer`：error 事件自动携带 span 路径（哪个请求链/会话链上出的错），随行落盘。
- console 层不挂（控制台保持现状，避免重复渲染）。
- 控制台自定义格式化器不支持 span 渲染为已知缺口，本次不扩展（文件层已覆盖 span 可读性），文档注明。

### D9 · 兼容性保底

- 启动日志（`Logging initialized.`、配置摘要、版本行）保留并补充新增配置项的值。
- dev 构建重置当天日志（runtime/error/frontend 三前缀）逻辑不变。
- 插件 WASM 日志桥（`[plugin:xxx]` 前缀 + Metadata 缓存）、前端 relay（批量 + 16KB 截断）不改动。

## Testing Decisions

- **测试原则**：只测外部行为（日志是否按过滤级别落盘、热调后级别是否即时变化、裁剪是否删对文件、JSON 是否可解析、命令/UI 契约），不测实现细节（不测 EnvFilter 语法、不测 span 宏展开、不测 non_blocking 内部队列）。
- **Rust 模块测试**（跑 `cargo test`）：
  - 日志构建 seam：临时目录 + 采样级别写入 → 断言 runtime/error/frontend 文件内容与级别过滤、JSON 格式可被解析、reload handle 切换级别后新写日志级别随之变化；先例：`system/logging` 现有 SharedWriter + `with_default` 捕获测试、`config` 的 LogConfig 解析测试。
  - 容量裁剪：构造日志目录（多个前缀 + panic.log + 伪旧文件）→ 断言超限后删最旧、当前文件保留、低于上限即停；先例：模块内纯函数测试（如 `fs_auth` 前缀比较测试风格）。
  - 丢弃计数告警路径：mock 计数增量 → 断言 warn 输出（可并入 seam 测试）。
  - span 插桩：对 2-3 个代表性 instrument 方法用捕获订阅者断言 span 名称与字段（request_id/session_id）正确；先例：`plugin/wasm_runtime/host_impl/log` 的 CaptureSubscriber 测试模式。
- **命令层**（Rust）：`set_log_level` / `open_log_dir` 走 Tauri 命令单元测试或手动冒烟（现有命令测试先例有限，以模块 seam 测试为主，命令做轻量契约断言语义校验）。
- **前端测试**（跑 `pnpm run test:run`）：设置 stores/composable 的 invoke mock 断言（调用 `set_log_level`/`open_log_dir`/`save_app_settings` 的参数与结果处理）；设置页渲染测试断言日志 section 存在且文案走 i18n；先例：`src/__tests__/stores` 现有 store 测试。
- **i18n 同步**：zh-CN 与 en 的 `settings.log.*` key 一一对应（既有双语言同步检查习惯）。
- **回归**：Rust 全量 `cargo test` + 前端 `pnpm run test:run` 绿；启动日志首行 `Logging initialized.` 行为不变。

## Out of Scope

- OpenTelemetry / Jaeger 上报（分布式追踪为多进程设计，单进程桌面应用用 request_id 日志字段足够，见 Further Notes）。
- 控制台自定义格式化器渲染 span（已知缺口，文件层已覆盖）。
- 前端内置日志查看器/在线 tail（本次只做打开目录 + 级别/格式/容量控制）。
- 插件 WASM 日志与移动端日志的 span 化。
- 日志脱敏引擎（认证凭据已只记长度/布尔；插件与前端 relay 属 dev 环境数据，不纳入本次）。
- 按单文件大小轮转（改用总容量裁剪，理由见 D5）。

## Further Notes

- **架构边界说明**：移动端 → 桌面端 HTTP → 会话 → PTY → WS 是单进程内多跳，用 request_id/session_id 字段即可全链路串联，不需要 OTLP/采样器/后端存储；若未来桌面端多进程化（如插件独立进程）再评估 `tracing-opentelemetry`。
- **命令字眼合规**：本文档涉及验证一律用 `cargo test` / `pnpm run test:run`（禁止 `pnpm run test` watch 挂起、禁止旧 npm 字眼）。
- **热调语义**：`set_log_level` 只影响本次运行；重启后的级别以持久化 `file_level` 为准，两者不互相覆盖。
- **实现拆分建议**：ticket 01 = seam 抽取 + non_blocking + 丢弃告警；ticket 02 = reload 热调 + JSON 开关 + 配置新增；ticket 03 = 容量裁剪任务；ticket 04 = 设置页 + i18n + 命令；ticket 05 = span 插桩 + span 列表 + tracing-error。