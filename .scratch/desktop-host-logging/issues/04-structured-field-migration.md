# 04: 存量日志结构化字段迁移

**What to build:** 全仓把关键关联键（`plugin_id` / `session_id` / `device_id` / `request_id` / `node_id` / `run_id` / `batch_id` / `peer` / `pid` / `client_id` / `task_id` / `config_id` 等）从消息字符串迁移为结构化字段（`key = %value`），消息只保留人类可读描述。AI 按 key grep 全链路（AGENTS.md Logging 节规范）从"仅新代码遵守"变为"全仓一致"。迁移不改变日志级别/时机/频率，不引入新日志。

**Blocked by:** None（可独立推进；建议按模块分批，每批跑测试）

**Status:** done

- [x] 审计基线：全仓 `tracing::(debug|info|warn|error|trace)!` 带 `{}` / `{name}` 捕获式参数调用分类（单行 186 处 + 多行宏，见 `audit.md`），产出待迁移清单
- [x] 插件域迁移：host.rs（含多行宏、复合标识拆字段）、loader.rs（保留）、storage.rs（保留）、fs_auth.rs（保留）、approval.rs（保留）、watcher.rs、host/app_cli.rs（保留）、host/services.rs、message_bus.rs、api_bridge.rs、wasm_runtime/host_impl/mod.rs（api 字段化）
- [x] 会话域迁移：commands/session.rs（config_id/session_id 字段化）、session_manager.rs、session_config.rs、session_output.rs、session_components.rs、commands/session_config.rs
- [x] 网络域迁移：server/ws/terminal_ws.rs（addr→client 字段）、server/ws/registry.rs（client_id+peer）、pty_process.rs（session_id+pid）、pty_reader.rs、events/sync_handler.rs（session_id/config_id）
- [x] 系统域迁移：system/lifecycle.rs（hook+priority 字段化）；lib.rs / commands/* 保留（错误文本/端口/路径）
- [x] Level 语义核对：迁移不提升/降级任何日志级别；热路径克制不放宽
- [x] 全量 cargo test 596 通过 + clippy 回基线 114 警告（0 新增）；迁移文件模块测试全绿