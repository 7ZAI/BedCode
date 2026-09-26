# 插件 WASM 日志优化——dev 调试模式 + trap 内部调用栈

> 整理自 2026-09-09 插件 WASM 日志专项 spec（2026-09-27 迁入 docs，
> Status: done，2026-09-10 实施完成，4 个 ticket 全部落地并验证）。
> 范围：bedcode-desktop 插件系统（WASM 运行时）；不涉及移动端与前端日志框架。
> 与 `docs/knowledge/logging.md`（落盘机制 / 排障手册）配套阅读。

## Problem Statement

插件 WASM 日志在「guest 主动日志」与「host call 错误」两层已较规范（`[plugin:xxx]` 前缀 +
动态 metadata、host_impl 各域带 `plugin_id` 的 warn/error），但存在三类缺口：

1. **trap 无 WASM 内部调用栈**：生产 Engine Config 未开启 wasmtime 的 backtrace。插件 panic、
   栈溢出、燃料耗尽、内存越界时，错误串只有 `wasm trap: unreachable` 一类单行信息，看不到插件
   内部哪个函数、哪一层调用崩的。AI agent 排查插件崩溃只能看到"哪个导出失败"，无法定位插件内部故障点。
2. **插件 wasm 恒 `--release` 构建**：dev 桌面端下插件也是 release 产物——保留 names section（函数名）
   但无 DWARF 行号，深度调试（定位到源码行）不可达；且没有"调试模式"开关来按需切换构建 profile。
3. **插件日志级别全局绑定**：插件日志 target 固定 `bedcode_lib::plugin::plugin_log`，tracing filter
   无法按插件区分（filter 不支持按字段过滤）。release 下想单查某个插件的 debug/trace 日志只能全局
   热调（`set_log_level`），会刷爆整个 runtime 文件。

另外：trap 错误目前只随 `AppError::Plugin` 返回值上抛，若调用方静默忽略（如某些 hook 路径），
崩溃证据不落宿主日志。

## Solution（四把刀，不动插件 ABI 与既有日志格式）

- **刀 1 · 开启 wasmtime backtrace（P0）**：`Config::wasm_backtrace_max_frames(Some(NonZeroUsize::new(32)))`
  开启（wasmtime 47 起 `backtrace` feature 在 default features 内，**零编译成本**，仅运行时配置项）。
  此后所有 trap（panic/unreachable、栈溢出、燃料耗尽、内存越界）错误串自动携带 `wasm backtrace:`
  函数调用栈（names section 函数名，release 构建即有），随 `AppError::Plugin` 进 error.log 与
  `mark_plugin_error`（Degraded 状态）。
  **AGENTS §8 已把它定为不得关闭的红线**（`wasm_backtrace_max_frames(Some(32))`，wasmtime 48.0.3
  default features 已含 backtrace）。
- **刀 2 · dev 插件调试模式（P1）**：`BEDCODE_PLUGIN_DEBUG=1` 时（dev 构建下）插件 wasm 以 debug
  profile 构建（保留 DWARF），运行时开启 `wasm_backtrace_details(WasmBacktraceDetails::Environment)`
  （读 `WASMTIME_BACKTRACE_DETAILS` env）获得带行号的栈；燃料预算联动放大（debug 产物指令数暴涨，
  现有 `FUEL_PER_CALL` 会误杀）。release 构建忽略该变量（debug profile 产物不会出现在 release 场景）。
  不新增持久化配置项（调试是会话态，配置面不膨胀）。
- **刀 3 · trap 统一宿主日志入口（P1）**：在 WASM 导出调用包装（双层 Result 的 Err 分支）统一补
  宿主侧 `error!(plugin_id, trap = ...)`，保证即使调用方静默也不丢崩溃证据（日志为证据，返回为控制流）。
- **刀 4 · per-plugin 日志级别（P2）**：`emit_plugin_log` 入口维护 `plugin_id → 级别阈值` 映射，
  低于阈值直接丢弃（不经过 filter，宿主侧判定）；支持 `BEDCODE_PLUGIN_LOG=com.bedcode.auto-task=trace`
  局部放开，避免全局热调刷爆 runtime。映射来源解析 `BEDCODE_PLUGIN_LOG`（`id=level` 逗号分隔），
  未列出的插件沿用宿主全局过滤语义；与既有 metadata 缓存同模式（`OnceLock<Mutex<HashMap>>`）。

**不改动**：`[plugin:xxx]` 前缀与插件日志落盘格式、wit 契约、`FUEL_PER_CALL` 之外的运行时看门狗
语义、宿主 4 层 subscriber 结构、前端 console relay。

## Implementation Decisions 补充

- **零成本前提已核实**：wasmtime 47.0.3 的 `default` features 含 `backtrace`（依赖含
  gimli/addr2line/object），开启配置项不增加编译产物与依赖。
- **Rust `--release` 构建的 wasm32-unknown-unknown 默认保留 names section（函数名）**：开启 backtrace
  后无需 debug 构建即可拿到函数级栈；行号（DWARF）才是 P1 调试模式的价值所在。
- **构建脚本**：插件构建脚本支持 debug profile（`cargo build --profile dev`），由环境变量透传；
  dev 桌面端启动脚本按 `BEDCODE_PLUGIN_DEBUG` 决定传参。release wasm 构建路径不动。
- **不改插件 ABI**：所有改动在宿主侧（wasm 运行时 / host_impl / 构建脚本），插件 crate 零改动。

## Testing Decisions

- 好测试的标准：只断言外部行为——"trap 错误消息包含 wasm backtrace 栈与函数名"、"低于阈值的
  插件日志被丢弃"；不测 Config 内部字段、不测 wasmtime 实现细节。
- 主 seam（P0）：Engine/Config 构建 + 一个真实 trap 的测试插件导出（`panic!` / `unreachable!`），
  断言返回的错误串含 `wasm backtrace:` 与插件内函数名。
- 次 seam（P2）：`emit_plugin_log` 阈值过滤，用 `CaptureSubscriber` 测试模式断言低于阈值的级别
  不出现、高于阈值的正常出现。
- P1 调试模式：自动化受限（需要真实 debug profile wasm 产物），列为构建脚本冒烟（产物含 DWARF
  section 检查），燃料倍率以真实插件跑通为准。

## Out of Scope（衔接项）

- 前端日志框架接入与 `report_frontend_log` / `devConsoleRelay.ts` relay 删除（另一会话专项）
- 桌面端通用日志改进：JSON 格式 span 链、HTTP 成功路径请求日志、启动早期 bootstrap.log——另立 spec
- 移动端日志（logcat 无 tracing-subscriber 生态，插件日志经 `[plugin:xxx]` 前缀转发 logcat 的既有行为不变）
- 插件 wasm 体积/性能优化（debug profile 仅为调试服务）
