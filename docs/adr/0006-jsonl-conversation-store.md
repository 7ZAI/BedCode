# 对话历史以插件数据目录 JSONL 文件为唯一存储

**Status**: accepted

## Context

AI Chatbox v1 把对话历史存进宿主 SQLite（自定义表），用户不可见、不可备份，且插件写库依赖宿主 DB 通道。重构目标之一是"对话日志落盘记录"——用户应能直接查看与备份自己的对话。

## Decision

双端插件（桌面/移动，各自独立实现）的对话历史唯一存储 = 插件数据目录下的 JSONL 文件，不再使用宿主 SQLite：

- `conversations/{convId}.jsonl`：首行 meta（id/title/createdAt/updatedAt/providerId/providerName/model/systemPrompt），后续逐行 message（role/content/timestamp/model?/usage?）
- `index.jsonl`：对话列表索引（每行一个 meta），写入时按 updatedAt DESC
- 数据目录在插件包目录之外（桌面 `{HomeDir}/.bedcode/ai-chatbox/`、移动 `{AppDownloadsDir}/ai-chatbox/`），卸载插件不清用户数据
- 写入全部经宿主 `fs_*`（授权后前缀放行）；宿主 `fs_write` 为整文件覆盖，追加 = 读-拼-写
- 删除对话 = 删对话文件 + 重写索引；重生成 = 覆盖文件末尾 assistant 行后再追加
- 旧 SQLite 数据不迁移（v1 无历史包袱）

## Considered Options

- **宿主 SQLite（沿用）**：读写方便，但用户不可见、不可备份，违背"对话日志落盘记录"诉求，且与纯 AI 对话插件"数据归用户"的定位不符。
- **插件 storage（键值）**：同样不可见，且单 key 存长文本不适配消息流。

## Consequences

- 对话历史对用户可见可备份可手工编辑，卸载插件数据保留。
- 宿主无目录扫描 API，对话列表依赖 `index.jsonl` 索引；损坏行跳过并记 warn（容错）。
- 超长对话（>500 条）追加 IO 放大（读-拼-写整文件）可接受；重生成需读-删-写两次整文件操作。
- 数据目录写权限依赖目录授权（见 ADR-0007），撤销授权后写失败走分类提示。
