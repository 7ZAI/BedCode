# 12: 内核业务语义清零（platform-kernel 清单收敛）

**What to build:** 内核不再携带任何产品身份与产品生命周期：`FILE_TRANSFER_PLUGIN_ID` 及其驱动的 peer-net 启停从 `manager/host/activation.rs` 移出，改为「插件按声明请求引擎服务、内核按声明装配」；两个一次性迁移模块登记退役条件并摘出内核主目录。修完后阶段 4「内核 ABI 冻结」才有可冻结的面。

**Blocked by:** 11（收敛后才有单一改造点）

**Status:** ready-for-human（涉及 peer-net 归属的产品裁决，见需裁决项）

## 现状（已复核）

| 残留 | 位置 | 性质 |
| --- | --- | --- |
| `FILE_TRANSFER_PLUGIN_ID` 常量 | `manager/host.rs:27`（定义）、`peer_net.rs:641` | 内核硬编码产品身份 |
| 激活/停用成功路径按该 id 开关 peer-net 节点 | `manager/host/activation.rs:69`、`:448` | 内核携带产品生命周期 |
| preauth 语义注释绑定 file-transfer | `manager/host/preauth.rs:13` | 文案残留 |
| `WasmHostContext` 持 `SessionManager` / `SessionConfigManager` | `wasm_runtime.rs:917-927`、`:246` | 阶段 3 存量（roadmap 明标终端本体留内核），非新违规但属冻结前必清 |
| 两个一次性迁移每次启动执行 | `plugin.rs:24-28`、`lib.rs:448,452`；`plugin/quick_actions_migration.rs`、`plugin/task_data_migration.rs` | 活的过渡态；quick_actions 反向依赖 `utils/auth/auth_center` 且硬编码业务 api 名 `com.bedcode.session.quick-actions-import` |

## 需裁决项

1. peer-net 节点启停的正确归属：(A) `host-peer` 增引擎级「按需启动节点」原语，由 file-transfer 插件激活时自行调用（推荐——同 `host-pty`/`host-mdns` 的「引擎留内核、语义归插件」形态）；(B) 插件声明 `dependencies` 由内核装配（但 peer 引擎是宿主服务而非 WASM 能力，需先定义「宿主服务也可被依赖声明」这一新形态）；
2. 两个 migration 的退役触发条件与时间窗（契约退役即删），以及删除前是否保留只读兜底；
3. `host-session` / `host-terminal` 两域与 platform-kernel「进程原语 vs 会话业务」清单的最终边界（属 roadmap 阶段 3 终端部分，本票只登记不实施）。

## 验收

- [ ] 按裁决项 1 改造后，`manager/**` 内 grep 无产品名（`file-transfer` / `session` 业务前缀 / `quick_actions` / `auto-task` / `pairing`），把这条 grep 固化为测试或 CI 断言（防回归）
- [ ] file-transfer 插件行为等价：激活/停用后 peer 节点状态与现在一致，界面维持；移动端不受影响（peer 面本就桌面独有）
- [ ] 两个 migration 摘出 `plugin/`（归 `manager/` 安装迁移面，或独立 `migrations` 面并登记退役条件），其硬编码业务 api 名改为常量清单并注明退役时机；`plugin.rs` facade 的 `pub mod` 文档注释随之更新
- [ ] `preauth.rs:13` 等文案残留清理
- [ ] `WasmHostContext` 的 `SessionManager` 依赖登记进 roadmap 阶段 3 未完成清单（不在本票实施，但必须显式留痕，禁止「视而不见」）
- [ ] 门禁：`cargo test` 全绿 + `cargo check --lib --tests`

## Comments

- 2026-09-21 立项：来源 spec §6 表。与 `microkernel-gap.md` 的「阶段 4 冻结前置清单」同源。
