# 02: 宿主 async 化门禁（A0-3 + A1 + A4）

**What to build:** 桌面插件运行时从 wasip2 sync store 切换到 async store + wasi-p3 async linker（wasmtime-wasi 48 p3 模块）；全部既有 wasip2 插件零回归；wasip3 fixture 组件闭环（async `get-random-bytes` 返回熵 / async 时钟可读 / 燃料限额 async 语义生效，含续费复核 A4）。**这是 spec §10.6 的硬门禁**：p3 模块实验性风险在此把关，失败则回退「wasip2 产物 + async store」过渡，wasi3 仍为最终目标。

**Blocked by:** 01

**Status:** ready-for-agent

- [ ] 既有 wasip2 插件（含测试 fixture）在 async store 下加载/激活/调用零回归，桌面 cargo test 全绿
- [ ] wasip3 fixture 组件实例化闭环：async `get-random-bytes` 返回熵、async 时钟可读
- [ ] 燃料限额在 async 语义下生效（含续费复核）；async 化后 `spawn_with_error_boundary` / 错误上下文规范不退化（AGENTS.md §6）
- [ ] 门禁结论记录：通过则继续；失败则记录回退路径（wasip2 产物 + async store）与受阻票据
- [ ] 移动端零改动（临时分叉维持，ADR 0019 偏离记录留票 14）
