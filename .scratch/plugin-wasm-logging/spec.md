# Spec: 插件 WASM 日志优化——dev 调试模式 + trap 内部调用栈

> Status: done（2026-09-10 实施完成，4 个 ticket 全部落地并验证）
> 范围：bedcode-desktop 插件系统（WASM 运行时）；不涉及移动端与前端日志框架
> 关联：`.scratch/plugin-wasm-logging/issues/`（ticket 由实施拆分）

---

## Problem Statement

插件 WASM 日志在「guest 主动日志」与「host call 错误」两层已较规范（`[plugin:xxx]` 前缀 + 动态 metadata、host_impl 各域带 `plugin_id` 的 warn/error），但存在三类缺口：

1. **trap 无 WASM 内部调用栈**：生产 Engine Config（`wasm_runtime` 构建处）未开启 wasmtime 的 backtrace。插件 panic、栈溢出、燃料耗尽、内存越界时，错误串只有 `wasm trap: unreachable` 一类单行信息，看不到插件内部哪个函数、哪一层调用崩的。AI agent 排查插件崩溃只能看到"哪个导出失败"，无法定位插件内部故障点。
2. **插件 wasm 恒 `--release` 构建**：dev 桌面端下插件也是 release 产物——保留 names section（函数名）但无 DWARF 行号，深度调试（定位到源码行）不可达；且没有"调试模式"开关来按需切换构建 profile。
3. **插件日志级别全局绑定**：插件日志 target 固定 `bedcode_lib::plugin::plugin_log`，tracing filter 无法按插件区分（filter 不支持按字段过滤）。release 下想单查某个插件的 debug/trace 日志只能全局热调（`set_log_level`），会刷爆整个 runtime 文件。

另外：trap 错误目前只随 `AppError::Plugin` 返回值上抛，若调用方静默忽略（如某些 hook 路径），崩溃证据不落宿主日志。

## Solution

在不动插件 ABI（wit 契约）与既有日志格式的前提下，做四件事：

- **刀 1 · 开启 wasmtime backtrace（P0）**：`Config::wasm_backtrace_max_frames` 开启（wasmtime 47 的 `backtrace` feature 已在 default features 内，**零编译成本**，仅运行时配置项）。此后所有 trap（panic/unreachable、栈溢出、燃料耗尽、内存越界）错误串自动携带 `wasm backtrace:` 函数调用栈（names section 函数名，release 构建即有），随 `AppError::Plugin` 进 error.log 与 `mark_plugin_error`（Degraded 状态）。
- **刀 2 · dev 插件调试模式（P1）**：`BEDCODE_PLUGIN_DEBUG=1` 时（dev 构建下）插件 wasm 以 debug profile 构建（保留 DWARF），运行时开启 `wasm_backtrace_details`（Environment 模式，读 `WASMTIME_BACKTRACE_DETAILS`）获得带行号的栈；燃料预算联动放大（debug 产物指令数暴涨，现有 `FUEL_PER_CALL` 会误杀）。
- **刀 3 · trap 统一宿主日志入口（P1）**：在 WASM 导出调用包装的 trap 分支（双层 Result 的 Err）统一打宿主侧 `error!(plugin_id, trap = ...)`，保证即使调用方静默也不丢崩溃证据。
- **刀 4 · per-plugin 日志级别（P2）**：`emit_plugin_log` 入口维护 `plugin_id → 级别阈值` 映射，低于阈值直接丢弃（不经过 filter，宿主侧判定）；支持 `BEDCODE_PLUGIN_LOG=com.bedcode.auto-task=trace` 局部放开，避免全局热调刷爆 runtime。

不改动：`[plugin:xxx]` 前缀与插件日志落盘格式、wit 契约、`FUEL_PER_CALL` 之外的运行时看门狗语义、宿主 4 层 subscriber 结构、前端 console relay（等待另一会话的前端日志框架落地后再评估删除）。

## User Stories

1. 作为桌面端 AI agent，我希望插件 panic/trap 时错误日志包含 WASM 内部函数调用栈，以便直接定位是插件哪个函数崩的，而不是只看到"哪个导出失败"。
2. 作为桌面端 AI agent，我希望燃料耗尽、栈溢出、内存越界、显式 panic 四类 trap 在日志里可以区分并可读，以便快速判断崩溃性质。
3. 作为桌面端开发者，我希望开启调试模式后插件错误栈带源码行号（DWARF），以便定位到插件源码的准确行。
4. 作为桌面端开发者，我希望调试模式是显式开关（环境变量），不影响默认 dev/release 构建路径，以便日常开发不受 debug wasm 体积与性能影响。
5. 作为桌面端开发者，我希望调试模式下燃料预算自动放大，以便 debug 构建（指令数暴涨）不会把正常插件调用误判为失控。
6. 作为桌面端 AI agent，我希望插件 trap 有宿主侧统一 error 日志（含 plugin_id 与 trap 详情），以便即使调用方忽略错误也有崩溃证据落盘。
7. 作为桌面端开发者，我希望可以按插件单独放开日志级别（如只看 auto-task 的 trace），以便 release 下排查单个插件问题时不刷爆全局 runtime 日志。
8. 作为桌面端 AI agent，我希望 per-plugin 级别过滤后日志仍保持 `[plugin:xxx]` 前缀与统一格式，以便既有 grep 习惯不受影响。
9. 作为插件开发者，我希望插件自身的 `[plugin:xxx]` 日志级别语义不受调试模式影响（debug 模式只是补宿主侧信息，不改变 guest 日志行为）。
10. 作为桌面端开发者，我希望调试模式的开启方式（环境变量名、效果、燃料行为）在文档中有明确说明，以便团队一致使用。
11. 作为桌面端 AI agent，我希望 error.log 中插件错误行能直接回答"哪个插件、哪个导出、WASM 内部哪条调用链"，以便无需重跑即可完成大部分插件故障归因。
12. 作为桌面端维护者，我希望以上能力不引入新的持久化配置项（调试是会话态），以便配置面不膨胀。

