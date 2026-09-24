# 宿主加密引擎化 + 业务残留下沉（Host Crypto Engine & Business Downsink）

Status: **✅ 全部 12 票完成（2026-09-24）**——票 01-04（crypto 引擎 + host-crypto 契约面，ABI v26）早批 land；票 05（协商套件参数化）/06（special_key 下沉）/07（shell 下沉）/08（session 线协议归位）同日 land；票 09a/09b/09c（WS 动作词表声明式化）user 裁决冻结前做完，宿主硬编码词表 switch 删除，`enums/` 终态 = 引擎级 + 传输面契约；票 10 文档记账落地（ADR 0022 v17/v18、CHANGELOG 双语、code-map、路线图阶段 4 状态；AGENTS §5/§7/§8 记账因并发 agent 在途大改暂缓，待其落地后合并时补）
Date: 2026-09-24
范围: **桌面端为主**（`bedcode-desktop/src-tauri/src/{utils/crypto, server/core/link_crypto.rs, enums/, server/websocket/,
wasm_core/host_api/, session/, plugins/terminal-session/}`、WIT/SDK）；移动端按 ADR 0018「双端偏离」登记（不强制跟演，见 §5 O1）
决策依据: 用户 2026-09-24 方向指令（三点：WS 只留加密抽象层 / 宿主只提供最基础 POSIX 级 API / 否则尽力与业务解耦）、
AGENTS §5（无业务内核 / 高内聚低耦合）、ADR 0022（裁剪线：宿主只留「离宿主无法实现、且无业务语义」的原语）、
`.scratch/2026-09-10-plugin-kernel-roadmap/spec.md`（阶段 3 ✅ / 阶段 4 冻结）、`.scratch/2026-09-23-session-engine-downsink/spec.md`（P1-b 在途）
承接: 会话/终端/认证**编排**已下沉 `com.bedcode.terminal-session`（2026-09-20~23）；本专项把最后两类宿主残留收尾：
① 加密实现从「WS 内联硬编码」提升为「引擎级算法注册表 + host-crypto 原语」，WS 只留抽象层；
② `enums/` 目录中按用户口径属业务的类型/词表逐项裁决去向（引擎级 / 线协议形状 / 下沉插件）。

> **路径基准提醒（2026-09-24 实测）**：P1-b（session-engine-downsink）主体完成**未提交**，工作区有 `wasm_core/` 重命名等在途。
> 本专项所有改动**等 P1-b land 后再开工**；`git status` 与 P1-b 重叠的文件（`session_gateway.rs`、`enums/` 引用方）开工前先确认基线。

---

## 0. 用户方向指令（原文要点，作为验收基线）

1. **WS 层面只留加密抽象层**；加密具体实现在宿主侧，通过**聚合全局加密方法大全**实现具体加密；插件通过**指定加密方法**调用宿主加密。宿主应添加一个**加密文件夹/模块**，实现任何协议的加密与各种加密算法的实现。
2. **宿主侧只提供最基础的 POSIX 系统调用 API**；例外只有两个：**WASM 性能限制**、**WASM 本身的边界限制**；除此之外，宿主侧**尽最大努力与业务解耦**，让插件调度宿主提供的 API 来实现业务。
3. 前面讨论的 `enums/` 业务代码迁移问题纳入本专项：`auth.rs` 加密协商、`special_key.rs`、`shell.rs`、`session.rs`、`control.rs`/`sync.rs` 动作词表等按「引擎级 / 线协议形状 / 下沉插件」三分类处置。

---

## 1. Problem Statement：现状与终态的差额

### 1.1 现状（2026-09-24 实测）

- **加密实现已集中在 `utils/crypto/`**（aes_gcm / chacha / x25519 / kdf / rsa / hybrid 六算法族），但：
  - 位于 `utils/` 工具层，无引擎身份、**无算法名注册表**（插件无法按名调用）；
  - `server/core/link_crypto.rs` **直接内联调用**算法（`link_crypto.rs:485` `crate::utils::crypto::aes_gcm::encrypt` 等），WS/HTTP 过滤层与具体算法强耦合；
  - WS 链路加密协商协议**硬编码**：`auth.rs::CryptoProposal`（v 固定 1、临时 X25519 `ek`）+ `AuthStage::Reauthenticate` 挑战-应答，耦合在 `conn.rs` 认证路径（`pending_ws_crypto`）——套件选择是宿主写死的实现选择，非引擎中性能力。
