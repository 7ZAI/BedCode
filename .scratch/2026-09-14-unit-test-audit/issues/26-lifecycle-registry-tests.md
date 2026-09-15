# 26 — lifecycle.rs 零测试：优先级排序 / 超时保护 / 窗口关闭阻止

**What to build:** 为 `LifecycleRegistry` 添加单元测试，覆盖异步钩子注册、优先级排序、超时保护、窗口关闭阻止逻辑。

**背景:** `lifecycle.rs`（327 行）是整个应用优雅关闭的核心基础设施，零测试。关键逻辑：
- `on_startup/on_shutdown/on_window_close_requested` 注册后按 `priority` 升序排序
- `run_startup_hooks/run_shutdown_hooks` 每个钩子独立超时（`SHUTDOWN_HOOK_TIMEOUT_SECS`），超时后继续
- `run_window_close_hooks` 任一 hook 返回 `false` 即阻止窗口关闭

**参考:** `.scratch/unit-test-audit/system-spec.md` §3

**测试清单:**

- [ ] 注册多个不同 priority 的钩子 → 执行顺序为升序
- [ ] 相同 priority 的钩子注册后顺序（当前 `sort_by_key` 保持插入顺序，不保证稳定 — 需验证并决定是否需要明确语义）
- [ ] 钩子超时时 → 超时日志输出，后续钩子继续执行
- [ ] 钩子正常完成 → 无超时日志
- [ ] `run_window_close_hooks` 所有 hook 返回 true → 返回 true
- [ ] `run_window_close_hooks` 某个 hook 返回 false → 返回 false，后续 hook 不再执行
- [ ] 无 hook 注册时 → `run_window_close_hooks` 返回 true

**Status:** done（2026-09-15）
