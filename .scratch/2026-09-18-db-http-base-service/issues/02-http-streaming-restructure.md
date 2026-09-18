# 02 — host-http 流式重构：宿主零业务语义

**What to build:** 修复审计发现 H2（架构红线）。从宿主删除 OpenAI SSE 专属解析（`choices[].delta.content` / `usage` 结构与相关反序列化类型），`sseFormat="openai"` 特殊格式清理。宿主只保留引擎级能力：原始 chunk 事件透传，或「通用 SSE 事件帧」——只按分隔符切分事件、透传 data 行原文（最终形态二选一，倾向后者：既有价值且仍无供应商语义）。`openai` 解析逻辑迁回插件层：与 ai-chatbox 消费侧确认迁移后的解析归属（插件自身 / 前端既有解析接管），保证流式行为对用户零感知。

**判定依据：** mDNS basic-capability-service spec D3「宿主零业务语义」裁决 + AGENTS.md §5 无业务内核红线；ai-chatbox 的流式格式属于产品语义，不构成内核原语。

**Blocked by:** None（与 ai-chatbox 插件侧的配合在实现时确认，不阻塞开工）。

**Status:** `ready-for-agent`

- [x] 删除宿主 OpenAI SSE 解析结构与其单测；新增通用 SSE 事件切分/透传（或纯 chunk 透传）实现 + 正反例单测（事件分隔符三种形态、跨 chunk 缓冲、非 UTF-8 处理）
- [x] 与 ai-chatbox 确认消费侧解析迁移方案并落地（插件内解析或前端接管，二选一留证）
- [x] 流式路径回归：stream 请求仍返回 {streamId, streamEvent}、事件通道消息格式对前端既有消费方兼容（或同步迁移前端适配）
- [x] 桌面 cargo test 全绿 + 插件侧（ai-chatbox）测试全绿

## Comments