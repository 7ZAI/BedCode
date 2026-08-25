# host-peer WIT 接口原语化收缩（ADR 0022 实施）

Status: ready-for-agent

依据：`docs/adr/0022-plugin-host-interface-primitive-boundary.md`（边界裁决与函数分类清单，本票不再重复论证）。

## Problem Statement

issue 12 切换时把对等网络四个模块的 Tauri 命令面 1:1 全量投影成 `host-peer` 的 27 个 WIT 函数，业务编排（任务/历史/接收策略/设置/重试）泄漏进 ABI 层。每次业务迭代需同步五处（WIT → 双端 SDK → 双端 host_impl → 插件翻译层 → devMock/fixtures），DTO 翻译做两遍，第二个对等网络消费者无法进入。

## Goal

`host-peer` 收缩为约 15 个引擎原语；file-transfer 插件用既有基础接口自建任务队列、历史、接收策略、设置与重试编排。宿主安全语义不下沉：无应答即拒的闸门默认行为保留。

## 前置条件 / Blocked

- Blocked by: `bedcode-desktop/packages/plugin-sdk-desktop/rust/wit/bedcode.wit` 与移动端副本的在途改动合入（当前 git M 状态，避免冲突）
- 实施窗口内两端须同版发布（ABI bump + 双端同版）

## Implementation Steps（阶段化，每步独立可验证）

### Phase 0 — 过渡拆分（可选，可跳过）

单一 `host-peer` 拆为 `host-peer-discovery` / `-connection` / `-trust` / `-data` 四个 interface，world 按需组合。不减少函数数，只改善结构与权限声明粒度。若直接做 Phase 1 可跳过本阶段。

### Phase 1 — 引擎原语补齐（纯增量，不动旧接口）

1. `cancel: func(session-id: string) -> result<bool, string>`：合并 cancel-transfer/cancel-receiving 语义（session-id = batch-id），宿主按会话表路由
2. 续传偏移查询：`pull-files` 增加 resume 参数，或独立 `query-written-offset: func(dir-id, rel-path) -> result<u64, string>`（真源在接收端落盘侧，见 ADR Consequences）
3. 双端 WIT 副本同步 + ABI version bump；SDK 绑定重生成
4. 验证：peer-net crate 测试全绿、双端 host_impl 编译通过、plugin-test 连通性测试覆盖新原语

### Phase 2 — 插件业务自持（file-transfer，双端）

1. 任务队列与历史迁移：发送/接收任务行落 `host-plugin-database`（表结构沿用现有 PeerTransferDto 字段）；bus `peer:transfer`/`peer:receive` 事件驱动状态机更新
2. 接收策略自持：策略配置（ask/always_accept/always_deny + timeout）存 `host-storage`；ask 弹窗流程插件编排（复用现有 useConsent 同款常驻单例模式），应答经新原语表达
3. 设置面迁移：download-dir / encryption 开关改存插件 storage；send/pull 时经原语参数传入引擎
4. 重试编排：批元数据（node_id/dir_id/rel_path/size）持久化，重试 = 重调 send-files/pull-files（带 Phase 1 续传偏移）
5. 翻译层收敛：删除插件 rust peer.rs 中仅服务旧命令面的 DTO 搬运代码，wire 形状翻译收敛到一层
6. 验证：双端插件 vitest 更新（fixtures 按 wire 形状重造）、devMock 迁移、双端 vue-tsc 干净

### Phase 3 — 旧接口退役

1. 从 WIT 删除下沉的 12 个函数及 host_impl 对应实现
2. 删除宿主 peer_transfer/peer_receive 中仅为 WASM 投影服务的命令包装（Tauri 命令面保留与否另行评估——主前端若仍直调则保留）
3. ABI version 再 bump；双端回归：发现→连接→首连确认→互发文件→断点续传→历史/设置全链真机验证

## Out of Scope

- peer-net 引擎本体（packages/peer-net）零改动（Phase 1 的 cancel 路由除外）
- 终端控制链路与其他 host-* 接口
- 主前端（非插件）的对等网络 UI 去留

## Testing Decisions

- 双节点对打 harness 扩展：cancel 合并语义 + pull resume 偏移查询两个新原语的引擎级行为
- 插件 vitest：任务状态机（事件驱动更新）、接收策略编排、重试元数据回放
- 真机清单沿用 issue 05/06/07 验收项 + 历史/设置迁移后的数据兼容检查（旧 storage 键读取或显式废弃提示）

## Prior art

- ADR 0022（本决策）
- `.scratch/peer-network/spec.md` 决策 11/12（peer-net 归属与迁移节奏——本票是 issue 12 的粒度修正而非方向变更）
