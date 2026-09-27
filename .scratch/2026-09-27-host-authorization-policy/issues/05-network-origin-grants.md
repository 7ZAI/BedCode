# 05: 网络侧授权记录与出站询问

**What to build:** wasm 应用首次访问某个新的网络地址时弹一次窗询问；用户同意后同一地址后续请求免询问。同一地址的一批并发请求只弹一次。

**Blocked by:** 01

**Status:** done（2026-09-28）

- [x] 目标归一化：`scheme://host:port`，默认端口显式化（https→443 / http→80），host 小写，去尾斜杠
- [x] query 与 fragment **绝不进入 target**（AGENTS §8 凭据红线）；归一化后落库的 target 不含 query
- [x] path 前缀为可选收紧项，匹配按 path **段边界**比较（`/v1abc` 不被 `/v1` 命中）
- [x] deny 记录优先于同 origin 的 allow 记录
- [x] 询问走**新事件 + 新命令**（决定枚举三态：allow_once / deny / deny_always），**不复用** fs 那个双布尔签名
- [x] 应答通道复用**宿主面凭证绑定**机制：非宿主面凭证代答必须拒绝
- [x] 同 origin 的并发请求**合并为一次弹窗**，答一次放行该批在途请求；合并窗口是显式常量，不做隐式去抖
- [x] 30s 超时按拒绝
- [x] 硬闸门位置不变且**不被记录放行**：私网/回环目标、公网→私网重定向仍被拒
- [x] 入站方向（`http_register_endpoint`）**零改动**
- [x] 设置页网络分区列出 origin 记录 + 逐条撤销
- [x] i18n 双语同步

## 关键实现事实

- **不 bump ABI**：`http_fetch` 的 import 签名与返回类型一字未改；询问是 `host-http` **内部**新增的失败模式。应答机制照抄 fs 现有的凭证绑定实现
- 策略只管**出站**——「请求地址」只对出站成立；入站的产品语义是「我要暴露什么接口」，那是业务编排，不归宿主决定

## 测试纪律

- [x] 单元测试：默认端口归一化、query 不入库、段边界前缀、并发合并只发一次、SSRF 闸门不被记录放行

---

## Conclusion

### 落地形状

| 位置 | 改动 |
|---|---|
| `wasm_core/security/network_auth.rs`（新增） | 归一化 / 记录匹配 / 判定管线 / 弹窗合并 / 三态应答 / 无询问面 |
| `wasm_core/security/auth_policy.rs` | 只加两处：`AuthPolicyStore::strategy()`（判定期读档位）+ `AuthRecordMatch.prefix_match`（网络前缀匹配要读的字段）。写入面复用 02 票已建的 `grant` / `deny` / `revoke` / `remove_deny` |
| `wasm_core/host_api/http.rs` | `http_fetch` 加 `net_auth` 视图 + `may_prompt` 两个参数；授权检查插在**解析请求之后、任何网络动作之前**（流式 / 非流式两条分支都在它之前收敛） |
| `wasm_core/host_api/context.rs` | `WasmHostContext.net_auth` 字段 + `NetworkAuthScope` trait（与 `FsAuthScope` 同形态） |
| `api_bridge.rs` / `lib.rs` | 新命令 `plugin_network_auth_respond`（宿主面凭证绑定）+ State 注册 |
| 前端 | `NetworkAuthDialog.vue`（新事件 `plugin:network-auth-request`）、`plugin/commands.ts` 的 `pluginNetworkAuthRespond`、设置页展开面板按资源分区（文件 / 网络）共用同一段行标记 |

### 三个需要解释的决定

1. **`allow_once` 按 origin 落 allow 记录**（票面「三态」与首句「同意后同一地址后续请求免询问」只有这一种调和方式）。
   若 `allow_once` 不落账：默认档下每次访问同一 origin 都重新弹窗（agent-hub 一次刷新几十个请求 → 策略在实践中等于不可用），
   且与 spec §4.1「default 档：新命中经用户确认后落 allow」直接冲突。网络侧没有「记住」勾选框，因为询问粒度**就是** origin——
   用户点头的语义已经是「这个地址可以访问」。落账点只有一处（`respond`），合并批次里 N 个请求共用一次询问、一次落账。
2. **合并窗口 = 显式常量 2s，不做隐式去抖**（票面要求）。① 弹窗在途时并入同一询问；② 答完后窗口内到达的请求复用同一决定。
   窗口**不因新请求重置**——滑动窗口会让持续访问永远见不到询问，等于把「总是询问」档架空；过期后第一次请求即重新询问。
