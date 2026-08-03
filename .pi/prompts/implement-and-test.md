---
description: worker 实现，tester 验证测试结果
---
使用 subagent 工具（agentScope 设为 "both"）以 chain 模式执行以下流程：

1. 先调用 "worker" agent 实现：$@
2. 再调用 "tester" agent 针对上一步改动的相关范围运行测试并报告结果（使用 {previous} 占位符）

按 chain 执行，步骤间用 {previous} 传递输出。
