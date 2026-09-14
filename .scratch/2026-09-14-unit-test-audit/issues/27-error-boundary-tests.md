# 27 — error_boundary.rs 零测试：panic 捕获基础设施

**What to build:** 为 `spawn_with_error_boundary` 添加单元测试，验证 panic 被捕获且不终止进程。

**背景:** `error_boundary.rs`（43 行）是所有 `tokio::spawn` 任务的 panic 防护网，零测试。该函数应捕获 future 中的 panic 并输出错误日志，防止后台任务静默崩溃。

**参考:** `.scratch/unit-test-audit/system-spec.md` §3

**测试清单:**

- [ ] `spawn_with_error_boundary("task-a", async { panic!("boom") })` → 不 panic，日志输出
- [ ] 正常 future（无 panic）→ 任务正常完成
- [ ] panic 消息为 `&str` → 被正确捕获和转换
- [ ] panic 消息为 `String` → 被正确捕获和转换
- [ ] 非 panic 的 panic（downcast 失败路径）→ "Unknown panic"

**Status:** done（2026-09-15）
