# 05 — 移动端 TS 加密核心与 pin 存储

**What to build:** bedcode-mobile 引入 `@noble/curves`（x25519）+ `@noble/ciphers`（AES-GCM）+ noble-hashes（sha256/hkdf），新建纯 TS 加密模块（建议 `src/services/linkCrypto.ts` 或 `src/utils/`，零 Vue 依赖、可独立 vitest）：一次性密钥对生成、ECDH、HKDF 方向分离派生（info/salt/AAD 常量与桌面 01 严格一致）、HTTP 信封编解码、WS 二进制帧头编解码、seq 收发计数器管理。pin 存储扩展：在既有连接凭据旁持久化 `kdPublicB64` / `kdFingerprint`（useMobileConnection 凭据域，重新配对/重认证时经 03 字段刷新）。互操作金样：消费 02/04 产出的 Rust 固定向量，TS 实现交叉解码验证，防两端实现漂移。

选型依据（spec「移动端实现选型」）：WS 是 WebView 原生 WebSocket 只能在 JS 侧处理；否决 Rust invoke 方案（每帧 IPC 延迟 + WS 帧不经 Rust）。

**Blocked by:** -（与 01 并行）

**Status:** ready-for-agent

- [ ] wrapper 单测（vitest 真算）：roundtrip、AAD 篡改拒绝、nonce 随机性、seq 管理、方向密钥隔离
- [ ] 金样向量：TS 解码 Rust 生成的 HTTP 信封与 WS 帧成功，TS 加密的载荷可被 Rust 向量流程验证
- [ ] pin 随凭据持久化，重启 App 不丢；重新配对可刷新
- [ ] `npm run test:run` 全绿；vue-tsc 干净
