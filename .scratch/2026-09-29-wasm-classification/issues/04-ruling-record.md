# 04: 两项待裁定的落定记录（已裁定，本票转为裁定记录）

**Status:** done（2026-09-29 用户裁定「按建议」）

## 张力 2：L1 基础服务的能力声明方式

| 选项 | 做法 | 评估 |
| --- | --- | --- |
| **2a（建议）** | 基础服务在 manifest 声明自己提供哪些 host-* 同形能力，宿主按声明装配路由（`capability.rs:96 ROUTABLE_CAPABILITIES` 从硬编码表改为注册表驱动） | ✅ **裁定采纳** |
| 2b | 沿用硬编码白名单 | 每加一个基础服务要改宿主代码 + 发版，违背可扩展初衷 |

现状：`ROUTABLE_CAPABILITIES` **只有 `host-storage` 一项**；`auth-policy` 是
**仅探测不路由**（`capability.rs:108`：注册为路由提供者会让任意插件接管认证策略，语义错误）。

## 张力 4：认证中心是 L2，能否暂留 terminal-session

| 选项 | 做法 | 成本 |
| --- | --- | --- |
| **4a（概念最纯）** | 拆成独立 `com.bedcode.auth-center` L2 应用，配对/QR/生物/JWT/策略/信任/记录六域迁出 | ⏳ **留作后续专项**（本批不拆） |
| **4b（建议）** | 认证中心暂留 terminal-session，兼任 L2 角色；L2 允许「一个插件兼任 L3+L2」 | ✅ **裁定采纳** |

**4b 落地的实现细节（新增，非选项）**：L2 身份须**静态声明 + 动态就绪两段**——
manifest 静态声明（决定加载顺序，进步骤 4 第二批）+ activate 内
`auth-center-register` 动态就绪（唯一性仲裁）。**不合并**：只有静态声明则无仲裁，
只有动态注册则加载顺序退化成时序巧合。

**已作废的中间方案**：给 terminal-session 加 `"type": "system"`（L1 基础服务）——
`system` 定义是「提供 http/mdns/pty 同形能力」，而 terminal-session 是**消费者**
（`dependencies: ["host-pty"]`），打该标签等于把消费者标成提供者，语义错误。
