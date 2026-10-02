# fail-visible：收到加密协商头但本端未参与 ⇒ 显性 4xx（不再把密文喂给业务解析）

**日期**：2026-10-02
**来源**：`.scratch/2026-10-01-cross-end-coverage-gaps` 票 02 落地过程中暴露；用户裁决走
「方案 B（只做 fail-visible，不改 opt-in 语义）」。
**状态**：resolved · 2026-10-02

## 1. 问题

跨端实测（`http_proxy_flow` 首轮）：移动端开启加密、桌面端未开启时，

```
POST /api/sessions/start  →  HTTP 200,  {"code":1002,"message":"configId required"}
```

密文被当 JSON 喂进插件，插件按业务错误回「缺 configId」——**传输层"成功"了**。GET 更隐蔽：
无请求体要解、响应也无加密标记，移动端默认非 strict 模式静默按明文续跑。

## 2. 根因：原始 spec 自身矛盾

`.scratch/http-ws-payload-encryption/spec.md` §6（`f1286e90b` 版）三处互相打架：

| 位置 | 表述 |
|---|---|
| §6 决策模型 3「响应绑定」 | 请求一旦带协商头，响应**必须**用 k_resp 回加密，**不取决于服务端子开关当前值** |
| §6 实现语义段 | 主开关关闭 → **过滤器不注册**（空链零开销快速路径） |
| §7 兼容矩阵前提 | **双方均已开启**为前提；**任一侧默认关即为明文** |

「不注册」⇒ 链上没人读协商头 ⇒ 「按线上信号自动识别」（§6 第 2 条）无从谈起。实现落在
「不注册」这一支，于是桌面端开关关 = 完全看不见信号。

**用户记忆中的设计**（「任一端开启就通知对方参与」）对应的是 §6 第 2、3 条——信号本身就是通知。
但 §7 把「任一侧关」等同于明文，等于否掉了它。**这份 spec 需要一次正本清源**（见 §6）。

## 3. 本次落点（方案 B）

不动 opt-in 语义（不动「关 ⇒ 不注册」），只把**静默错误显性化**：

- `bedcode-server-core/src/filter.rs`：新增 `TrafficFilterChain::contains(name)`（无分配，
  锁中毒退化为「不在链上」——与 `is_empty` 的中毒退化方向**刻意相反**：那条放行明文（可解读），
  这条要放行的是无法解读的密文）。
- `bedcode-server-http/src/middleware/http_filter.rs`：协商头提到快速路径之前；新增分支
  `!skip_filtering && !negotiation.is_empty() && !contains(link-crypto)` → `400` +
  `CODE_INVALID_REQUEST` + 点名 `trafficEncryption` 与两端各自修法的消息。
  与过滤器 Reject 分支**同一错误信封**，客户端按 code 归类一致。

**GET 一并拒**：否则「GET 静默明文通、POST 神秘报业务错」的不一致更难排查。

**HEAD / `/ws` 不在裁决范围**（`skip_filtering`）：HEAD 响应无 body 可加密，`/ws` 帧级加密已退役
（`TrafficChannel::WsPlugin => false`），保持现状。HEAD 若带协商头仍是既有盲区，无现实产生者。

**环回对端的同类形态有意不修**：`link_crypto::is_exempt` 对环回对端硬豁免（「本地调用永不加密」
是硬约束不是配置项）。过滤器在链上 + 环回对端 + 带协商头 ⇒ 同样会掉进业务解析，但环回侧
没有任何现实产生者（桌面 WebView / hook 脚本都不走移动端代理面），故不在本次范围，记此备案。

## 4. 行为变更（诚实声明）

本次**收紧**了一个既有行为：桌面端关 + 移动端开时，POST 原先「200 + 垃圾业务码」、GET 原先
「静默明文通过」，现在**都显性 4xx**。这是有意的取舍：不可解读的密文静默通过，比显性拒绝更贵。
移动端默认 `strictMode=false` 的「降级提示」在这条路径上不会触发（响应压根没到解密那一步），
用户看到的是带处置建议的 400。

## 5. 验证

- `bedcode-server-http` 53 + 4 绿（含 3 条新单测：未注册时拒绝 / 在链上时不误伤 / 默认态明文不误伤）
- `bedcode-server-core` 41 绿；宿主 `cargo test` **884 lib 绿 / 0 红** + 全部集成 target
- `cross-end-tests` **11/11 绿**（新增契约 P-006）
- **变异自检**：把新分支条件改成 `false` → P-006 精确打红，且失败信息复现原症状
  `got 200 body={"code":1002,"message":"configId required"}`（证明这条断言真的咬住了旧行为）
- 两个 crate `cargo fmt --check` clean；根 eslint 0 error / 117 warning（同基线）

## 6. 未采纳 / 仍待裁决

- **方案 A**（按 §6 第 3 条改语义：桌面端开关关也按协商头参与）**未采纳**。理由：它要推翻
  「默认关闭 = 零开销 / 不注册」这个 opt-in 基线，且需先回答「环回豁免与空链快路径还留不留」。
  若日后要采纳，建议先正本清源 §6/§7 的矛盾（开 ADR）。
- **§6/§7 的 spec 矛盾本身尚未正本清源**：本次只让**行为**fail-visible，文档矛盾仍在
  `link_crypto.rs` / `http_filter.rs` 的注释里显式记着（代码读得到，不靠口头约定）。
- peer-net / 生物认证 / QR 扫码仍属范围外。
