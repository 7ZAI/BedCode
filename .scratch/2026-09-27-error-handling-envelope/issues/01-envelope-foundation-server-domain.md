# 01：信封地基 + 服务器域首个垂直切片（tracer bullet）

**Type:** task
**Spec:** `../spec.md`（§2 核心契约 / §4 P1+P3 范围子集 / §3 A·E·F 组局部）；契约单一事实源 `docs/adr/0030-error-envelope-and-user-prompt-boundary.md`
**Blocked by:** None — can start immediately
**Status:** done（2026-09-27；frontier 移交 02/03）

**What to build:** 用户启动/停止/重启服务器失败时，界面只显示友好提示（如「操作未完成，请稍后重试」「操作超时，请重试」），不再出现任何技术原文、命令名或内部标识；错误详情（原文/堆栈/anyhow 链）带追踪号只进产生方日志——含前端侧：release 构建下 error/warn 级日志也落盘（此前 release 为空函数，前端侧详情不落盘）。

本票是整条错误信封链路的**首个垂直切片**：从 Rust 序列化边界一路打通到前端 toast。信封是全局契约（所有 Tauri 命令的 rejection 从字符串变为信封对象），前端解析层从本票起同时兼容「对象 | 遗留字符串」两种形状——其余域在迁移前显示通用文案属预期的 expand 阶段，不阻塞本票验收。

**集成测试约束（用户指令）**：本票**只编写**集成测试文件，**不执行**；统一在票 05 全量执行。

**Acceptance:**

- [ ] 宿主错误序列化层：错误跨边界为信封 `{code, request_id, params?}`（`code` 语义化；`request_id` 每次失败生成、短随机、同条 tracing 日志带出；信封**永不携带**错误原文/堆栈字段）；`Display`/日志全量详情不变；新增显式友好码变体供调用点覆盖兜底
- [ ] 分类规则落地：未映射变体一律兜底 `host.internal`；需 UI 特定文案/参数的调用点显式构造友好码
- [ ] 前端消费层：失败统一归一到 `UserError{code, request_id, params?}`；三类收敛函数（invoke rejection 解析 / 集中友好 toast + 日志 / 未知异常兜底）各有行为断言
- [ ] IPC 超时 → 超时码友好提示（不再出现命令名）；v1 仅该码提供「重试」按钮
- [ ] `errors.*` 基码 i18n（兜底 / 超时 / 前端兜底）zh + en 同步
- [ ] 前端 logger：release 下 error/warn 级仍转发后端落盘（info/debug 裁剪）
- [ ] 服务器域接入：启动/停止/重启失败与 Server 视图的原始错误直显全部改为友好提示（含对应 i18n 键去 `{error}` 插值）
- [ ] 单元测试全绿（针对性过滤，不跑集成）；**编写**（不执行）覆盖跨边界形状的集成测试
- [ ] 交付说明：grep 复核本票范围内无技术原文直显