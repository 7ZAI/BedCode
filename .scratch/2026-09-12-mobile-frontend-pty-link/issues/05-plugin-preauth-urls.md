# 05 — 插件 manifest 新增 `preauthUrls` 字段（SDK types + 宿主收集进 Egress L2）

**What to build:** 插件 `plugin.json` 新增 `preauthUrls: string[]` 字段（glob host+path，仿 `preauthDirs` 先例）：① `bedcode-mobile/packages/plugin-sdk-mobile/rust/src/types.rs` 的 `PluginManifest` 解析（**注意：是 bedcode-mobile/packages/plugin-sdk-mobile，不是仓库根 packages/**）；② SDK manifest 类型/模板同步；③ 宿主插件加载时收集声明 → 注册进 Egress L2 白名单（ticket 02 的声明合并接口）。

**Spec:** §5.6 机制要点 2、§9 D6

**Blocked by:**

**Status:** done

## 关键实现事实（handoff §2 已核实）

- `PluginManifest` 定义在 `bedcode-mobile/packages/plugin-sdk-mobile/rust/src/types.rs`（`bedcode-plugin-api-mobile` crate，宿主 `src-tauri/Cargo.toml:101` path 依赖）；`preauth_dirs: Vec<String>`（camelCase `preauthDirs`）先例在 80-87 行，`preauth_urls` 照此加（JSON 字段 `preauthUrls`）。
- 宿主侧 manifest 加载器在 `src-tauri/src/plugin/`（收集 `preauth_dirs` 的位置是照抄点）；宿主收集后传给 egress 的 L2 声明合并接口（ticket 02）。
- 声明粒度：host + 可选 path 前缀（glob）；示例声明进 ticket 12（ai-chatbox）。
- 改 SDK 后需验证宿主 `cargo test` 与 SDK 构建链路不破坏（manifest-gen 权限用例先例存在）。

## 实现清单

- [x] `types.rs` `PluginManifest` 加 `preauth_urls: Vec<String>`（serde 默认空数组，兼容老 manifest）
- [x] SDK manifest 类型（TS side）/ 模板 / dev-shell mock 同步 `preauthUrls` 字段
- [x] 宿主插件加载器解析 `preauthUrls` 并收集进 Egress L2 声明（调 ticket 02 接口）
- [x] manifest 权限用例补充（preauthUrls 解析/缺失默认/glob 格式校验）
- [x] SDK 重建（`pnpm run build`）验证不破坏宿主/插件构建

## 验证

- `cargo test`（src-tauri + SDK crate）全绿；`pnpm run build`（SDK）通过
- 插件加载后 egress L2 声明集合包含声明的 URL 模式

## Comments

- 2026-09-11 完成：`packages/plugin-sdk-mobile/rust/src/types.rs` `PluginManifest` 加 `preauth_urls`（camelCase `preauthUrls`，serde default）；宿主 `plugin/manager.rs` `scan_and_load` 加载时 `egress::policy().register_plugin_urls`（manifest 静态属性，加载即注册）、`uninstall` 时 `unregister_plugin_urls`；SDK TS `src/types.ts` 加 `preauthUrls?: string[]`（dev-shell 无 preauthDirs 先例，无需 mock 同步）；traits.rs 测试插件与 manager.rs seed_plugin 构造补字段。单测：`test_manifest_preauth_urls_parse`（解析 + camelCase 回写）+ 默认空数组断言。验证：SDK crate 63 测试全绿、SDK TS build 成功、宿主 258 全绿。
