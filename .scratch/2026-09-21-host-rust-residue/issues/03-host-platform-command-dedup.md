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

**Status:** done（2026-09-21）

## 前端改动点（已定位）

- [x] `src/composables/commands/sessionCommands.ts:18` `list_wsl_distributions()` ——**实测消费方为零**：
      唯一调用方 `stores/wsl.ts`（启动预加载）不产出任何页面读取（`distros` / `isAvailable` 全仓
      无读取点），`useAvailableEnvironments` 是纯平台推导（不查 WSL 列表）→ 按判据③**直接删封装 +
      删 store**，不迁插件命令面（迁移一个无消费者的封装等于造新的孤儿 plumbing）
- [x] `src/composables/commands/sessionCommands.ts:23` `is_wsl_available()` → 同上，随 store 一同删除
- [x] `src/composables/commands/settingsCommands.ts:47` `getLocalIpAddresses()` ——**零调用方**（唯一
      importer `useDesktopCommands.ts` 只是聚合 re-export）→ 删除；本地 IP 的产品面留在插件
      `session.network.info`（插件设备页仍在用）
- [x] 消费方复核：`useAvailableEnvironments` 不依赖 WSL 列表（平台白名单推导），行为不变；
      `useNetwork` 已在票 05 批次删除；插件未激活的提示文案场景不存在（宿主不再有该 UI）
- [x] `src/composables/useTauri.ts` 兼容 re-export 与孤儿 composable 已在票 05 批次清理完毕

## Rust 侧改动点

- [x] 删 `commands/wsl.rs` 两命令 + `src-tauri/src/lib.rs` 注册（`commands::wsl::*`）+ `commands.rs` 模块声明
- [x] 删 `commands/system.rs::get_local_ip_addresses` + 注册项；**逻辑未丢**——搬为
      `system::info::local_ipv4_addresses()`（引擎事实：设备名兜底 / `SystemInfo` 采集 /
      `server/supervisor.rs` 状态信息三处内部调用方改指向它，`commands/qr.rs` 的调用点随票 05 注销）
- [x] `commands/wsl.rs` / `system.rs` 内已无消费者的 `use` 与常量清理（`Database` / `PairingCode`
      / `Mutex` / `PairingService` 导入随配对命令面注销一并删除）
- [x] 确认 `pty::list_distributions` / `pty::is_wsl_available` 仍被
      `host_impl/platform.rs::platform_wsl_distros` 使用（**保留**，那是原语实现）

## 验收

- [x] `pnpm run test:run`（桌面）受影响用例全绿（session store / terminal-flow / fixtures drift：37 passed）；
      根目录 `pnpm exec eslint .` 见本批收尾记录
- [x] 桌面 `cargo test --lib` 全绿（1088/0）
- [x] 行为复核：WSL 发行版列表在宿主前端**已无展示位**（无消费方），本地 IP 由插件
      `session.network.info` 承载；宿主不再提供这两个命令，故不存在「空白/假数据」形态

## Comments ③ 实施记录（2026-09-21）

- 与票面预期的一处偏离：票面假设「前端仍 invoke 宿主命令 → 迁到插件命令面」，实测三个封装里
  两个是孤儿、`stores/wsl.ts` 的唯一消费方是启动预加载 → 按判据③（宿主页面也不用）直接删除
  宿主侧 plumbing，而不是把孤儿 invoke 平移到插件命令面。
- 内部消费方（`system/info.rs`×2 / `server/supervisor.rs`×1）是本票最容易漏的一处：删命令时
  必须同时搬运实现体，否则启动即编译失败（已验证）。

## Comments

### ① 为什么 `is_wsl_available` 要单独看

`host-platform.wsl-distros` 在「宿主无 WSL」时**显性报错**（空数组与「未安装」不可区分，
见 v19 票 13 决策）。前端 `is_wsl_available` 是「可用性布尔」，改走插件后需把「报错 =
不可用」这条语义显式写下来（而非静默 false），否则用户看到的是「无发行版」而不是「未安装 WSL」。

### ② 与「设备页已删」的关系

设备页（`/devices`）与设置页配对分组已退役，`get_connected_devices` 只服务通知指纹种子化
（已回引擎事实）。本票不涉及它。
