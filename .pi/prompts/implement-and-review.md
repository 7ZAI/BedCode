---
description: worker 实现，reviewer 审查，worker 根据反馈修正
---
使用 subagent 工具（agentScope 设为 "both"）以 chain 模式执行以下流程：

1. 先调用 "worker" agent 实现：$@
2. 再调用 "reviewer" agent 审查上一步的实现（使用 {previous} 占位符）
3. 最后调用 "worker" agent 根据审查反馈修正（使用 {previous} 占位符）

按 chain 执行，步骤间用 {previous} 传递输出。
