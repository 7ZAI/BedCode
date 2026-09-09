# 02: 日志级别运行时热调 + JSON 结构化格式

**What to build:** 日志级别从启动时静态配置改为运行时热调——runtime 与 console 层的过滤器挂 reload 句柄，新增 `set_log_level` 命令，用户不重启即可切换各级别；同时 `LogConfig` 新增 `format`（text/json）字段，JSON 输出应用到 runtime 与 error 文件层，供脚本/agent 按字段（level、time、target、session_id）过滤分析。

**Blocked by:** 01（reload 句柄放入 01 的句柄集）

**Status:** resolved

- [x] `set_log_level` 命令调用后，runtime 文件后续写入的日志级别立即变化（info↔debug 往返验证），不重启、不落盘持久化
- [x] dev 构建初始仍为 debug；热调只影响本次运行，重启后回落到持久化 `file_level`
- [x] `format=json` 时 runtime 与 error 文件每行为合法 JSON（可解析），包含 level/time/target/字段；`format=text` 行为与现状一致
- [x] `format` 仅启动时生效（设置页 UI 注明"重启后生效"；UI 在 04 落地）；不影响按天命名与 max_files 保留策略
- [x] 单元测试：seam 上断言 reload 切换后新写日志级别变化生效；JSON 输出可被解析、error 层 JSON 仅含 ERROR 事件
- [x] `cargo test` 全量绿
## Answer

已完成（2026-09-09）。

**实现要点**：
- `system/config.rs`：`LogConfig` 新增 `format`（text/json，默认 text；仅启动生效）；properties 读写/分组/注释同步，resources/config.properties 加 `log.format=text`
- `system/logging.rs`：runtime/error 文件层按 `format` 分支（`event_format(tracing_subscriber::fmt::format::json())`）；E 泛型不同无法运行时替换，双分支各自 Box 化统一返回；frontend/console 层始终 text。Cargo.toml 为 tracing-subscriber 开启 `json` feature
- `commands/system.rs`：新增 `set_log_level`（五级白名单校验 → `global_setup().file_level_reload.reload(EnvFilter::new(level))`，仅影响本次运行，不落盘）；已注册进 invoke_handler
- **踩坑**：fmt::Layer 无 `.json()` 方法（那是 builder API），JSON 化要用 `format::json()` 函数 + `.event_format()`；JSON 字段嵌套在 `fields` 对象（`{"fields":{"message":..,"session_id":42},"level":"INFO",...}`），断言按嵌套结构写

**验证**：`cargo test --lib` 577 全绿（新增 JSON 可解析+字段断言、reload 热调三段式断言：info 初始 debug 过滤 → reload debug 后落盘 → reload error 后 warn 过滤）；logging 模块 8 测试全过。
