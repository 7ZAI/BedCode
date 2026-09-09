# Spec: 桌面端宿主日志补齐——缺失日志填补 + 存量结构化字段迁移

> Status: done（4 ticket 全部落地，2026-09-10）
> 范围：bedcode-desktop（主机宿主代码）；不涉及移动端、插件 WASM 日志（已由 `.scratch/plugin-wasm-logging/` 覆盖）
> 关联：`.scratch/desktop-host-logging/issues/`（ticket 由实施拆分）

---

## Problem Statement

`desktop-logging-overhaul`（6b7681f6）落地了日志架构（non_blocking、热调、基于 span 的调用链、容量治理），但业务代码层的应用不完整，存在两类缺口：

1. **信息缺口（三处 P0**，desktop-logging-overhaul 审查时明确另立 spec 排期，尚未实施**）**：
   - **HTTP 成功路径零日志**：`http_filter.rs:106` 创建了 `http_request` span（`request_id/method/path` 字段），但 span 内无事件——请求只有被 TrafficFilterChain 拒绝时才有 `warn`（`http_filter.rs:148`）。AI agent 无法回答"哪个设备在何时调了哪个 API、结果如何、耗时多少"。
   - **json 模式缺 span 链**：`logging.rs:428/443` 的 json 事件格式未开 `.with_span_list(true)`——error.json 行只有 level/time/target/line_number，没有 request_id/session_id 上下文，与 text 模式的行内 span 链（05 已实现）不对称。
   - **启动早期日志黑洞**：build_logging 之前（`lib.rs:150-168` config 复制/加载失败、dev reset）仍走 `eprintln!`——debug 构建控制台可见，release 构建完全不落盘，config 加载失败等启动证据丢失。
2. **存量日志未按规范迁移（字段化）**：AGENTS.md Logging 节要求"关联键必须用结构化字段（`session_id = %x` 形式），禁止拼进消息字符串——AI 按 key grep 全链路的前提"，但存量业务代码仍有大量 `tracing::info!("...{}...", session_id)` 式的拼串调用（全仓约 183 处带 `{}` 参数的 tracing 调用，其中关键 ID 类占约六成），AI 无法按 key 检索。

## Solution

在不动日志架构（build_logging / subscriber 结构 / 文件命名 / 控制台格式）的前提下，做四件事：

- **刀 1 · 存量拼串迁移（P1，工作量最大）**：全仓把关键关联键（`plugin_id` / `session_id` / `device_id` / `request_id` / `node_id` / `run_id` / `batch_id` / `peer` / `pid` 等）从消息字符串迁移为结构化字段（`key = %value`），消息只保留人类可读描述。**分类原则**：机器可检索的 ID 一律字段化；人类读数（路径、文件名、错误文本、JSON 片段）保留在消息里。迁移不动日志语义（级别、时机、频率），只改字段承载方式。
- **刀 2 · HTTP 请求全路径日志（P0）**：`run_traffic_filter` 在既有 `http_request` span 内补事件——请求完成时按结果记 `info!`（成功）/ `error!`（失败或 5xx），携带 `request_id`（span 字段自动继承）+ `status` + `duration_ms`；频率语义：每请求一条完成日志（HTTP 请求量受控，见级别语义表：高频 API 走 debug？——**决定：成功走 debug、失败/拒绝走 warn/error**，避免 info 刷屏）。不做 body 内容记录（payload 可能含敏感数据）。
- **刀 3 · json span 链（P0）**：json 分支的 runtime/error 事件格式开启 `.with_span_list(true)`（`tracing_subscriber::fmt::format::json()` 现有 API，零成本），error.json 行即带完整 span 路径；text 分支行为不变。
- **刀 4 · 启动早期 bootstrap 日志（P0）**：新增进程级 bootstrap 日志通道：init_logging 之前（config 加载、resources 复制、dev reset）的日志写入 `bootstrap.log`（同日志目录，non_blocking），build_logging 完成后由 runtime 文件接管；保证 release 构建启动早期证据不丢。不引入新依赖（复用 tracing-appender）。

不改动：subscriber 4 层结构与过滤语义、`error.*.log` / `runtime.*.log` / `frontend.*.log` 命名与轮转、控制台格式化器、`[plugin:xxx]` 前缀、级别热调机制、容量裁剪任务、前端设置页。

## User Stories

1. 作为桌面端 AI agent，我希望 HTTP 成功请求也有结构化日志（request_id/status/duration），以便复现"哪个设备何时调了哪个 API"的时间线，而不是只知道被拒的请求。
2. 作为桌面端 AI agent，我希望 error.json 与 error.*.log 一样携带 span 链（request_id/session_id），以便 JSON 模式下也能按 key 检索错误上下文。
3. 作为桌面端维护者，我希望 release 构建下 config 加载失败等启动早期错误有日志落盘（bootstrap.log），以便远程/现场排查启动失败不再靠猜测。
4. 作为桌面端 AI agent，我希望全仓插件相关日志的 `plugin_id`、会话相关日志的 `session_id` 都可用 grep 精确检索（key=value），以便跨模块链路查询不受消息文案影响。
5. 作为桌面端开发者，我希望迁移后日志级别与频率语义不变（级别表格、热路径克制），以便不因字段化改造引入新的日志噪声。
6. 作为桌面端开发者，我希望 panic hook 与启动极早期（logging 系统本身未就绪前）的日志行为不受 bootstrap 改造影响。
7. 作为桌面端维护者，我希望 bootstrap 日志不改变现有 runtime 文件命名与轮转语义，以免打乱既有 grep 习惯。

