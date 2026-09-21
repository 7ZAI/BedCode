# 03: 桌面审批链闭环（P0-2，ADR 0020 落地）

**What to build:** 用户安装的 zip 插件在**首次启用前**必须经人工批准其权限清单，且批准与插件目录内容哈希绑定——批准后换文件即撤销批准。桌面 `security/approval.rs` 从死码变成激活路径上的闸门；移动端已有的形态（`bedcode-mobile/src-tauri/src/plugin/manager.rs:633,652,659`）作为行为参照，前端审批 UI 复用同一交互语义。

**Blocked by:** 01（权限清单展示与过滤依赖词汇真源）

**Status:** ready-for-agent（裁决已收齐并落票，见「裁决（2026-09-22 用户裁决）」块，无待确认项）

## 现状（已复核）

- `approval.rs` 的 `approve` / `verify_approval` / `effective_permissions` / `compute_dir_hash` 桌面生产**零调用**（全仓唯一命中是 `host/install.rs:191` 的 `revoke`）；
- 激活路径 `manager/host/activation.rs:149-151` 直接按 manifest 全量 `grant_permissions`；
- 桌面 `downloader.rs:112` 注释自陈无 `wasm_hash`（移动端有）；`downloader.rs:87-109` 解压无体积/条目上限；
- ADR 0020 状态写「已实施（桌面端 2026-08）」并把「`process:run` 声明即授予」「无内容钉扎」列为它要解决的风险 3、4 → **文档与代码相反**。

## 需裁决项（先答再动工）

1. 批准时机：安装即弹 / 首次启用前门禁 / 权限变更时重弹（移动端是激活门禁 + 存量首启自动批准一次，桌面是否照此）；
2. 信任分档：`resources` 随包插件是否继续「全量授权、无需审批」（ADR 0020 表 3 现口径），若是，则本票的门禁只作用在 `UserInstalled` 来源；
3. 高危位的额外确认：`process:run` / `pty:spawn` / `terminal:input` / 主库面是否逐位确认而非整单批准。

## 验收

- [ ] 激活前置 `verify_approval`：无批准或哈希不匹配 → `PluginState::NeedsApproval` + 拒绝激活 + 前端可见原因（禁止静默降级为「部分权限」）
- [ ] 生效权限 = 批准 ∩ 请求（`effective_permissions`），`storage` 恒授予的特例随票 02 一并取消
- [ ] 批准时对插件目录算 SHA-256（`compute_dir_hash` 已有实现与单测），激活时重算比对，不匹配即撤销批准并落 `NeedsApproval`
- [ ] 桌面 manifest 增 `wasm_hash`（与移动端对齐），`downloader.rs` 安装期校验；同时补 zip 解压**体积/条目数/单文件上限**（越界 fail-visible，遵「调用方声明 + 宿主上下限 + 越界报错」口径）
- [ ] 存量兼容：已启用的用户插件首启自动批准一次并 `warn` 留痕（照移动端先例），不得把用户现有功能打死
- [ ] 审批 UI 维持既有界面风格（用户偏好：界面维持），审批弹层归 `plugin/security` 的前端面，i18n 同步 zh-CN/en
- [ ] ADR 0020 更新：状态改为与代码一致（实施前后各改一次），并把「签名链仍为后续」写回
- [ ] 门禁：`cargo test`（含新增审批链单测与激活门禁闭环用例）+ `pnpm run test:run` + `pnpm exec eslint .` 0 error

## Comments

- 2026-09-21 立项：来源 spec §4-P0-2。修完本票才谈得上「权限声明面有意义」——票 04/05/06 的攻击链第一环（自报高危权限）由本票截断。

### 裁决（2026-09-22 用户裁决，开工前已定，实施时不得再自行取舍）

1. **批准时机**：激活前门禁 + 存量自动批准一次 —— 照移动端先例（`bedcode-mobile/src-tauri/src/plugin/manager.rs:633,652,659`）。
   `verify_approval` 装在激活前置：无批准或目录哈希不匹配 → `PluginState::NeedsApproval` + 拒绝激活 + 前端可见原因；
   已启用的用户插件首启自动批准一次并 `warn` 留痕。**不做「权限清单变更时重弹」**（本轮不加，留作后续可选项）。
2. **信任分档**：`resources` 随包内置插件**免审批**（沿用 ADR 0020 表 3 现口径），本票门禁只作用在 `UserInstalled` 来源。
3. **高危位**：**整单批准 + 高危位视觉强调**，不做逐位单独确认。
   高危位清单固定为 `process:run` / `pty:spawn` / `terminal:input` / `database:main`（后者由票 02 新增，
   已按「仅第一方按需申请」定档）；弹层里逐条红色标识 + 后果文案。
   ⇒ 前置依赖：票 01 登记的「权限展示文案仅覆盖 13/30」必须在票 03 内补齐到全 31 条（zh-CN 与 en 同步），
   否则审批弹层会把没文案的位显示成 `desktop.plugin.perm.unknown`。
