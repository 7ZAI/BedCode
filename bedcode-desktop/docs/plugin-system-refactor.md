# 插件系统重构 — 进度跟踪

> 目标：以 `bedcode-plugin-api`（SDK crate）为 ABI 与接口的**单一事实来源**，
> 宿主与插件共同引用；接口签名按功能域拆分为 trait；消除重复定义与弱类型契约。
>
> 分支：`dev`。每阶段完成后 `cargo check` + `cargo test` 验证，逐阶段提交。

---

## 架构决策记录

| # | 决策 | 理由 |
|---|------|------|
| D1 | 移除 `plugin.token` 机制（不修复） | 用户确认不再需要；网关从未实际校验，属死逻辑 |
| D2 | ABI 名称/签名常量集中在 SDK `abi` 模块，宿主注册与测试均引用 | 消灭三处重复定义，漂移在测试期暴露 |
| D3 | 宿主能力按功能域拆为 `host/*` trait（Storage/Database/Terminal/Session/Events/Http/Fs/Config/Log/Bus），`WasmHost` 实现全部子 trait | 插件业务依赖抽象而非 god object；**修正**：`RustPluginContext` 后端为异步，不 impl 同步 trait（避免 runtime 内 block_on 死锁），保持独立异步 API |
| D4 | 配置白名单 = `ConfigKey` 枚举，宿主 match 穷尽 | 结构性消灭"白名单有、实现无"的漂移 bug |
| D5 | 跨进程载荷（`SyncEvent`/`SessionLifecycleEvent`/`PluginQuestion`）类型化进 SDK，两端共享 | 字符串契约 → 穷尽 match，编译器兜底 |
| D6 | `WasmPlugin` 保持单 trait（宏生成约束），但**载荷全部类型化**（`invoke_command(args: Value)`、`on_session_lifecycle(&SessionLifecycleEvent)` 等） | trait 拆分会使 `wasm_entry!` 无法条件生成导出；类型化已解决核心问题 |
| D7 | 命令 ID 以 manifest `contributes.commands[].id`（带前缀全名）为唯一来源，dispatch match arm 必须同名 | 与 ai-chatbox 现有模式一致；前端用 registry 全名调用 |
| D8 | hook 端点认证依赖网关 JWT 中间件 + 本地网络信任（`/api/plugin/*` 无 JWT 时放行） | 沿用现状；hook 脚本仅在 BedCode 注入 `BEDCODE_SESSION_ID` 的 PTY 中生效 |
| D9 | `HostError` 首期仅承载状态码，错误详情仍在宿主日志；完整错误透传留到 ABI v2 | 避免破坏现有 ABI（out_ptr 语义不变） |

---

## P1 — 契约修复与 ABI 常量化

前置：工作区已含上一轮未完成的 `host_functions.rs` 拆分（未提交），config.rs 处于半改状态（编译不过），本阶段一并收尾。

- [x] **P1.1 移除 plugin.token（端到端）**
  - `system/config.rs`：删 key 描述项、"插件配置"分组、`from_properties` 解析、`test_ensure_valid_token` 测试、残留空行
  - `system/constants/auth.rs`：删 `MIN_PLUGIN_TOKEN_LEN`
  - `system/constants/plugin.rs`：删 `ENV_BEDCODE_TOKEN`（`ENV_BEDCODE_PORT` 暂留，P5 迁入 SDK）
  - `plugin/wasm_runtime/host_functions.rs`：`CONFIG_WHITELIST` 移除 `"plugin.token"`
  - `plugins/auto-task/rust/src/lib.rs`：`on_session_lifecycle` 删除 token 读取与空值跳过
  - `plugins/auto-task/rust/src/hooks.rs`：`ensure_project_hooks`/`build_hooks_config`/`is_hooks_port_token_matching` 去掉 token 参数（改名 `is_hooks_port_matching`，env 前缀仅 `BEDCODE_PORT`）
  - `plugins/auto-task/rust/src/token.rs`：删除死文件（无 `mod token`）
  - `plugins/auto-task/scripts/auto_task_hook.py`：删 3 处 token 门控、payload `token` 字段、`&token=` 查询参数、文档字符串
  - `server/controllers/plugin_controller.rs` + `server/middleware/jwt_auth.rs`：更新注释（无 plugin token 概念）
  - `plugins/auto-task/rust/src/state.rs`：网关认证注释措辞调整
