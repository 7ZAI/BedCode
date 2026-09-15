# 01: HTML UI 原型与设计定稿

**What to build:** Agent Hub 页面的可交互 HTML 原型：三套结构迥异的分区方案（二级侧栏 / 顶部分段 / 单页下钻），底部悬浮条与 URL hash 切换变体，内嵌本机实查数据；评审产出设计结论回写 spec。

**Blocked by:** None (can start immediately)

**Status:** resolved

## Answer

- 变体 B「顶部分段导航」胜出（顶部 pill 分段六段：概览 / 安装与更新 / Skills / 供应商 / 使用统计 / 会话日志）
- 四家 CLI 使用官方品牌图标：claude / openai / opencode 取自 simple-icons（CC0），pi 为官方标（用户提供，evenodd 镂空）
- 会话日志解析视图升入 v1（主从布局：会话列表 → 归一事件流 + 原始行切换）
- 原型存档：`.scratch/agent-hub/prototype/index.html`（`#variant=b`），实现按 `frontend-styles` 规范翻译为 Vue 组件
