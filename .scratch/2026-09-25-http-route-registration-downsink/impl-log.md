# HTTP 路由代码注册下沉专项 — 实施日志

> 状态：实施中 ｜ 起始：2026-09-25 ｜ 测试节奏：先单测、后集测，集测一次跑（spec §6，用户裁定 ⑧）

## 阶段进度

- [x] 阶段 1：WIT host-http 服务端域 + SDK 封装 + 宿实现 + 动态路由注册表（命名空间 key + 冲突仲裁 + 停用回收）+ 插件名唯一性激活校验 + ABI v29
- [x] 阶段 2：网关收口（删静态表改查动态表）+ 模板匹配引擎 + 测试重构
- [x] 阶段 3：认证整合（jwt_gateway 公开判定走声明面 + auth_center 角色发现 + WS 认证对齐）
- [x] 阶段 4：terminal-session 迁移（代码注册 + sessions REST + terminal-bg）+ session_controller 删除 + manifest 静态面退役 + 契约锁
- [ ] 阶段 5：全量回归（desktop cargo test 含集成 target / 插件 / SDK / vitest / eslint）+ ADR 0022 修订记录 + CHANGELOG（进行中）

## 关键设计决策（实施中确认）

1. **动态注册表位置**：`server/http/registry.rs`（新模块，镜像 `server/websocket/endpoint.rs` 形态），
   key = `(owner, host_path, method)`；内部端点路径 `/api/plugin/<owner>/<path>` 单独索引。
2. **权限门**：`network:http`（前端面 `http.registerEndpoint` 已同权限位，Rust 侧一致）。
3. **HTTP 端点上限**：新增 `PLUGIN_HTTP_MAX_ENDPOINTS_PER_PLUGIN = 64`（terminal-session 迁移后 42 条）。
4. **未激活宿主别名 → 404**（不再 200+1007「plugin not activated」）：停用回收后别名即消失，
   `/api/configs` 未注册 → 路由表 404。行为差异如实记录（spec §4 影响面未列，补记）。
   `/api/plugin/*` 面的「未激活 200+1007」保留（路由常驻、handler 应答）。
5. **模板段**：注册 host 允许 `{name}` 段（名称 `[a-zA-Z_]\w*`），捕获值经路由匹配传给插件
   `params` 字段；宿主不拿捕获值构造任何路径（防注入）。
6. **sessions REST 内部端点**：内部 path 用「动作名」形态（`sessions/stop` 等，不带 `{id}`），
   模板只出现在 host 路径；插件按内部 path 分派 + 读 `params.id`。mobile 只走 host 别名，内部
   面不需模板可达。
7. **verify_abi 版本检查只拒「新于宿主」**：旧产物（v28）在 v29 宿主上仍可实例化（接口函数
   新增不破坏 import 实现）；ABI bump 是 SDK 契约版本声明，stale hint 文案补 v29 反向判据
   （新产物跑旧宿主 → 点名「升级 BedCode」）。
8. **terminal-bg（待决已决）**：保留宿主读文件 + 注册表门控——背景图二进制不可经 host-fs
   读取（`fs:read` 返回 String），新增字节通道成本过高；URL 归属/生命周期/档位由插件注册
   声明，宿主按注册表门控应答（未注册/属主未激活 → 404）。
9. **H1（host_impl/http.rs 权限门）**：已存在（http_fetch 早有 check_permission），本次新增
   register/unregister 同样过门，待决默认纳入即完成。
10. **manager registry http_endpoints 表整表退役**：manifest 静态声明面删除后表无生产消费方
    （前端从未消费 toolProviders），连同 register_tool_providers 一并移除；`plugin.json` 的
    `contributes.httpEndpoints` 字段保留在 SDK 类型（老 manifest 解析容差）。

## 验证记录

- 阶段 1–4 针对性单测全绿：registry 11 / host_api::http 14 / gateway 12 / plugin_controller 10 /
  jwt_auth 9 / server::http 52 / activation 4 / system_component_test 6 / SDK 146 / 插件 329 /
  http_e2e 1 / 桌面 lib 全量 919（pty:exit 曾因负载时序抖动一次，复跑稳定）。
- 阶段 5 全量回归：
  - 桌面 lib 920/1（唯一失败 = p3_async_host_import 在途测试，非本次代码；编译期曾因该
    agent WIT 面未落地编红，其同步后恢复）
  - 集成 target 10/10 全绿（broadcast_shutdown / build_manifest_smoke / http_auth_biometric /
    link_crypto_http / pty_session_chain / server_integration / ws_auth_rules）——pty_session_chain
    验证 sessions REST 新链路（网关模板 → 插件 sessions_http）真实闭环
  - 前端 vitest 807/807 全绿（83 文件；全量跑 OOM 两次系机器内存压力——p3 agent 跑飞测试
    占 100% CPU + 并发，NODE_OPTIONS=8192 + 空闲后稳定通过，已知 flake）
  - eslint 0 error（120 warning 不计入门禁）
  - fmt：我改的叶子文件逐个 rustfmt（HEAD-clean 判定），host.rs / runtime.rs / lib.rs /
    task/mod.rs 因声明外部子模块不整文件 fmt（lib/task 的既有漂移与我的新增混存，留痕）
  - 打包产物重建：resources/plugins/desktop/com.bedcode.terminal-session（旧 manifest 无
    network:http 导致集成测试注册失败，rebuild 后全绿）
- 并发观测：p3_async_host_import（另一 agent 在途，wasip3 host-api 优化专项）曾使桌面 lib
  test target 编红（WIT 面未落地）+ 其测试运行占 100% CPU（疑似跑飞，未触碰他人进程）；
  Cargo.toml 的 profile.test.wasmtime 改动亦属其专项，非本次。

## 阶段 5 待办

- [x] 全量回归（上述全绿）
- [x] ADR 0022 修订记录（v29）+ CHANGELOG + code-map 更新
- [ ] 提交（dev 分支；需先确认与他人文件的边界）
