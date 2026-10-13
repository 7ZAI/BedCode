# 票 01 · 功能级契约盘点与裁决

**来源**：`.scratch/2026-10-13-domain-owned-contracts/spec.md` §3–§5。

**Blocked by**：None（可立即开始）。

**Status**：**resolved**（2026-10-13 用户全部按推荐确认，确认记录见文末 Comments；审计产出 `audit.md`）。

## What to build

按 spec §2 的 R1/R2 把双端契约面**逐 interface、逐函数**盘一遍，产出 `.scratch/2026-10-13-domain-owned-contracts/audit.md`：

1. **全量清单**：双端 world 的 import / export（含可选导出），每项标注：功能描述、当前归属（core / 域分片 / 端 cap）、实现落点（内核 core_binding / 能力域 crate / 端宿主）。基准 = 双端 `compose.json` + 生成物 `rust/wit/` + 各 crate `bindgen!` 路径。
2. **逐项判定**：对每个 interface 用 R1/R2 给出归属结论：
   - core 保留（R1 三条件全满足，逐条给理由——`host-platform` 的 `pick-*` 是否属「机制必需」必须显式复核）；
   - 域共享契约（功能级交集）；
   - 端 cap（端独有功能）。
3. **合并提案**（R2 后半句：「功能一致 → 统一成一个函数并视为交集」）：对 spec §5 的 C1–C6 逐案给**统一签名草案 + 破坏面分类**（minor bump 级别）+ 两端实现可行性核查（移动 SAF 列目录 / 保存框 / 通知选项支持；桌面终端流插件面是否存在）。每案给推荐，不给开放题。
4. **最小问题集**：只列需要用户拍板的（预计：C1 不支持选项的处置、C3/C5 是否统一、ws 契约房、notify 契约房）。每问附推荐与理由。

## 验收标准

- [ ] `audit.md` 在场：每个 interface / 函数有唯一归属标注，无「待定」漏项
- [ ] C1–C6 每案有统一签名草案（或「保留端扩展」结论 + 理由）
- [ ] ws 客户端契约房、notify 契约房结论明确
- [ ] 最小问题集经用户确认（Comments 记录），T-02/T-03/T-04 的输入被冻结
- [ ] 无代码改动（本票纯盘点）

## 门禁

- 每条结论可回查（文件 + 行号 / 命令）；命令字眼以 `docs/commands.md` 为准
- 不碰任何被并行线修改的文件（只读盘点；产出只写本任务目录）

## 注意

- 「功能」判定的证据必须来自**两端现状**（cap + 实现），不臆测未来能力；一端缺的能力要么按 R2 留端扩展，要么作为「新增能力」单独立项（不在本改造内偷偷补齐）。

## Comments

### 2026-10-13 · 审计完成 + 用户裁决（全部按推荐）

**产出**：`.scratch/2026-10-13-domain-owned-contracts/audit.md`（303 行；双端 world 逐 interface/逐函数清单、R1/R2 判定、C1–C6 合并提案、union 基线、现状疑点）。

**用户答复（2026-10-13，原话「全部按照推荐来进行」）**：

| 问题 | 裁决 |
| --- | --- |
| Q1 通知统一（C1） | (a) 统一为共享 `host-notify.notify(title, body, options-json)`；(b) 桌面不支持 `vibrate`/`sound` **静默忽略 + WIT 文档声明**；(c) 移动附加 4 函数改名移动扩展 `host-notify-mobile`（cap-mobile）；(d) 契约房 = **新建轻量 crate `packages/bedcode-notify-engine`**（`wit/notify.wit`）；桌面 `host-events-desktop` 删除 |
| Q2 目录选择（C2） | 统一为 core `host-platform.pick-folders()`（`string[] JSON`，空数组=取消），删 `pick-folder` 与桌面 `host-platform-desktop.pick-folders`；移动 SAF 返回 0/1 项；`host-platform` 留 core |
| Q3 C3/C4/C5 | 均保留端扩展，不统一（C3 移动无同形 SAF 等价、C4 桌面无等价、C5 桌面插件面无等价；理由记录入 ADR/CHANGELOG） |
| Q4 ws 客户端契约房 | **选项 A**：`packages/bedcode-ws-client-engine/wit/ws.wit`；桌面 ws 客户端实现**暂留** `bedcode-server-websocket` 的 common-host 绑定（以该文件为契约源）；服务端 impl 迁桌面宿主 |
| Q5 core 复核 | `host-platform` 的 `pick-*` / `platform` 留 core |
| C6 导出面 | 保留现状（不进 R2 交集判定；理由记录） |

**冻结输入（T-02/T-03/T-04 的输入面）**：audit §F 已按上述裁决填写；union 基线见 audit §0。
