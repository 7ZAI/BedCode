# 05: JWT 密钥治理（A3）

**What to build:** 宿主 JWT 密钥由硬编码常量改为 secret-store 托管：首启随机生成 + 持久化；既有签发/验签调用面不变，行为零回归。

**Blocked by:** 04

**Status:** ready-for-agent

- [ ] 首启随机生成密钥并持久化；重启后密钥稳定（同一 token 重启前后验签均通过）
- [ ] 既有 jwt 单测全绿（无行为回退）
- [ ] 密钥明文不出宿主、不进日志（只记长度）；失败路径带操作上下文（AppError）
