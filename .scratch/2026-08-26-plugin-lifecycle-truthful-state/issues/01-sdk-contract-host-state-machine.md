# 01 — SDK 契约补全 + 宿主状态机：Degraded 终态端到端

**What to build:** 插件启动初始化失败时，宿主不再静默标记 Activated，而是如实进入 Degraded 终态并落日志。打通「WIT 契约携带结果 → SDK 骨架上抛 → 宿主状态机解释」整条链路（后端半程）。

设计依据见同目录 `../spec.md` §3.1–3.3、§3.5。

**Blocked by:** None — can start immediately.

**Status:** done（2026-08-26 审计确认，全部 checklist 已落地）

- [x] WIT lifecycle 接口：`on-startup` / `on-shutdown` 携带 `result<_, string>`（activate/deactivate 不动）
- [x] SDK 宏生成的 lifecycle 骨架不再吞错：用户实现的 Err 上抛为导出错误 + 规范日志；`WasmPlugin` trait 用户面方法签名不变（插件源码零改动升级）
- [x] 静态注册 trait 的 `on_startup`/`on_shutdown` 返回类型改为 `Result`，inventory entry 函数指针类型同步；默认实现 Ok
- [x] 绑定层区分「guest 报告的失败」与「调用本身失败」，两者都可被宿主分别识别
- [x] `PluginState` 新增 `Degraded(String)` 与 `Activating`（serde tag 形状与既有变体一致）
- [x] 宿主激活流程：activate 成功但启动初始化失败 → 置 Degraded；扩展点注册（命令/视图/订阅/api 清单）照常完成；每插件输出终态日志行
- [x] Degraded 可重试激活回到 Activated；对 Degraded 执行停用干净回落 Deactivated；`is_activated()` 门禁语义保持严格 Activated
- [x] 汇总日志按真实状态分计数（activated/degraded/error），Degraded 不计入 activated
- [x] 持久化语义不变：仍存用户意图（bool），auto-activation 失败不回写 false、下次启动重试
- [x] 测试插件工程提供「on_startup 必然失败」用例开关
- [x] workspace `cargo test` 全绿；4 个内置插件随构建链重编通过（预计零源码改动）

## 实现记录（2026-08-26 审计补录）

- **SDK**：WIT `on-startup`/`on-shutdown` 携带 `result<_, string>`；ABI_VERSION v7→v8；`wasm_entry!` 四个 lifecycle 导出结果如实上抛 + HostLog 规范日志；`BedcodePlugin::on_startup/on_shutdown` 返回 `anyhow::Result<()>`、Entry fn pointer 同步、默认实现 Ok
- **绑定层**：component.rs `on_startup/on_shutdown` 双层 `Result`（外层=调用故障，内层=guest 自报失败），宿主分别处理
- **宿主状态机**：activate_plugin 置 Activating 中间态 → on_startup 自报失败记 startup_failure → phase 3 写 Degraded 且扩展点注册照常完成；panic 落 Error（Store 已污染）；Degraded 可重试激活；停用干净回落；汇总日志分 activated/degraded/error 计数；get_activated_state 对 Degraded 记 true（意图≠健康快照）
- **测试**：host.rs 新增 v8 e2e ×2（on_startup 失败→Degraded→移除开关重试→Activated→停用回落；persist=true 时 Degraded 以 true 落库）；组件测试插件提供 `component-test-fail-startup` storage 开关（wasm 产物已随 v8 重编）。spec §4 所列「SDK 宏单测 Err 传播」由宿主侧真实组件 e2e 覆盖（真实 wasmtime 加载路径，强于原生胶水代码单测），不另建
- **验证（2026-08-26 复核）**：desktop lib 522 绿 / SDK crate 74 绿 / 集成测试 ws_auth_rules、http_auth_biometric、ws_session_route、pty_session_chain 绿；broadcast_shutdown 失败为已归档火绒环境干扰（服务端日志证实 verify 请求从未到达，见 `.scratch/peer-network/issues/03-mdns-discovery-and-capabilities.md`），与本票无关；4 内置插件产物均为 v8（2026-08-25 22:39–22:42 构建，源码无更新），dev 实测 ai-chatbox/auto-task/scheduler 加载激活正常
