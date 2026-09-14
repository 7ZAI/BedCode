# 09 — 前端 useLinkEncryption 收缩 + 删 linkCrypto.ts + @noble 依赖 + encryptHttp 推送

**What to build:** `src/composables/useLinkEncryption.ts` 收缩为「开关/状态查询 + set_link_crypto_context 推送」（删除前端加解密）；删除 `src/services/linkCrypto.ts` 与 @noble 依赖（D2：信封进 Rust）；`syncLinkCryptoContextToNative` 同步补推 `encryptHttp`（ticket 04 的 Rust 字段）；pin 刷新收束——auth 响应带 kdPublicB64 由 Rust 代理解析（ticket 03），前端 localStorage 保留作设置页展示，裁决在 Rust。

**Spec:** §2 现状、§4 前端段、§9 D2、handoff §3.4/§3.5

**Blocked by:** 03, 04

**Status:** done

## 关键实现事实（handoff §2/§3 已核实）

- 现状 `useLinkEncryption.ts`：localStorage 存设置（`link-encryption`）与 pin（`link_kd_public_b64`/`link_kd_fingerprint`）；`syncLinkCryptoContextToNative` 经 `set_link_crypto_context` 推 Rust，**当前只推 enabled/strictMode/encryptWsEvent/kdPublicB64——缺 encryptHttp**；pin 刷新事件 `ws_link_crypto_pin`（Rust→前端）；applyPin 校验 32 字节。
- `linkCrypto.ts` 删除前已由 ticket 01 字节级对齐（derive_http_keys 金样单测锁住）；删除即验收 D2。
- @noble 依赖位置：`bedcode-mobile/package.json`（@noble/curves 等）；删除后 `pnpm install` 更新锁文件。
- HTTP 加密判定（Rust 侧）= `enabled ∧ encrypt_http ∧ kd_public_b64.is_some()`（对齐前端 `isChannelEncryptionActive('http')`）。

## 实现清单

- [x] `useLinkEncryption.ts` 删加解密逻辑，收缩为开关/状态查询 + `set_link_crypto_context` 推送
- [x] `syncLinkCryptoContextToNative` 补推 `encryptHttp`（与 Rust 字段名对齐）
- [x] 删除 `services/linkCrypto.ts` 的 HTTP 部分；移除 @noble 依赖（package.json + 锁文件）——**范围修正见 Comments**
- [x] pin 展示路径保留（localStorage），裁决在 Rust；`ws_link_crypto_pin` 事件处理保持
- [x] 检查其他引用 linkCrypto.ts / @noble 的代码点（含插件 shared runtime 如有）
- [x] 单测适配：useLinkEncryption 状态/推送逻辑不回归

## 验证

- vitest 全绿；全仓 `rg "linkCrypto|@noble"` 无残留（除历史文档）
- 桌面端解密回归依赖 ticket 01 金样 + 真机联调（验收 3）

## Comments

- 2026-09-11 完成（**D2 范围修正**）：spec §7 边界「终端 WS 本分支不搬迁」→ `useTerminalSocket.ts` 仍依赖 linkCrypto.ts 的 WS 加解密（deriveWsSession/generateEphemeral/parseCryptoEcho/WsSessionCrypto）——**文件与 @noble 依赖保留**，删除的是 **HTTP 部分**（deriveHttpKeys/encryptRequest/decryptResponse/HttpTrafficKeys/SealedHttpRequest/wirePath/httpAad + HTTP 常量），字节级对齐已由 ticket 01 金样 + http_proxy_flow 覆盖。文件头注释更新为 WS-only。
- `useLinkEncryption.ts`：本无加解密（只有状态/pin），收缩点=删除死代码 `notePinFromAuthData`（HTTP 通道 pin 刷新已收束 Rust：http_proxy 解析 auth 响应 → `update_link_crypto_pin` + emit `ws_link_crypto_pin` → 前端 initLinkCryptoPinSync 落地 localStorage 作设置页展示）；`syncLinkCryptoContextToNative` 补推 `encryptHttp`。
- 测试：useLinkEncryption.test.ts 重写——补 sync 推送参数断言（enabled/strictMode/encryptWsEvent/encryptHttp/kdPublicB64）；notePinFromAuthData describe 改为经 `initLinkCryptoPinSync` + mock listen 事件驱动 applyPin（handler 收 `{ payload }` 事件形状）。linkCrypto.test.ts 删 HTTP describe（注：该文件在 `src/services/` 不在 vitest include `src/__tests__/**`，属孤儿测试，保留 WS 部分保持可编译）。
- 验证：vitest 362 ✓、vue-tsc exit 0、eslint 0 error；残留扫描干净（仅 1 处注释提及 deriveHttpKeys）。
