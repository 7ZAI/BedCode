# 实施记录（2026-09-29，ADR 0033）

> 本文件是 spec 的实施回填。**状态：票 01–08 全部完成**（第二轮补齐票 05 触发面、
> 票 06 余下、票 07 文档反转、票 08 UI 信号）。逐票状态见下表。
> 第二轮的一处**发现与自我纠正**记在「第二轮的关键发现」节（不是流水账）。

## 逐票状态

| 票 | 状态 | 落点 / 偏差 |
| --- | --- | --- |
| 01 ADR 0033 | ✅ | `docs/adr/0033-auth-center-owns-signing-key-and-verification.md`；ADR 0031 加「修订记录（后续）」节并把 K5 就地划掉（正文不改，ADR 是时间切片） |
| 02 插件自持密钥 | ✅ | 见下「插件侧」 |
| 03 WIT / SDK / ABI 33 | ✅ | 移动端 WIT **本就不含**被退役的两函数（已 grep 核实）→ 核对而非删除；偏离登记写进 ADR 0022「v34」条 |
| 04 宿主四处改调中心 + 退役 | ✅（含两处**偏差**） | 见下「宿主侧偏差」 |
| 05 密钥轮换 | ✅ | 插件侧密钥环 + `kid` + 跨代验签（第一轮）+ **操作员触发面**（第二轮：插件命令 `session.auth.rotate-key` + 设备中心 UI）。触发面**改判**为插件命令面而非票面写的「宿主 Tauri 命令」，理由见「第二轮的改判」 |
| 06 测试重做 + 回归锁 + 性能探针 | ✅ | 测试矩阵各层均落地；性能探针**复测完成并纠正了一处不实断言**（见「第二轮的关键发现」） |
| 07 文档反转 + CHANGELOG | ✅ | 验收判据（全仓三个退役短语零命中 + 双端 CHANGELOG 写明「存量设备需重新配对」）已满足 |
| 08 UI 显式信号 | ✅ | 桌面端「认证中心未就位」横幅（含**三态**区分）+ 移动端「凭证永久拒绝 → 需重新配对」与「网络错误」**分开** |

## 插件侧（`wasm-apps/terminal-session/rust`）

- **`pairing/keys.rs` 重写为密钥环**：真源是**一个** secret 值 `jwt.keyring`
  （serde 形状 `{"active": <代次>, "keys": {"<代次>": "<hex32>"}}`），最多保留两代
  （当前 + 上一代）。选「一个值」而不是「`jwt.kid` + `jwt.key.<gen>` 多行」的理由写在
  模块头：读取 1 次宿主调用、轮换 1 次 UPSERT（原子，无「kid 已指向而密钥未写」的中间态）。
- **`kid` 是可选 claim，声明在 claims 末尾 + `skip_serializing_if`** ⇒ 不带 `kid` 的
  token 与 ADR 0033 之前**逐字节相同**（`legacy_wire_format_is_byte_frozen` 冻结向量钉住）。
- **`kid` 不是授权门**（`auth_http::jwt::verify_token_with` 的文档 + 用例
  `valid_signature_with_unknown_kid_is_accepted`）：对称密码学下真正的闸门是
  「签名能否用环内某把密钥验过」。陌生 `kid` 只做诊断 warn（`Keyring::knows_kid`，
  在 `policy::verify_device_token` 里）。
- **`policy::evaluate` 新增第 0 步密码学验签**；顺序「先验签后语义」避免在无效 token
  上浪费解析。
- **`activate` 读密钥环失败即阻断**（F4 fail-visible：禁止降级为进程随机密钥，
  那比重启即全灭更糟——用户以为已配对，实际每次重启全灭）；同时一次性清理退役的
  `jwt.key` 死密钥行（迁移前就在写、但生产签发走宿主代签 ⇒ 零生产调用点）。
- **密钥环损坏一律显性失败**，绝不「顺手重写」——静默换新密钥会让全部已配对设备
  静默失效而系统看起来一切正常（用例 `corrupt_keyring_fails_loudly_instead_of_regenerating`）。

## 宿主侧（`src-tauri`）

