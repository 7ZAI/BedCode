# 02 — monitor 去环（task 指标快照注册制）

**Type:** task
**Blocked by:** None — can start immediately.
**Status:** done（2026-09-24 执行完成；提交见 dev 票 02 commit，工作区红态系并发 session-events 在途，与票 02 无关）

**What to build:** 解除 `monitor.rs::MetricsRegistry::snapshot()` 对 `manager::task::task_metrics_snapshot` 的内联直接调用（隐藏环：host_api/fs → monitor → manager → host_api）。把 task 指标段从「monitor 主动 import manager」反转为「manager 自注册快照源」。

- **方案**：`monitor.rs` 定义 `MetricsSource` trait（`fn snapshot(&self) -> serde_json::Value`）与注册口（`register_source("task", Box<dyn MetricsSource>)` 或等价的 `snapshot_sources` 表）；`MetricsRegistry` 全量快照时遍历已注册源合并输出。
- **`manager::task` 侧**：把 `task_metrics_snapshot` 包装为 `MetricsSource` 实现，在 runtime 初始化或首次访问时注册（注册点必须幂等，重复注册覆盖不叠加）。
- **行为零变化**：`snapshot()` 的 JSON 输出结构必须与现状逐字节一致——`{"plugins": {...}, "task": {...}}` 两段都要在；消费方（bus/status 命令面/前一阶段 code-map 描述）不感知变更。
- 若注册点存在时序问题（monitor 快照在 task 注册前被调用得到空 task 段），需处理为「快照先于注册返回 `task` 段缺失的确定性降级」而非 panic——与插件未激活时 status 自愈语义同类，fail-visible 但可恢复。

**验收：**

- [x] `rg "manager::task::task_metrics_snapshot" monitor.rs` 零命中（monitor.rs 不再 import manager::*；仅剩余 doc-link 注释引用，条款允）
- [x] `snapshot()` 输出结构与现状逐字节一致（`snapshot_byte_shape_matches_legacy_plugins_plus_task_layout` 对照测试固化 `{"plugins":…,"task":…}` 形状；task 段枚举字段 `jobsSubmittedTotal/jobsRejectedTotal/unitsCompletedTotal/concurrentUnitsPeak/eventsDroppedTotal` 不变，另有 task.rs 既有 `metrics_snapshot_shape_is_stable` 双锁）
- [x] `cargo test` 过滤到 monitor/status/task 相关用例满绿（monitor 17/17 + metrics 4/4 + status 12/12；全量 lib 编译通过）

## 执行记录（2026-09-24）

- 方案落地：`MetricsSource` trait（monitor.rs）+ `MetricsRegistry::register_source`（幂等覆盖）+ `snapshot()` 遍历已注册源合并；`manager/task.rs` 新增 `TaskMetricsSource` 包装 + `register_task_metrics_source`；注册点 = `WasmRuntime::with_config`（runtime 初始化，晚于 monitor 创建、早于任何快照消费方，无降级窗口）
- 时序处理：裸 `MetricsRegistry::new()` 未注册源时 `task` 段缺失而非 panic（确定性降级，`unregistered_source_segment_absent_without_panic` 锁）
- 验证窗口（20:0X，工作区一致性窗口内）：cargo test --lib monitor 17/17（含 4 个新测试）、metrics 4/4、status 12/12、engine_limits e2e 通过；rustfmt --check monitor.rs 干净
- 并发备注：验证后 票 01（async 基础设施中立化）与 session-events 在途改动持续落地（events/matcher.rs E0046 等），随后工作区编红——与票 02 无关；票 02 代码仅含自身文件（monitor.rs 全文 + task.rs 两处 hunk + runtime.rs 一处 hunk），以 HEAD+票02 状态单独编译自洽（不依赖票 01）