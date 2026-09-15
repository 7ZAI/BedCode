# 06: 使用统计 MVP + 会话日志（claude / pi）

**What to build:** 统计与日志的完整竖切，先覆盖 claude 与 pi 两家 JSONL：插件私有库（host-plugin-database）建 `parse_watermark` / `usage_session` / `provider_preset`（无 key 列）表；claude、pi 适配器按文件水位（size/mtime）增量解析本地会话数据为使用记录（token 字段映射覆盖 snake_case `message.usage.input_tokens` 与 camelCase `message.usage.input` 两套命名，时间戳 ISO8601Z 归一）；看板按天/CLI/项目/模型聚合（每日 tokens 堆叠条 + 汇总表 + 会话级简版明细）；会话日志解析视图（主从：会话列表 → 归一事件流，用户/助手/工具/系统角色 + 助手消息模型与 token 明细 + 「原始 JSONL」行切换）。opencode 与 codex 不在本票。

**Blocked by:** 02

**Status:** resolved

- [x] 两家真实数据解析入库，水位幂等（重复扫描不产生重复会话记录；WIT 无 stat 原语，JSONL append-only 语义以 size 为水位，mtime 列预留恒 NULL）
- [x] 看板四个维度可切换，抽查数字与源数据一致；无 $ 成本字段不估算（claude 有真实 totalCostUSD 则存、pi 存 usage.cost.total）
- [x] 日志视图的列表/事件流/原始行切换可用；与统计共用一次解析（同一适配器层：扫描出聚合、打开单会话出事件流+原始行，一次读盘双消费）
- [x] 适配器单测覆盖：截断行、空 usage、大量行流式、时间戳解析

**实现要点**（2026-09-13）：
- 纯解析层 `usage_parse.rs`（ISO8601 手工解析无新依赖、claude/pi 适配器、事件归一）+ 域编排 `usage.rs`（schema/水位/扫描/聚合/命令）；扫描沿用 detect/skills 的「枚举进程 → on_process_done 回灌」异步流，`== 分段 ==` 标记复用 skills::parse_listing
- 实机核验关键事实：claude 同一 assistant message 按内容块拆多行且每行重复完整 usage（220 行 / 97 个 message.id）→ 聚合按 message.id 首现去重；pi 首行 session header 携带 id/cwd，user content 为单块对象形态
- auth 闸门：AUTH_KEY 非 granted 时扫描整体降级 auth-required（fs_auth 第三层按路径弹窗，禁止扫描引发弹窗风暴）；`provider_preset` 表按票据 05 预留建表（本票完成时已由 05 并行建好）
- 自动扫描：面板打开且状态 idle（从未扫描）时自动触发一次（spec §4.5「应用打开面板时增量扫描」，同 useSkills 的 idle→自动扫描模式；auth-required 不自动触发）
- 看板按模型维度在 Rust 侧展开 models_json（会话多模型按消息级归属，不按主导模型摊派）；按天用 SQLite `localtime` 日切
- code-review 收尾：适配器分派提取 `parse_by_adapter`（扫描回灌/打开会话共用，票 07 扩适配器单点）、upsert 参数表去重、水位 INSERT 失败补 warn（不再静默）、伪测试移除

**验证证据**：
- 插件 crate：`cargo test`（rust/）85/85 通过（新增 usage 域 25+ 用例：ISO8601 Z/小数/偏移/空格/非法输入、claude message.id 去重与多模型聚合、cost-state 最后值、isMeta→system、tool_result→tool、pi camelCase usage/cost 累计/对象 content、截断行跳过、6000 行大量流式 + 事件上限截断、主导模型判定、事件 wire 形状、扫描脚本双平台形态）
- `cargo fmt` 已过；`cargo clippy` 余量与 auto-task 既有 `&sql_params!` 惯用法一致（非门禁）
- 宿主：`cargo test`（bedcode-desktop/src-tauri）11 个套件全绿（604 单测 + 集成）
- 前端：`pnpm run test:run` 67 文件 / 619 用例全绿（新增 format.test.ts 11 用例：token 缩写无假零、时长分段、会话/事件时间、项目 ~ 折叠、cost null 不估算）
- 根目录 `pnpm exec eslint .` 0 error（120 warning 均为存量，非本票引入；顺手清掉 AgentHubView 中因本票交付而失效的 TabPlaceholder 死导入）
- i18n：`hub.st.*` / `hub.lg.*` 共 39 个 key 同步出现在 zh-CN 与 en，MessageSchema 编译期校验通过
- 插件完整构建（wasm32-unknown-unknown + componentize + 产物复制）成功
- 测试后无残留进程/监听端口

**剩余验证（真机人工）**：
- `pnpm run tauri:dev` 打开「使用统计」→（idle 自动扫描 / 或立即扫描）→ 抽查会话 tokens 与源 JSONL 一致（去重语义抽查一条多内容块 assistant message）
- 「会话日志」主从视图：列表过滤/事件流角色呈现/原始 JSONL 切换；统计明细行点击跳日志分区
- pi `usage.cost.total` 逐消息累计假设需实机核对一条多轮会话（影响「数字与源数据一致」）
- Windows 实机：`dir /s /b /a:-d ...*.jsonl` 枚举与反斜杠路径归一化（spec §3.1 遗留验证项）

**移交票据 07**（code-review spec 轴发现，spec §4.6「会话列表按 CLI/项目/时间过滤」的 MVP 缩窄）：
- 会话列表目前仅 CLI 过滤；项目/时间过滤待 07 与 opencode/codex 适配器一起补齐（后端 list_sessions 加 project/时间范围参数 + 前端过滤器 UI）
