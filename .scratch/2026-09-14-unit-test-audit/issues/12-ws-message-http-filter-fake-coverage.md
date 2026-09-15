# 12 — ws/message.rs 补 Terminal 变体 + http_filter.rs 出站拒绝与 /ws 快速路径

**What to build:** 两处虚假覆盖修复：(a) `ws/message.rs:1175` `message_type_mapping_covers_all_variants` 名为"covers all variants"但漏掉 `Terminal` 变体；(b) `ws/message.rs:1346` `json_round_trip_preserves_all_variants` 每类枚举只覆盖 1 个代表变体；(c) `http_filter.rs:183-189` 出站拒绝分支（`run_outbound` 返回 Err → 500 响应）无任何测试覆盖；(d) `http_filter.rs:98` `/ws` 前缀快速路径从未被测试。

**Blocked by:** 无

**Status:** done（2026-09-15）

- [ ] 修改 `test_message_type_mapping_covers_all_variants`：向 `cases` vec 补一条 `(Message::input("s", "x", None), "terminal")`
- [ ] 修改 `test_json_round_trip_preserves_all_variants`：为每个 `SyncPayload` / `SessionControlAction` / `TerminalAction` 变体补一条 round-trip
- [ ] 新增 `test_http_filter_outbound_rejection_returns_500`：注册 `on_outbound` 返回 `Reject` 的过滤器，断言响应 500 + body 含 "reject" + 未泄露下游 handler 的实际 body
- [ ] 新增 `test_http_filter_ws_prefix_fast_path_skips_filtering`：`/ws/terminal` 请求走快速路径，不缓冲、不走过滤器链
- [ ] 新增 `test_http_filter_head_method_skips_filtering`：HEAD 方法走快速路径
- [ ] `cargo test --lib server::ws::message::` + `cargo test --lib server::middleware::http_filter::` 通过

## 证据

- `ws/message.rs:1175` `message_type_mapping_covers_all_variants` 名为"covers all variants"但漏掉 `Terminal` 变体——测试名暗示"逐变体验证"是虚假覆盖
- `ws/message.rs:1346` `json_round_trip_preserves_all_variants` 每类枚举只覆盖 1 个代表变体：`AuthStage` 7 变体仅测 `Authenticated`（漏 `Reauthenticate`/`ExchangeCertificate` 等 6 个）、`SyncPayload` 10 变体仅测 `TaskQueueChanged`（漏 `SessionStopped` 等 9 个）；`TerminalAction` 5 变体经 `input`/`subscribe`/`unsubscribe` 包装已全覆盖（无 `Output` 变体）
- `http_filter.rs:79` `/ws` 前缀快速路径从未被测试——WebSocket 升级请求会走完整缓冲+过滤器链，导致连接失败或性能退化
- `http_filter.rs:183-189` 出站拒绝分支（`run_outbound` 返回 Err → 500 响应）无任何测试覆盖——加密链路可能静默降级为把未加密响应写回客户端

## 根因

1. `ws/message.rs` 两个测试名为"covers all variants"但实际漏测关键变体，是虚假覆盖
2. `http_filter.rs` 出站拒绝分支与 `/ws` 快速路径是核心安全与性能路径，但零测试

## 修复方向

1. 补 `Terminal` 变体到 `message_type_mapping` 测试
2. 为每个枚举变体补 round-trip 测试
3. 补出站拒绝分支测试（500 + 未泄露下游 body）
4. 补 `/ws` 前缀快速路径测试

## 影响面

修复后，WS 协议线格式漂移与加密链路降级风险有回归网。当前这些路径任何回归 CI 都抓不到。

## Comments

- 2026-09-14 审计发现，见 `../http-ws-spec.md` §5.2 与 §5.6
- 出站拒绝分支是加密链路的核心安全路径，零测试是高危
