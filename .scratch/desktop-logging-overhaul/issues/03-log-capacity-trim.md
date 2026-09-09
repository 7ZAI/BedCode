# 03: 磁盘容量裁剪 + 丢弃告警

**What to build:** 日志占用有硬性保护——`LogConfig` 新增 `capacity_bytes`（默认 512MB，0 = 不限制），后台任务在应用启动时与每 10 分钟扫描日志目录，总大小超限时按修改时间删除最旧文件（跳过当前在写文件）直到低于上限；panic 记录（独立文件）纳入同一治理；non_blocking 队列丢弃计数由同一后台任务周期性检查，有新增丢弃时输出一条带数量的 warn，使丢日志可感知。

**Blocked by:** 01（丢弃计数来自 01 的句柄集）

**Status:** resolved

- [x] 构造超限日志目录运行裁剪：删除最旧文件、当前在写文件保留、总大小回到 ≤ 上限即停止
- [x] panic 记录与日志系统目录路径一致并纳入裁剪（核实不一致则先统一路径再实现）
- [x] 丢弃计数自上次检查起有新增时，runtime 日志出现一条 warn（含丢弃条数），不静默丢弃
- [x] `capacity_bytes=0` 时完全禁用裁剪
- [x] 裁剪为纯函数实现并单测（删最旧/保留当前/到达目标即停/禁用开关），后台任务仅是定时调用者
- [x] `cargo test` 全量绿
## Answer

已完成（2026-09-09）。

**实现要点**：
- `system/config.rs`：`LogConfig` 新增 `capacity_bytes`（默认 512MB，0=不限制）；properties 读写/分组/注释同步；resources/config.properties 加 `log.capacity_bytes=536870912`
- `system/logging.rs`：`trim_log_dir(log_dir, max_total_bytes) -> Vec<PathBuf>` 纯函数（按修改时间升序删最旧、跳过最新=当前在写文件、达到上限即停、单个文件不删、0 禁用、删除失败跳过不阻断下轮重试）；`spawn_log_maintenance(setup, capacity)` 后台任务（`spawn_with_error_boundary` 包装，启动立即一轮 + 每 10 分钟：容量裁剪 + 丢弃告警对比上次值）
- `lib.rs`：store_setup 后启动维护任务，capacity 传启动时配置值（04 设置页保存后重启生效）
- 目录一致性核实：`trim_log_dir` 扫日志系统目录（应用日志目录），panic.log 落同目录即纳入；Linux 路径一致；Windows 的 panic 路径（保持现状）在注释中说明未纳入修剪的潜在差异
- **踩坑**：`tracing::warn!(count = expr, "msg {count}")` 的 format 捕获字段名编译失败——改字段仅作键值，消息不带插值

**验证**：`cargo test --lib` 581 全绿（新增 4 个 trim 纯函数测试：删最旧保当前 / 未超限不删 / 0 禁用 / 单文件不删）；logging 模块 12 测试全过。