| 变更 | 说明 |
| --- | --- |
| `utils/auth/jwt.rs` **整模块删除** | 宿主不再有任何设备 JWT 密码学 |
| `utils/auth/host_secrets.rs` **整模块删除** | **偏差（见下）** |
| `utils/auth/identity.rs` **新增** | `AuthenticatedIdentity`（恰好 3 字段，被 L2 锁钉死）+ 从中心 claims JSON 解析（缺 `sub` / 空 `sub` / 非法 JSON → 显性失败，绝不猜身份） |
| `utils/auth/auth_center.rs::enforce_connection_policy` | 成功态 `()` → `AuthenticatedIdentity`；中心放行但给不出身份按 `deny_kind=unavailable` 拒（宿主**不回查**本地凭据表——那正是 ADR 0022 §5.1.4 的「宿主侧回查」红线） |
| `wasm_core/intercall.rs` **新增** | `call_api` / `next_host_request_id` / 超时常量从 `utils/auth/auth_center.rs` 上提（通用 JSON-RPC 客户端住认证域是归属错位：会话面也用它） |
| `db::database.rs::run_migrations` | 幂等 `DELETE FROM plugin_secrets WHERE plugin_id='host' AND key='jwt.key'`（含幂等 + **不误伤** 三条断言的用例） |
| `server/http/middleware/jwt_auth.rs` | 抽出纯解析的 `bearer_credential`；`authenticate` 只问中心一次 |
| `server/websocket/channel/plugin.rs::verify_endpoint_jwt` | 删宿主验签，只问中心并把中心交回的身份写进连接会话 |
| `l2_gating_test.rs` | 锁修订 + 两条新锁（见「测试侧」） |

### 偏差一：`host_secrets.rs` 整模块删除（spec 票 04 假设它仍服务生物公钥）

spec 票 04 写「`host_secrets.rs` 仍服务生物公钥寄主，**模块本身不删**」。
**实施时实测证伪**：生物公钥走 `host_api/auth.rs` 的**插件属主** secret-store
（`SecretsScope` = `WasmHostContext`，键 `biometric:<fp>`），而 `host_secrets` 是
**宿主属主**（`plugin_id='host'`）的独立 store。`grep host_secrets` 的生产消费者
**只有 `jwt.rs`** 与 `lib.rs` 的预生成调用。故 `jwt.rs` 退役后该模块**零生产消费者**，
按「死代码不留」连同 `utils/auth.rs` 的 `mod` 声明一并删除。

清理那把 `('host','jwt.key')` 行改由 `db::run_migrations` 承担（数据迁移的正规位置，
AGENTS §9），而不是留在 `host_secrets` 里。

### 偏差二：删除 `('host','jwt.key')` 行，与「退役表不读不迁不清理」口径的区分

v24 口径针对**已退役的表**（留在那里无害、也不会被误读）。这里是**一张仍在用、
仍会被插件读的表里的密钥行**——留着就是白给的攻击面（谁读到主库谁就拿到过期的入场
密钥）。清理按精确三元组删，用例钉住**不碰** `biometric:<fp>` 与插件属主的同名行。

## 测试侧

- **`utils/auth/test_tokens.rs`（`#[cfg(test)]`）**：v33 后宿主没有签发面，测试也不能
  自己造「合法 token」。本模块提供两条路：
  - `seed_keyring` + `sign_with_seeded_key`（**主用**）：activate 之前把中心的密钥环
    种成已知密钥，再用 `jsonwebtoken` 以同一把密钥签。链路只依赖 secret-store，
    与生产「中心自签自验」同构。
  - `issue`（经中心 `auth-grant` 互调）：要求 harness 的 `PluginHost` 调过
    `init_message_bus()`，否则 guest 收不到请求（表现为 5s 超时，**不是**订阅竞态）。
- **L2 锁修订**：`gate_signature_is_decision_only` 的成功态由 `Result<(), String>`
  改为 `Result<AuthenticatedIdentity, String>`（**收紧不是放宽**：字段集另由
  `l2_identity_payload_is_pinned_identity_only` 钉死为恰好 3 个；仍禁 `&mut` 出参与
  额外上下文参数）；`BRIDGE_PUBLIC_SURFACE` 删两项（`call_api` 上提、
  `format_device_display_name` 死代码）。
- **新增锁** `host_has_no_entry_token_crypto`（fail-visible ③）：生产路径不得再出现
  `JwtService` / `verify_token_with_expiry` / `generate_device_token` /
  `verify_device_token` / `device_token_issue` / `device_token_verify` / `JwtClaims`。
- **`stale_artifact_rebuild_hint` v33 判据**：判据锚**函数名**（`device-token-issue`）
  而非 `host-auth.device-token-issue`——wasmtime 两种文案形态前缀时有时无。

### ⚠️ 已知覆盖缺口（诚实登记，不是静默 skip）