3. **任务单元（core-task 池线程）走无询问面**（`may_prompt = false`）：弹窗会占住池槽位 30s（与 fs 任务单元同款约束）。
   池线程只认记录、未记录即 `no-record` 拒绝，且**判据与弹窗面同源**（共用 `read_wired_strategy` + 同一套记录匹配）。

### 刻意留下的两处「以后都拒绝」

- **合并窗口内的重复 `deny`**：用户点了「拒绝」但不落账，若应用持续轮询同一个 origin，会每 2s 弹一次。
  这是「本次拒绝 / 以后都拒绝」二态的固有代价（要消除它就得引入第四态「本轮拒绝」，票面未给）。
- **非默认网络策略档位在判定面显性报错**（票 06 才是网络两档）：忽略档位等于「用户配了更严格的档位，实际按更宽松的走」，
  属 AGENTS §8 fail-visible。当前没有任何写入口能产生这个状态，属兜底而非已知可达路径。

### 与 spec 的偏差 / 遗留

- **spec §8.2 的「每 (应用, 资源) 封顶 500 条 + core-monitor 计数」未实现**：封顶属于共享写面（`grant` / `deny`），
  与 02 票的 fs 落账是同一处代码，本票不夹带改动；网络侧 origin 数量天然有界，风险低。留给 06 或独立小票。
- **授权决策未计入 core-monitor**：`SecurityFramework` 的 monitor 由 `host.rs` 两阶段注入，而本票的判定面不走框架
  （接框架是 spec §6.2 明确推迟的重构票），故 allow / deny 计数只在 `tracing` 里。
- **私网直连不是拒绝**（与 spec §4.2 写法的差异）：现有 `is_private_target` 只用于客户端选择（LAN 文件服务直连），
  私网**直连**请求在改造前也不被拒。本票不动这个位置（动了会打断 LAN 传输），授权层对它**没有**内网免询问旁路
  （`private_target_gets_no_authorization_free_pass` 锁住）。SSRF 硬闸门的可测面是重定向裁决
  （既有 `redirect_decision_blocks_public_to_private` 锁）+ 「授权检查位于执行之前」
  （`unrecorded_origin_is_denied_before_any_network_io` 用计数型 mock server 断言命中数为 0）。

### 验证证据

- `cargo test --lib network_auth`：**26 passed**；`cargo test --lib host_api::http`：**20 passed**
- 变异自检 **10 项全部被杀**（改回子串前缀比较 / 取消并发合并 / 端口不显式化 / query 进 target /
  授权检查挪到执行之后 / 删掉 deny 优先块 / 超时改为放行 / 落账写到错误 target / 未知决定兜底放行 /
  无询问面忽略 deny）。其中「首次写的 M6 交换 deny/allow 块」**未真正生效**（策略守卫夹在两块之间，锚点不匹配），
  已改用「删掉 deny 优先块」重做并确认被杀——变异自检本身也要断言「变异确实落地」。
- 前端：`NetworkAuthDialog.test.ts` 6 passed + `AuthorizationViewNetworkRecords.test.ts` 5 passed
  + 02 票的 `AuthorizationView.test.ts` 9 passed（重构展开面板后仍绿）；`pnpm run test:run` 全量 **109 文件 / 1319 测试全绿**
- `pnpm exec eslint`（根目录，覆盖本票全部改动的非测试文件）：**0 error**
- 全量 `cargo test` 快照：**1003 passed / 1 failed**。唯一失败是 `server::http::middleware::jwt_auth::tests::jwt_tier_alias_without_token_is_401`
  与 `server::http::gateway::tests::registered_alias_falls_through_when_plugin_surface_unavailable` 的**既有跨测试竞争**：
  两者都在进程级注册表里登记同一个 host 别名 `/api/configs` 且路径写死、清理靠测试末尾的 purge。本票未触碰这两个文件与该注册表。
  收尾复跑时另一会话正在改 `fs_auth.rs` / `auth_policy.rs`，lib test 目标一度编译不过（错误全在其在途文件内），未能取到第二次全量快照。

### 实施注记

- 落地期间**另一个会话在同一工作树并行实施 02/03/04 票**（`fs_auth.rs` / `framework.rs` / `auth_policy.rs` /
  `AuthorizationView.vue` / `commands.ts` 都在被同时改）。本票的共享面改动压到最小：
  `auth_policy.rs` 只加 `strategy()` 与 `prefix_match` 两处、`AuthorizationView.vue` 把 02 票写死的
  「文件分区」泛化成按资源分区（`RECORD_RESOURCES`，行标记仍只有一份）、`plugin/commands.ts` 的新函数追加在文件末尾。
