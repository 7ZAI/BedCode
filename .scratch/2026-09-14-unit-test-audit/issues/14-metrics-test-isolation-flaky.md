# 14 — 修复 MetricsCollector 测试隔离缺陷（flaky）

**What to build:** `MetricsCollector` 添加测试可用的 `new()` 构造器，metrics 测试改用独立实例而非全局单例，消除与 `link_crypto` 测试的跨模块计数器污染。

**Blocked by:** 无

**Status:** done（2026-09-15）

- [ ] `MetricsCollector` 添加 `pub fn new() -> Self`（生产代码保持 `global()` 不变）
- [ ] `MetricsInner` 实现 `Default` trait
- [ ] `metrics.rs` 测试改用 `MetricsCollector::new()` 构造独立实例，移除对 `global()` 的依赖
- [ ] 移除 `reset_and_sample_report_totals_and_sliding_window_rates` 测试开头的 `collector.reset()` 与 `sleep(50ms)`（独立实例无需 reset）
- [ ] `cargo test --lib server::` 完整套件连续跑 5 次，metrics 测试 0 失败
- [ ] 保留原有断言强度（`encrypted_frames` / `decrypt_failures` / 速率窗口等）

## 证据

实测 `cargo test --lib server::` 即暴露（2026-09-14 复核连跑 3 次，第 2/3 次失败）：

```
assertion `left == right` failed
  left: 5    （第 3 次为 3）
 right: 2
```

`encrypted_frames` 期望 2（测试内 `inc_encrypted_frame()` ×2），实际 5/3（多出的来自 `link_crypto` 经真实链路调 `MetricsCollector::global()`：`inc_encrypted_frame` 调用点 :720/:781/:812、`inc_decrypt_failure` :725/:786/:823、`inc_response_key_miss` :807——由 `http_full_request_response_cycle_through_filter` 等 lib 内直接走 `on_http_inbound/outbound` 的测试触发）。

单独 `cargo test --lib server::metrics::tests::` 全绿——证明是并行调度下的跨模块污染，非逻辑缺陷。

`MetricsCollector::global()` 为 `LazyLock<MetricsCollector>` 全局单例（`metrics.rs:94-95`），无 `new()` 构造器。测试注释已承认此问题（"全部断言收敛在单个测试函数内，避免并行测试互相污染计数器"），但 `reset()` 缓解不足——`link_crypto` 测试通过真实代码路径在 `reset()` 与断言之间写入同一全局。

## 根因

全局单例 + 无构造器隔离 + `reset()` 时序窗口 + 并行测试调度。`link_crypto` 的 24 个测试调用 `MetricsCollector::global().inc_encrypted_frame()` 是正确生产行为（metrics 应记录加密帧），但在 lib test binary 内跨模块共享同一实例。

## 修复方向

1. `impl MetricsCollector { pub fn new() -> Self { Self { inner: Arc::new(MetricsInner::default()) } } }`（或内联构造）
2. `MetricsInner` 实现 `Default` trait
3. metrics 测试首行改 `let collector = MetricsCollector::new();`，删除 `reset()` 与 `sleep`
4. 生产代码路径（`global()`）零改动

## 影响面

零生产代码行为改动（仅新增 `new()` 构造器 + `Default` 实现）。测试隔离后，metrics 测试不再依赖并行调度，CI 稳定性提升。

## Comments

- 2026-09-14 审计发现（第二轮），见 `../http-ws-spec.md` §5.1
- 第一轮审计（票据 08-13）未捕获此 flaky，因为首轮未完整跑 `cargo test --lib server::` 套件
- 关联票据 15（系统性治理）
