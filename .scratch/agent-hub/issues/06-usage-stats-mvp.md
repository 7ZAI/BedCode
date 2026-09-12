# 06: 使用统计 MVP + 会话日志（claude / pi）

**What to build:** 统计与日志的完整竖切，先覆盖 claude 与 pi 两家 JSONL：插件私有库（host-plugin-database）建 `parse_watermark` / `usage_session` / `provider_preset`（无 key 列）表；claude、pi 适配器按文件水位（size/mtime）增量解析本地会话数据为使用记录（token 字段映射覆盖 snake_case `message.usage.input_tokens` 与 camelCase `message.usage.input` 两套命名，时间戳 ISO8601Z 归一）；看板按天/CLI/项目/模型聚合（每日 tokens 堆叠条 + 汇总表 + 会话级简版明细）；会话日志解析视图（主从：会话列表 → 归一事件流，用户/助手/工具/系统角色 + 助手消息模型与 token 明细 + 「原始 JSONL」行切换）。opencode 与 codex 不在本票。

**Blocked by:** 02

**Status:** ready-for-agent

- [ ] 两家真实数据解析入库，水位幂等（重复扫描不产生重复会话记录）
- [ ] 看板四个维度可切换，抽查数字与源数据一致；无 $ 成本字段不估算
- [ ] 日志视图的列表/事件流/原始行切换可用；与统计共用一次解析（不重复读盘）
- [ ] 适配器单测覆盖：截断行、空 usage、大量行流式、时间戳解析
