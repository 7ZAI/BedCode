---
description: scout 收集上下文，planner 制定实现计划（不实现）
---
使用 subagent 工具（agentScope 设为 "both"）以 chain 模式执行以下流程：

1. 先调用 "scout" agent 找出与以下内容相关的所有代码：$@
2. 再调用 "planner" agent，基于上一步的上下文为 "$@" 制定实现计划（使用 {previous} 占位符）

按 chain 执行，步骤间用 {previous} 传递输出。**只返回计划，不要实现。**
