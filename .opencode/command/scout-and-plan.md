---
description: scout 收集上下文，planner 制定实现计划（不实现）
---

使用 task 工具按顺序链式执行以下步骤，每一步都将上一步的完整输出原样并入下一步的 prompt：

1. 委派 `scout` subagent：找出与以下内容相关的所有代码并返回压缩上下文：$ARGUMENTS
2. 委派 `planner` subagent：基于 scout 的输出，为「$ARGUMENTS」制定实现计划（prompt 中包含 scout 的完整输出）

**只返回计划，不要实现。**最后原样输出 planner 的计划。
