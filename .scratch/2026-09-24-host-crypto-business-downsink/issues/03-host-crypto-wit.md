# 03: host-crypto WIT 契约（interface + 权限位 + ABI + SDK 五同步点）

**What to build:** 定义插件按名调用宿主加密的契约面：WIT 新增 `host-crypto` interface，函数按「算法名 + 参数」调度（AEAD 加密/解密、X25519 ECDH、HKDF 派生、RSA 加解密/签名验签、混合信封封装/解封）。同步新增权限位 `crypto:aead` / `crypto:asym` / `crypto:kdf`（按风险域拆分，对齐 `ws:client`/`ws:server` 先例）。ABI bump（desktop v25→v26）。SDK 五同步点全落：权限词汇唯一真源 → 生成物 JSON / 前端合法集 → 宿主能力清单 → host_impl 权限门 → manifest 构建映射表。桌面独有，按 ADR 0018 双端偏离登记，移动端不跟演。

**Blocked by:** 01

**Status:** done（2026-09-24 落地；四插件重建全绿）

- [x] WIT `host-crypto` interface 定义，函数签名含算法名参数（枚举/字符串白名单），密钥材料由调用方（插件）提供
- [x] 权限位 `crypto:aead` / `crypto:asym` / `crypto:kdf` 入 SDK 唯一真源，五同步点全落、词汇漂移锁不翻红
- [x] ABI v25→v26 同步 abi.rs / WIT / CHANGELOG（v26 注释登记，desktop 独有偏离） / CHANGELOG / AGENTS §7 / WIT；桌面专属按 ADR 0018 双端偏离登记
- [x] SDK 契约测试：合法算法名通过（129 项全绿 + 宿主 1162 全绿）、白名单外算法名构造失败（fail-visible）