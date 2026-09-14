# 01 — link-crypto crate 补 `derive_http_keys`（X25519 + HKDF，字节级对齐前端 JS 金样）

**What to build:** 在 `packages/link-crypto`（Rust crate，仓库根，桌面端已使用）新增 `derive_http_keys`：输入本端静态 X25519 密钥 + 远端公钥（eph 或 kd）+ wirePath，派生 HTTP request/response 两向 AES-GCM 密钥。**必须与前端 `linkCrypto.ts` 的 `deriveHttpKeys` 逐字节一致**（前端实现删除前的对齐金样，见 spec §5.1 / §9 D2）。

**Spec:** §5.1（信封格式逐字节一致）、§9 D2（本分支删 linkCrypto.ts，信封进 Rust）

**Blocked by:**

**Status:** done

## 关键实现事实（handoff §2，已核实）

- crate 已有：`http_aad` / `encrypt_http_body` / `decrypt_http_body` / `HttpEnvelope` / `parse_negotiation` / `generate_ephemeral`（ws.rs）/ `Direction`。**缺 `derive_http_keys`**。
- 前端 JS 语义（`bedcode-mobile/src/services/linkCrypto.ts`，删除前须字节级对齐）：
  - 密钥派生 = `HKDF-SHA256(salt = wirePath(path) 剥 query, info = "bedcode-link-crypto/v1/http/request|response", 32B)`
  - 信封 `{v:1, n:<b64 12B nonce>, ct:<b64 ct||tag>}`；AAD = `"v1" || dir(1B, req=0x01/resp=0x02) || u32be(pathLen) || path`
  - 协商头 `X-BedCode-Crypto: v1 <eph_pub_b64>`
- **wirePath 剥 query**：wirePath 即请求 path，剥掉 `?` 之后的 query 后再作 salt——对齐前端实现。

## 实现清单

- [x] `packages/link-crypto` 新增 `derive_http_keys`（X25519 共享密钥 + HKDF-SHA256，request/response 两向 key 返回）
- [x] 提供与前端金样对拍的字节级单测（固定密钥向量 → 断言 HKDF 输出、AAD 拼接、信封 JSON 结构、nonce 顺序）
- [x] `cargo test`（packages/link-crypto 及依赖它的两端 crate 不破坏）

## 验证

- `cd packages/link-crypto && cargo test` 全绿；固定向量断言（非随机密钥）证明字节对齐
- 桌面端宿主 `cargo test` 不回归（crate 被两端共用，改动是纯新增）

## Comments

- 2026-09-11 完成：`lib.rs` 新增 `HttpTrafficKeys` + `derive_http_keys` + `wire_path`（剥 query）；`x25519_dh` 上移 `pub(crate)` 供 ws.rs 复用（DRY）。金样向量由真实前端 linkCrypto.ts（@noble）经临时 vitest 脚本生成（已删）：priv=0x01..=0x20、kd=0x21..=0x40、path=/api/sessions → request=`0dad8fe9…54092`、response=`42047000…1daac`；带 query path 派生一致（wirePath 对齐）。验证：link-crypto 5 测试全绿；移动宿主 cargo check + cargo test 全绿（251 passed）。
