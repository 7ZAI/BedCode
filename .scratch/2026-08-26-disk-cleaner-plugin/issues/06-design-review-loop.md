# 06 — 前端设计审查闭环（spec §6.5）

**What to build:** 对插件全部页面（扫描结果页 / 目标勾选列表 / 隔离区页 / 置灰项展示）做设计打磨与截图评审闭环：先用 ui-ux-pro-max skill 设计实施（frontend-styles 为项目基线），dev 运行态 Chrome headless 截图交 vision subagent 评审（显式指定 `范围: 桌面应用内`，加载 design-taste-frontend-v1 品味基线），按意见修改后复审，直到无阻断性问题。

**Blocked by:** 05

**Status:** ready-for-agent

- [ ] 全部页面经 ui-ux-pro-max 设计打磨，通过 frontend-styles 自查（token-bound、无反模式）
- [ ] 截图 → vision 评审 → 修改 → 复审循环走完，最终无阻断问题
- [ ] 各轮截图与评审结论留档 `.scratch/disk-cleaner-plugin/reviews/`