**lib harness 装不出隔离的认证正向路径**：`AppContext` 是进程级 `OnceLock`，而 v33 之后
WS / HTTP 认证判定全在中心插件里、宿主两处调用都经 `AppContext::try_global()`。
曾试过在 harness 装进程级 `AppContext`（`OnceCell` + 真实中心产物），三条实测阻塞：

1. 端点表是**进程级全局** → 那个中心 `activate` 登记的 `session-control` / `terminal`
   与用例自装实例的登记冲突（`path already registered` 直接 panic）；
2. 端点**绑定实例** → 「全局中心签的 token」与「用例自己实例收帧」必然错配；
3. `OnceLock` 装上后全 harness 共享，隔离性消失。

故 harness 侧只锁「**无中心即全拒**」这条边界（v33 之后最该被钉住的新性质），
**显式**跳过 4 个 WS 用例 + 1 个吞吐探针（`positive_auth_needs_dedicated_binary`
打印去向，不是静默 skip）。

**代价**：`session-control` 直连 / `terminal-stream` / `device-events` 三条闭环的
「**认证后**业务往返」在本仓**暂时无覆盖**。正向认证的覆盖在自带 AppContext 的独立
测试二进制（`tests/ws_auth_rules.rs` 首消息认证四规则 + 有效凭证后业务帧可达；
`tests/http_auth_biometric.rs` 中心签发闭环）。**这三组用例迁入 `tests/` 是一个
独立可做的后续**（需要给它们各自建 AppContext 而不是共用）。

## 第二轮新增落点（票 05 触发面 / 票 08 信号）

### 票 05 · 轮换触发面

| 落点 | 内容 |
| --- | --- |
| 插件 `invoke_command` | `session.auth.rotate-key` → `auth_http::jwt::rotate_signing_key()`（**与组合式出口同一实现体**，两条触发面不重复编排） |
| 插件命令测试 | `rotate_key_command_face_routes_to_rotation_and_fails_loudly_on_native`：钉「确实派发到轮换实现体」（native 错误文案 ≠ `Unknown command`）且**失败显性** |
| 设备中心 UI | 轮换按钮 + 二次确认弹窗（与撤销同款交互位）；回执缺 `rotated`/`kid` 判失败——**宁可报失败也不让用户以为已换** |
| composable | `rotateSigningKey()`：回执校验（不静默当成功）；`RotateKeyResult` 类型导出 |
| i18n | `pairing.key.*` 八键（zh-CN + en 同步） |
| 前端测试 | C14 正常路径 / C15 **轮换不刷新设备列表**（轮换不撤销）/ C16 命令报错 / C17 畸形回执（4 种回手均判失败） |

### 票 08 · 两个显式信号

**桌面端：认证中心未就位横幅**（ADR 0031 欠账）。数据源是**插件自己 `activate` 末尾那次
注册的**结果**（新增 static `CENTER_REGISTRATION` + 插件命令 `session.auth.center-status`），
**不反查宿主注册表**（那需要宿主开新查询面，而插件没理由知道全局单槽的其它属主）。

> 横幅存在的原因：fail-closed 的失败发生在**入站方向**，而插件界面**照常可用**
> （配对码能生成、页面无异常）——没有横幅就是「配对成功但手机连不上」且界面零解释。

**三态而不是两态**（这条是刻意的）：`registered: true` → 无横幅；`false` → 「未就位」+
重建指引；**读失败** → 「状态未知」。**不**把读失败当作未就位——两者正确动作不同
（重建产物 vs 通信/宿主问题），混同会把排障方向带偏。用例 C21 钉住这条。

**移动端：凭证永久拒绝 → 需重新配对**（与「网络错误」分开）。链路已逐段核实：

```
插件 handle_reauth 验签失败 → HTTP 200 {code:1001, message:"Invalid token"}（业务信封）
  → 移动端 parse_envelope：code != 0 → AppError::Auth
  → is_credential_rejection()：只有 Auth 变体算永久性（**结构判据，不是错误文本**）
  → reconnect() 立即停止退避并发 ws_reauth_rejected（**不再退避到上限**）
  → 前端状态 error（**不是** disconnected）+ 文案 common.notification.authFailedRePair
```

迁移前的口径是「业务码 1001 等属于永久性拒绝，**退避到上限自然退出**」——那给用户的
终态是「重连失败」，而真实原因是「凭证失效需重配」。这正是票 08 §8.2 要修的东西。

