# 05: 协商套件参数化

**What to build:** 把 WS 链路加密的协商从「宿主硬编码套件（X25519 + 挑战应答，v 固定 1）」改为「套件可指定」：插件可经配置/互调选择加密套件（算法名），宿主执行协商与帧加密。线协议增量演进（expand–contract）：旧字段（未携带套件名）按默认套件继续工作，移动端旧端不断流；新字段并存一版窗口后再退役旧字段。数据面加解密仍留宿主的过滤链（WASM 边界/性能约束不变），下沉的是「套件选择」这个低频控制面。

**Blocked by:** 02、04 + P1-b land

**Status:** done（2026-09-24 落地，最小忠实范围）

## 落成形态
- 线协议 expand：`CryptoProposal` 增可选 `suite: Option<String>`（`#[serde(default, skip_serializing_if)]`）——
  旧客户端（移动端）不携带 → None → 默认套件（不断流，AGENTS §9 增量演进）；移动端 TS
  `parseCryptoEcho` 只读 `{v,ek}`，回执套件为 None 时被 skip，wire 形状不变，零移动端改动。
- 套件引擎化：`link_crypto.rs` 新增 `LinkSuite` + `DEFAULT_LINK_SUITE` + `resolve_link_suite`——
  名称寻址、未知套件名 fail-visible 拒绝（绝不静默降级到默认，协商是安全面）；三算法名
  都经 crypto 引擎注册表白名单校验（套件与引擎词汇不漂移，单测断言）。
- 协商接线：`channel/terminal.rs` + `channel/event.rs` 先 `resolve_link_suite` 再握手；
  审计日志 `suite = <名>`（只记套件名，不落密钥材料）。
- 数据面不变：帧加解密仍在宿主链路，算法选择只发生在协商控制面（grep 断言：
  `resolve_*`/`resolve_link_suite` 调用点 = 协商 + host-crypto 原语，无数据面热路径）。

## 门禁
宿主 `cargo test --lib` 1181/0（含新增 4 项：auth suite 往返/缺省/未知拒绝 + 默认套件三算法
注册表断言 + 缺省/默认名/未知名三方覆盖）；link_crypto 35/0；eslint 未动前端。
移动端零改动（字段可选 + 回执 skip）。

- [x] 协商入口收套件名参数（算法名），按名从注册表选套件；未携带时走默认套件（存量兼容）
- [x] 线协议增量演进：`suite` 缺省 → 默认套件，移动端旧版不携带仍能按默认连接（集成断言：crypto_proposal_suite_optional_expand + resolve_link_suite_default_when_absent_or_default_name）
- [x] 数据面加密仍在宿主链路；grep 断言算法选择不落在数据面热路径（`resolve_link_suite` 只在协商点）
- [x] 审计：协商选中套件名记 `suite =` 日志，不落密钥材料