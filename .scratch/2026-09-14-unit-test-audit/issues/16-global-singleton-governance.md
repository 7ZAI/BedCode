# 16 — 全局单例测试模式系统性治理

**What to build:** 审计所有 `LazyLock` 全局单例的测试隔离风险，建立「测试用 `new()` 独立实例、生产用 `global()`」的规范，预防 metrics 类 flaky 重演。

**Blocked by:** 14（metrics 隔离修复是首个实例）

**Status:** done（2026-09-15 修复）

- [ ] 审计 `src/server/` 全部 `LazyLock` 全局单例，评估跨模块污染风险
- [ ] `TrafficFilterChain::global()`：当前仅 `http_filter.rs` 测试使用 + 生产代码，确认无其他模块并发写入
- [ ] 其他全局单例（`GlobalOutputManager::global()` 等跨模块共享的）评估
- [ ] 为存在跨模块调用方（生产代码在多个模块调用 `global()`）的单例添加 `new()` 构造器
- [ ] 建立规范：全局单例必须有 `new()` 构造器供测试隔离，`global()` 仅供生产
- [ ] `port_checker.rs`（2 测试）与 `client_info.rs`（1 测试）守卫力变异验证
- [ ] `cargo test --lib server::` 完整套件连续 5 次无 flaky

## 证据

### 全局单例清单

`grep -rn "LazyLock" src/server/`：
- `MetricsCollector::global()`（`metrics.rs:94-95`）— **已暴露 flaky**（票据 14）
- `TrafficFilterChain::global()`（定义于 `src/server/filter.rs:175-176`；`http_filter.rs` 为使用方）— 潜在风险，当前未暴露

### MetricsCollector::global() 跨模块调用方

| 模块 | 调用次数 | 调用方法 |
|---|---|---|
| `link_crypto.rs` | 7 | `inc_encrypted_frame()` ×3（:720/:781/:812）/ `inc_decrypt_failure()` ×3（:725/:786/:823）/ `inc_response_key_miss()` ×1（:807） |
| `terminal_ws.rs` | 6 | `inc_ws_sent()` ×4（:253/:269/:289/:305）/ `inc_ws_received()` ×2（:517/:525） |
| `app.rs` | 1 | `inc_http_request()` |
| `supervisor.rs` | 2 | `reset()` / `sample()` |

**污染链路**：`link_crypto` 24 测试 + `terminal_ws` 代码 → `MetricsCollector::global()` → metrics 测试断言失败。当前仅 link_crypto 测试暴露（terminal_ws 零测试，无法在测试中触发污染）。

### TrafficFilterChain::global() 风险评估

`http_filter.rs` 6 个测试均使用 `TrafficFilterChain::global()`，并有 `clear()` 清理（`http_filter.rs:369`）。当前无其他模块测试并发写入该链，故未暴露。但若未来其他模块测试也使用 `TrafficFilterChain::global()`，将面临与 metrics 相同的污染风险。

## 根因

Rust `LazyLock` 全局单例在 lib test binary 内跨模块共享同一实例。任何通过真实代码路径调用 `global()` 的测试都会污染依赖该单例的测试。这是**系统性设计问题**，非个别文件缺陷。

## 修复方向

1. **短期**（票据 14）：metrics 添加 `new()` 构造器，测试用独立实例
2. **中期**：审计其他全局单例，预防性添加 `new()` 构造器（`TrafficFilterChain` 优先）
3. **长期**：建立规范，在 AGENTS.md §6 Rust 规范中记录「全局单例必须有 `new()` 构造器」
4. **规范文档**：在 code-map 或 AGENTS.md 中明确「哪些模块走集成测试、哪些走单测」，避免边界模糊

## 影响面

短期仅影响 metrics 测试。长期治理涉及测试架构规范，不影响生产代码。

## Comments

- 2026-09-14 审计发现（第二轮），见 `../http-ws-spec.md` §6.1
- 系统性问题，非个别文件缺陷
- 与票据 14 互补：14 修复 metrics 实例，16 建立系统性规范