## Implementation Decisions

- **存量迁移分类标准（刀 1）**：
  - 字段化（`key = %v` / `key = ?v` 或字段宏）：机器检索用的 ID——`plugin_id`、`session_id`、`device_id`、`request_id`、`node_id`、`run_id`、`batch_id`、`peer`（IP/地址）、`pid`、`client_id`、`task_id`、`config_id`。
  - 保留在消息：人读内容——错误文本、路径、命令名、文件名、URL、JSON 片段、数量。
  - 边界：`format!("{}::{}", plugin_id, name)` 这类拼接进复合标识的，拆为字段（`plugin_id` + `command`）。
  - **禁止把 `{:?}` 复杂结构直接塞字段**（字段应为标量/Display）；确需调试完整结构的保留在消息或改 `%` Display。
  - 迁移顺序：插件域（host.rs / loader.rs / approval.rs / wasm_runtime）→ 会话域（session_*.rs / commands/session.rs）→ 网络域（server/ peer_* / events/*）→ 系统域（system/* / lib.rs）。
  - 每个文件迁移后跑该模块测试；全量 cargo test 收尾。
- **HTTP 日志频率（刀 2）**：成功 `debug!`（不刷 info），4xx `warn!`，5xx/异常 `error!`；span 已带 request_id/method/path，事件只补 `status`、`duration_ms`、`error`（异常时）。
- **json span 链（刀 3）**：`json()` event_format 的 builder 上加 `.with_span_list(true)`（runtime 层 + error 层两处）；字段形式为 `"span": [{...}]`，兼容既有 jq 分析（不破坏现有字段）。
- **bootstrap 日志（刀 4）**：目录 = 应用日志目录（与 runtime.*.log 同目录，`bootstrap.log`）；先于 build_logging 用 `rolling::never` + `non_blocking` 建最小 writer，`spawn_log_maintenance` 的容量裁剪可覆盖它（.log 后缀天然纳入）；dev reset 语义初始化后由现有 `reset_today_logs`（改为也处理 bootstrap.log）或 bootstrap writer 重建。
- **不新增配置项**：bootstrap 无条件启用（量小）；HTTP 完成日志不设开关（debug/warn 级已被现有级别热调覆盖）。

## Testing Decisions

- 好测试标准：断言外部行为——"error.json 的 span 字段存在且含 request_id"、"HTTP 请求完成产生含 status/duration_ms 的 debug/warn 事件"、"plugin_id 以字段形式记录而非消息内"。
- **刀 1 主 seam**：对迁移示例文件（host.rs / session.rs）用既有测试断言「不再含 `tracing::...("...[{}..."` 拼接关键 key」——用编译期/静态断言或正则审计脚本（脚本放 `.scratch/desktop-host-logging/audit.rs` 或 CI 前 grep 清单）；行为回归由既有测试全量保障（级别/时机不变）。
- **刀 2 主 seam**：`http_filter` 若已有 middleware 测试则扩展断言完成日志字段；无则新增轻量测试（构造 ServiceRequest 走 run_traffic_filter，捕获 tracing 事件断言）。
- **刀 3 主 seam**：`logging.rs` 既有 `json_config` 测试（`system::logging::tests`）扩展——用 CaptureSubscriber 或直接断言 event_format 配置含 span_list；更实：构建 subscriber 发一个带 span 的 error 事件，检查 json 输出含 `"span"` 键。
- **刀 4 主 seam**：bootstrap writer 初始化 + `init_logging` 接管流程的单测（临时目录 + with_default）；断言 bootstrap.log 文件存在/内容非空；release 路径以 `cargo test --release` 冒烟（可选）。
- **回归保护**：既有 4 层 subscriber 测试（logging.rs tests）、http 相关测试、插件调用测试全量保持通过；确认刀 1 迁移不改变任何日志级别/频率。

## Out of Scope

- 文件层时间戳 UTC → 本地化（另一项审查发现，与"缺日志"无关，另议）
- 移动端日志
- `[plugin:xxx]` 前缀与插件 WASM 日志（plugin-wasm-logging 已闭环）
- 前端 console relay 删除（等前端日志框架落地）
- 日志检索 UI / web 界面

## Further Notes

- **衔接 desktop-logging-overhaul**：架构（non_blocking/热调/span 文件层链路）已闭环；本 spec 是它的"业务代码应用收尾"——把 span 链路插到 HTTP 全路径、把 json 模式对称、补启动证据、把 AGENTS.md 字段化规范落到存量。
- **audit 基线**：`rg -n 'tracing::(debug|info|warn|error|trace)!' src/ -g "*.rs" | grep -E '"[^"]*\{\}'` 当前 183 处（含人读参）；关键 ID 类集中在 `src/plugin/host.rs`（~56 处字段化已做 + 存量拼串）、`src/commands/session.rs`、`src/session/*`、`src/server/*`。
- **AI 检索受益**：迁移后 `rg "session_id ="` / `rg "plugin_id ="` 即可全链路；拼串时代的 `rg "Plugin '{}'"` 类反模式消失。
- **风险与对策**：183 处迁移面大——按模块分批、每批跑测试；拼串但难以抽字段的（如复合标识、动态格式）留待 review 决定，不强行。