# 10: 可观测面闭环——monitor 消费端 + config 运行时覆盖层

**What to build:** 让 core-monitor 从「只写不读」变成能回答问题：内核指标经一条 Tauri 命令出口，前端诊断页（`/server` 诊断入口或插件管理页）消费并展示；`set_config` 的运行时覆盖层要么接上入口，要么删掉这一层。修完 platform-kernel 清单里的「运行时」面才算有反馈回路。

**Blocked by:** 无

**Status:** ready-for-agent

## 现状（已复核）

- 生产端全接线：燃料（`component.rs:956`）、内存增长（`wasm_runtime.rs:326`）、调用耗时/trap/激活（`component.rs:1019,1030-1068`）、授权计数（`framework.rs:156`）、总线丢弃（`bus.rs:251,268`），全原子零日志，热路径克制达标（AGENTS §8 合规）；
- 消费端为零：`MetricsRegistry::snapshot` 无任何生产调用者（无 Tauri command；前端 `lib.rs:690` 的 `get_server_metrics` 与内核指标无关）→ 票据 03（`.scratch/wasm-core/issues/03-core-monitor.md`）承诺的「前端诊断页统一数据源」未兑现；
- 注册表条目在插件卸载后不回收（量小，但要判：是设计还是漏）；
- `WasmRuntime::set_config`（`wasm_runtime.rs:730-736`）生产零调用 → core-config 的「配置文件 + 运行时覆盖」两层中的后者是装饰；配置文件亦无 watcher，改文件必须重启。

## 验收

- [ ] 新增只读命令（命名遵 AGENTS §6：`get_*`）导出内核指标快照，形状稳定并在文档里钉字段清单；插件未加载时的降级行为明确
- [ ] 前端诊断消费面落地（界面维持既有风格，`frontend-styles` 强制）：至少展示每插件 内存峰值/燃料消耗/调用耗时与次数/trap 次数/授权拒绝数/总线丢弃数
- [ ] 上一轮登记但未做的 US11 吞吐维度（`.scratch/wasm-core/audit-2026-09-15.md` §6-4：总线消息数/字节数/topic 分布）在本票一并决定是否补，补则遵「热路径克制」
- [ ] 指标条目回收：插件卸载后清理，或明确「常驻且量小」并注释理由
- [ ] `set_config` 二选一：接上入口（含校验与「只影响新建 Store」的可见提示）或删除该层并把 core-config 文档改口径为「编译期默认 + 配置文件」；禁止保留无人调用的分层
- [ ] 单测纪律：快照形状断言 + 计数递增断言（含并发用例），禁止无断言的 smoke 测试
- [ ] 门禁：`cargo test` + `pnpm run test:run`（`--pool=forks`）+ `pnpm exec eslint .` 0 error + i18n zh-CN/en 同步

## Comments

- 2026-09-21 立项：来源 spec §2（`monitor.rs`、`config.rs` 两行）。