- **`enums/` 目录 9 文件三类混置**（详见 §4）：
  - 引擎级：`pty_status.rs`（PTY 进程状态）、`special_key.rs` 的按键→字节翻译（灰区，ADR 0022 D1 明文列「特殊键/组合键 API」为产品语义）、`shell.rs::SessionLaunchConfig` 的裸 argv 部分；
  - 线协议形状（server 留宿主，类型必须宿主持有，但**词表来源可声明式化**）：`control.rs` / `sync.rs` / `summary.rs` / `session.rs` wire 面；
  - 业务残留：`auth.rs` 协商协议、`shell.rs::ExecutionEnvironment/WindowsShell`（WSL/环境选择）、`session.rs::SessionType`、动作词表硬编码 switch。
- **P1-b 在途**：会话真源已下沉插件（登记域/状态机/广播点），`lib.rs` 关窗守卫已改经 `session_gateway::list_views` 探源插件（2026-09-23 定案：失败回退空列表）。`host-session` 剩余函数待退役（ABI v25→v26，见 session-engine-downsink P4）。

### 1.2 终态（微内核划界）

```
宿主（内核，只留引擎 + WASM 做不到的）
├── crypto/ 引擎模块：算法注册表（名称→实现，白名单）+ 抽象 trait + 统一错误类型
│   ├── AEAD（aes-256-gcm / chacha20-poly1305）
│   ├── 非对称（x25519 / rsa-oaep / rsa-pss）
│   ├── KDF（hkdf-sha256）
│   └── 混合（hybrid ECIES）
├── host-crypto 原语（插件按名调用；权限位 crypto:aead / crypto:asym / crypto:kdf）
├── WS/HTTP 加密抽象层：link_crypto 只依赖注册表接口，协商套件参数化（算法名可注入）
├── 其余五类通用引擎 + 安全边界（不变）
插件 com.bedcode.terminal-session（业务真源，不变）
├── 会话登记/状态机/生命周期（P1-b 已完成）
├── 按键组合→转义字节翻译（special_key 下沉，宿主 pty 只收裸字节）
├── 环境/发行版选择（ExecutionEnvironment 随 host-session 退役）
└── WS 动作词表声明式贡献（_http_endpoint 模式，宿主通用路由）
```

**验收红线**：① 宿主 `link_crypto` 不再内联任何具体算法实现（全部经注册表抽象接口）；② `enums/` 目录终态 = 只剩「引擎级类型」与「线协议形状（登记为传输面契约）」两类，业务语义类型零残留（或逐项登记后置去向）；③ 插件可经 `host-crypto` 按名调用加密；④ 新增宿主能力一律按「最基础 POSIX 级 API，且离宿主无法实现」双判据立项，否则下沉插件。

---

## 2. 硬约束（不解决就无法落地，必须先裁决/改造）

| # | 约束 | 证据 | 影响 |
| --- | --- | --- | --- |
| H1 | **数据面不可进 WASM**：逐帧加解密 / PTY 字节流在宿主收发热路径，WASM 不可重入、同实例串行（A0-3 红线）、无 push 模型（ADR 0022 D3） | ADR 0022 D3；AGENTS §7 串行红线 | 帧加解密、PTY 写入必须宿主执行；可下沉的是**低频控制面**（协商流程、套件选择、按键翻译→字节由插件算好传入） |
| H2 | **server 留宿主**：HTTP/WS 服务器 = WASM 做不到 | session-engine-downsink H4 | 移动端线协议形状类型必须宿主持有（WASM 无共享类型）——`control.rs`/`sync.rs` 类型「迁不走」，迁的是**词表来源**（声明式 vs 硬编码） |
| H3 | **安全红线：crypto 原语不得成为认证旁路** | AGENTS §8「认证链路只走既有 auth 模块，禁止旁路」 | 原语只给中性算法，不给编排；算法名白名单（词汇表单一真源）+ 权限位 + 调用审计；宿主密钥（Kd/JWT keystore）**不**经原语暴露，只服务内部 filter 链 |
| H4 | **权限词汇唯一真源 + 五同步点** | AGENTS §7（permission-vocabulary / 宿主能力清单 / host_impl 门 / manifest 映射表 / SDK 生成） | 新增 `crypto:*` 权限位必须五处同步落，漏一处即词汇漂移锁翻红 |
| H5 | **ABI/双端偏离**：host-crypto 新 interface → ABI bump；移动端不强制跟演（ADR 0018 先例） | ADR 0018/0022「双端偏离」节 | host-crypto 桌面独有登记偏离；移动端加密线协议（ws_client.rs + `packages/link-crypto` 共享 crate）不受影响 |
| H6 | **P1-b 在途未提交**：`wasm_core/` 重命名 + session 下沉主体在工作区 | git status（2026-09-23 晚实测） | 本专项开工前 P1-b 必须 land；`session_gateway.rs` / `enums/` 引用方改动与其重叠，禁止并发 |
| H7 | **线协议增量演进**：协商参数化不得破坏旧端 | AGENTS §9「字段演进遵循增量原则」 | 协商套件参数化走 expand–contract：新字段并存 → 迁移 → 退役旧字段 |