- [x] **P1.2 补齐 WASM host function 权限校验**
  - `host_session_list` / `host_session_get` / `host_session_config_list` → `session:read`
  - `host_broadcast_sync` → `broadcast`
  - 补齐 `PERMISSION_SESSION_READ` / `PERMISSION_BROADCAST` 导入
- [x] **P1.3 TaskQueueChanged 同步链路**
  - 桌面 `events/sync_event.rs`：`DesktopSyncEvent::TaskQueueChanged { session_id, queue_count, action }`
  - 桌面 `enums/sync.rs`：`SyncPayload::TaskQueueChanged` 变体
  - 桌面 `events/sync_handler.rs`：新增 handler arm（仿 `handle_session_mode_changed`）
  - 桌面 `host_functions.rs`：`host_broadcast_sync` 增加 `TaskQueueChanged` 解析分支
  - 移动 `src-tauri/src/enums/sync.rs`：同步新增变体（线协议对齐）
  - 移动 `src-tauri/src/handler/sync.rs`：新增 arm（log + emit）
  - 移动 `src-tauri/src/router/event.rs`：`MobileEvent::SyncTaskQueueChanged` + 转发 arm（`ws_sync_task_queue_changed`）
- [x] **P1.4 auto-task 命令 ID 对齐 manifest**
  - `plugins/auto-task/rust/src/lib.rs`：`invoke_command` match arm 改为全名（`auto-task.get-task-status` 等），`_http_endpoint` 保留
  - 附带修正：manifest 删除 4 个无 dispatch 实现的队列命令声明（add/remove/list/clear-queue 仅走 HTTP 端点，声明为 command 会在命令面板触发 Unknown command）
- [x] **P1.5 SDK `abi` 模块**
  - `packages/plugin-sdk-desktop/rust/src/abi.rs`：`NAMESPACE`、`MEMORY`、`export::*`（11 个导出名）、`import::*`（27 个导入名）、`HOST_FN_SIGNATURES` / `PLUGIN_EXPORT_SIGNATURES` 签名表、`ABI_VERSION`
  - `lib.rs` 导出 `pub mod abi`
- [x] **P1.6 宿主引用 abi 常量 + 自检测试**
  - `host_functions.rs`：本地 `register!` 宏统一命名空间与错误上下文，27 处注册引用 `abi::import::*`
  - `wasm_runtime.rs`：导出调用用 `abi::export::*`，`"memory"` → `abi::MEMORY`
  - `test_wasm_export_signatures` 改用 `abi::PLUGIN_EXPORT_SIGNATURES`
  - 新增测试 `test_host_fn_registration_matches_abi`：遍历 `abi::HOST_FN_SIGNATURES`，与实际 Linker 注册签名逐一比对
- [x] **P1.7 验证**
  - `cargo check`（宿主 + SDK 双 target）✅
  - SDK 单测 5/5 ✅；宿主 plugin 模块 25/25 ✅；config 15/15 ✅；全量 133/136 ✅（3 个失败为预存问题，见日志）
  - WASM 测试插件构建 + auto-task WASM 构建 ✅；移动端 `cargo check` ✅
  - **修复 4 个阻塞测试基础设施的预存问题**（详见进度日志）

## P2 — SDK 功能 trait 化

