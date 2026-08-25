# 01 — 桌面端链路加密密钥基础设施与过滤器骨架

**What to build:** 桌面端生成并持久化链路加密静态身份密钥 Kd（X25519；指纹 = SHA-256(Kd_pub) 前 16 hex，重启不变），提供 link-crypto 协议模块骨架（HKDF 方向分离派生函数、HTTP 信封与 WS 帧的类型定义、AAD 构造——算法组合层，不放算法实现，原语复用 `utils/crypto/`）。落地加密过滤器骨架注册进 `TrafficFilterChain::global()`：入口最先执行本地豁免三分支（① channel == WsLocal；② peer 解析为 loopback SocketAddr；③ route 命中 `/api/auth/*`、`/health` 明文白名单），豁免外的流量本期暂直通（真实加解密由 02/04 填充）。落地 **`trafficEncryption` 配置域**（spec §6）：`enabled`（bool，**默认 false**）+ `encryptHttp` / `encryptWsTerminal` / `encryptWsEvent`（bool，默认 true）+ `allowPlaintextFallback`（bool，默认 true），持久化于既有设置存储；提供 get/set Tauri 命令，set 即热更新进程内配置快照（`Arc<RwLock>` 或等价物）。宿主启动时按快照决定是否注册：**默认配置（enabled=false）不注册，行为与现状完全一致**；开启后常驻注册、子开关在过滤入口逐流量判定；关闭即 unregister（空链零开销路径），均无需重启服务。

参考：spec「密钥体系」「本地豁免规则」节；`server/filter.rs` 只消费不改 trait；新模块建议 `src-tauri/src/server/link_crypto.rs`；Kd 持久化选型（SQLite settings 或配置文件）在本 ticket 定并与既有密钥类存储机制对齐。

**Blocked by:** -

**Status:** ready-for-agent

- [ ] Kd 首次启动生成、重启后不变；指纹稳定且可通过命令查询
- [ ] 豁免单测三分支全绿（WsLocal / loopback / 白名单路由），非环回非白名单流量正常进入链
- [ ] 默认配置（enabled=false）首次启动链为空，全服务流量行为与现状一致（回归既有 server 测试）
- [ ] enabled=true 后过滤器出现在 `list_names()`；再置 false 链恢复空，HTTP 零开销快速路径生效
- [ ] 配置域 get/set 命令可用：set 后快照热更新即时生效（无需重启）；子通道开关逐通道判定有单测
- [ ] 非法值（未知字段/类型不符）被拒绝且原配置保留
- [ ] HKDF 派生单测：同输入结果确定、方向 info 隔离（k_req ≠ k_resp）、salt 参与
- [ ] `cargo test --lib` 全绿；公开项有文档注释；错误走 AppError 不裸字符串
