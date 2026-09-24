# 执行记录：websocket 业务下沉专项（票 02–09，跳过票 01）

Status: **in-progress**（2026-09-25 起由本 agent 执行）
承接：用户指令「执行 issues/ 除票 01 外所有 ready-for-agent 票据」；票 01（通用插件端点骨架）
在前置专项 `.scratch/2026-09-18-ws-base-service/spec.md`（done）与现有代码（`channel/plugin.rs`、
manifest wsEndpoints、`ws_control.rs`）中已就绪，跳过。

## 执行顺序（依赖序）

| 序 | 票 | 内容 | 依赖 | 状态 |
| --- | --- | --- | --- | --- |
| 1 | 02 | connection-context（WIT/SDK/宿主实现 + ABI v28 + 测试） | 01（已就绪） | done（4c5186377） |
| 2 | 05 | 会话事件归插件（bus/emit，撤 broadcast_sync 会话路径） | 无 | done（e3796630a） |
| 3 | 06 | 任务事件归插件（bus/emit，撤 broadcast_sync 任务路径） | 05 | done（7460dd941） |
| 4 | 03 | 会话控制直连端点闭环（connection-context 身份接线 + e2e） | 02 | done（cb894d90a） |
| 5 | 04 | 终端输入输出流端点（ws_terminal.rs + 真实 PTY 闭环） | 02 | done（5e72b4de8） |
| 6 | 07 | 设备派生与认证记录归插件（宿主删 Device 事件/DTO/session_count） | 02 | done（450ca7c17） |
| 7 | 08 | 宿主业务硬切（删旧路由/枚举/服务/订阅器/PTY 广播映射/同步桥） | 03-07 | todo |
| 8 | 09 | 终态性能与全量门禁（全量测试 + eslint + 文档 + lens） | 08 | todo |

## 关键现状（已核实）

- ABI 现 v27；版本常量单一真源 `packages/plugin-sdk-desktop/rust/src/abi.rs::ABI_VERSION`；
  宿主校验语义 `plugin.version > ABI_VERSION → 拒绝`，同批内追加函数不 bump（v19/v20 惯例），
  破坏性删除才 bump。**本专项所有破坏性删除统一落 v28**（spec §3.1/P1.1），因此 v28 bump 放票 02 一次完成。
- 插件 WASM 产物提交在 `src-tauri/resources/plugins/desktop/<id>/*.wasm`；SDK 变更后需
  `node plugins/<id>/scripts/build.js --rust-only` 重建（wasip3 nightly 2026-09-16）。
- 宿主 connect/disconnect 事件 topic：`<owner>::ws:client-connect|client-disconnect`；
  帧经 `events-ws.on-client-message(endpointId, clientId, kind, payload)` 回调（payload 为原始字节）。
- 插件激活期宿主自动从 manifest `contributes.wsEndpoints` 登记端点（`manager/host/register.rs`）；
  认证策略 `auth: jwt` 由宿主校验首帧 `{"type":"auth","token":"<jwt>"}`，超时/失败 close 4001。
- 插件定时器最小间隔 1s（`timer.rs::MIN_TIMER_INTERVAL_SECS`）——ws_terminal 输出泵
  按「客户端帧驱动 drain + 1s 兜底定时 drain」设计（见票 04 计划）。
- 事件面（票 05/06）：插件现有 `broadcast_sync(SyncEvent::*)` 调用点：
  launch.rs / actions.rs / session/mod.rs / task/state.rs / task/queue.rs / task/scheduled.rs。
  桌面前端经 `host-events.emit`（Tauri 事件，`context.events.on`）消费；跨插件走 host-bus。
- 旧路由存活：`/ws/terminal/session/{id}`、`/ws/event`、`/ws/plugin/{plugin_id}/{path}`（票 08 删前两条）。
- 桌面前端终端输出走命令面 `session.output.pull`（native invoke → 插件 ring-fetch），不经 WS。

## 安全/纪律

- 共享 worktree：`server/websocket/**`、`wasm_core/host_api/{ws,pty,events}.rs`、插件 rust、
  SDK 均无在途改动（git status 仅 .vscode/.zcode/CHANGELOG/SplashLoading 属他人）；只动自己的文件。
- 每票结束：针对性测试（cargo test 过滤 + 插件 cargo test）绿 → commit（conventional commits）。
- 全量回归只在收尾（票 09）跑；禁止每步全量。
- 旧产物失败提示走既有 `stale_artifact_rebuild_hint`（点名 v28）。
- 不提供旧客户端兼容；移动端不纳入、如实登记（文档口径）。