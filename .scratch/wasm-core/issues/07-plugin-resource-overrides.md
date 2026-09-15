# 07: per-plugin 资源覆盖：manifest 请求 + 安全模块仲裁

**What to build:** 单插件可经 manifest 声明 `resourceOverrides` 请求 Store 资源上限覆盖（燃料预算、线性内存、表元素、实例/内存/表数量），由**安全模块仲裁**：请求值钳制在「内核配置值」与「编译期硬上限」之内（逐字段 min）——插件只能自我收紧，放宽请求被钳回上限并 warn 记录。缺省/无该字段的旧插件零迁移，行为与现状完全一致（继承内核配置）。实例化时按插件解析出的限额建立 Store。

补齐 spec `Implementation Decisions → 配置模块` 与 US6 中票据 02 声明「延后到票据 04（安全模块）」、但票据 04 未承接的部分：`StoreLimits::clamped_within` 此前无生产调用方（审核发现 C-1）。

**Blocked by:** 02（core-config）、04（core-security 统一授权框架）

**Status:** resolved

- [x] SDK `PluginManifest` 新增 `resourceOverrides`（`Option<ResourceOverrides>`，serde default + skip_serializing_if，旧 manifest 零迁移）
- [x] `core-config`：`StoreLimits::apply_overrides`（None 字段继承内核配置）
- [x] `core-security`：仲裁入口 `SecurityFramework::resolve_store_limits`（钳制 + 越界 warn，结构化字段 plugin_id）
- [x] 实例化接线：`instantiate_component` / `load_plugin_from_file` 接收 overrides，Store 限额取仲裁结果
- [x] 测试：SDK manifest 解析（含部分字段 + 缺省）；`apply_overrides` 继承/覆盖；仲裁收紧保留、放宽钳回、低于配置值钳到配置值；端到端（插件请求内存上限后 Store 按该上限运行）
- [x] `cargo test` 全绿（桌面 + SDK）

## Comments

- 2026-09-15 立项：来源为同日代码审核（`.scratch/wasm-core/audit-2026-09-15.md` 发现 C-1）——spec 要求的能力未接线，`clamped_within` 为死代码。
- 2026-09-15 完成：SDK `ResourceOverrides`（6 个可选字段，`Copy`，camelCase）+ manifest 字段 + TS 双侧类型同步（`types.ts`）；宿主 `StoreLimits::apply_overrides` 只做请求合并，仲裁入口 `SecurityFramework::resolve_store_limits` 做双重天花板钳制（`min(请求, 内核配置值, 编译期硬上限)`），越界 warn（plugin_id 结构化字段）。接线：`instantiate_component` / `load_plugin_from_file` 增加 `resource_overrides` 参数，生产路径（`PluginHost` 加载与实例重建）传 `manifest.resource_overrides.as_ref()`。
  设计取舍：仲裁点放安全模块（spec「由安全模块仲裁」），配置模块只负责合并与钳制原语，职责边界清晰；`fuel_debug_multiplier`（全局调试开关）与 `max_wasm_stack_bytes`（Engine 构建期参数）不在可请求面，恒继承。只 warn 不新增监控字段——保持 monitor 快照形状稳定（spec 有形状锁定测试）。
  测试 6 个（SDK 2 / config 1 / framework 3）+ 实例化端到端 1 个：桌面 `cargo test --lib` 660（+5）全绿，SDK `cargo test` 79（+2）全绿。