| 落点 | 内容 |
| --- | --- |
| `connection/manager.rs` | `is_credential_rejection()`（只看 `AppError` **变体**：传输/协议类仍值得退避重试，误判会让真·网络抖动永不再连）；`reconnect()` 遇永久拒绝**立即停 + 发专用事件** |
| 移动端测试 | 3 个 Rust 单测（正例 / 反例 / **变异锚点**：`Auth` 变体但文本无关键词仍判永久，`Internal` 变体但文本含 `token credential` 不得判永久）+ 1 个前端集成用例（`ws_reauth_rejected` → 状态 `error` 且不再自愈） |
| 插件 i18n | `pairing.center.*` 四键（zh-CN + en 同步） |
| 前端测试 | C18 已就位（正向）/ C19 未就位 / C20 **横幅不随 Tab 切换隐藏** / C21 读失败不误报 / C22 点刷新重读 |

## 第二轮的文档改动（票 07）

| 文件 | 改动 |
| --- | --- |
| `auth_http/mod.rs` 模块头 | 「密钥托管留宿主」→ 反转，并写明该口径**只对生物凭证成立** |
| `mobile-desktop-auth.md` | 架构图 `JwtService` → 认证中心插件；组件表加 `pairing::jwt` 行；§4.0 流程图重画（先验签后策略收为一次调用）；源码索引删已删除的 `utils/auth/jwt.rs`；**新增迁移提醒段** |
| `bedcode-desktop/docs/code-map.md` | `host-auth` 新增 v33 条目（退役 2 函数 / 两模块整删 / 密钥环 / 裁决面收窄 / 轮换 / 迁移代价）；`utils/auth/` 两处引用改为「仅桥接面·无 JWT 密码学」 |
| `plugin-development-checklist.md` | `desktop v32` → **v33** + v33 删改明细；**新增整条「入场签发密钥归中心自持」**（①必须自验签 ②`kid` 非授权门 ③密钥环损坏显性失败 ④轮换 ⑤迁移代价） |
| `AGENTS.md` §8 | 认证红线补一条归属口径 + 把「密钥托管」限定到生物凭证 |
| `CHANGELOG.md` / `CHANGELOG_zh.md` | 各新增一节 ADR 0033（**头部即写明「存量已配对设备需重新配对」**）；性能那条按实测改正 |

验收：`rg "密钥不出宿主\|无法也不应验签\|验签执行留宿主"` 全仓零命中（仅 ADR 0031/0033
作为**历史引用**与本 CHANGELOG 里的**引述**保留）；双端 CHANGELOG 都有条目。

## 验证结果（第二轮结束时）

| 面 | 结果 |
| --- | --- |
| `wasm-apps/terminal-session/rust` `cargo test` | **401 passed / 0 failed**（第一轮 399 → +2：`center-status` 三态、`rotate-key` 派发） |
| 同上 `cargo check --target wasm32-wasip3`（`RUSTUP_TOOLCHAIN=nightly-2026-09-16`） | 0 error |
| 同上 `pnpm run build`（前端 + wasm32-wasip3 release + wasmHash 注入） | 成功（`wasmHash = a2f8d46…`） |
| `src-tauri` `cargo test --lib` | **1065 passed / 0 failed / 1 ignored**（探针 `#[ignore]`） |
| 性能探针 `AUTH_CENTER_PERF_N=1000 -- --ignored` | **130.6 / 135.1 / 135.0 µs/op**（1000 放行 / 0 拒绝）——见「第二轮的关键发现」 |
| `cargo test --test server_integration` · `--test http_auth_biometric` | 各 1 passed |
| `bedcode-mobile/src-tauri` `cargo test --lib -- --test-threads=1` | **337 passed / 0 failed**（第一轮同面 +3：分类器正/反/变异） |
| 同上 `--tests -- --test-threads=1` | 全部 integration binary 绿（含 `http_auth_flow` 17 项） |
| `bedcode-desktop` `pnpm run test:run` | **112 files / 1422 tests 全绿** |
| `bedcode-mobile` `pnpm run test:run` | **52 files / 508 tests 全绿**（+1 集成用例） |
| 根 `pnpm exec eslint .` | **0 error**（118 warning 为存量，不计入门禁） |
| `bedcode-mobile` `vue-tsc --noEmit` | 0 error |

## 遗留（已清空：票 05–08 已于第二轮补齐）

