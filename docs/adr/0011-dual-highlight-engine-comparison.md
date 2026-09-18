# 高亮引擎刻意双轨：桌面 hljs / 移动 Shiki 对比实验

**Status**: accepted

## 对比实验状态：进行中（P1–P4 已落地，人工验收待执行）

P1–P4 已全部实施并提交（桌面 hljs / 移动 Shiki 双轨均已上线），自动化验证全绿：双端 vitest（桌面 131 / 移动 130）+ 双端 cargo test（15 / 16）+ 宿主根回归（桌面 249 / 移动 82），i18n 双端同步核对通过。

剩余为不可自动化的**人工对比实验**（需真机 + 真实模型）：桌面/移动各跑同一条含多代码块的长回复，记录打字机流畅度、流式过程布局稳定性（fence 未闭合期）、闭合后高亮观感（hljs 低饱和 vs Shiki IDE 级）。结论决定后续统一引擎方向，实验完成前不启动统一引擎决策。实验完成后在本 ADR 追加结果与结论，并将 Status 更新为 resolved。

## Context

AI 聊天代码高亮的引擎取舍：hljs（正则启发式、同步、轻）vs Shiki（VS Code 同款 TextMate grammar、保真度高、异步）。移动端宿主已集成 Shiki（`createHighlighterCore` + oniguruma WASM + 静态语言导入单例），桌面端宿主没有。

## Decision

本期渲染管线改造中，双端高亮引擎**刻意不一致**，以真实效果对比为后续统一决策提供数据：

- 桌面端 ai-chatbox：保持 hljs（同步 `highlightElement`，现有 CSS token 双套配色沿用）
- 移动端 ai-chatbox：改用 Shiki（复制宿主单例接入模式，深浅色双主题切换）
- 渲染管线以 `HighlightEngine` seam 隔离引擎（高亮只跑已闭合代码块、收敛单点），桌面注入 hljs adapter、移动注入 shiki adapter

对比结论出来后统一引擎时，将 Shiki 封装为宿主插件 API（`context` 能力）供插件免自带实例调用，属后续项，不在本期。统一引擎是 obvious path，此处为 deliberate 实验偏离。

## Considered Options

- **两端统一 hljs**：省事，但失去 IDE 级保真度对比依据。
- **两端统一 Shiki**：桌面宿主无 Shiki 先例，异步高亮与流式管线首战混做，风险叠加。

## Consequences

- 移动插件自带一份 Shiki 实例（bundle 与 WASM 加载成本，宿主桥接是后续优化点）。
- 若 Shiki 胜出并统一，桌面宿主需引入 Shiki + SDK 增加高亮 API，两端插件改为调用宿主能力，届时删除插件内 hljs/shiki adapter。
- 高亮引擎是独立于流式管线（节流/fence 补偿）的审美决策，管线本身与引擎选择解耦。
