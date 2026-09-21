# 03: `wsl` / `local_ip` 宿主命令去重（前端改走插件命令面后注销）

**What to build:** 注销宿主三个与 `host-platform` 原语重复的 Tauri 命令，前端改走会话中心
插件的既有命令面：

| 宿主命令（删） | 宿主文件 | 插件命令面（已有，落点） |
| --- | --- | --- |
| `list_wsl_distributions` | `commands/wsl.rs:13` | `session.environment.wsl-distros`（`plugins/session/rust/src/lib.rs:793`） |
| `is_wsl_available` | `commands/wsl.rs:23` | 同上（插件按 distros 非空/可用性自判，或复用 `host-platform.wsl-distros` 错误通道） |
| `get_local_ip_addresses` | `commands/system.rs:358` | `session.network.info`（`plugins/session/rust/src/lib.rs:879`） |

理由：`host-platform.wsl-distros`（v19 函数级追加）与 `host-platform.local-ipv4-addresses`
已是内核原语，宿主命令是**同一能力的产品面重复实现**；且这两个命令的宿主消费方都是
「会话/设备」业务 UI（WSL 用于会话配置表单的执行环境分支、本地 IP 用于设备页展示）。

**决策依据:** ADR 0022 裁剪线（宿主能力只暴露引擎原语；产品面走插件）；
`.scratch/2026-09-20-host-business-decarriage/spec.md`（宿主业务清零）；
code-map 已记「`get_local_ip_addresses` 与 `host-platform.local-ipv4-addresses` 重复」。

**Blocked by:** 无（**但必须前端同改**：Rust 单侧删除会直接断掉 invokes）

**Status:** ready-for-agent

## 前端改动点（已定位）

- [ ] `src/composables/commands/sessionCommands.ts:18` `list_wsl_distributions()` → 插件命令面
      （`context.commands.execute('session.environment.wsl-distros')` 或经插件贡献的设置分组取值）
- [ ] `src/composables/commands/sessionCommands.ts:23` `is_wsl_available()` → 同上口径
- [ ] `src/composables/commands/settingsCommands.ts:47` `getLocalIpAddresses()` → `session.network.info`
- [ ] 消费方复核：`useAvailableEnvironments`（设置页会话分组的执行环境分支）、
      `stores/wsl.ts`（启动预加载）、`useNetwork`（当前孤儿）——确认改后行为一致、
      插件未激活时的降级文案走 i18n
- [ ] `src/composables/useTauri.ts` 的兼容 re-export 与孤儿 composable 一并清理
      （审计已记：`useNetwork` / `useWsl` / `usePairing` / `useQrCode` / `useConnectedDevices` 生产无消费者）

## Rust 侧改动点

- [ ] 删 `commands/wsl.rs` 两命令 + `src-tauri/src/lib.rs` 注册（`commands::wsl::*`）
- [ ] 删 `commands/system.rs::get_local_ip_addresses` + 注册项
- [ ] `commands/wsl.rs` / `system.rs` 内已无消费者的 `use` 与常量清理（`system/constants/*` 复核）
- [ ] 确认 `pty::list_distributions` / `pty::is_wsl_available` 仍被
      `host_impl/platform.rs::platform_wsl_distros` 使用（**保留**，那是原语实现）

## 验收

- [ ] `pnpm run test:run`（桌面）全绿；根目录 `pnpm exec eslint .` 0 error
- [ ] 桌面 `cargo test --lib` 全绿
- [ ] 手工/闭环：插件激活时 WSL 发行版列表与本地 IP 展示与迁移前逐项一致；
      插件未激活时给出明确提示（不空白、不假数据）

## Comments

### ① 为什么 `is_wsl_available` 要单独看

`host-platform.wsl-distros` 在「宿主无 WSL」时**显性报错**（空数组与「未安装」不可区分，
见 v19 票 13 决策）。前端 `is_wsl_available` 是「可用性布尔」，改走插件后需把「报错 =
不可用」这条语义显式写下来（而非静默 false），否则用户看到的是「无发行版」而不是「未安装 WSL」。

### ② 与「设备页已删」的关系

设备页（`/devices`）与设置页配对分组已退役，`get_connected_devices` 只服务通知指纹种子化
（已回引擎事实）。本票不涉及它。
