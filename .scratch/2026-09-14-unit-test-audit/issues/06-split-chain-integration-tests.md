# 06 — pty_session_chain 集成测试拆分为独立可隔离场景

**What to build:** `tests/pty_session_chain.rs` 用 1 个 `#[tokio::test]` 串 5 个子场景、零隔离。前序场景失败/超时会导致后序场景永不执行，回归锁（尤其场景 5 的 restart 回归）长期处于「没人知道它在不在」的状态。拆成 5 个独立 `#[tokio::test]`，共享 fixture 用惰性初始化 + 静态锁串行化。

**Blocked by:** 无

**Status:** done（2026-09-15 修复）

- [ ] 提取共享 fixture：`TEST_ENV`（lazy `OnceLock<Mutex<Option<Env>>>`）持有 `AppContext` + 服务器 handle + 任务 + 端口
- [ ] 全局静态锁串行化：`WebSocketManager::global()`、`AppContext`（OnceLock）、`WsSessionRegistry` 跨测试共享，禁止并行起停服务器（沿用 `.scratch/desktop-integration-tests/spec.md` 的教训）
- [ ] 5 个独立测试：创建会话 / 订阅+echo 往返 / 停止+状态一致 / 未认证拒绝 / restart 后输出链路可用
- [ ] 跨场景依赖显式化：场景 3、5 依赖场景 1 的 `session_id`，需各自独立创建会话（或经 fixture 惰性提供，失败信息指明前置）
- [ ] 缩短超时预算：实跑 0.34s，当前 20s × 2 过于宽松，建议 5s（失败更快暴露）
- [ ] `cargo test --test pty_session_chain` 全绿，且各测试单独可跑

## 证据

当前结构：`pty_session_chain_flow` 单函数内含 5 段 `// ==================== 场景 N：... ====================` 注释块。

变异测试中实测到的失败表现：场景 2 失败时 `panicked at tests/pty_session_chain.rs:495:5`，测试终止，**场景 3/4/5 从未执行**。

超时与实跑严重不对称：

```
cargo test --test pty_session_chain  → finished in 0.34s（连跑 3 次 0.34 / 0.33 / 0.34）
而场景 2 与场景 5 各挂 Duration::from_secs(20) 预算
```

## 为什么是问题

1. **回归锁被遮蔽**：场景 5 守着 commit 19c0c3d30（restart 丢输出管理器注册 → 桌面打开终端窗口空白，`SESSION_NOT_FOUND`）。若场景 2 因环境抖动常红，场景 5 的守卫能力无人知晓，直到它自己出问题才暴露——而那时问题会被误归因到场景 2。
2. **失败定位成本高**：panic 堆栈只指向 495 行附近，需要人工对照场景注释判断断在哪一段。
3. **CI 时长被最坏情况锁定**：任一场景失败即消耗 20s（可 ×2）。

## 注意事项

- 拆分后每个测试都要独立 `init_test_app_context()`。`AppContext` 是 `OnceLock`，重复 init 会静默跳过（当前代码已处理：`try_init` 幂等），但要确认第二个测试拿到的 config_id 仍是有效的（`sessions` 表为 `:memory:`，跨测试进程不共享 —— 每个测试进程独立，同进程内共享）。
- 若两个测试都需 `StartSession`，各自的 `session_id` 独立，`config_id` 相同（`SessionManager::new` 内建 config）。
- 服务器实例：要么共享一个（fixture 惰性启动一次），要么各自启停。共享更省，但必须串行锁。
- 收尾 `handle.stop(true)` 只在最后一个测试执行，或在 fixture drop 时统一做。

## Comments

- 2026-09-14 审计发现，见 `../spec.md` §5.1
