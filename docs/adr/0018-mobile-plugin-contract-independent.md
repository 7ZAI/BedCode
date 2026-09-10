# 移动端插件契约独立于桌面端维护

桌面端与移动端各持一份独立的 `bedcode.wit`（WIT 单一事实来源），契约层（宿主能力 import 集 + 插件导出回调集）随两端产品能力分化各自演进，同名词义保持对齐（差异表见 `docs/implementation-plans/mobile-wasmtime-component-migration.md` §3.2），传输机制层（组件模型 + wit-bindgen）两端一致。

移动端有意**不含** session 能力：会话状态机在桌面端，移动端曾以 noop 占位（被删除的代码化石）；契约中缺失使插件编译期即失效，优于运行时拿到空值——这一不对称是产品决策，不是遗漏。共享超集方案被否决：会把移动端宿主永远 `unreachable!` 的接口泄漏给插件 SDK。对齐语义的历史记录：`docs/implementation-plans/mobile-wasmtime-component-migration.md` §8（Q2/Q3）。