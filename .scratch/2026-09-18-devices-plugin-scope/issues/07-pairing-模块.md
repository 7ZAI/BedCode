# 07: pairing 模块（B2）

**What to build:** 配对码 / QR token / JWT 签发校验策略从宿主语义层平移至认证中心插件（宿主保留密码学引擎与密钥托管），与宿主实现**行为等价**（对照测试：同一输入同输出）。

**Blocked by:** 06, 05（插件骨架 + JWT 密钥治理链路完整）

**Status:** ready-for-agent

- [ ] 对照测试：配对码 / QR token / JWT 签发校验与宿主实现同一输入同输出
- [ ] pairing 策略单测：TTL 边界、一次性语义（正反例）
- [ ] HS256 官方 test vector（RFC 7515）通过
- [ ] 密钥经 host-auth secret-store 获取，明文不出宿主、不进日志
- [ ] 插件 cargo test 全绿
