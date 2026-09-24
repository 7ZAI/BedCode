# 01: crypto/ 引擎壳 + 算法注册表（prefactor）

**What to build:** 把现有散落的加密实现聚合为一个宿主引擎级 `crypto/` 模块，使算法可以被「名称」统一寻址调用。核心是一个注册表（算法名 → 实现）加白名单词汇表，一种统一抽象 trait（AEAD / 非对称 / KDF / 混合各一），以及统一的错误类型。`utils/crypto/` 的既有算法实现保留不动，注册表引用它们。效果：任何插件/内部服务经名字（如 `aes-256-gcm`、`x25519`、`hkdf-sha256`）拿到实现做加解密，未知算法名显式拒绝而非静默失败。

**Blocked by:** None（本票只新建 `crypto/` 引擎壳，不与 P1-b 在途改动面重叠，可立即开始）

**Status:** done（2026-09-24 落地；cargo test 全量 1148 绿，crypto:: 60 项含 link_crypto 存量；rustfmt 干净）

- [x] `crypto/` 引擎模块含注册表（名称→实现）、白名单词汇表、抽象 trait 与统一错误类型；`utils/crypto/` 算法实现被注册表引用而非复刻
- [x] 按名调度可用：`resolve_aead("aes-256-gcm")` 命中 AES-GCM，未知算法名报错（fail-visible，不留静默回退）
- [x] 白名单是引擎级词汇表；最小子集落地（aes-256-gcm / chacha20-poly1305 / hkdf-sha256 / x25519；rsa / hybrid 留待按需扩展，见 spec O5）
- [x] 单元测试：每族至少一遍按名往返；拒绝未知名；错误类型带操作上下文（非裸字符串）