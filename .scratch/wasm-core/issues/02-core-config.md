# 02: core-config：运行参数配置化

**What to build:** wasmtime Engine 构建参数（fuel 开关、编译缓存、并发策略）与 Store 资源上限（线性内存、表元素、栈深、实例数、内存数、单次调用燃料预算、debug 放大倍率）收进单一配置结构，默认值等于当前生产常量；支持配置文件加载（未知字段容忍）与运行时覆盖；资源限制器改为配置驱动；单插件可经 manifest 请求资源覆盖，由安全上限钳制。调参不再需要改代码重新编译。

**Blocked by:** 01（五模块骨架归位）

**Status:** resolved

- [x] 配置结构字段默认值与现运行时常量逐一相等（有对照测试）
- [x] 配置文件加载：缺失文件用默认、未知字段忽略、非法值报错带上下文
- [x] 运行时覆盖生效（覆盖后新建立的 Store 按新上限运行）
- [x] 资源限制器行为等价：超限增长仍被拒绝，限额来自配置
- [x] `cargo test` 全绿

## Comments

- 2026-09-13 完成：CoreConfig/EngineConfig/StoreLimits 落在 core-config（默认值=历史生产常量，对照测试锁定）；配置文件 `wasm-core.json`（缺失→默认、未知字段容忍、非法值报上下文）；运行时覆盖经 `WasmRuntime::set_config`（校验非法拒绝、只影响新建 Store）；ResourceLimiter 与燃料注入点全部配置驱动。单插件 manifest 覆盖的仲裁延后到票据 04（安全模块），钳制机制 `StoreLimits::clamped_within` 已备。注意：覆盖内存上限不得低于组件最小内存（17 页 ≈ 1.1MiB），否则实例化直接失败。cargo test 627 全绿。
