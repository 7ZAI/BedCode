# 04: `reveal-in-dir` 原语化（`host-platform` 追加，退役宿主插件 API 面）

**What to build:** 把「在系统文件管理器中定位并选中文件/目录」从**宿主插件 API 面**
（前端 `context.system.revealInDir` → Tauri 命令 `plugin_reveal_in_dir`）改为**内核原语**
`host-platform.reveal-in-dir(path)`，插件经 SDK 直接调用；随后退役旧命令与
`system:open` 权限。

**决策依据:** ADR 0022 裁剪线——`host-platform` 是通用平台能力域（`pick-files` 与
`pick-folder` 已按同一裁决从 host-peer 迁入）；「用系统文件管理器定位」与选源对话框同属
平台交互、无业务语义，按同一口径归 `host-platform`。当前实现挂在**宿主命令 + 插件 API 桥**
上，是「能力已存在但没有原语」的遗留形态。

**Blocked by:** 无（ABI 追加需与 `abi.rs` / WIT / SDK 五同步点同批次落）

**Status:** ready-for-agent

## 现状（锚点已核实）

- 宿主实现：`src-tauri/src/commands/opener.rs:40` `plugin_reveal_in_dir`（平台分发
  `reveal_in_dir_platform`：Windows `SHOpenFolderAndSelectItems` COM（含 `\\?\` verbatim
  前缀剥离与 `ERROR_FILE_NOT_FOUND` 兜底）/ macOS `open -R` / Linux `xdg-open`），
  入口做「插件已激活 + `system:open` 权限」双重校验
- 前端插件 API：`src/plugin/context.ts:294` `revealInDir`、`src/plugin/commands.ts:193`
  `invoke('plugin_reveal_in_dir', ...)`、`src/plugin/permission.ts:69`
  `'system:open': ['system.revealInDir']`、`src/plugin/types.ts:440`
- 声明方：`plugins/file-transfer/plugin.json:22` `system:open`
- 命令注册：`src-tauri/src/lib.rs:705`

## 改动

- [ ] WIT（ABI **v22**，desktop-only）：`host-platform` 追加
      `reveal-in-dir: func(path: string) -> result<_, string>`，权限跟 `platform` 域
      （或新增 `platform:reveal`——按风险域拆分口径二选一，选定后五同步点全落）
- [ ] SDK：`HostPlatform::platform_reveal_in_dir`（或 trait 追加）+ `wasm_host.rs` 绑定
      + `component.rs` 转发 + `host_impl/platform.rs` 实现本体（**从 `commands/opener.rs`
      搬入** `reveal_in_dir_platform` 及平台分支，含 verbatim 前缀与兜底注释一并搬）
- [ ] 插件侧：file-transfer 改为经 SDK 调原语（`context` 面不再需要宿主 API）
- [ ] 退役：删 `commands/opener.rs::plugin_reveal_in_dir` + `require_system_open` +
      lib.rs 注册 + 前端 `commands.ts` / `context.ts` / `types.ts` / `permission.ts` 的
      `revealInDir` 映射；**`system:open` 权限是否一并退役按「是否还有其它消费者」裁决**
      （五同步点：SDK 常量与 API 映射 / 打包 CLI / 前端合法集合 / 宿主能力清单 / host_impl 权限门）
- [ ] `open_log_dir`（同文件）**保留**：它是宿主设置页的日志目录入口，不经插件权限链

## 验收

- [ ] 三平台行为不回归（Windows 中文路径 / `\\?\` 前缀 / PIDL 失败兜底 / macOS `open -R` /
      Linux `xdg-open`）：至少钉住参数与平台分支选择的单测（真实 GUI 行为留真机）
- [ ] 插件未激活调用 → 显性报错；路径不存在 → 显性报错（与现状 `NotFound` 文案一致）
- [ ] 桌面 `cargo test --lib` + SDK/插件 crate 全绿；插件产物重建 + manifest 一致；
      前端 `pnpm run test:run` + 根 eslint 0 error
- [ ] AGENTS §7 能力清单计数、`abi.rs` 版本沿革、CHANGELOG、code-map 同步

## Comments

### ① 为什么值得单独一票

它不是「删代码」而是「换形态」：需要一次 ABI 追加 + 权限归属裁决 + 三平台实现搬迁 +
插件消费方改造 + 前端桥退役。混在 v21（删函数）窗口里会同时承担两向兼容风险，故独立成票。

### ② 待裁决项（开工前先定）

1. `reveal-in-dir` 归 `platform` 现有权限域，还是新开 `platform:reveal`（后者更贴风险域拆分，
   但要付五同步点的税）；
2. `system:open` 权位是否随本票退役（若 file-transfer 是唯一消费者则退役，反之保留）。
