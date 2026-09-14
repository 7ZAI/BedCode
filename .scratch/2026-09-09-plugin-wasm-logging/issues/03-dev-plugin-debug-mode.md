# 03: dev 插件调试模式

**What to build:** dev 构建下设置 `BEDCODE_PLUGIN_DEBUG=1` 后，插件 wasm 以 debug profile 构建（保留 DWARF 调试信息），插件崩溃时日志中的调用栈带源码行号——AI agent / 开发者可直接定位到插件源码的准确行；燃料预算自动适配 debug 产物（指令数暴涨不误杀正常调用）。release 构建忽略该变量，日常开发不受 debug wasm 体积与性能影响。

**Blocked by:** 01（行号栈是 backtrace 能力上的叠加，且复用 01 的测试基建）

**Status:** ready-for-agent

- [x] 调试模式开关：dev 构建下读 `BEDCODE_PLUGIN_DEBUG` 环境变量（非空即开）；release 构建忽略
- [x] 插件构建脚本支持 debug profile 构建（保留 DWARF），由开关透传；release wasm 构建路径零改动
- [x] 运行时按环境模式解析 DWARF 行号（不硬编码强制），无调试信息时零开销回退到函数名栈
- [x] 调试模式下燃料预算联动放大，以真实 debug profile 插件冒烟校准——正常调用不被误判失控
- [x] 开关开启方式、效果、燃料行为与现有文档（AGENTS.md Logging 节插件 WASM 段）一致
- [x] 冒烟验证：开启调试模式构建的插件触发 trap，错误日志含 `file:line` 行号
