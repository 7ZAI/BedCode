# Mission: 精通 Matt Pocock Skills 工作流

## Why

我每天用 Claude Code / opencode 这类 AI 编码助手在 BedCode（Tauri + Vue + Rust）项目里干活，本地装了一整套 Matt Pocock 的工程技能（grilling、to-spec、to-tickets、implement、tdd、code-review、wayfinder 等 25+ 个）。现在的问题是：技能太多记不住，遇到具体场景不知道该调哪个、怎么串起来用，结果要么手写一遍技能已经做的事，要么用错技能把流程走歪。掌握这套技能的"选哪个、怎么用、怎么串"之后，我的 agent 辅助开发会从"随机召唤"变成"按流程走主线 idea → ship"。

## Success looks like

- 给我一个真实开发场景（修 bug、做新功能、接别人提的 issue、大重构），我能立刻说出该用哪个技能、为什么不是隔壁那个
- 能讲清楚"主线流程 idea → ship"的完整路径：grill-with-docs → (to-spec → to-tickets) → implement → tdd → code-review，以及三条 on-ramp（triage / diagnosing-bugs / wayfinder）在哪里并入主线
- 知道何时**不该**用某个技能（例如不要 triage 自己 to-tickets 产出的票、well-scoped 的小功能别上 wayfinder、不要 compact 到阶段中途）
- 能把这套技能串进 BedCode 现有的 .scratch/ issue tracker + docs/agents/ + .pi subagent 体系里用

## Constraints

- 教学用中文，技术术语保留英文（skill 名、触发词、CONTEXT.md 等）
- 我用 opencode，技能同时存在于 `C:\Users\binblink\.agents\skills\` 和 `C:\Users\binblink\.claude\skills\`
- 一次课要短、能很快做完，给一个能复用的小胜利
- 优先用可打印的速查表（reference）+ 带测验的互动课（lesson）

## Out of scope

- 自己写新技能（`writing-great-skills` 单独再学，本 mission 只学"怎么用现成的"）
- BedCode 仓库本身的 .pi subagent 体系深入（scout/planner/worker 等）——只在"怎么和 skills 串"这一层碰
- 具体某个外部库的 API 文档（find-docs / ctx7 的使用细节按需补，不进主线）