- [x] **P2.1** SDK 新增 `host/` 模块：`HostStorage` / `HostDatabase` / `HostPluginDatabase` / `HostTerminal` / `HostSession` / `HostEvents` / `HostHttp` / `HostFs` / `HostLog` / `HostBus` / `HostConfig` 11 个子 trait + `HostError` + `HostApi` 聚合 trait（blanket impl），文件按功能域一一对应宿主 host function 分组
- [x] **P2.2** `WasmHost` 改为无状态 unit struct（删除无用的 `plugin_id` OnceLock —— 插件身份本就由宿主 Caller state 维护），逐个 impl 子 trait，返回值全部 `Result<_, HostError>` 类型化；`wasm_entry!` 宏内日志改 UFCS 调用（展开处无需 import trait），删除 HOST static
- [x] **P2.3** `ConfigKey` 枚举（`NetworkPort` / `HomeDir`），宿主 `host_config_get` 以 `from_str` 过滤白名单 + 穷尽 match 取值，删除 `CONFIG_WHITELIST` 字符串数组
- [x] **P2.4** ~~`RustPluginContext` impl 同一组 trait~~ → **D3 修正**：`host/*` trait 定义为同步 WASM ABI 契约（含 mock 抽象用途）；`RustPluginContext` 的后端是异步的（tokio），强行 impl 同步 trait 需在 runtime 内 `block_on`（死锁风险），故保持独立异步 API。两类插件 API 统一推迟到原生插件路线重新评估时
- [x] **P2.5** 迁移 auto-task（lib/hooks/state/queue）/ ai-chatbox（lib/commands/db/ai_client）/ test_plugin 到新签名：`WasmHost` unit 构造、`Result` 错误处理（失败路径带 `HostError` 描述入日志）、`ConfigKey` 枚举
- [x] **P2.6** 验证：SDK 双 target ✅；宿主 check + plugin 测试 25/25 ✅（ABI 签名测试证明 trait 化对 WASM ABI 零影响）；auto-task / ai-chatbox 双 target 构建 ✅

## P3 — 载荷类型化

- [x] **P3.1** SDK `events.rs`：`SessionLifecycleEvent` / `SyncEvent` / `PluginQuestion` / `PluginQuestionOption`（serde tag 与现线协议逐字节一致：`event_type` snake_case / `type` PascalCase）
- [x] **P3.2** `wasm_entry!` 宏：`invoke_command` 收 `Value`、`on_message` 收 `BusMessage`、`on_session_lifecycle` 收枚举（宏内做 JSON↔类型转换；生命周期载荷解析失败按协议错误返回 -1）
- [x] **P3.3** `WasmPlugin` trait 签名类型化；`HostEvents::broadcast_sync(&SyncEvent)`
- [x] **P3.4** 宿主：`DesktopSyncEvent` 增加 `From<SyncEvent>`（穷尽 match），`host_broadcast_sync` 改为 serde 解析 + From 转换（删除 40 行字符串 match）；`PluginQuestion` re-export 自 SDK；`PluginLifecycleListener` 用 SDK 枚举序列化
- [x] **P3.5** 迁移两插件：auto-task（生命周期 match 变体解构、3 处 `SyncEvent` 广播、questions 反序列化为 `Vec<PluginQuestion>`）/ ai-chatbox（`invoke_command` + 8 个 command 函数收 `Value`）
- [x] **P3.6** 验证：SDK 双 target ✅；两插件双 target ✅；宿主 plugin 25/25 + config 15/15 ✅

## P4 — 宿主侧代码划分

- [x] **P4.1** `host_functions.rs`（1540 行）→ `host_functions/` 目录：`mod.rs`（注册组装）+ `memory.rs`（内存协议辅助，宿主侧唯一一份）+ 11 个按域子模块（storage/database/terminal/session/events/http/log/fs/config/bus/lifecycle），与 SDK `host/*` 一一对应
- [x] **P4.2** 统一 `check_permission` 守卫，替换约 30 处 check/log 复制粘贴；`host_terminal_send` 改用 `PERMISSION_TERMINAL_INPUT` 常量（最后一个权限魔法字符串）
- [x] **P4.3** `wasm_host.rs`（宿主侧）删除：SQL 校验/列转换 → `host_functions/database.rs`（主库/插件库查询逻辑合并为 `query_to_json`，消除两份重复）；HTTP 代理/SSE → `host_functions/http.rs`（消除与 SDK `wasm_host.rs` 的撞名）
- [x] **P4.4** `PluginServices` trait（定义于 `wasm_runtime`，实现于 `host::PluginHost`）：`WasmHostContext` 改持 `Arc<RwLock<Option<Arc<dyn PluginServices>>>>`，`wasm_runtime` 模块不再 import `host::PluginHost`，模块依赖单向化
- [x] **P4.5** 验证：check 0 错误；plugin 25/25 + config 15/15 ✅

