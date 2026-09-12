# 03: core-monitor：指标注册表与运行时埋点

**What to build:** 内核有了可观测性：指标注册表支持计数器/仪表/直方图；每个插件 Store 的线性内存当前值与峰值、每次导出调用的燃料消耗与耗时聚合、插件生命周期事件（加载/激活/停用/降级/trap）计数可查；一个快照导出 API 一次性返回全量指标 JSON。热路径埋点为纯原子操作、零日志；状态迁移级事件走 tracing 结构化字段（plugin_id 用 key=%value）。监控 sink 可扩展（当前内存快照，未来可接文件/远程）。

**Blocked by:** 01（五模块骨架归位）

**Status:** resolved

- [x] 内存记账：Store 内存增长后当前值/峰值查询正确（单测模拟增长序列断言）
- [x] 调用聚合：N 次导出调用后次数/总燃料/耗时可查
- [x] 生命周期事件计数：加载/激活/trap 各 +1 且带最近时间戳
- [x] 快照导出形状稳定（JSON 含插件维度标签）
- [x] 热路径无日志调用（代码审查 + 测试无 log 依赖）
- [x] `cargo test` 全绿

## Comments

- 2026-09-13 完成：core-monitor 落地——MetricsRegistry（plugin_id 维度，全原子零锁热路径）+ PluginMetrics（内存当前值/峰值经 ResourceLimiter 回调记账；导出调用次数/燃料/耗时直方图经 exports() 续费点与 RAII CallTimer；生命周期六事件含最近时间戳）；快照导出 `{plugins:{id:{...}}}` JSON 形状有测试锁定。埋点接线：exports() 燃料记账、12 个导出方法计时、log_trap→Trap、instantiate→Instantiate、activate 成败、deactivate、host 阶段 3 Degraded。副产物：consume_fuel=false 时 set_fuel/get_fuel 链路整体跳过（配置可关燃料看门狗）。CallTimer 持 Arc<PluginMetrics>（owning）解决 track_call 借用冲突。cargo test 634 全绿。