---

## 3. 分票建议（tracer-bullet 垂直切片，依赖序）

详见本目录 `issues/`（to-tickets 流程，待用户批准后发布）：

| # | 票 | 交付 | 依赖 |
| --- | --- | --- | --- |
| 01 | crypto/ 引擎壳 + 算法注册表（prefactor） | 注册表（名称→实现）+ 白名单 + 抽象 trait + 按名调度测试 | 无（避开 P1-b 面） |
| 02 | link_crypto 改走注册表抽象接口 | WS/HTTP 过滤层不再内联算法调用（WS 只留抽象层） | 01 |
| 03 | host-crypto WIT 契约（interface + 权限位 + ABI + SDK 五同步点） | 插件按名调用的契约面 | 01 |
| 04 | 宿主 host_api/crypto.rs 实现 + 权限门 + 审计 | 原语实现面 + 权限/白名单/审计测试 | 03 |
| 05 | 协商套件参数化 | AuthStage::Reauthenticate 协商改按名选套件（expand–contract，旧字段并存） | 02、04（P1-b land） |
| 06 | special_key 下沉 | 按键→转义字节翻译迁插件，宿主 pty 收裸字节 | P1-b land |
| 07 | shell.rs 下沉 | ExecutionEnvironment/WindowsShell 随 host-session 退役；SessionLaunchConfig 收窄为裸 argv | P1-b land、host-session 退役 |
| 08 | SessionStatus/SessionType 业务视图收窄 | 宿主只剩 PTY 进程状态；业务视图走插件副本 + 线协议形状 | P1-b land |
| 09a | WS 动作词表声明式化 · expand | 接入声明式路由（_http_endpoint 模式），旧硬编码 switch 并存 | P1-b land |
| 09b | WS 动作词表迁移 · migrate | 现有动作逐批改走声明式路由 | 09a |
| 09c | WS 动作词表退役 · contract | 删除宿主硬编码业务词表 switch | 09b |
| 10 | 文档与记账 | ADR 0022 补记 / AGENTS §5§7§8 / code-map / CHANGELOG / 路线图阶段 4 / 移动端受损清单 | 02–09c 全部 |

---

## 4. 附：现状契约与消费方清单

### 4.1 `utils/crypto/` 算法库（引擎化素材）

| 文件 | 算法 | 现消费方（内联调用） |
| --- | --- | --- |
| `aes_gcm.rs` | AES-256-GCM AEAD | `link_crypto.rs:485` 等 |
| `chacha.rs` | ChaCha20-Poly1305 | （备选，调用方按平台择一） |
| `x25519.rs` | X25519 ECDH | `link_crypto.rs`（Kd 身份/协商） |
| `kdf.rs` | HKDF-SHA256 | `link_crypto.rs:313-314`（方向分离派生） |
| `rsa.rs` | RSA-OAEP / RSA-PSS | （现无生产调用方？待核查） |
| `hybrid.rs` | ECIES（X25519+AES-GCM） | （文件加密传输面） |

### 4.2 `enums/` 三分类处置清单

| 文件 | 类型 | 分类 | 处置 |
| --- | --- | --- | --- |
| `auth.rs` | AuthStage/AuthPayload | 线协议形状（wire） | 保留传输面；`CryptoProposal` 协商协议参数化（票 05） |
| `special_key.rs` | KeyCombo/KeyCode | **业务残留（灰区）** | ADR 0022 D1 明文列产品语义 → 下沉插件（票 06） |
| `session.rs` | SessionStatus/SessionType | 混合 | SessionStatus 收窄为线协议 + PTY 状态（票 08）；SessionType 下沉（票 08） |
| `control.rs` | SessionControlAction/TerminalAction | 线协议形状 + 硬编码词表 | 词表声明式化（票 09）；形状保留 |
| `pty_status.rs` | PtySessionStatus | **引擎级** | 保留（host-pty 原语消费） |
| `shell.rs` | WindowsShell/ExecutionEnvironment/SessionLaunchConfig | 业务残留（环境选择）+ 裸 argv | 下沉 + 收窄（票 07） |
| `summary.rs` | SessionSummary | 线协议形状 | 保留；task 字段不透明透传复核（并入票 08/09） |
| `sync.rs` | SyncPayload | 线协议形状 + 任务域词表 | 词表声明式化（票 09） |
| `plugin.rs` | PluginQuestion | 已迁 SDK re-export | 不动 |