## P5 — 插件 DX 公共件

- [ ] **P5.1** SQL 参数绑定：新增 host function `host_plugin_db_execute_params` / `host_plugin_db_query_params`（rusqlite 真绑定），abi 签名表同步更新
- [ ] **P5.2** SDK `sql.rs` params 辅助；auto-task 迁移（消灭 30+ 处手写 `replace('\'', "''")`）
- [ ] **P5.3** SDK `http_response.rs`（ok/ok_with_data/error）、`args.rs`（`CommandArgs` 提取器）、`constants.rs`（`CLAUDE_CONFIG_DIR_NAME` 等共享常量，宿主 `constants/plugin.rs` 反向 re-export）
- [ ] **P5.4** `test_plugin` 移出 SDK → 独立 crate `packages/plugin-test/`，更新 `wasm_runtime` 测试构建路径
- [ ] **P5.5** ai-chatbox 迁移（sql params、http_response、constants）
- [ ] **P5.6** 验证

## P6 — ABI 演进能力与运行时优化

- [x] **P6.1** `wasm_entry!` 生成 `__bedcode_abi_version` 导出（= `abi::ABI_VERSION`，当前 v2）；宿主 instantiate 时校验插件要求版本不超过宿主支持版本，超过则拒绝加载并明确提示升级；缺失导出视为 v1 遗留插件兼容加载
- [x] **P6.2** HTTP 代理：`HTTP_CLIENT`（连接超时 10s + 总超时 120s）/ `HTTP_STREAM_CLIENT`（仅连接超时，SSE 长连接不截断）两个全局 `LazyLock<reqwest::Client>` 复用连接池；超时常量入 `constants/plugin.rs`
- [x] **P6.3** `MessageBus::publish` 改 `Handle::try_current().spawn()` 异步投递，消除原"host function → block_on_async → dispatch 内再 block_on_async"的嵌套阻塞
- [x] **P6.4** WASM 线性内存双向回收（`ABI_VERSION` 因此升到 v2）：
  - `__bedcode_allocate` 改为 `std::alloc` 精确 Layout 分配（与回收配对，避免 Vec 容量不确定的 UB）
  - 新增 `__bedcode_deallocate` 导出；旧插件无此导出时宿主自动退化为 v1 不回收行为
  - 宿主侧：所有 host function 参数读取改 `read_wasm_string_consume`（读完即回收 guest 内存）
  - 插件侧：`WasmHost` 所有结果读取经 `read_and_free_result`（拷贝为 Rust String 后立即归还）
- [x] **P6.5** SDK `Cargo.toml` 清理：删除未使用的 `chrono` / `async-trait` / `tokio`（trait 均为手写 `Pin<Box<dyn Future>>`，无运行时依赖）
- [x] **P6.6** 验证：SDK 双 target ✅；宿主 check 0 错误 ✅；plugin 测试 25/25 ✅（ABI 签名表现在覆盖 33 个函数，含 abi_version/deallocate；连通性测试实际执行了双向回收路径）；两插件双 target 构建 ✅；全量测试仅余 3 个预存失败（jwt/session_output，与重构无关）

---

## 进度日志

- **2026-07-27** 完成全量代码审阅（SDK 10 文件 / 宿主 plugin 模块 13 文件 / auto-task 5 文件 / ai-chatbox 结构）。
  发现并确认：plugin.token 死逻辑、WASM 侧 session/broadcast 权限缺口、TaskQueueChanged 广播丢失、
  auto-task 命令 ID 与 manifest 不一致（前端按 registry 全名调用 → 当前命令面板触发 auto-task 命令必失败）。
