---
description: worker 实现，reviewer 审查，worker 根据反馈修正
---

使用 task 工具按顺序链式执行以下步骤，每一步都将上一步的完整输出原样并入下一步的 prompt：

1. 委派 `worker` subagent 实现：$ARGUMENTS
2. 委派 `reviewer` subagent 审查上一步的实现（prompt 中包含 worker 的完整输出：改动文件与关键函数）
3. 委派 `worker` subagent 根据审查反馈修正（prompt 中包含 reviewer 的完整审查结果）

最后汇总：实现内容、审查发现的问题及修正结果。
