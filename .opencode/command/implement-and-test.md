---
description: worker 实现，tester 验证测试结果
---

使用 task 工具按顺序链式执行以下步骤，每一步都将上一步的完整输出原样并入下一步的 prompt：

1. 委派 `worker` subagent 实现：$ARGUMENTS
2. 委派 `tester` subagent 针对上一步改动的相关范围运行测试并报告结果（prompt 中包含 worker 的完整输出：改动文件与涉及的 Rust/前端范围）

最后汇总：实现内容与测试结果。若测试失败，如实报告失败详情，不要自行绕过。
