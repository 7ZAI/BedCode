# 任务模型 —— 轮次制任务与命令过滤一刀切

---
status: accepted
---

auto-task 插件的任务实体语义此前混乱：`task_history` 把"任务 = 一行输入"，`task_queue` 把"任务 = 队列项"，队列出队时又只更新"最新一条记录"导致任务内容丢失（占位符 `Auto Task` / `/clear` 污染）。我们决定：**任务 = agent 会话内的一次执行轮次**（从发起输入到 agent 回答完成），任务只记录发起输入与常规信息（状态、起止时间、结果、执行 agent），轮次内的问答与命令不产生独立任务。命令以"提交行以 `/` 开头"一刀切过滤（与 Claude Code 自身语义一致），过滤逻辑做成可扩展函数并预留白名单机制（未来 `/skills xxx` 这类任务型斜杠命令放行）。队列出队不再依赖输入行重建，由插件直接写任务行（description = 发起输入）。

## Considered Options

- **维持"任务 = 一行输入"**：需要记录轮次内全部问答/命令消息，复杂且与"初始 prompt 即任务"的用户心智不符。否决。
- **命令白名单判定（非一刀切）**：Claude Code 斜杠命令集合持续演进（`/skills` 等任务型命令出现），白名单需要持续维护且 v1 无法穷举。先一刀切 + 预留白名单函数，待命令集合稳定后演进。
- **队列出队依赖输入行重建写任务**：`terminal_send("/clear\n\n{prompt}")` 被拆成两行提交时 `/clear` 会抢先建任务行，时序竞争导致内容错误。否决，改为插件自写任务行。

## Consequences

- `task_history.agent`（执行 agent，CLI 级）与 `source`（user / queue / scheduled）字段开始填充；`input_tokens` / `output_tokens` 列预留（v1 不解析，JSONL 深化需求回填）。
- 任务记录是任务队列的最终归档视图：队列项出队后即成为任务记录，二者同一实体不同状态。
- 深化需求（轮次内对话细节、token 统计）统一走"按 agent 会话 ID 解析 JSONL 日志"，不在任务表里冗余子消息。
