# 10 — 6 个 controller + app.rs + supervisor.rs 补测试

**What to build:** 6 个 HTTP controller（`auth_controller.rs`、`file_controller.rs`、`git_controller.rs`、`plugin_controller.rs`、`session_controller.rs`、`config_controller.rs`）+ `app.rs`（路由装配 + 服务器启动）+ `supervisor.rs`（8+ 生命周期方法）全部零测试。补覆盖路径穿越、命令注入、插件 panic 传播、生命周期管理等关键安全与可靠性路径。

**Blocked by:** 无

**Status:** partial（2026-09-15：file_controller 路径穿越纯函数 is_within_root + 3 测试；git_controller 抽 is_valid_branch_name 命令注入白名单（含空串拒绝修复）+ 4 测试；plugin_controller 抽 plugin_http_status 非法 status 回退 + 2 测试；jwt.rs 抽 jwt_error_message 消除 terminal_ws/auth_controller 两处重复 + 1 测试，变异自检 2 处被捕获。auth/session/config controllers 的端到端 handler 测试 + app.rs 路由装配 + supervisor.rs 生命周期需 mock actix，待用户决策）

- [ ] 新增 `test_auth_controller_pairing_endpoint`：测 `/api/auth/pairing` 端点的请求验证、错误响应、权限检查
- [ ] 新增 `test_auth_controller_verify_endpoint`：测 `/api/auth/verify` 端点的完整流程
- [ ] 新增 `test_file_controller_resolve_working_dir_path_traversal`：`resolve_working_dir` 拒绝 `../` 路径穿越
- [ ] 新增 `test_file_controller_list_directory_out_of_root`：列目录越界返回 403
- [ ] 新增 `test_git_controller_command_injection`：git 命令参数注入被拒绝
- [ ] 新增 `test_plugin_controller_inactive_plugin_returns_error`：未激活插件返回错误
- [ ] 新增 `test_plugin_controller_illegal_status_fallback`：非法 status 回退到默认
- [ ] 新增 `test_plugin_controller_plugin_panic_propagates`：插件 panic 被捕获并返回 500
- [ ] 新增 `test_app_router_setup`：路由装配完整性（所有声明的路由可访问）
- [ ] 新增 `test_app_server_start_stop`：服务器启动与停止的生命周期
- [ ] 新增 `test_supervisor_start_stop_restart`：`start`/`stop`/`restart` 生命周期
- [ ] 新增 `test_supervisor_update_port`：`update_port` 正确更新监听端口
- [ ] `cargo test --lib server::controllers::` + `cargo test --lib server::app::` + `cargo test --lib server::supervisor::` 通过

## 证据

- 6 个 controller 全部零测试（grep `#[cfg(test)]` 返回空）
- `app.rs:155-335`：路由装配 + 服务器启动零测试
- `supervisor.rs:110-322`：8+ 生命周期方法（start/stop/restart/update_port）零测试
- 安全风险：`file_controller.rs` 路径穿越、`git_controller.rs` 命令注入、`plugin_controller.rs` 插件 panic 传播

> 复核（2026-09-14）：`git_controller.rs` 生产代码**已有**分支名白名单校验（`run_git_checkout` 只允许字母/数字/`-`/`_`/`/`/`.`，且 `run_git_command` 用 argv 数组不经 shell），补测目标是锁住该校验本身而非暴露的注入漏洞；`file_controller.rs:28` `resolve_working_dir` / 列目录越界 403 逻辑与 `plugin_controller.rs` panic 捕获路径均零测试，需补。

## 根因

controller 层全部零测试，app.rs 与 supervisor.rs 的生命周期管理零覆盖。这些是 HTTP 服务的入口与生命周期，任何回归直接影响服务可用性。

## 修复方向

1. 为每个 controller 补请求验证、错误响应、权限检查的测试
2. 为 `app.rs` 补路由装配完整性测试
3. 为 `supervisor.rs` 补生命周期管理测试

## 影响面

修复后，HTTP 入口与生命周期的回归能被抓到。但 controller 测试需要 mock 或集成环境，成本较高。

## Comments

- 2026-09-14 审计发现，见 `../http-ws-spec.md` §5.12
- 路径穿越与命令注入是安全红线（AGENTS.md §8）
