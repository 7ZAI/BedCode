# 插件 manifest 与配置的单一真源约定

**Status**: accepted

## Context

桌面端插件 manifest 历史上有两套并行真源：`plugin.json`（前端/宿主读取）与 Rust 插件 `lib.rs` 里 `serde_json::json!{...}` 宏（Rust 端 manifest() 返回）。两者各自手写，字段容易漂移——这正是随后引入 manifest 自动回填（manifest-gen）要解决的前置问题：自动回填写 `plugin.json`，但若 Rust 仍读 `json!` 宏，自动回填对 Rust 侧 manifest 无效。

插件配置（`contributes.configuration` 描述 schema 的用户可配置项）读写则选择"复用通用 storage 通道、统一约定一个 key"还是"引入专用 config runtime API"作为两备选。专用 API 可加类型校验与变更 hook，但要新增一条宿主↔插件通道并与现有 storage 双轨维护。

## Decision

1. **manifest 单一真源 = `plugin.json`**：Rust 插件的 `manifest()` 不再用 `json!{...}` 宏手写副本，改为 `serde_json::from_str(include_str!("../../plugin.json"))`，与前端/宿主共享同一份 `plugin.json`。manifest-gen 自动回填只写 `plugin.json`，Rust 端 `include_str!` 触发重编译即自动同步。

2. **配置单一真源 = 插件 storage 的固定键 `config`**：不引入专用 config runtime API。插件运行时读写配置统一走 `context.storage.get('config')` / `context.storage.set('config', value)`，与宿主配置页（`pluginStorageGet(pluginId, 'config')`）共享同一 key，保证插件读到的值即用户在配置页填写的值。SDK 导出 `PLUGIN_CONFIG_STORAGE_KEY = 'config'` 常量消除这个约定的硬编码漂移。

## Consequences

- manifest 任何字段变更只改一处 `plugin.json`；manifest-gen + `include_str!` 让前端与 Rust 自动同步，无双写漂移风险。
- 新增/裁剪 manifest 字段（如未来两端对齐扩展点）只需改 `plugin.json` 与 api crate 类型，无需再同步各插件 `lib.rs` 的 `json!` 宏。
- 插件配置无独立类型校验/变更 hook 能力：schema 在 manifest 声明，运行时只做松散的 storage 值读写，校验依赖插件自行处理。这是为"复用现有 storage、零新通道、契约最简"付出的代价。
- `config` 这个 key 成为插件与宿主之间的隐式契约常量，SDK 显式导出它由插件引用避免拼写漂移；改这个 key 会同时影响所有插件与宿主配置页，属刻意的高摩擦。
- 非标 `contributes` 字段（如 auto-task 的 `provides`）经 api crate 类型显式声明后随 `plugin.json` 单一真源承载；未在类型声明的未知字段由 serde 默认忽略（`PluginContributes` 未启用 `deny_unknown_fields`）。