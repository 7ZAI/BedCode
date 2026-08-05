# auto-task 插件优化：任务记录 / 定时任务 / 多 agent 扩展

日期：2026-07-20
依据：`/tmp/bedcode-auto-task-handoff.md` 交接文档、ADR-0003、ADR-0004、CONTEXT.md「自动任务」词汇表。

## 目标

1. 任务记录：记录经 BedCode 执行的任务（时间 / 结果 / 执行 agent），可查询，预留 token 统计列
2. 定时自动任务：指定时刻新建会话执行一组 prompt（一次性，错过标 missed，失败标 failed）
3. 本期仅适配 Claude Code，codex/opencode/pi 留 agent profile 扩展点

## P1 插件 Rust 模型（不动核心）

1. 命令过滤：`is_command_input()`（`/` 开头）+ 白名单预留函数；`on_input_submitted` 建任务前过滤
2. `detect_agent(command)` 取代 `session_command_is_claude` 硬编码；agent profile registry（claude `clear_command=/clear`）
3. 队列调度状态机重做（queue.rs）：
   - task_queue 状态：pending → waiting → executing → done，另有 cancelled
   - 全新会话（该 session 无终态 task_history）→ 跳过 clear 直接发 prompt
   - 有上下文 → 置 waiting + 发 clear_command；SessionStart idle 推送到达 → 发 prompt → executing + 写任务行（source='queue'）
   - waiting 超时 60s 兜底：重试一次 clear，再失败 → cancelled + 广播
   - 出队直接写任务行（description=prompt、source、agent），不再更新最新记录
   - 迁移：task_queue 加 `dispatch_attempts` 列
4. 任务行填充 `agent`（detect_agent 结果）、`source`（user/queue/scheduled）
5. 迁移：task_history 加 `input_tokens`/`output_tokens` 预留列
6. scheduled_jobs 建表（id/name/config_id/trigger_at/prompts/status/created_at/executed_at/error）
7. 修已知 bug：auto_task_hook.py `handle_session_end` INTERRUPT_REASONS 分支加终态/idle 守卫

## P2 核心宿主 API（WASM ABI 扩展）

8. `session_create(config_id)` host function：SDK `HostSession` trait + `wasm_host.rs` 绑定 + `abi.rs` 导出 + 宿主 `host_functions/session.rs` 注册（包 `SessionManager::create_session`）
9. 宿主定时器：插件 `timer_register(interval_secs, command_id)`；宿主 tokio interval 到点调用插件命令（附当前时间参数）；插件以 DB trigger_at 做幂等

## P3 定时任务逻辑（插件）

10. scheduled-jobs CRUD + HTTP 端点（`scheduled-jobs/*` 仿 `task-queue/*`）
11. 定时回调（scheduler-tick command）：到期 → session_create → Created 事件（带 session_id/config_id）→ prompts 入队；失败 → failed；重启后过期 → missed
12. 事件广播 `task:scheduled-changed`（broadcast_sync + bus + emit_event）

## P4 前端重构（侧边栏任务视图）

13. Tab1 任务记录：list-task-history 支持筛选（status/agent/时间）+ 分页 + 行内详情 + 统计聚合接口
14. Tab2 定时任务：新建（config + 时间 + 多 prompt）、列表、删除、状态
15. AutoTaskModal 保留；i18n key 双语同步

## P5 验证

16. `cargo test`（插件 + 核心）、`npm run test:run`、编译检查 target 大小

---

## 实施记录（2026-07-20）

已全部落地。验证结果：

- **插件 wasm32** `cargo check` 通过；原生 `cargo test` 7 项全过（agent 识别/命令过滤/registry 单测）
- **SDK** `cargo check --features wasm` + `cargo test` 通过；ABI 升至 v6（新增 `SESSION_CREATE` / `TIMER_REGISTER`，签名表同步）
- **桌面宿主** `cargo check` + `cargo test --lib --no-run` 编译通过；`session_create` 包核心 `SessionManager::create_session`，`timer_register` 走 `PluginServices::register_plugin_timer`（tokio interval、停用时中止）
- **移动端** `cargo check` 通过（sync 通道补 `TaskScheduledChanged` 全链：枚举/handler/MobileEvent/EventForwarder）
- **前端** `npm run test:run` 217 项全过；插件 `npm run build:frontend` 通过
- **已知环境问题**：桌面 `src-tauri` 的 `cargo test --lib` 测试二进制在本机 Windows 加载器报 `STATUS_ENTRYPOINT_NOT_FOUND`（30 个导入 DLL 全为系统/CRT，无 cargo 产物；与本次代码无关的运行期环境问题）。ABI 注册与 `HOST_FN_SIGNATURES` 一致性已静态校验。

附带改进：`wasm_host.rs` 为非 wasm32 target 新增 `native_link_stubs`，让插件 crate 在 Windows 原生 `cargo test` 可链接（此前无法跑），`wasm_entry!` 的 `__bedcode_deallocate` 按 wasm32 条件导出避免重复符号。

核心新增（`docs/adr/0003` 与 `0004` 落地为代码）：命令过滤一刀切、agent profile registry、任务=轮次模型、队列出队自写任务行、waiting 状态机+clear 感知、定时任务（session_create + Created 事件入队 + missed/failed）、宿主定时器、双 Tab 前端、三通道事件广播。