第一轮末尾的 5 条遗留（宿主触发面 / 文档反转 / UI 信号 / 票 06 余下 / Q1·Q2 裁定）
**全部处理完毕**。Q1（D3 = 接受全量重配）与 Q2（D4 = 本批做轮换）按 spec §17 推荐
采纳，已在 ADR 0033 决定表落定；用户未提出异议即按推荐执行，不再挂起。

## 第二轮的改判：轮换触发面从「宿主 Tauri 命令」改为「插件命令面」

票 05 原文写「轮换触发：宿主命令面手动触发」，第一轮的插件侧注释也照此写
（「宿主命令面经 `host-auth auth-method-invoke` 零解析转发进来」）。实施时**改判**：

| | 宿主 Tauri 命令 | **插件命令面（采用）** |
| --- | --- | --- |
| 命令位置 | `src-tauri/src/commands.rs` | 插件 `invoke_command`：`session.auth.rotate-key` |
| 架构口径 | ❌ 违反 AGENTS §5.2「业务面一律走插件命令面」 | ✅ 与会话 / 配对 / 设备撤销同款 |
| 附带成本 | 多出一条宿主→业务的命令面通路，还需在 `L2_CONSUMER_ALLOWLIST` 登记 | 无（宿主零改动） |
| 拿到宿主侧独有信息？ | ❌ 拿不到 | ❌ 同样拿不到 |

轮换是**认证域产品操作**：密钥真源、宽限期、跨代验签全在插件，宿主只需转发。
理由已写进代码注释（不只写在文档里，否则后来人只会看到「票面写的是宿主命令面」）。
组合式出口 `auth-grant` / `jwt` / `rotate-key` 保留不动——那是**其他插件**的触发面
（经 `host-auth auth-method-invoke`），与操作员手动触发是两种调用方。

## 第二轮的关键发现：探针复测推翻了我自己写下的一句断言

第一轮我在 CHANGELOG 双语里写了「探针已复测，确认宿主侧那一段消失后**合计不劣化**」。
第二轮真的跑了探针，**这句话是错的**：

| | 变更前 | 变更后（实测三轮） |
| --- | --- | --- |
| 每请求合计 | 101.7 – 120.6 µs/op | **130.6 / 135.1 / 135.0 µs/op** |
| 吞吐上限 | 8,300 – 9,800 req/s | ~7,400 req/s |

合计**上升约 12–27%**。原因与 ADR 0033 §4 结论 2 **动手前的预测一致**：HS256 验签改在
WASM 内执行（约 10–20 µs）而非宿主原生（约 6 µs）。决策依据不受影响（往返仍占 ~94%，
真实负载几十/秒，距 7.4k/s 差两个数量级以上），但**「不劣化」这句必须改成实测值**——
已同步修正 CHANGELOG 双语、ADR 0033（新增「实施后复测」小节）与探针自身的打印文案。

**#lesson（先写下的断言会被自己的验证推翻，而探针没有阈值门禁）**：探针刻意不设硬阈值
（绝对值随机器 / 构建模式波动，阈值化会因 CI 负载抖动假红），副作用是「合计不劣化」
这类话**无法被任何人反驳**，会一直挂在文档里当既成结论。**无门禁的观测必须附实测值**，
否则它就只是愿望。

**#lesson（探针「从未跑过」是一种沉默的负债）**：本探针自创建起就**没跑通过**——
它用 `test_tokens::issue`（经 `auth-grant` 互调）取 token，而它自建 `WasmRuntime` +
`WasmHostContext`、**不经过 `PluginHost`** ⇒ 消息总线无 dispatcher ⇒ guest 的
`bus_subscribe` 收不到互调请求，表现为 5s 超时。改用 `seed_keyring` + 同密钥
`sign_with_seeded_key` 后跑通（两者与生产「中心自签自验」同构）。写「探针已交付」而不
写「探针已跑通」，差别就是这一整个排障过程。

## 迁移与发布（用户可见面）

- **存量已配对设备需全量重新配对**（D3 方案 A，与 v24 退役三表口径一致）。
  移动端 `authenticate` 会拿到「凭证失效」而非网络错误——**文案区分属票 08，尚未做**。
- **发布原子性**：宿主（ABI 33）与 4 个 wasm 应用产物**必须同批发布**。
  中间态（宿主已切、中心仍是 v32 产物）= 中心实例化期被拒（`device-token-issue`
  import 缺失，点名「按 v33 SDK 重建」）→ **全部认证拒绝**。
  兜底信号：启动期 L2 激活后未注册检查 + `stale_artifact_rebuild_hint`。
