# 01: 插件 trap 错误携带 WASM 内部调用栈

**What to build:** 插件 panic、栈溢出、燃料耗尽、内存越界等 WASM trap 发生时，宿主侧拿到的错误信息包含 WASM 内部函数调用栈（`wasm backtrace:` + 函数名），随 `AppError` 进入 error 日志与插件 Degraded 状态——AI agent / 开发者无需重跑即可从错误串直接定位崩溃发生在插件内部哪个函数，而不是只知道"哪个导出失败"。

**Blocked by:** None（可立即开始）

**Status:** ready-for-agent

- [x] WASM 运行时引擎配置开启 wasmtime backtrace（帧数上限 32），任何既有配置（燃料、内存、栈深度、编译缓存）不受影响
- [x] 触发 trap 的插件调用（如显式 panic、燃料耗尽）返回的错误串包含 `wasm backtrace:` 前缀与插件内函数名（names section，release 构建即可读）
- [x] 开启后既有插件调用行为不变——现有 WASM 调用测试套件全量保持通过
- [x] 新增测试：以真实 trap 的测试插件导出断言错误消息含调用栈（沿用现有燃料耗尽 trap 测试的构造方式），并断言正常调用（非 trap）错误串不含栈、行为不变
