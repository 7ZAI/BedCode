# 08: 第一方免询问项的可见性与可撤销

**What to build:** 宿主里那批硬编码的免弹窗目录（如 agent-hub 的 `~/.agents`、`~/.claude/skills`；terminal-session 的项目级 `.claude`/`.codex`/`.pi`/`.opencode`）在管理界面显式列出，用户可撤销对某一目录的免询问——撤销后插件访问该目录被直接拒绝。

**Blocked by:** 02, 07

**Status:** done（2026-09-28）

- [x] 读模型导出第一方免询问项（目录 + 归属应用），作为独立分区呈现
- [x] 撤销 = 落一条 `effect='deny'` 记录，由「deny 优先于第一方层」拦截
- [x] 「总是询问」档下第一方免询问项**仍放行**（票 03 已定），但**必须可见**
- [x] 设置页与详情页**两处都展示**该分区
- [x] 第一方清单的段名匹配语义不变（`ProjectSegment` 落不成可复用的授权记录——项目根由用户每次选）
- [x] 撤销记录自行移除后，恢复免询问
- [x] 单元测试：第一方项出现在读模型中；撤销后该目录被 deny 拦截；移除撤销记录后恢复

## 关键实现事实

- **保留这项的理由**：票 07 当初退役了「内置插件 = 任意路径放行」特权，但留下了这批**有注释归属**的免弹窗目录。不把它显式化，它在新管理界面里就是 §8 要防的不可见面——用户看 agent-hub 的目录清单会以为它只能碰自己授权过的地方

## 实现记录（2026-09-28）

**票 07 已铺好的地基**（本票未重做）：读模型 `firstPartyDirs` 字段
（`auth_policy::first_party_trusted_dirs()` 投影 + `auth_policy` 测试
`first_party_dirs_are_exported_per_owner`）、`~/` 前缀展开（`resolve_revoke_target` +
测试 `revoke_expands_tilde_home_prefix_for_fs`）、`always_ask_does_not_reopen_first_party_dirs`
（档位管不着这批内置项）、`revoke_records_deny_that_wins_over_first_party_dir`
（deny 优先于第一方层）。

**本票新增**

| 内容 | 位置 |
|---|---|
| 第一方项行标记（抽成组件，详情页原有内联标记迁入） | 新增 `src/components/AuthFirstPartyRow.vue` |
| 设置页「内置免询问」分区 + 逐条撤销 | `src/views/AuthorizationView.vue`（展开面板内，策略控件与记录分区之后） |
| `PluginDetailView` 改用共享行标记 | `src/views/PluginDetailView.vue`（删掉 `firstPartyLabel` import 与本地 `firstPartyKindLabel`） |
| i18n（空态 / 撤销提示） | `settings.authorization.sections.firstPartyEmpty` · `firstPartyRevoked`（zh-CN + en 同步） |
| 恢复出口的行为用例 | `fs_auth::tests::removing_the_revoke_record_restores_the_first_party_exemption` |
| 设置页分区用例（5 条） | `src/__tests__/views/desktop/AuthorizationViewFirstParty.test.ts` |

**两处决定**

1. **行标记抽成组件而不是两处各写一份**：两页展示的是**同一批条目**（读模型同一
   字段）。各写一份的漂移形态很具体——「详情页能看到免询问项、设置页看不到」，
   而 spec §7 的裁定正是「这批特权必须两处可见」。一处标记根治。
2. **`project-segment` 形态不渲染撤销按钮**（票面「段名匹配语义不变」的直接
   后果）：项目根由用户每次选，落不成可复用的授权记录；给一个「撤销」按钮却没有
   可落账的目标 = 骗人的 UI。命令面（`revokeFirstParty`）再挡一道
   `kind !== 'home'` 即返回，防止前端被插件面凭证驱动着去撤销一个段名。

**测试**

- Rust：读模型导出（既有）+ deny 优先于第一方层（既有）+ 档位无关（既有）+
  **恢复出口**（新增，走与界面完全相同的写面 `AuthPolicyStore::revoke` →
  `remove_deny`，断言恢复后落回 `FsGrantLayer::FirstPartyDir` 而非记录层/弹窗）
- 前端 5 条：两种形态的展示、`~/` 前缀 target 的撤销参数、段名形态无按钮、
  空态、失败走统一错误消费层

**变异自检**

| 变异 | 杀死 |
|---|---|
| 设置页删掉整个内置免询问分区 | 5 项（新增的 5 条全红） |
| `AuthPolicyStore::remove_deny` 变空操作（`Ok(0)`） | 1 项（`removing_the_revoke_record_restores_the_first_party_exemption`） |
