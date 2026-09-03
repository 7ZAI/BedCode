# 06 — 移动端 useHttpApi 加密接线与降级策略

**What to build:** `useHttpApi.ts` 的 `request()` 包装加密判定链：已 pin Kd 且本端 `trafficEncryption.enabled` 与对应通道子开关开 → 请求注入 `X-BedCode-Crypto: v1 <ek_b64>` 头 + body 信封化（每请求全新临时密钥对）；响应带标记头 → 用 k_resp 解信封还原 `ApiResult`。降级策略（遵循 spec §6 配置语义，strictMode **默认关**）：预期加密而收到明文响应 → strictMode 开启时断连报「连接被拒：加密协商失败」；strictMode 关闭（默认）→ 明文续跑 + 「连接未加密」提示态（不自动清 pin——pin 只随重新配对刷新，防主动降级攻击抹除信任锚）。`/api/auth/*` 引导端点保持明文直发（pinning 建立即刻）。新增文案 i18n 同步 zh-CN / en。

**Blocked by:** 02, 03, 05

**Status:** ready-for-agent

- [ ] vitest（mock fetch）：开关开→断言请求头与 body 已按信封格式加密、响应正确解密为 ApiResult；开关关→全明文直发（默认态回归）
- [ ] 明文响应 + strictMode=true → 断连错误路径；strictMode=false（默认）→ 未加密提示态
- [ ] `/api/auth/*` 端点不加密直发（既有配对流程测试回归绿）
- [ ] 协商失败不清除 pin（单测断言凭据不变）
- [ ] i18n key zh-CN / en 成对出现
- [ ] `npm run test:run` 全绿；vue-tsc 干净

## Comments

- 2026-08-26 实现：request() 信封化 + 协商头注入 + 响应解密/downgrade 分支（strict 断连返回 LINK_ENCRYPTION_DOWNGRADE 错误码，UI 文案映射归 issue 08）；pin 经 notePinFromAuthData 在响应拦截层自动刷新，配对调用方零改动。
