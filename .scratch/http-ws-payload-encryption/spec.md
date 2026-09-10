# Spec: 局域网链路报文加密（HTTP/WS 载荷加解密）

Status: ready-for-agent

关联：`.scratch/peer-network/spec.md`（对等网络，TLS 属其域）、`docs/adr/0001`（传输栈）。本 spec 只覆盖**终端主机链路**（桌面端 Actix HTTP+WS 服务 ↔ 移动端）的报文级加解密。

## Problem Statement

桌面端服务监听 `0.0.0.0`，与移动端的全部通信（REST API、终端 WS、事件 WS）均为明文：同一 WiFi 内任何设备抓包即可看到终端输出、文件内容、JWT token 与配对码。JWT 认证只解决「谁在调用」，不解决「链路上被谁看了」。传输插件已有独立的应用层加密（peer-net），但主机链路本身裸奔。

拦截机制已就位：`server/filter.rs` 的 `TrafficFilterChain` 责任链已统一接入 HTTP 请求/响应体（`middleware/http_filter.rs`）与 WS 收发帧（`ws/terminal_ws.rs` 四个 hook），转换型过滤器可原地改写载荷——本 spec 规划在此接缝上落地真实的加解密过滤器。

## Solution

在 `TrafficFilterChain` 上注册一个 **链路加密过滤器**，为移动端↔桌面端的流量提供报文级加密：

- 密钥体系复用既有原语（`utils/crypto/`）：X25519 ECDH + HKDF-SHA256 + AES-256-GCM；协议形态对标 Noise NK（发起方持有响应方静态公钥），与 peer-net 传输加密同一套技术栈。
- 桌面端持有一把**持久化静态 X25519 身份密钥 Kd**；移动端在配对时 pin 下 Kd 公钥，之后所有会话密钥经 ECDH 临时密钥 × Kd 派生——被动窃听者拿不到任何明文，主动中间人因无法伪造 Kd 而被 pinning 校验拦下。
- **本地豁免**：环回流量（桌面 WebView 本地终端、Claude Code hook 脚本调 `/api/plugin/*`、本机任意 localhost 调用）一律不加密不过滤；只有非环回对端的流量进入加解密。
- 协商式升级：客户端声明能力头/字段才加密，老版本互通不破坏。
- **加密默认关闭（opt-in）**：开箱行为与现状完全一致；双端设置中显式开启后才参与协商，支持通道粒度子开关与 strict 防降级（同样默认关）。
- 失败即断（fail-closed）：解密失败 HTTP 返 400、WS 直接关连接，绝不静默降级为明文（沿用 peer-net fail-fast 原则）。

## Goals / Non-goals

**Goals**

1. 移动端↔桌面端的 HTTP REST、WS 终端通道、WS 事件通道三类流量全程加密。
2. 环回流量零影响：本地终端、hook 脚本、桌面 WebView 行为完全不变。
3. 默认态零惊扰：所有开关默认关，不改变任何既有部署的行为；开启后协商式升级、老版本互通不被破坏（协商失败 → 明文可用 + UI 提示）。
4. 复用 `TrafficFilterChain` 接缝与 `utils/crypto` 原语，不引入 TLS、不动监听端口。

**Non-goals**

- TLS / 证书体系 —— 属 peer-net 对等网络域（ADR 0027），终端链路继续走应用层加密
- file-transfer 插件传输流量 —— 已有独立加密层（f7ada90a），不受本 spec 影响
- 公网/中继场景、跨子网直连
- 完美前向保密的形式化证明（工程上做到临时密钥每会话/每请求全新即可）
- 桌面↔桌面流量（当前无此链路）

## 威胁模型

