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

**Status:** done（2026-09-21；两处待裁决由用户裁定：**不叠加权限门** → `system:open` 随之退役）

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

- [x] WIT（ABI **v22**，desktop-only）：`host-platform` 追加
      `reveal-in-dir: func(path: string) -> result<_, string>`，权限跟 `platform` 域
      （或新增 `platform:reveal`——按风险域拆分口径二选一，选定后五同步点全落）
      → **裁决结果：不加权限门**（与 `pick-*` 同口径；见下方 Comments ③）
- [x] SDK：`HostPlatform::platform_reveal_in_dir`（trait 追加）+ `wasm_host.rs` 绑定
      + `component.rs` 转发 + `host_impl/platform.rs` 实现体
- [x] 插件侧：file-transfer 改为经 SDK 调原语（`context` 面不再需要宿主 API）
- [x] 退役：删 `commands/opener.rs::plugin_reveal_in_dir` + `require_system_open` +
      lib.rs 注册 + 前端 `commands.ts` / `context.ts` / `types.ts` / `permission.ts` 的
      `revealInDir` 映射；**`system:open` 权限一并退役**（五同步点全落）
- [x] `open_log_dir`（同文件）**保留**：它是宿主设置页的日志目录入口，不经插件权限链

## 验收

- [x] 三平台行为不回归（Windows 中文路径 / `\\?\` 前缀 / PIDL 失败兜底 / macOS `open -R` /
      Linux `xdg-open`）：单元测试钉住纯函数与参数选择（`strip_verbatim_prefix` /
      `unix_reveal_command` 两平台参数 / 缺路径 NotFound），真实 GUI 行为留真机
- [x] 插件未激活调用 → 显性报错（原语由宿主插件调用链仲裁）；路径不存在 → 显性报错
      （`reveal: path not found: …`，与原命令文案逐字一致）
- [x] 桌面 `cargo test --lib` + SDK/插件 crate 全绿；插件产物重建 + manifest 一致；
      前端 `pnpm run test:run` + 根 eslint 0 error
- [x] AGENTS §7 能力清单计数、`abi.rs` 版本沿革、CHANGELOG、code-map 同步

## Comments ③ 实施记录（2026-09-21，用户裁决 + 落点偏离说明）

**裁决**：不叠加权限门（选项 a）。理由：`reveal-in-dir` 与 `pick-files` / `pick-folder` /
`wsl-distros` / `local-ipv4-addresses` 同属「平台交互动作」——不读取任何数据（路径本就由
调用方提供），`host-platform` 域保持「无权限门」的一致性优于新开 `platform:reveal` 的
形式化收税。连带结论：`system:open` 权限退役（其唯一消费者 file-transfer 改走原语）。

**五同步点全落**（`system:open` 退役）：

| 同步点 | 落点 |
| --- | --- |
| SDK 常量与 API 映射 | `packages/plugin-sdk-desktop/rust/src/permission.rs`：删 `PERMISSION_SYSTEM_OPEN` 常量 + 清单项 + `(PERMISSION_SYSTEM_OPEN, &["system.revealInDir"])` 映射 |
| 前端合法集合 | `src/plugin/permission.ts` 删 `'system:open': ['system.revealInDir']`（合法集合即由该表推导） |
| 宿主命令面 / 权限门 | `commands/opener.rs` 删命令 + `require_system_open`；`lib.rs` 删注册 |
| 宿主能力清单 | `host_impl/platform.rs::platform_reveal_in_dir`（`host-platform` 域无权限门，登记在同一域） |
| 插件消费方（打包侧随 manifest 校验） | `plugins/file-transfer/plugin.json` 删 `system:open` + 加 `file-transfer.reveal-in-dir` 命令；README 权限表同步 |

**落点偏离（记录在案）**：票面要求实现本体搬进 `host_impl/platform.rs`，实测**不可行**——
`host_impl/mod.rs` 是 `pub(super) mod platform;`，命令层（`commands/opener.rs::open_log_dir`
仍要用同一份平台分发）不可见。故实现本体落在**引擎模块** `system/opener.rs`
（`reveal_in_dir` 平台分发 + `reveal_existing_in_dir` 校验入口 + `strip_verbatim_prefix` /
`unix_reveal_command` 纯函数），`host_impl/platform.rs::platform_reveal_in_dir` 退化为
「校验 + `Result<(), String>` 适配」；这也更贴「原语实现不挂命令层」的分层。

**插件侧改造**：file-transfer 新增自有命令 `file-transfer.reveal-in-dir`（前端
`context.commands.execute('file-transfer.reveal-in-dir', { path })`），WASM 侧调
`h.platform_reveal_in_dir(&path)`（与既有 `file-transfer.pick-files` 同形）；
`context.system.revealInDir` 与其 mock（dev-shell）、SDK TS 类型 `SystemAPI`、
i18n `noSystemOpenPermission`（zh-CN / en）一并删除。

**ABI**：`abi.rs` v21 → v22（沿革条目 + 用例改名 `test_abi_version_is_v22`）；WIT 函数级追加，
纯增量、v21 及以下产物不受影响。**票 02 阶段 B 的 ABI 号让位为 v23**（已在票 02 内更新）。

## Comments

### ① 为什么值得单独一票

它不是「删代码」而是「换形态」：需要一次 ABI 追加 + 权限归属裁决 + 三平台实现搬迁 +
插件消费方改造 + 前端桥退役。混在 v21（删函数）窗口里会同时承担两向兼容风险，故独立成票。

### ② 待裁决项（开工前先定）

1. `reveal-in-dir` 归 `platform` 现有权限域，还是新开 `platform:reveal`（后者更贴风险域拆分，
   但要付五同步点的税）；
2. `system:open` 权位是否随本票退役（若 file-transfer 是唯一消费者则退役，反之保留）。