### 4.3 权限位先例（SDK `rust/src/permission.rs`）

`ws:client`/`ws:server`（网络域拆分）、`pty:spawn`/`pty:io`（进程域拆分）、`database:main` 独立高危位、
`storage` 私有库位——**host-crypto 按 `crypto:aead` / `crypto:asym` / `crypto:kdf` 三域拆分**（§5 O4 待裁决粒度）。

### 4.4 现 WIT interface 清单（`bedcode-desktop/packages/plugin-sdk-desktop/rust/wit/bedcode.wit`）

host-storage / host-terminal / host-events / host-bus / host-peer / host-mdns / host-platform / host-api-call /
host-http / host-timer / host-process / host-app / host-websocket / host-fs / host-log / host-config /
host-session / host-database / host-plugin-database / host-auth / host-pty / host-task / command / lifecycle /
events / events-binary / terminal-hooks / manifest / abi / events-ws —— **无 host-crypto**（票 03 新增，ABI v25→v26）。

---

## 5. 裁决记录（开放点，开工前用户定案）

| # | 开放点 | 选项 | 建议 |
| --- | --- | --- | --- |
| O1 | host-crypto 双端？ | a) 桌面独有（ADR 0018 偏离登记） b) 移动端同步 | a——移动端插件生态薄、加密线协议已有共享 crate；需要时再补 |
| O2 | special_key 下沉时机 | a) 本专项立即 b) 登记后置 | a——ADR 0022 D1 字面支持下沉，灰区不拖入冻结 |
| O3 | WS 动作词表声明式化 | a) 冻结前做（工程量大） b) 登记为冻结后演进 | ✅ 用户 2026-09-24 裁决：**冻结前做**，走 expand–contract（票 09a/09b/09c） |
| O4 | crypto 权限粒度 | a) 三域（aead/asym/kdf） b) 单域 `crypto` c) 更细（aead:encrypt…） | a——对齐 ws:client/ws:server 先例 |
| O5 | 算法白名单初始集合 | a) 全量六族 b) 最小子集（aes-gcm/x25519/hkdf） | b 起步、按需扩；白名单是引擎级词汇表（单一真源） |
| O6 | 协商套件参数化的线协议兼容 | 增量演进（旧字段并存） | 旧字段保留一个版本窗口，移动端旧端不断流 |

---

## 6. 验收标准

- `cargo test`（桌面 lib + 插件 + 集成）全绿；`pnpm run test:run`（前端）全绿；eslint 0 error（若动前端）
- 票 01：按名调度 + 白名单拒绝未知算法名（单测）
- 票 02：`link_crypto.rs` 无 `crate::utils::crypto::*` 直接调用（grep 断言）
- 票 03/04：插件按名调用加密往返集成测试；无权限位调用被拒（fail-visible）；审计日志含 `[plugin:xxx] crypto:*` 与算法名
- 票 05：协商套件参数化后旧端（不携带套件名）按默认套件连接不断流（集成测试）
- 票 06：宿主 pty 输入路径不再消费 KeyCombo（grep 断言）；插件自算转义字节闭环
- 票 07：`ExecutionEnvironment`/`WindowsShell` 宿主消费方清零（grep 断言）；`host-session` 退役后 `shell.rs` 类型移除或收窄
- 票 08：`enums/` 目录终态分类复核通过（§4.2 表落地）
- 票 09（若做）：WS 动作帧经插件声明路由；宿主无业务词表 switch（grep 断言）
- 票 10：ADR 0022 / AGENTS §5§7§8 / code-map / CHANGELOG / 路线图阶段 4 状态 / 移动端受损清单 全部更新

---

## 7. 风险与控制

| 风险 | 控制 |
| --- | --- |
| P1-b 未 land 就并发改 `session_gateway.rs` / `enums/` 引用方 → 冲突 | 开工前置检查：`git log -3` + `cargo check --lib`；票 02/03/04（crypto 面）与 P1-b 面不重叠，可先行 |
| crypto 原语被插件滥用绕过认证（AGENTS §8） | 白名单 + 权限位 + 审计日志 + 原语不给编排；评审纪律 |
| ABI bump 影响存量插件产物 | 按 ADR 0018 双端偏离登记；存量插件需按新 SDK 重建（对齐 v21/v24 先例） |
| 协商参数化破坏移动端旧版 | expand–contract：旧字段并存一版，增量演进（§9 协议规范） |
| 动作词表声明式化工程量大 | §5 O3 裁决：冻结前做 or 登记后置；后置时票 09 降级为登记 |