| 攻击者能力 | 防护 |
|---|---|
| 同网被动嗅探（抓包看终端输出/JWT/配对码） | 全载荷 AES-256-GCM；token/配对码仅在加密信封内出现 |
| 主动中间人：篡改/注入帧 | GCM 认证标签 + AAD 通道绑定，篡改即解密失败断连 |
| 主动中间人：替换密钥协商 | 移动端 pin Kd_pub；会话密钥必经 ECDH(eph, Kd)，伪造即派生不一致 |
| 降级攻击：剥离协商字段迫使明文 | 移动端 strict 模式（用户显式开启，默认关）：预期加密而收到明文 → 断连并提示；strict 关闭时降级为提示不阻断（opt-in 设计接受的残余风险，UI 提示可见） |
| 重放整条 HTTP 请求 | 与今日明文 JWT 同级（token 本身可重放），不劣化；WS 握手重放因服务端每次新鲜 s_eph 无增益 |
| 首次配对瞬间主动 MITM | 残余风险（TOFU）：QR 配对码属带外信道 + 设置页展示双方指纹供人工核对 |

前提假设：Kd 私钥与移动端 pin 存储不被本机恶意软件读取；AES-GCM nonce 管理按下文执行。

## 协议设计

版本串统一为 `bc-link-crypto/v1`。编码一律 base64（std alphabet），字段命名与既有 `HybridEnvelope` 风格一致。

### 1. 密钥体系

```
桌面端 Kd      X25519 静态身份密钥对，首次启动生成，随宿主配置持久化（ticket 01 选型：
               SQLite settings 或配置文件，与 peer-net 节点身份存储机制对齐）
               指纹 = SHA-256(Kd_pub) 前 16 hex，设置页展示供人工核对
移动端 pin     配对成功响应中获得 Kd_pub_b64，持久保存于移动端连接凭据旁
会话/请求密钥   全部临时派生，不落盘：
               shared = ECDH(临时私钥, Kd_pub)          ← 认证来源（pinning）
               k_*    = HKDF-SHA256(shared, salt, info) ← info 区分用途/方向
```

HKDF info 前缀统一 `bedcode-link-crypto/v1/<domain>/<direction>`，domain ∈ {http, ws}。方向分离保证请求/响应、收/发四个密钥两两独立。

### 2. HTTP 单发加密（无状态）

每次请求独立协商，服务端零跨请求密码状态：

1. 移动端生成一次性 X25519 密钥对，请求头携带：
   `X-BedCode-Crypto: v1 <ek_b64>`（ek = 临时公钥）
2. 请求体 = 信封 JSON（Content-Type 不变，仍 application/json）：
   `{ "v": 1, "n": "<nonce_b64 12B>", "ct": "<ciphertext_b64>" }`
3. 两端各自派生：`salt = ASCII(path)`（路径绑定防跨端点信封搬运）；
   `k_req = HKDF(shared, salt=path, info=…/http/req)`、`k_resp = HKDF(shared, salt=path, info=…/http/resp)`
4. 服务端用 Kd 私钥解请求 → handler 收到**明文** → 用 k_resp 加密响应体（同信封格式），
   响应头回 `X-BedCode-Crypto: v1` 作为标记
5. AAD = `"v1" || direction || path_len || path`；nonce 每信封随机 12B（密钥每次全新，无复用风险）

无认证端点（`/api/auth/*` 配对引导）保持明文——它们本身就是 pinning 建立即刻；`qr_connect` / `verify_pairing_code` / `reauthenticate` 的响应体新增 `kdPublicB64` + `kdFingerprint` 字段，移动端据此建立/刷新 pin。

**实现要点（请求级状态传递）**：filter 是全局单例、入站出站两次独立调用，而 k_resp 只能从该请求的 ek 派生。方案：加密过滤器内部维护并发安全短 TTL 缓存 `(peer_addr, ek_b64) → k_req/k_resp/path`，入站时写入、出站命中后移除，30s 清扫兜底。actix 已缓冲整体 body，同请求出入站间隔毫秒级，缓存命中率恒定。

### 3. WS 会话加密（有状态）

握手挂在既有首消息 JWT 认证上，不加新往返：

```json
→ {"type":"auth","token":"<JWT>","crypto":{"v":1,"ek":"<m_eph_b64>"}}
← {"type":"auth_ok", …, "crypto":{"v":1,"ek":"<s_eph_b64>"}}
```

