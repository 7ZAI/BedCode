---
description: 完整实现流程 - scout 收集上下文，planner 制定计划，worker 实现
---

使用 task 工具按顺序链式执行以下步骤，每一步都将上一步的完整输出原样并入下一步的 prompt：

1. 委派 `scout` subagent：找出与以下内容相关的所有代码并返回压缩上下文：$ARGUMENTS
2. 委派 `planner` subagent：基于 scout 的输出，为「$ARGUMENTS」制定实现计划（prompt 中包含 scout 的完整输出）
3. 委派 `worker` subagent：按 planner 的计划完成实现（prompt 中包含完整计划）

执行中不要重复 subagent 已完成的工作。最后汇总 worker 的实现结果（改动文件、测试结果）。