## Implementation Decisions

- **backtrace 开启方式**：`Config::wasm_backtrace_max_frames(Some(NonZeroUsize::new(32)))`——v47 推荐 API，替代已 deprecated 的 `wasm_backtrace(bool)`；帧数上限取 32（默认值），避免深栈时错误串过长。
- **调试模式判定**：`cfg!(debug_assertions)` 前提下读环境变量 `BEDCODE_PLUGIN_DEBUG`（非空即开），不新增持久化配置项；release 构建忽略该变量（debug profile 构建产物不会出现在 release 场景）。
- **行号解析**：`Config::wasm_backtrace_details(WasmBacktraceDetails::Environment)`（默认读取 `WASMTIME_BACKTRACE_DETAILS` env），不硬编码 Enable——release 无 DWARF 时零开销。
- **燃料联动**：调试模式下 `FUEL_PER_CALL` 按倍率放大（常量加注释说明 debug 产物指令特性；倍率由实施时以真实 debug 插件冒烟校准），其余燃料语义不变。
- **构建脚本**：插件构建脚本支持 debug profile（`cargo build --profile dev`），由环境变量透传；dev 桌面端启动脚本（plugin-dev.js）按 `BEDCODE_PLUGIN_DEBUG` 决定传参。release wasm 构建路径不动。
- **per-plugin 级别**：`emit_plugin_log` 入口前按 `(plugin_id, level)` 查阈值映射（`OnceLock<Mutex<HashMap>>`，与既有 metadata 缓存同模式），低于阈值直接 return；映射来源解析 `BEDCODE_PLUGIN_LOG`（`id=level` 逗号分隔），未列出的插件沿用宿主全局过滤语义。
- **trap 统一日志**：在 WASM 导出调用包装（双层 Result 的 Err 分支）统一补宿主侧 `error!`，携带 `plugin_id` 与 trap 详情；与现有 `AppError::Plugin` 返回并存（日志为证据，返回为控制流）。
- **不改插件 ABI**：所有改动在宿主侧（wasm_runtime / host_impl / 构建脚本），插件 crate 零改动。

## Testing Decisions

- **好测试的标准**：只断言外部行为——"trap 错误消息包含 wasm backtrace 栈与函数名"、"低于阈值的插件日志被丢弃"；不测 Config 内部字段、不测 wasmtime 实现细节。
- **主 seam（P0）**：Engine/Config 构建 + 一个真实 trap 的测试插件导出（`panic!` / `unreachable!`），断言返回的错误串含 `wasm backtrace:` 与插件内函数名。prior art：`component.rs` 现有燃料耗尽 trap 测试（`test_engine` + `Store::set_fuel` + 断言 trap 错误消息），扩展同模式用例。
- **次 seam（P2）**：`emit_plugin_log` 阈值过滤，用 `host_impl/log.rs` 既有 `CaptureSubscriber` 测试模式（`with_default` + `Arc<Mutex<Vec<CapturedEvent>>>`）断言低于阈值的级别不出现、高于阈值的正常出现。
- **P1 调试模式**：自动化受限——需要真实 debug profile wasm 产物；列为构建脚本冒烟（产物含 DWARF section 检查：`wasm-objdump -h` / `llvm-dwarfdump`），燃料倍率以真实插件跑通为准。
- **回归保护**：既有插件调用路径（`component.rs` 全套调用测试、宿主 4 层 subscriber 测试）全量保持通过，确认开启 backtrace 不改变正常调用行为。

## Out of Scope

- 前端日志框架接入与 `report_frontend_log` / `devConsoleRelay.ts` relay 删除（另一会话专项，见 Further Notes 衔接项）
- 桌面端通用日志改进：JSON 格式 span 链（`.with_span_list`）、HTTP 成功路径请求日志、启动早期 bootstrap.log、文件时间戳本地化——另立 spec 排期
- 移动端日志（logcat 无 tracing-subscriber 生态，插件日志经 `[plugin:xxx]` 前缀转发 logcat 的既有行为不变）
- 插件 wasm 体积/性能优化（debug profile 仅为调试服务）

## Further Notes

- **零成本前提已核实**：wasmtime 47.0.3 的 `default` features 含 `backtrace`（Cargo.toml 确认，依赖含 gimli/addr2line/object），开启配置项不增加编译产物与依赖。
- **P0 立即可读**：Rust `--release` 构建的 wasm32-unknown-unknown 默认保留 names section（函数名），开启 backtrace 后无需 debug 构建即可拿到函数级栈；行号（DWARF）才是 P1 调试模式的价值所在。
- **与「桌面端日志审查」衔接**：审查还发现桌面端宿主日志三项 P0 级缺口（json 层 span 链缺失、HTTP 成功路径零日志、启动早期 eprintln 不落盘），与本次 spec 正交，建议随后另立 spec；AGENTS.md Logging 节已同步补充格式规范与异步日志规范（本次一并落地）。
- **调试模式手感**：`BEDCODE_PLUGIN_DEBUG=1` 需要开发者在启动 tauri dev 前设置；考虑在设置页「日志设置」区加一个"插件调试模式"开关的可行性（读取环境变量并提示重启生效），列为可选增强，不在本 spec 强制范围。