- master IKM = `ECDH(m_eph, s_eph) || ECDH(m_eph, Kd)`（双 ECDH：前者给前向保密，后者给服务器认证）；transcript salt = `"bc-link-crypto/v1" || m_ek_b64 || s_ek_b64`
- expand 出 c2s / s2c 各一组（32B AES key + 4B nonce 随机前缀），info 含 channel 类型
- 未带 `crypto` 字段 → 维持现状明文（兼容老客户端）；带了则 `auth_ok` 必须带回 crypto，否则移动端按降级处理

数据帧加密（终端与事件通道一致，`/ws/terminal/local` 永远豁免）：

- **类型保持原则**：text 帧 ↔ text 帧 JSON 信封（`{"v":1,"n":…,"ct":…}`，控制/业务 JSON 体量小，base64 开销可接受）；binary 帧 ↔ binary 帧 `[ver u8][seq u32][ct]`（TBv2 输出流高频大帧，二进制头高效）。帧的 actix 类型不变 → `terminal_ws.rs` 分派逻辑零改动
- nonce = `prefix[0..4] || u64BE(seq)`，seq 会话内从 0 单调递增（沿 peer-net SessionCipher 惯例）；接收方严格校验 seq == expected+1（TCP 保序，违例即攻击或 bug）
- AAD = `"v1" || channel || direction || origin`
- 心跳 Ping/Pong 与 Close 是协议控制帧，不过滤链、不加密（现状如此）

### 4. 本地豁免规则（强制）

过滤器入口最先执行，任一命中直接 `Verdict::Continue` 原样放行：

1. `ctx.channel == WsLocal`（双保险，构造侧已标）
2. `ctx.peer` 解析为 SocketAddr 且是 loopback（127.0.0.1 / ::1）——覆盖 hook 脚本调 `/api/plugin/*`、本机工具直连 REST、WebView 的一切环回调用
3. `ctx.route` 命中明文白名单：`/api/auth/*`（pinning 引导期）、`/health`

效果：桌面端本地体验完全不变；加密只发生在「非环回对端」的流量上。

### 5. 失败语义（fail-closed）

| 场景 | 行为 |
|---|---|
| HTTP 入站解密失败/缺信封 | `Reject` → 400（现有通路），warn 日志含 filter 名与原因 |
| HTTP 出站加密失败 | 500（现有 Reject 语义），不返回半加密响应 |
| WS 入站帧解密失败 | **关连接 Close(4003)** 并 warn——不丢帧：TBv2 序列流丢一帧即破坏 ack 环与渲染序 |
| WS 出站加密失败 | 关连接（同理，静默丢帧不可接受） |
| 移动端 pin 存在但服务器未按约定加密 | strict 开启：断连 + UI 报「连接被拒：加密协商失败」；strict 关闭（默认）：明文续跑 + 「连接未加密」提示 |
| 日志红线 | 密钥材料、明文载荷一律不入日志；metrics 新增 encrypted_frames / decrypt_failures 计数 |

### 6. 可配置参数（开关）

**决策模型（配置如何映射到每条流量的加解密，安全边界所在）：**

1. **发送方配置决定参与意愿**：本端 `enabled` + 对应通道子开关开启、且已具备密码能力（移动端已持 pin / 桌面过滤器已注册）→ 该方向消息才加密，并携带自描述信号（HTTP 请求头 `X-BedCode-Crypto`；WS 为握手期协商，连接级全有或全无，不存在逐帧选择）。
2. **接收方按线上信号自动识别**：见合法信封即解密（信封自带 GCM 认证标签，识别错误必然解密失败而非误解）；未见信封按明文处理——但受下条约束。
3. **两条例外安全耦合**（纯「发送方说了算」不满足的部分，缺一不可）：
   - **响应绑定**：HTTP 请求一旦带协商头，响应*必须*用该请求派生的 k_resp 回加密，不取决于服务端子开关当前值（服务端开关决定是否参与，参与了必须守协议）；否则请求侧加密、响应侧明文回流，最敏感的数据（auth 响应里的 JWT、文件内容）恰好泄露。
   - **降级检测**：接收方处于「预期加密」状态（移动端已 pin 且 strictMode 开）时，收到明文不是静默接受而是断连报错——防中间人剥离协商头迫使明文（TLS 早期降级攻击同型）；strictMode 默认关时的对应行为是明文续跑 + 显式 UI 提示。

