# 06: 文档 + CHANGELOG 双语

**Blocked by:** 01-05, M1, M2

**Status:** todo

- [ ] ADR 0031 定稿 + 状态转「已实施」（`docs/adr/0031-*.md`）
- [ ] ADR 0022 追加修订记录条目 + 「双端偏离」节补 v32 条款 + 「授权策略 = 安全闸门」节标注取代关系
- [ ] `docs/knowledge/mobile-desktop-auth.md`：认证中心章节重写（注册语义、fail-closed、移动端 close code 语义）
- [ ] `docs/knowledge/plugin-development-checklist.md`：认证中心条目（谁能注册、组合式认证怎么用）
- [ ] `AGENTS.md` §8：认证链路段补「认证中心角色经 `auth-center-register` 注册；无中心 = 拒绝」
- [ ] 两端 `docs/code-map.md` 更新（`host_api/auth_center.rs` 新模块）
- [ ] `CHANGELOG.md` + `CHANGELOG_zh.md` 同批条目（desktop ABI 31 → 32）
- [ ] spec §11 票表状态逐票勾销

## 关键实现事实

- 改文档先改**单一事实源**（AGENTS §4 末行）：命令 → §3；边界 → ADR 0022/0031；契约 → WIT；再核对引用方
- 移动端偏离条款要写明「恢复条件」（移动端需要本地认证中心时再补该端 interface）
