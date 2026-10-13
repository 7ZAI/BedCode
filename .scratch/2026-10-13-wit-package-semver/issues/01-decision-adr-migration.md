# 票 01 · 决策、ADR 与台账迁移

**来源**：`.scratch/2026-10-13-wit-package-semver/spec.md` §2.1（ABI 退役评估）、§5 D1 项 5/9、§6 推进顺序第 2 条、§7 T1。

**Blocked by**：None（可立即开始）。

**Status**：**✅ done（2026-10-13）**——ADR `docs/adr/0051-wit-package-semver-and-abi-retirement.md`
落文（R6 / R7 / 新 bump 政策 + 起算 `0.1.0` / 记账去处 / 双端偏离）；台账迁入
`CHANGELOG.md` + `CHANGELOG_zh.md` 双语「契约演进台账（整数 ABI）」节（桌面 v1…v38 /
移动 v1…v21 无断档）；`abi.rs` 文件**未删**（删除随票 04 退役面）。

## What to build

WIT 包 semver 化（D1）的决策层全部落文，作为后续契约层改动的裁决依据与版本纪律：

1. **新 ADR**：记录三项已裁决/待确认决策——
   - R6：**不派生**——wit 包版本独立从 `0` 起算（建议 `0.1.0`），`ABI_VERSION` 与 `interface abi` **一次性退役**、不分两阶段（项目研发起步期无用户，AGENTS.md §0）；
   - R7：版本真源**仓根单点化**（双端 compose.json 引用唯一常量），共享产物场景（file-transfer 单产物）要求双端版本对齐，非共享场景双端各自演进；
   - **新 bump 政策**：任何 WIT 变更（**含函数级追加**）→ patch bump（`0.1.0` → `0.1.1`）；interface 增删 / 签名变 → minor bump（`0.1.x` → `0.2.0`）——退役 `abi` 后版本纪律没有运行期载体，此政策必须显式落 ADR；起算值双端同为 `0.1.0`。
2. **双端 `abi.rs` 台账迁移**：删除前先把桌面 v1…v38 / 移动 v1…v21 的逐版语义迁入 CHANGELOG 双语条目 + ADR 引用（否则契约演进史断档，`core.wit` 头注已声明「WIT 内不复制历史」，记账去处必须显式指定）。迁移完成后再删 `abi.rs`。
3. **CHANGELOG 双语**：`CHANGELOG.md` + `CHANGELOG_zh.md` 同步条目（决策 + 台账 + 后续各票落地记录入口）。
4. **双端偏离条款**：桌面/移动各自演进路径记录（ADR 0018 维持不撤销）。

## 验收标准

- [x] ADR 落文：含用户裁决原文、新 bump 政策（patch/minor 规则）、起算值 `0.1.0`、R7 单点化与共享产物对齐条款、双端偏离条款
- [x] 双端 `abi.rs` 台账（桌面 v1…v38 / 移动 v1…v21）已迁 CHANGELOG/ADR，逐版语义无断档；`abi.rs` 文件删除随票 04 的契约面同步执行（本票只做迁移落文，迁移已核对在场）
- [x] `CHANGELOG.md` 与 `CHANGELOG_zh.md` 双语同步
- [x] 后续各票可在本票裁决基础上开工（无「待裁决」遗留项）

## 门禁

- 用户裁决记录（R6/R7 已裁；bump 政策与起算值经本票确认后即为最终）
- 文档审查：注释与代码同步纪律（AGENTS.md §6），spec §2.1 的 5 项职能替代性表核对无误

## 注意

- 本票纯文档/台账面，**不碰任何被并行线（file-transfer single-wasm）活跃修改的文件**（compose.json、cap-*.wit、wiring_lock.rs 等在途文件不在此票范围）。
- 命令字眼以 `docs/commands.md` 为准。