除上述三点外，是否参与完全由双端各自配置驱动，互不感知对端开关值。

加密行为全部由双端用户可配置，**所有开关默认关闭（opt-in）**：默认配置下过滤器不注册，开箱行为与现状完全一致。参数持久化于各自既有设置存储，**运行期变更即时生效、无需重启**。实现语义：主开关关闭 → 过滤器不注册/注销（空链零开销快速路径）；主开关开启 → 过滤器常驻注册，子开关经进程内配置快照（`Arc<RwLock>` 或等价物）在每次过滤入口逐流量判定，不做反复注册/注销。

**桌面端 `trafficEncryption` 配置域**：

| 参数 | 类型 / 默认 | 语义 |
|---|---|---|
| `enabled` | bool = **false** | 主开关：默认关闭=过滤器不注册；开启后整体参与加密 |
| `encryptHttp` | bool = true | HTTP REST 载荷加解密（含插件动态端点的非环回调用）；主开关开启时的通道粒度控制 |
| `encryptWsTerminal` | bool = true | WS 终端通道帧加解密 |
| `encryptWsEvent` | bool = true | WS 事件通道帧加解密 |
| `allowPlaintextFallback` | bool = true | 服务端对未协商的老客户端放行明文；false 时非环回未协商请求一律拒绝（强加密模式） |

子通道开关默认 true 是有意设计：用户只需打开主开关即获得全通道覆盖，粒度收窄是显式动作。

**移动端 `trafficEncryption` 配置域**：

| 参数 | 类型 / 默认 | 语义 |
|---|---|---|
| `enabled` | bool = **false** | 主开关：默认关闭全明文直发；关闭→开启需已建立 pin（无 pin 时引导先配对） |
| `strictMode` | bool = **false** | 用户显式开启后：预期加密而遭降级 → 断连报错；默认关 → 明文续跑 + 提示 |
| `encryptHttp` / `encryptWsTerminal` / `encryptWsEvent` | bool = true ×3 | 对称控制本端对应通道是否参与协商（关某通道即对该通道发明文请求/握手不带 crypto 字段） |

约束：
- 豁免规则（环回 / WsLocal / `/api/auth/*`、`/health` 白名单）**不受任何开关影响，恒生效**——「本地调用永不加密」是硬约束不是配置项
- 服务端子开关只作用于「是否执行加解密」，不改变协商协议本身；客户端子开关关闭的通道直接不发协商字段（等效老客户端行为）
- 配置读写提供 Tauri 命令（get/set），set 即触发快照热更新；非法值（未知字段/类型不符）拒绝并保留原值

### 7. 兼容矩阵（双方均已开启对应通道加密为前提；任一侧默认关即为明文，与现状一致）

| 桌面 \ 移动端 | 新版 | 老版 |
|---|---|---|
| 新版 | 加密 | 明文可用（按请求头/握手字段自动判定），日志 info 提示 |
| 老版 | 移动端回退明文 + UI 提示「连接未加密」；strict 下拒绝连接 | 现状 |

## 接线点映射（现状 → 改动）

| 位置 | 现状 | 本 spec 改动 |
|---|---|---|
| `server/filter.rs` | 责任链 + FilterContext{channel,direction,peer,route,data} | 不改 trait；新过滤器消费现有字段 |
| `middleware/http_filter.rs` | 空/WS/HEAD 快速路径 + 整体缓冲 + Reject→400 | 不改；加密过滤器注册后自动生效 |
| `ws/terminal_ws.rs` | text/binary 四个过滤 hook + WsLocal 标记 | 仅扩展 auth/auth_ok 消息 schema（crypto 可选字段）+ 解密失败 Close 4003 |
| `controllers/auth_controller.rs` | 配对/登录响应 DTO | 增加 kdPublicB64/kdFingerprint 字段 |
| `utils/crypto/*` | x25519/aes_gcm/kdf/hybrid 齐备 | 新增 link-crypto 协议模块（组合这些原语，不放算法实现） |
| 移动端 `useHttpApi.ts` | tauriFetch + JWT 注入 | 请求加密包装 + 信封解析 + strict 判定 |
| 移动端 `useTerminalSocket.ts` 等 | 首消息 auth + 明文帧 | 握手 crypto 字段 + 帧编解码层 |

