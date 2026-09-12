# 06: core-plugin-manager：系统组件类型与能力装配框架

**What to build:** 插件分两种类型并可装配闭环：manifest 声明 `type: system | application`（缺省 application，旧插件零迁移）与 `dependencies`（应用插件声明依赖的系统组件能力）。能力注册表以能力名为键、提供者为值（宿主 Rust 原语或 WASM 系统组件实例，二选一装配）；系统组件内置、默认启用、只停不删、先于应用插件加载激活；应用插件的 import 由 Linker 按注册表路由到宿主原语或系统组件实例（host-side 转发，组件间不共享内存）；依赖缺失则激活失败报明确错误。系统组件与应用插件走同一条 Store/安全/监控管线，trap 隔离不扩散。能力不携带业务语义（ADR 0022 裁剪线）。

**Blocked by:** 01（五模块骨架归位）

**Status:** ready-for-agent

- [ ] manifest 解析：`type`/`dependencies` 缺省兼容旧插件（现有插件 manifest 无需修改即可加载）
- [ ] 加载顺序：系统组件先于应用插件激活（集成测试断言顺序）
- [ ] 最小 WASM fixture 闭环：fake 系统组件注册能力 + fake 应用插件消费，调用经 Linker 路由到达组件实例并返回正确结果
- [ ] 依赖缺失：应用插件激活失败且错误信息指明缺失的能力名
- [ ] 系统组件 trap 不影响其他插件（隔离测试）
- [ ] `cargo test` 全绿