- **2026-07-27** P1 启动。
- **2026-07-27** **P1 完成**。验证：plugin 模块 25/25 测试通过（含两个 ABI 自检）、config 15/15、
  全量 133/136、SDK 5/5、auto-task 双 target 构建、移动端编译通过。

  P1 过程中修复了 4 个**阻塞整个测试套件**的预存基础设施问题（此前 `cargo test` 在桌面端完全无法运行）：
  1. **Windows 测试二进制启动即 0xc0000139**：`tauri-plugin-dialog → rfd` 导入 comctl32 v6 的
     `TaskDialogIndirect`，测试二进制无 SxS 清单声明。修复：`build.rs` 通过 `/MANIFEST:EMBED` +
     `/MANIFESTINPUT` 合并标准 comctl32 v6 清单片段（`/MANIFESTDEPENDENCY` 带空格会被 cargo 拆参
     触发 LNK1181，无空格写法产生非法 XML 触发 SxS 14001，均不可用）。
  2. **`build_test_wasm` 构建失败**：新版 cargo 的 `--manifest-path` 不再接受目录，必须指向
     `Cargo.toml` 文件。
  3. **测试插件无 .wasm 产物**：SDK crate 缺 `crate-type = ["lib", "cdylib"]`，rlib 不产出 wasm。
  4. **tao 事件循环禁止在测试线程创建**（worker 线程 panic）：插件宿主 `WasmRuntime` /
     `WasmHostContext` / `FsAuthChecker` 的 `app_handle` 改为 `Option<Arc<AppHandle>>`，
     测试走无头构建（`None`），生产传 `Some`。emit/数据目录类能力在无头上下文优雅降级。
     顺带移除了未使用的 `tauri/test` dev-dependency。

  预存测试债务（与本次重构无关，文件未被修改）：
  - `utils::auth::jwt::tests::test_token_expiry`（jwt.rs:253 断言失败）
  - `session::session_output::tests::test_on_output_caches_to_pending_when_inactive`（期望 0 得 3）
  - `session::session_output::tests::test_activate_uses_current_max_seq`
- **2026-07-27** **P2 完成**。SDK `host/` 11 个功能 trait + `HostError` + `ConfigKey` + `HostApi` 聚合；
  `WasmHost` 无状态化；两插件 + test_plugin 全量迁移到 `Result<_, HostError>` 签名。
  关键验证：ABI 签名测试（export 表 + Linker 注册表）在 trait 化前后均通过 —— 接口重构未触碰线协议。
- **2026-07-27** **P3 完成**。跨进程载荷全部类型化：`SyncEvent`（广播）/ `SessionLifecycleEvent`（生命周期）/
  `PluginQuestion` 进 SDK，两端共享。宿主 `host_broadcast_sync` 的字符串 match 删除，改为 serde + 穷尽 `From` 转换 ——
  新增事件类型时编译器强制双端同步（TaskQueueChanged 静默丢失类问题结构性绝迹）。
  `invoke_command` 参数、`on_message`、`on_session_lifecycle` 全部类型化，JSON↔类型转换收敛进 `wasm_entry!` 宏。
- **2026-07-27** **P4 完成**。1540 行的 `host_functions.rs` 拆为 13 个按域文件（与 SDK `host/*` 一一对应）；
  宿主侧 `wasm_host.rs` 撞名文件删除，逻辑并入 database/http 域；`PluginServices` trait 使模块依赖单向化；
  统一 `check_permission` 守卫。插件测试耗时 4.4s → 0.6s。
- **2026-07-27** **P5 完成**。SQL 参数绑定（ABI v2 新增 4 个 `*_params` host functions，rusqlite 真绑定）；
  auto-task 约 30 处手写 `replace('\'', "''")` 转义全部消除；`sql_params!` / `CommandArgs` /
  `http_response` / 共享 constants 入 SDK；test-plugin 独立为 `packages/plugin-test` crate。
- **2026-07-27** **P6 完成，全部六个阶段收官**。ABI 版本协商（`__bedcode_abi_version`）上线；
  线性内存双向回收（`__bedcode_deallocate`，旧插件自动退化）；HTTP Client 复用 + 超时；
  消息总线 spawn 化。**遗留事项**（非本次范围）：① 3 个预存测试失败（`jwt::test_token_expiry`、
  `session_output` ×2）；② 移动端队列 UI 消费 `ws_sync_task_queue_changed` 事件；
  ③ `emit_event` / `bus_*` 尚无权限门控（P1 仅补齐 session/broadcast）；
  ④ 主库 SQL 前缀校验为正则方案，长期可考虑 AST 解析。