## 移动端实现选型

推荐 **TS 侧纯 JS 实现**：`@noble/curves`（x25519）+ `@noble/ciphers`（AES-GCM）+ noble-hashes（SHA256/HKDF）——审计过的标准算法库，与 Rust 侧互操作由测试钉死；HTTP 走 tauriFetch（Rust reqwest）但加密在 JS 完成，WS 是 WebView 原生 WebSocket 只能在 JS 侧处理。备选方案（Rust command 经 invoke）被否：每帧 IPC 往返在终端输出高频场景延迟不可控，且 WS 帧根本不经 Rust。

性能依据：AES-256-GCM 在 ARMv8/x86 均硬件加速，noble 实现单帧微秒~亚毫秒级，远低于网络 RTT；HTTP 整体缓冲成本 http_filter 注释已声明可接受。

## Testing Decisions

1. **Rust 协议单测**：roundtrip、AAD 篡改拒绝、nonce/seq 管理、请求级缓存并发与 TTL、loopback 豁免三分支（沿 filter.rs 既有测试风格，GLOBAL_CHAIN_LOCK 串行化全局链）。
2. **actix 集成**：真 middleware 往返（请求加密→handler 收明文→响应加密→客户端解密），Reject→400 语义。
3. **WS 集成**：握手协商→加密 text/binary 往返→篡改帧触发 Close 4003→WsLocal/环回不加密（参考 tests/ws_auth_rules 惯例）。
4. **mobile vitest**：crypto wrapper（noble 真算）、useHttpApi 信封注入与 strict 降级分支、socket 握手状态机（mock WebSocket）。
5. **互操作金样测试**：Rust 固定向量 ↔ TS 实现交叉验证同一信封（防两端实现漂移）。
6. **真机清单**：弱网长会话、后台杀进程重连（reauth + rekey）、双端开关四象限、指纹核对流程。

验收基线遵循 AGENTS.md Done When：cargo test、npm run test:run、i18n zh-CN/en 同步、公开项文档注释。

## Out of Scope

见 Goals 一节；另加：密钥手动轮换 UI（重装/重新配对即换）、会话中途 rekey（重连即换，够用）、metrics 面板展示（只埋计数）。

## Issue 切分建议（已发布至 `issues/01~08`）

| # | 内容 | 依赖 |
|---|---|---|
| 01 | 桌面密钥基础设施：Kd 生成/持久化/指纹、link-crypto 协议模块、过滤器骨架（loopback 豁免 + 开关注册接线） | - |
| 02 | HTTP 半边：信封格式、请求级响应密钥缓存、过滤器 HTTP 实现 + actix 集成测试 | 01 |
| 03 | auth 流程下发 Kd_pub：三个端点响应扩展 + 指纹字段 | 01 |
| 04 | WS 半边：auth/auth_ok crypto 扩展、双 ECDH 派生、text/binary 帧加解密、Close 4003 | 01 |
| 05 | mobile TS 加密核心：noble 三件套、wrapper、pin 存储、互操作金样 | - （可与 01 并行） |
| 06 | mobile useHttpApi 接线：加密请求、strict/降级策略、pin 校验 | 02, 03, 05 |
| 07 | mobile WS 接线：终端 + 事件 socket 握手扩展、帧编解码、重连 rekey | 04, 05 |
| 08 | 收尾：双端设置开关 + i18n、metrics 计数、code-map/docs 更新、真机验证清单 | 06, 07 |

Further Notes：协议字段一经发布即为兼容性表面，v1 从简但 `v` 字段必须存在；peer-net 传输加密与本 spec 密钥体系**不共享**密钥（不同 domain info，天然隔离）。
