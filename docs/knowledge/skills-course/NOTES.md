# NOTES.md — 教学笔记

## 用户偏好
- 用 opencode（不是纯 Claude Code），技能装在 `~/.agents/skills/` 和 `~/.claude/skills/` 两处
- BedCode 仓库已配置好 issue tracker（`.scratch/<feature>/issues/` 本地 markdown）、triage labels、domain docs（单 context，根 `CONTEXT.md` + `docs/adr/`），即 `setup-matt-pocock-skills` 的产物已存在 —— 见 `docs/agents/issue-tracker.md`、`docs/agents/triage-labels.md`、`docs/agents/domain.md`
- 仓库还有 .pi subagent 扩展（scout/planner/reviewer/worker/tester），agentScope 要传 "both"

## 教学策略
- 这是一套"流程性"知识 + "选择"技能：主线流程（idea→ship）要靠 storage strength（记住路径），选技能要靠 fluency + 交错练习（给随机场景立刻选对）
- 课 1 先给全景图 + 主线流程 + 一个"选哪个技能"的测验，建立骨架
- 后续课按主线一节一节钻：grilling → to-spec/to-tickets → implement/tdd → code-review，再分 on-ramps
- 速查表 `reference/skill-selector.html` 是核心 reference，每个 lesson 都链它

## 已知坑（要先记进 learning-record 或课里讲）
- `ask-matt` 本身 `disable-model-invocation: true` —— 它是"问哪个技能合适"的路由器，靠用户手动调
- `triage` 只处理**你没创建的** issue；`to-tickets` 产出的票已经 agent-ready，别再 triage
- `wayfinder` 只给"大到一次 session 装不下、又看不清路线"的活；well-scoped 的小功能别上
- `/compact`（内置）留在同一对话、在阶段之间用；`/handoff` 是 fork 到新对话。别 compact 到阶段中途
- `implement` 内部驱动 `tdd`，结尾跑 `code-review` 再 commit —— 别单独把 tdd 当完整 spec 流程用
- 上下文卫生：grill → spec → to-tickets 保持**一个未中断的 context window**；每个 implement 开**新**窗口从 ticket 干
