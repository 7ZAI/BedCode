# 07 — 文档与 ADR 收口

**What to build:** 让代码之外的单一事实源与实现一致到「下一个人不用问」：代码地图补上新域、ADR 0022 追加 host-websocket 裁决、术语表按需增补、ABI 注解与版本表一致、本 feature 的 spec 状态翻转。文档命令字眼必须与仓库黄金命令一致（`cargo test`、`pnpm run test:run` 等）。

**Blocked by:** 06

**Status:** done（2026-09-19；`lens_diagnostics` 为 pi agent 专属工具，本环境不适用，以可运行的替代证据收口，见 Comments）

- [x] 桌面代码地图更新：宿主能力实现域新增 websocket 一节；新增的 WS 服务层落位说明（`bedcode-desktop/docs/code-map.md`，票 04/05 落地）
- [x] ADR 0022 追加 host-websocket 裁决：零业务代码红线、属主作用域事件 topic、端点命名空间、权限按域拆分、过滤链参与与链路加密排除（含理由）——ADR 0022「新增 host-websocket」节 + 修订记录 v5
- [x] `CONTEXT.md` 术语按需增补（连接句柄 / 端点句柄 / 属主作用域 topic / 事件帧双通道）——新增「插件宿主能力 (Plugin Host Capabilities)」小节
- [x] ABI 注解一致性：SDK 常量、WIT `abi` 接口版本表、测试断言三处版本序列一致且无漂移（v14 三处对齐，核对结论见 Comments）
- [x] 本 spec 状态翻转；票根（issues/）与 spec 的任务清单双向对齐，无悬空项
- [x] 收尾自查：本环境无 `lens_diagnostics`（pi agent 专属）；替代证据 = `cargo test` 全绿（lib 912 + 集成）+ 三个 fixture e2e + 过滤链/链路加密断言 + 无残留进程

## Comments

### 2026-09-19 收口记录

**ABI 三处一致性核对（v14）**

| 落点 | 实际内容 | 判定 |
| --- | --- | --- |
| SDK `packages/plugin-sdk-desktop/rust/src/abi.rs` | `pub const ABI_VERSION: u32 = 14;` + 演进注释逐条列出 v13（host-mdns v2）/ v14（host-websocket + events-ws） | ✓ |
| WIT `packages/plugin-sdk-desktop/rust/wit/bedcode.wit` `interface abi` 文档表 | v10 → v11（host-peer 传输控制三原语）→ v12（总线二进制 + events-binary）→ v13（host-mdns v2）→ v14（host-websocket + events-ws），与 `abi.rs` 序列逐条对应 | ✓（漂移已于票 04 校正） |
| 测试断言 | `abi.rs::tests::test_abi_version_is_v14` → `assert_eq!(ABI_VERSION, 14)` | ✓ |

另核对 `world plugin` 已 import `host-websocket`、`world plugin-ws` 存在且注释标 v14、`events-ws` 未进 `plugin` world 必选导出（动态探测，旧插件零回归）。

**术语增补（`CONTEXT.md` → 「插件宿主能力 (Plugin Host Capabilities)」）**

- 连接句柄（`wsc-<uuid>`，属主私有、停用回收 4005）；
- 端点句柄（`wse-<uuid>` + 对端 client-id；路径后缀 + 宿主注入属主命名空间段）；
- 属主作用域 topic（`ws:<event>.<owner>`，标识在 payload、非属主物理订阅不到、不重放、快照自愈）；
- 事件帧双通道（状态事件走 bus owner topic / 消息帧走可选导出 `events-ws`，未导出即丢弃 + 首次 warn + 计数）。

每条均按 CONTEXT.md 既有体例补 `_Avoid_` 反例，避免与「事件通道（移动端 `/ws/event`）」「终端会话」等既有术语混淆。

**spec ↔ 票根双向对齐**

- spec 顶部 Status 翻转为 done（含真机门禁与前端证据的待补项说明）；
- spec §5 阶段 A/B checklist 全部勾选，其中 A4 标注「前置检查否决后关闭」并指回票 01 证据链，B7 标注 jwt 成功分支遗留；
- 票 01 closed / 02 done / 03 done / 04 done / 05 done / 06 done / 07 done，无悬空项。

**结论偏差点（需知情）**

- `lens_diagnostics mode=all` 与 `lens_*` 系列是 pi agent 专属工具，本环境（CodeBuddy）不可用；替代证据为实际运行的 `cargo test` 全绿 + 三个 e2e + 过滤链断言 + 进程/端口无残留。若后续由 pi agent 接手，仍建议补跑一次 `lens_diagnostics mode=all`。
- 前端 lint/测试（`pnpm exec eslint .`、`pnpm run test:run`）本机无 node/pnpm 未跑（票 06 Comments 有登记与风险评估）。
