# 更新日志

本文件记录本项目所有值得关注的变更。

格式遵循 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.0.0/)，
版本遵循 [语义化版本](https://semver.org/lang/zh-CN/)。

> 本文档为中文版本；英文版见 [`CHANGELOG.md`](./CHANGELOG.md)（GitHub Release 流程读取该文件）。

## [未发布]

#### 跨端真实互连集成测试（新增 `cross-end-tests` 工程 + 两端 lib 改名；无生产逻辑、无 ABI、无版本号变动）

- **做了什么**：新增仓库根 Rust 包 `cross-end-tests/`，在同一测试进程内让
  **桌面端真实 Actix 服务器 + 真实 `com.bedcode.terminal-session` WASM 产物**与
  **移动端真实客户端代码**（`AuthHttpClient` / `SessionHttpClient` / `TerminalLinkManager`）
  互连——零 mock、无 adb、无 WebView、无模拟器，可直接进 CI。此前两端的集成测试
  各自 mock 对方（桌面侧对面是通用 reqwest / tokio-tungstenite 客户端，移动端对面是
  假桌面服务器）：两套 mock 各自自洽，**契约在两端之间漂移时两边全绿而真实链路已坏**
- **L0 前置——两端 lib 同名去重**：`bedcode_lib`（两端同名）→ `bedcode_desktop_lib` /
  `bedcode_mobile_lib`。同名 crate 无法同时出现在一个依赖图里，不改名则本工程只能依赖
  一端。影响面已核实并限定在两端各自的 src / tests / bench，加上少量字符串常量
  （EnvFilter 默认值 `bedcode_lib=debug`、插件日志 target 前缀、`android-backup` 手工保留
  副本里的 `System.loadLibrary`、四份文档表格）。路径依赖默认以 **package 名**为键，
  故 `Cargo.toml` 需显式写 `package = "..."`
- **已知行为变化（非静默）**：用户既有的 `bedcode_lib=debug` EnvFilter（及任何按插件
  日志 target 过滤的规则）需改新名——这是开发调试配置而非产品功能。Android 的 `.so`
  文件名跟随 lib 名，`System.loadLibrary("bedcode_mobile_lib")` 随之变化；`gen/android`
  下的副本被 gitignore 且由 tauri 自动重建，**但手工保留的
  `android-backup/app-java/generated/Rust.kt` 副本已同步修改**，否则恢复即失配
- **覆盖（每个场景 = 一个独立测试二进制：`AppContext` 是进程级 `OnceLock` 单例）**：
  配对 / QR / 重认证真实往返（含四类反例）；**从客户端侧证明 ADR 0033 密钥环轮换宽限期**
  （轮换前的 token 在 HTTP 与 WS 两面仍能通过认证，轮换后新签发 token 带新 `kid`）；
  会话控制 HTTP 面（含 `1002` 错误信封与「remove 幂等 vs stop/input 严格」这条**不对称**
  契约）；**终端流闭环**——真实 `bash` PTY 字节经桌面插件 → `host-pty` → WS → 移动端
  ingest 门控 → 页面通道；无中心在册 / 伪造凭证的 fail-closed（HTTP + WS 两面）；桌面停用 /
  激活插件对移动端连接的联动影响
- **只有跨端互连才能暴露的两处发现（初稿契约写错，实测行为更有价值）**：
  ① 无中心在册时配对端点返回 **401 而非 404**——免凭证档位由插件在 activate 期登记，
  未激活即**不存在无认证的自助配对入口**；② 对不存在的会话 `remove` 是**幂等成功**
  （`actions::remove_via_host` 注释有据），而 `stop` / `input` 严格报 1002。这条不对称
  现已在跨端层钉死，未被绕过
- **诚实边界（记录而非隐藏）**：`deny_kind` 三态是宿主**日志**字段而非 wire 字段，
  客户端一律只看到 401（这是有意的，不泄露部署状态）；生物认证正向路径需要 Android
  Keystore 中的真设备私钥，无头进程构造不出（只覆盖「未绑定 → 1008」反例）；QR 的
  「桌面扫码确认」UI 步骤被跳过，改为直接驱动插件自身的 `qr-code-generate` 入口
- **CI**：`test.yml` 新增 `cross-end` job（ubuntu-latest，复用桌面插件产物构建）。
  验证：改名后两端 `cargo test` 全量绿（桌面 lib 1058 + 全部集成 target；移动端全量），
  `cross-end-tests` 7/7 绿；运行前已重建两端插件产物

> 以桌面端为主（路线图阶段 2 + 阶段 3 会话部分合并为一个批次执行），外加一项移动端
> 基础建设变更（wasmtime 47 → 48，见「基础建设」节）——**两端版本号均不动**；桌面批次的
> 范围豁免与移动端受损清单见「文档」节。

#### `packages/` 下的测试夹具合并为一个按 feature 选择的 crate（桌面测试基建；无生产代码、无 ABI、无版本号变动）

- **做了什么**：6 个 SDK 绑定夹具 `plugin-http-test` / `plugin-task-test` / `plugin-pty-test` /
  `plugin-wasip3-test` / `plugin-sdk-test` / `plugin-ws-test` 合并为单一
  `packages/plugin-sdk-fixtures` crate（一个夹具占一个 `[features]` 槽位）并**删除**。
  手写绑定的 `plugin-component-test`（30+ 调用点）**一并删除**——它的四个可观测行为
  （`test.panic`、`test.storage-get`、启动失败注入、`name`/`args`/`stored` 回包形状）
  已移入 `sdk` 夹具；DB 往返拆为新命令 `test.db-roundtrip`，使被燃料与延迟探针高频调用的
  `test.echo` 不必背两次建表+插入+查询的开销。六份几乎逐字重复的「mtime 检查 +
  `cargo build`」收成一份参数化 `build_sdk_fixture(feature)`，原六个 builder 各变成一行转发
- **feature 互斥是硬约束，但理由不是人们直觉的那个**：spike 推翻了最直观的说法。两个
  feature 同时编**不会**撞名——`wasm_entry!` / `export!` 在各自 module 内生成不冲突的符号，
  产物还大了约四倍、两个夹具都编进去了。互斥的真正理由是**产物歧义**：
  `build_sdk_fixture(feature)` 按 feature 名归档产物，一个产物必须无歧义地对应一个夹具。
  cargo 表达不了「至多一个」，故该约束由 `compile_error!` 兜底
- **产物同名冲突及其引发的竞态**：各 feature 产出的都是
  `bedcode_plugin_sdk_fixtures.wasm`，后构建覆盖先构建。解法是按 feature 归档成带后缀的
  产物名。但这**还不够**——测试并行时，某线程会归档到**另一个线程半写完**的文件
  （实测报错 `failed to parse WebAssembly module`），故 build + 归档改为进程级互斥锁串行，
  取锁后再做一次新鲜度缓存复查
- **漂移锁抓出的是真实覆盖漏洞，不只是数字变了**：
  `production_manifests_declare_only_known_vocabulary` 会断言自己扫了多少份 manifest，
  5 份 `plugin.json` 合进一个 crate 的分 feature `http.json` / `pty.json` / `task.json` /
  `ws.json` + 根 `plugin.json` 后，从 11 掉到 7。原锁只认单夹具形态的 `plugin.json`
  文件名，于是 **4 份 fixture manifest 静默掉出了权限词汇表校验**。现改为扫目录下全部
  `*.json`、靠「有 `id` 字段」筛出 plugin manifest（`package.json` / `tsconfig.json`
  无 `id`，天然跳过），下限提到 11 并写明构成
- **明确接受的代价**：`plugin-component-test` 删除后，**没有任何东西能造出「缺少某个可选
  interface」的组件**——SDK 的 `wasm_entry!` 无条件导出全部 interface。依赖这种产物的三个
  降级测试已移除，即「旧插件产物仍能加载、可选导出缺失时降级为 `Ok(false)`」**不再有测试
  覆盖**。宿主的探测与降级代码未动，消失的只是它的测试覆盖
- **分夹具 manifest**：合集 crate 每夹具一份 `<fixture>.json`；根 `plugin.json` 归 `sdk`
  夹具——因为 `#[plugin_api]` 在编译期硬读该路径（ADR 0005 单一真源）并比对 trait 方法名
  与 `api` 字段，让模块另指一份会让那个防漂移比对形同虚设
- **一个隐性的夹具产物陈旧 bug 被查出并修复**：收拢后的新鲜度检查只盯合集 crate 自己的
  3 个文件，而它在**调用 cargo 之前就短路返回**——所以「cargo 的依赖指纹会发现 SDK 变了」
  **不成立**。SDK 一改，所有缓存产物全部陈旧：**测试全绿但跑的是旧产物**。现改为递归遍历
  SDK 源目录树（而非枚举文件清单——原来各 builder 的手写清单本身就是同一个坑的定时炸弹），
  其下任一文件更新即重建。已用冷缓存验证：6 个 feature 全部现场重建，套件全绿
- **观察到一次无法复现的偶发失败**：重建四个 wasm 应用产物后的首次全量跑报
  1055 绿 / 2 红；此后 5 次（含一次删光全部夹具产物、迫使 6 个 feature 现场重建）均
  1057 全绿。最可能是与应用产物重建过程重叠，但这是**推测**——当时未捕获失败用例名，且未再现
- **`plugin-wasi-test` 保持独立 crate 且固定 `wasm32-wasip2`**：曾按「共享 `WasiCtx` + p2/p3
  两套 linker 均注册 ⇒ 与 target 无关」的推断把 preopen 迁到 wasip3，**实测证伪**——两个
  preopen E2E 均 trap 于 `filesystem_method_descriptor_open_at`：p3 linker 接上了，但预打开
  目录的能力没建到 p3 filesystem 接口上。已回退，两个用例全绿，结论写进了源码注释。
  夹具合并不要求统一 target
- **`plugin-bench-test` 保持独立**（749 行 / 29 命令）：性能基准夹具与功能闭环夹具是两类东西
- **验证**：宿主 `cargo test` **lib 1057 绿 / 0 红 / 1 ignored**，全部集成 target 绿
  （broadcast_shutdown、build_manifest_smoke、error_envelope_integration、
  http_auth_biometric、link_crypto_http、pty_session_chain、server_integration、
  ws_auth_rules）；`scripts/wasip3-toolchain.sh fixture` 仍能从合并后的 crate 产出通过
  magic 校验的 Component。`bedcode.wit` 未动，无 ABI 影响

### 功能

#### 终端历史截断不再弹 toast —— 双端静默处理（桌面端 wasm 应用 `com.bedcode.terminal-session` + 移动端；**WIT/ABI 不动**）

- **做了什么**：历史输出超过缓存上限（环淘汰/截断）时，桌面与移动端都不再提示用户，
  仅留一条后台日志便于排查——对齐成熟终端产品（不提示、用户无感知）。顺带清理宿主侧
  迁移后已无消费方的 `desktop.terminal.historyTruncated` 文案 key 与插件/移动端 i18n key

#### SDK 的 `wasm_entry!` 不再把 guest 自报失败记为 error 级（桌面 SDK 行为变更；四个 wasm 应用产物已重建；SDK 包需重新发布）

- **做了什么**：宏内 10 条失败路径——`activate` / `deactivate` / `on_startup` /
  `on_shutdown` / `on_message` / `on_message_binary` / `on_process_done` /
  `on_ws_message` / `on_ws_client_message` / `on_task_event`——由 `log_error` 改为 `log_warn`
- **依据**：AGENTS.md §8 的硬规则「**guest 自报的可处理错误不升 `error!`**」。这些 `Err` 是
  插件自己报告的失败（双层 Result 的内层），不是宿主侧证据。真正的宿主故障（trap、
  Store 中毒、fuel 耗尽）本来就由宿主运行时自己以 `error!` 记录并计数，所以降级**不会
  漏掉任何一次宿主故障**；它只是阻止 error 级退化成「某个插件返回了个 Err」——那会让这个
  级别失去告警价值
- **为何错这么久没被发现**：契约锁 `engine_limits::test_component_guest_self_reported_failure_no_host_error`
  一直只跑手写 wit-bindgen 夹具（直接返回 `Err(String)`，根本不经本宏），SDK 这条路径
  从未被放到该断言前面。夹具并到 SDK 绑定形态后才第一次照到
- **影响面**：SDK 行为变更，四个 wasm 应用产物已重建（`pnpm run build`，wasmHash 重新
  注入），SDK 包需重新发布。WIT 未变、ABI 号未动、宿主代码未改
- **运维提醒**：若有基于 error 级插件日志的告警，将不再在那里看到 guest 自报失败；消息
  以同样内容降为 warn 保留，宿主侧故障不受影响
- **验证**：SDK crate 158 绿 + `wasm32-wasip3` release 构建通过；宿主 `cargo test` lib
  1057 绿，全部集成 target 绿

#### 生物认证面完全下沉认证中心 —— 公钥托管 + 验签执行离开宿主（B-downsink，桌面端 ABI v33 → v34；**破坏性：已绑定生物认证的设备需重新绑定**；移动端 WIT/ABI 不动）

- **为什么下沉**：生物认证的**编排**（挑战签发/单次消费/过期、配对判定、HTTP 端点）早已在认证中心（`terminal-session` 插件），宿主只剩「公钥托管（`plugin_secrets` 的 `biometric:<fp>` 行）+ P-256 验签执行」两块——与 v33 入场 JWT 迁中心（ADR 0033 D1）同路线。P-256 公钥是**公开材料**（私钥永在移动端安全硬件，ADR 0002），托管位置无泄露面变化；验签执行点从宿主引擎移到中心 WASM
- **宿主侧变更**：`host-auth` **退役 3 原语** `biometric-credential-bound` / `biometric-verify-signature` / `biometric-credential-bind`；`utils/auth/biometric.rs`（挑战管理器 + 验签）与 `system/app_context.rs` 的 `biometric_challenges` **整面删除**；宿主 `plugin_secrets` 的 `biometric:*` 死行按 v33 `jwt.key` 同款幂等清扫（不碰密钥环）
- **中心侧接管**：生物公钥真源 = 插件私有库新增 `auth_biometric_keys` 表（`biometric_key_get/set/delete`），验签在 WASM 内 p256（`auth_http/biometric.rs::verify_biometric_signature`，p256 crate 已在 wasm32-wasip3 探针验证可编译）；挑战闸门改查私有库，绑定/解绑写私有库（与配对记录解耦）
- **fail-visible ②**：旧 v33 产物仍 import 那 3 函数 → **实例化期**拿到点名「按 v34 SDK 重建」的错误，不是 trap 不是静默降级
- **迁移，明说**：宿主旧 `biometric:*` 行被清，已绑定生物认证的设备需**重新绑定**（一次性；配对记录与配对码 / QR / JWT 认证不受影响）。移动端 wire 流程（`/api/auth/biometric-*` 挑战-应答）**逐字节不变**，移动端**零改动**；宿主（ABI 34）与重建后的中心产物**必须同批发布**

#### 认证中心持有入场签发密钥，宿主不再有任何设备入场密码学（ADR 0033，桌面端 ABI v32 → v33；**破坏性：存量已配对设备需全量重新配对**；移动端 WIT/ABI 不动）

- **架构复核发现了什么**：`plugin_secrets` 里**躺着两把同名的 `jwt.key`**——一把属主是 `host`（供宿主 `JwtService`），一把是 `com.bedcode.terminal-session` 在 guest 内自己生成的；而插件那把是**死的**：生产签发走回宿主的 `device-token-issue` 原语，插件自带的完整 HS256 实现在生产路径上**零调用点**，而 WIT 注释却写着「插件密钥域与宿主不同，**无法也不应验签**」。「密钥域隔离」成立，「**密钥不出宿主**」不成立——guest 里就躺着一份明文副本。写错的那句现在是**被登记的失效假设**，不是被悄悄改掉
- **为什么可以交给中心**：`HS256` 是**对称**算法，验签方必须持签发方的秘密。而中心**本来就能**通过 `device-token-issue` 给任意设备签发入场凭证、且 `enforce_connection_policy` 会据此放行——「中心能签发」与「中心持有密钥」的权限差距，只差一次可审计的 API 调用 vs. 一块内存里的裸密钥。交出密钥**不增加任何实际授权面**，只是让实现**匹配既有的授权事实**。论证的边界也一并写明：它成立**因为中心可信**，而换掉中心（ADR 0031 本就允许）就等于换掉信任锚
- **宿主侧变更**：`utils/auth/jwt.rs`（`JwtService`）与 `utils/auth/host_secrets.rs` **整模块删除**——后者实测**零生产消费者**（生物公钥走的是**插件属主**的 secret-store，宿主属主的 store 只服务过 JWT），故随使用者一同删除。`host-auth` **退役 `device-token-issue` / `device-token-verify`**。四处验签点（HTTP `/api` 中间件 / 端点级认证档 / 协议网关 / WS 插件端点）改为**只问中心一次**（原为「宿主先验签 → 再问中心策略」两步），失败面收窄为一次调用。残留的 `('host','jwt.key')` 死密钥行由幂等迁移按**精确三元组**删除，不碰 `biometric:<fp>` 与插件属主行
- **宿主不许把这份事实偷偷查回来**：裁决门成功态由 `()` 改为 `AuthenticatedIdentity`，字段集**恰好三个**（`device_id` / `device_name` / `fingerprint`，被锁钉死）；中心放行却给不出可用身份时按**中心侧故障**处理（`deny_kind=unavailable`），宿主**绝不**回查本地凭据表——那正是 ADR 0022 §5.1.4 的「宿主侧回查」红线
- **密钥轮换同批落地**：轮换是把「guest 侧泄露」从理论风险变成**不可恢复风险**的解药。密钥环最多保留两代（当前 + 上一代），上一代在**最长 token TTL（7 天）**的宽限期内继续可验签；**轮换不撤销既有 token**（撤销归撤销域）。触发面 = 设备中心的操作员按钮（插件命令 `session.auth.rotate-key`）与组合式出口（`auth-grant` / `jwt` / `rotate-key`）
- **`kid` 是诊断标签，不是授权门**：它是声明在 claims **末尾**的**可选**字段 ⇒ 不带 `kid` 的 token 与迁移前**逐字节相同**（冻结向量钉住）。对称密码学下真正的闸门是「签名能否用环内某把密钥验过」——来自已裁代次的 token 在那一步就先被拒，而拿到环内密钥的一方写什么 `kid` 都验得过。拿 `kid` 当闸门**既冗余又让人误以为它参与了安全决策**
- **密钥环损坏一律显性失败，绝不「顺手重写」**：静默换新密钥会让全部已配对设备静默失效而系统看起来一切正常；`activate()` 读密钥环失败即**阻断激活**，不降级为进程随机密钥（那会让人以为已配对，实际每次重启全灭）
- **fail-visible 三形态齐全**：旧 v32 产物在**实例化期**拿到点名缺失 import +「按 v33 SDK 重建」的错误（不是 trap、不是静默降级）；退役函数名已从构建链权限映射表移除，词汇表外条目**加载即抛**；`host_has_no_entry_token_crypto` 让 `JwtService` / `verify_token_with_expiry` / `generate_device_token` / `verify_device_token` / `JwtClaims` 在生产路径重新出现即测红
- **迁移，明说**：签发密钥换了手，故**存量已配对设备需全量重新配对**（一次性，每个约 30 秒）——与 v24 退役 `pairings` / `connection_history` / `session_configs` 三表的既有口径一致。移动端拿到的是「凭证失效」类业务码而非网络错误，且它把 token 当**不透明串**，故移动端**无需改代码**。宿主（ABI 33）与重建后的中心产物**必须同批发布**：宿主已切而中心仍是 v32 产物 = 全部认证拒绝
- **性能不是决策依据，且代价略有上升——实测而非估算**：变更前实测中心往返 95.7–113.5 µs/op、宿主原生验签 5.96–7.09 µs/op（占热路径约 6%），合计 101.7–120.6 µs/op。变更后复测（N=1000，三轮）为 **130.6 / 135.1 / 135.0 µs/op** —— 合计**并非持平**，上升约 12–27%。这是搬迁的预期代价而非意外：HS256 验签改在 WASM 内执行（约 10–20 µs）而非原生（约 6 µs），ADR 0033 §4 在动手前就预判了这一点。决策理由不受影响——成本的大头是往返（~94%）而非密码学，吞吐仍约 7.4k req/s，而真实负载是几十/秒量级，且 token TTL 7 天意味着这条路径不是逐帧调用。D1 是纯按信任模型决策的；这里记录数字是因为「零代价」这种没被实测过的断言，恰恰最容易误导后来人

#### 认证中心改为显式注册，「没有中心」从「放行」变成「拒绝」（ADR 0031，桌面端 ABI v31 → v32；移动端 WIT/ABI 不动）

- **事故是什么**：2026-09-29 桌面端把所有带 JWT 的请求与 WS 连接全部拒掉（`auth-policy not provided by this plugin` → HTTP 401 / WS close 4001），而移动端无退避地自愈重连，98 秒内刷出 616 次拒绝、616 次 toast。宿主本身没坏，它只是**选错了认证中心**：SDK 让每个用 SDK 构建的插件**无条件导出** `auth-policy`（默认拒绝实现），而宿主取「按 id 升序的第一个候选」当中心——排序第一的恰好是没实现真实策略的 `com.bedcode.agent-hub`，于是全局拒绝
- **「谁是认证中心」从猜测变成注册事实**：中心插件在 `activate()` 内调 `host-auth.auth-center-register(methods)`，宿主持**单槽注册表**（`wasm_core/host_api/auth_center.rs`），第二个注册者**被点名拒绝**（含在册属主），停用时注销，宿主在插件停用/卸载时兜底回收。旧的「能力探测 + 排序取首个」与第二套发现机制（已退役的 `SESSION_MARKER_API` api_registry 锚点）**删除而非弃用**——防回接锁 `retired_auth_center_discovery_is_not_reintroduced` 让任一条被加回即测红
- **fail-closed 取代两条 fail-open 降级**：「无中心 → 放行」与「中心调用失败 → 放行」均已删除。无中心在册、中心调用失败、中心拒绝现在是**三类可区分**的拒绝，带结构化 `deny_kind`（`no_center` / `unavailable` / `policy`）与可读原因——三者的排障路径完全不同。代价是有意的：中心插件未激活时本机全部需认证面不可用，**认证面失效时放行等于无认证裸奔**
- **旧中心产物无法再静默失败**：启动序列在激活 L2 组件后就位检查「是否真有中心在册」，未注册则打**点名插件与修法**的 `error` 日志（「按当前 SDK 重建并在 activate 内调 auth-center-register」）；实例化期指引同时补了**反向**分支（v32 产物跑在旧宿主 → 提示「升级 BedCode」而不是「重建产物」）
- **组合式认证**：新增 `auth-methods-list` / `auth-method-invoke`，其他插件可直接复用中心已实现的认证方式（配对码 / QR / 生物 / JWT），不必复制第二份实现。宿主只校验「method 在注册表内」，随后**零解析**转发到中心 `auth-grant` 互调 api；中心返回的业务拒绝原样透传
- **移动端同批修（不动协议与 ABI）**：WS 客户端现在**保留关闭码**（`ServerClosed { code, reason }`；未携带按 1006），并把 **4001 / 4003 判为致命**——不自愈重连、只发一次「需重新配对」toast，因为对着一个正在故意拒绝的宿主机重试只会制造日志风暴。退避额外加了**硬下限 1s**（钳在 `calculate_delay` 里而不只是配置里）与**同因熔断**（连续 5 次同因失败即放弃，原因变化重置）作纵深防御
- **迁移**：本仓所有插件产物均由源码构建，故桌面宿主与重建后的中心产物天然同批；其余三个 wasm 应用不是中心，旧产物在 ABI 32 宿主上仍可加载

#### 插件分类落为三层显式分层，「谁先加载」不再靠隐式顺序（桌面端宿主 + 插件 SDK；**不动 WIT/ABI —— 只动 manifest**）

- **今天的现状不是「没有分类」，而是分类说不出我们需要的东西**：`PluginKind` 原本两值（`Application` / `System`），而 `System` 的定义是「向能力注册表提供 host-* 同形能力」——它是**基础服务的代理身份**，却**零插件使用**（`ROUTABLE_CAPABILITIES` 只有 `host-storage` 一项）。有两样东西装不进去：认证中心是**宿主网关的裁决依赖方**（宿主主动调它，本仓唯一的反向依赖，既不是「提供能力」也不是「产品功能」）；「用完即弃」的 worker 则完全没有表达位——`CallModel::EventLoop` 是**常驻**属主任务，与即用即弃语义正好相反
- **三个角色、加载顺序 `L1 → L2 → L3`**：`basic-service`（L1 引擎域组件，最先激活，其 host-* 导出注册为能力提供者）→ `internal-business`（L2 宿主裁决依赖方）→ `business-app`（L3 业务应用面，**缺省**）。启动序列里原本「一批 System 组件」拆为**两批显式**（`activate_role_driven_components`），持久化批量就是 L3 批且现在只收 L3；批内按 id 排序，不引入隐式优先级规则。历史拼写 `system` / `application` 仍按 serde 别名解析（旧产物零迁移，序列化统一写新拼写）；非法 `type` 在**构建期**（`manifest-validate.js`，文案点名合法取值）与**加载期**（serde）双侧拒，不静默回落 L3
- **宿主只认谓词、不认角色名**：加载顺序读 SDK 常量 `PluginKind::ROLE_DRIVEN_LOAD_ORDER`，所有判定点走 `is_role_driven()` / `provides_host_capabilities()` / `is_business_app()`；宿主源码里**不出现** `PluginKind::BasicService` / `InternalBusiness` / `BusinessApp`——新增角色不必改宿主，分类学也不会退化成「宿主按角色名做业务判断」（锁 `host_switches_on_role_predicates_not_role_values`）
- **L1/L2 是角色驱动：启停真源是「角色」不是用户开关**——两者不进持久化激活表。单个失败仍不阻断其余：L2 激活失败时 L3 照常激活，失败以点名角色的 `error` 日志 + 插件列表 Error 态可见
- **`lifecycle: ephemeral`（业务 worker）只登记类型、别的都没做**：它的存在理由是**内存生命周期**而非业务分层——wasm 线性内存只增不减，宿主单实例限额只记账不释放，长驻实例处理大输入会单调涨到限额并 trap，且不重启进程不自愈。一次性实例机制与调度框架未落地，故声明 `ephemeral` 在**构建期与加载期双侧显性拒绝**（文案点名 ADR 0032 §6 缺口清单）——静默当常驻处理恰好是它存在理由的反面
- **L2 红线落成代码而非散文**：`internal_business_host_dependency_stays_gated` 把「宿主可碰 L2 桥接面的文件」钉死为显式白名单（网关 / WS 认证中间件 = 安全闸门；`session_gateway` = 零解析转发；`host-auth` 组合式认证原语 = 零解析转发 + 单中心仲裁），并做**反向自检**（白名单条目不再命中即报错），防止白名单腐化后静默放行越界消费点；`l2_gate_returns_decision_only` 把裁决门签名钉死为 `Result<(), String>`——宿主一旦能从 L2 回复里解析出产品载荷即转红
- **第一个真实 L2 是 `com.bedcode.terminal-session`，两段判据同批落地**（ADR 0031）：静态声明 `"type": "internal-business"` 决定加载顺序，`auth-center-register` 做动态就绪与唯一性仲裁，只上线其一会让两段判据自相矛盾。**该角色的用户可见后果**：认证中心的启停转为角色驱动——它不再进持久化激活表，在插件管理页停用它只对当前会话生效，下次启动按角色恢复（与 fail-closed 同向）

#### Agent Hub 文件访问改为批量授权 —— 一个业务一次弹窗，而不是逐文件弹窗（桌面宿主 + wasm 应用 `com.bedcode.agent-hub`；**不动 WIT/ABI**）
- **要解决的问题**：一次业务操作读写多个文件时，每个文件各自弹一次授权框——更有甚者授权根本「记不住」：宿主进程产物（`~/.bedcode/agent-hub/runs/*.log`）**每次运行都是新文件名**，精确粒度的「记住」只记了这一次的文件，下一次运行再问一次；usage 扫描读 `~/.claude/projects/` 下的 Claude 会话文件，而该目录**没有任何授权覆盖**（全局 `.claude/` 白名单退役后静默读失败）；打开供应商页一次弹三个框（settings.json + 两个桥接文件存在性）；应用一次供应商到 Claude 最多弹五次
- **宿主**：`~/.bedcode/agent-hub/runs` 加入 agent-hub 第一方免弹窗清单（判据②：插件自持瞬时产物，Exact 粒度授权无法表达「整目录」——逐次弹窗每次运行都重复）。范围严格限于 runs/ 子目录，不含用户内容；审计投影与 `auth_policy` 概览测试自动覆盖
- **插件侧**：activate 批量清单补 `~/.claude/projects`（usage 扫描的会话真源）；`scan-usage` 改为对**本次扫描的实际来源根目录**批量授权（一次弹窗列全部，已授权路径宿主静默跳过），取代以「activate 固定清单」为闸门的旧逻辑——同时解掉旧闸门永远够不到的 Claude / 自定义来源；供应商状态组装（`build_state`）与 Claude 目标应用在动作前一次性批量授权三个 Claude 路径（settings + 两桥接）
- 成本：不动 WIT/ABI、权限位零改动；新增逻辑全在 guest（`scan_auth_roots` 去重纯函数、`claude_auth_paths`）带单测；前端仅注释更新

#### 按应用分档的授权策略与授权记录管理 —— 文件/网络两侧三档 + 可见可撤的界面（桌面端宿主；**不动 WIT/ABI**）
- **要解决的问题**：宿主此前有「每应用文件授权」（`fs_granted_paths`，扁平前缀表，授了读**就等于**授了写），
  但**网络侧零记录**——声明了 `network:http` 的插件可以静默访问任意地址。用户诉求有三点（每应用独立记录、
  按资源的授权策略、界面上可查可撤），此前只有第一点大致存在
- **每个（应用 × 资源）一条三档策略**，只回答一个问题：*遇到记录未覆盖的目标时，要不要问用户* ——
  **总是询问 / 默认 / 始终允许**。它**只决定是否询问**：不决定权限位是否生效，也**不放松任何硬闸门**
  （manifest 声明门、SSRF 与公网→私网重定向拦截、规范化失败即拒、配额、属主隔离全在策略层之外或更靠前）。
  「忽略权限」这一措辞被明确否决：安全界面宁可低估，不可高估。策略**判定时实时读取、不缓存**——
  缓存会让「我已改成总是询问」变成策略语义上的谎言
- **授权记录成为真源**（新增两表 `plugin_auth_policies` / `plugin_auth_records`，幂等迁移 + 索引）：
  文件侧记规范路径前缀**加生效操作集**，于是「授权读」不再等于「授权写」；旧的扁平表退化为
  **只读回退**，仅对「没有被显式管理过的子树」生效（存量用户零感知）。网络侧记归一化 origin
  （`scheme://host:port`，默认端口显式化、host 小写）——**query 与 fragment 绝不进入落库 target**
  （凭据红线），可选的 path 前缀按**段边界**匹配，`/v1` 不会命中 `/v1abc`
- **出站 `host-http.fetch` 会询问**：未记录的新 origin 弹一次窗（新事件 + 新命令，两者都绑定宿主面凭证
  ——插件面凭证不能代答）；同 origin 的在途请求**合并为一次弹窗**并带 2s 复用窗口，因为 agent-hub
  一次刷新就会向同一站点发几十个请求，逐次弹窗等于该功能在实践中不可用。`deny_always` 落硬拒绝记录，
  且硬拒绝记录在**任何档位**下都生效——「总是询问」跳过的是 allow 记录，绝不跳过用户已经说过的拒绝
- **留痕是事实而非承诺**：「始终允许」下的免询问放行仍以 `source='always_allow'` 落账，界面上标
  **「未经确认」**并与用户确认记录视觉区分。「总是询问」档下用户点「允许」**不落账**——该档不读记录，
  落一条没人会读的记录只会在管理界面显示一条假的「用户已授权」。每（应用, 资源）封顶 500 条，
  超出丢弃并在 core-monitor 计数
- **管理界面在设置页「应用授权」**（二级入口，不占一级菜单——权限不是业务门面），按**风险排序**列出
  全部已安装 wasm 应用（任一资源为「始终允许」即置顶）。每行展开为该资源的三档控件、文件目录与网络地址
  清单及逐条撤销；应用详情页给出对应的四分区（用户已授权 / 免询问自动放行（未经确认）/ 内置免询问 /
  硬拒绝）。「取消授权」= 删 allow 记录 + 落一条 deny 记录；deny 本身可移除——两种意图都有出口。
  详情页原有「权限」区块改名**「申请的权限」**：静态 manifest 声明与运行期落账是**正交事实**，
  合并会暗示两者之间存在实际并不存在的映射
- **第一方免询问目录清单改为可见可撤**（`~/.agents`、`~/.claude/skills`、项目级
  `.claude`/`.codex`/`.pi`/`.opencode`）：这批条目**刻意**排在档位之上（否则「总是询问」会把一次技能同步
  拆成 N 次点击），而这正是 spec 要求它们**在两页都可见**的原因。撤销 = 落一条 deny 记录，由判定链在
  第一方层之前拦下；移除该 deny 即恢复免询问。段名形态保持只读——项目根每次由用户选，落不成可复用的目标
- **生命周期**：停用**保留**记录与策略（停了又开不该变成全部重新弹窗），卸载**清空**（重装即全新授权，
  与 ADR 0020 内容哈希钉扎同调）。两者用一条锁固定——它们看起来是同一个操作，很容易被「顺手统一」
- 代价：不 bump ABI（`fetch` 的 import 签名与返回形状一字未改——变的是宿主内部的**准入条件**，
  已按 v22 总线 topic 与 `fs:pick` 的先例登记在 WIT 的「不 bump 版本号的语义变更」段）、
  移动端零改动（ADR 0018：移动端是自持业务 App，SAF 本就按 tree URI 授予）、四个 wasm 应用零改动

#### 系统原生文件选择器升级为「声明即启用」的受限能力 —— 新增 `fs:pick` + 选择后授权校验（桌面端宿主；**不动 WIT/ABI，改的是 manifest 准入**）
- 系统选择器本身**早就存在**（`host-platform.pick-files` / `pick-folder` / `pick-folders` 走 `tauri-plugin-dialog`
  —— Windows IFileDialog、Linux xdg-desktop-portal），但**无门**：任何插件都能调起它白拿绝对路径，选完之后也没人管。
  现按用户诉求拆成两段：
- **准入门 —— 新权限位 `fs:pick`（manifest 必须单独声明）**：未声明的插件拿到**点名缺失权限位**的显性
  `Err`，且**在弹对话框之前**就返回（既不是无信息的 "permission denied"，也不是让用户先把盘逛完）。
  刻意**不并入** `fs:read` / `fs:write`：「能调起选择器」与「能读任意路径」是两种权力，合成一位会把「只想让用户
  挑一个文件传出去」的插件被动升级成全盘读取。词汇表 32 → 33；真源仍在桌面 SDK，两份生成物（打包 CLI +
  宿主前端）重出，词汇漂移锁与 `generated_vocabulary_know` 同时覆盖新位。`manifest-gen` 亦按
  `platform_pick_*` 调用点自动推导 `fs:pick`，两个真实消费方（`file-transfer` / `agent-hub`）已自动声明
- **结果门 —— 路径交到插件手里之前先过授权**：对话框返回后，每个选中路径都走既有 `fs_auth` 三层校验。
  已落在**已授权目录**前缀下（或第一方归属清单内）的路径**静默放行**；只有未授权部分才弹一次框，且多路径
  合并为一次询问。拒绝 / 超时 / 无弹窗通道 → `Err` 且**一个路径都不回传**——回空数组会被读成「用户没选文件」，
  从而静默丢数据（fail-visible）
- **选择器来源的授权按「所在目录」落账**（文件 → 父目录，目录 → 自身）：用户点头的对象是这个目录，按文件记账
  会让同一次传输里的每个文件都再问一遍。弹窗会明说「勾选记住 = 授权该目录」，且重复授权不再堆叠重复条目；
  顺带修掉 `respond` 持 pending 队列锁跨 `await` 落账的问题（改为先出队再落账）
- 取消选择仍是取消，不额外弹授权框。落锁：门禁**顺序**由「错误必须点名权限位而非无头限制」断言；「已授权 ⇒
  不弹框」在无头校验器里落锁（那里根本弹不出框，能过本身就是证明）；目录级落账走**真实 `respond` 链**断言。
  两个变异体（目录落账改回文件落账、取消「已授权前缀跳过」）均使套件转红
- **不 bump ABI**（沿用 v22 总线 topic 的先例）：`pick-*` 三个 import 与签名一字未改，变的是 manifest 层准入
  条件——权限门本就是运行期机制，与「没声明 `fs:read` 就调 `fs.read` 被门」同一套。旧产物调 `pick-*` 会在
  调用点拿到点名权限位的错误（改 manifest 即可，不必重编 wasm），已登记在 WIT 的「不 bump 版本号的语义变更」段
- **移动端有意不跟演**（ADR 0018 双端偏离，已在 WIT 登记）：移动端 SAF 选择器本身就是系统按 tree URI 授予访问权，
  「选择即授权」在那里天然成立，无需该位。`reveal-in-dir` 维持无门——它不把插件没有的路径交给插件

#### Agent Hub 使用统计看板重做 —— 会话列表去重、多维图表（桌面端，wasm 应用 `com.bedcode.agent-hub`；**WIT/ABI 不动，宿主未动**）
- **重复的会话列表已删除**：统计分区与日志分区渲染的是同一批 `usage_session` 行，且两者**游标语义相反**（追加 vs 按页替换）、**筛选条件共享**——于是「统计加载更多 → 日志翻页」会出现重复行，日志表格甚至显示 45 行却标着「第 1 页」。现在会话级明细**只在日志分区**，看板只回答聚合问题。`statSessions/statTotal/statLoaded/loadMoreSessions/setListFilter` 一并删除（已上锁：断言返回面无这些 API，且挂载只发一次 `list-usage-sessions`）
- **新增聚合参数**（guest `usage::get_stats`，全部在实机 160 会话 / 8 项目 / 2 CLI 库上实测过）：**时间窗**（`days`，0 = 全部，越界在服务端夹到 `1..=3650`、非法值回落全量而非静默截断）；`total` 增加活跃天数、去重项目 / 模型数、首末时间戳；各分组行补齐三个缓存 / 推理桶与 `last_at`；**新增 `byHour`**——按宿主本地时区的 7×24 节奏矩阵
- **token 总量只定义一次且不含推理**：`输入 + 输出 + 缓存读 + 缓存写`。推理是输出的**子集**（claude 的 `thinking_tokens` 就在 `output_tokens` 内），重复相加会放大总量；改为单列「推理（输出内）」指标，并有测试锁住（加错会得到 150 而非 100）
- **图表手写 SVG/HTML，未引入图表库**（产物 +24 KB，而 ECharts 估摸 +500 KB）：token / 会话 / 成本 / 时长趋势面积图（含**构成堆叠**模式，色带按**固定语义序**——逐日重排会让色带左右乱窜、无法追踪）、**7×24 节奏热力图**、**CLI 占比环**、项目 / 模型排行条。指标选择全局统一，一个选择同时驱动趋势、排行与占比
- **测试（而非评审）抓出一个真实排行 bug**：`GROUP BY` 查询里的 `ORDER BY tokens_in + tokens_out DESC` 会被 SQLite 解析成**输入列**（组内任意一行的值）而非 `SUM()` 别名——「Top 项目」实际顺序等同随机（1.28 亿 tokens 的主项目排到第 9 行里的第 2 位）。`byProject` / `byCli` 现改为显式重写聚合，并上锁：有人改回裸列名即红
- **热力图按名次分档，不按线性比例**：真实用量强偏态（最高时段 19.3M、最低 0.1M；单日最高 128M、最低 2 万），线性分档会把除峰值外全部压进最低档，恰好丢掉「哪些时段次高」这个图表存在的意义。测试断言在实机数据上四档全部用上
- **无障碍是登记在册的契约，不是事后补的**：4 个分类色由一次性穷举搜索在本仓自有对比度判据下定出（12 套主题：对卡片 ≥3:1、两两 ΔE ≥25、与语义三色 ≥25、与色板 primary ≥20、相邻色相间隔 ≥50°、浅深同色相）；热力图低档作为**登记豁免**（顺序标度的低端本就该「若有若无」，且每格有 `title` + aria + 逐行合计），作为交换补了**更严**的检查——亮度单调 + 顶档 ≥3:1。趋势图的交互层是 HTML 覆盖按钮，键盘 focus 与 hover 走同一条取值路径；图形 `aria-hidden`，另配可切换的数据表作为第二读取通道
- **旧时间窗响应按序号丢弃**（与探测域 `AgentHubState.seq` 同一手法）：切窗与扫描回流同时在飞时，否则会出现「pill 选新的、数据是旧窗」；已有测试复现该竞态
- 另：默认时间窗改为近 30 天（pills 里一键可切「全部」）；窗内无数据会显式说明并给「看全部」出口，而不是摆一个空看板
- 成本：WIT/ABI 不动、宿主模块未动、无新命令，`get-usage-stats` 增加一个可选入参并多返回两个键；插件 `dist/index.js` 由 422 KB 增至 446 KB（gzip 123 KB）

#### Agent Hub 日志来源支持每来源多目录（桌面端，wasm 应用 `com.bedcode.agent-hub`；**WIT/ABI 不动，宿主未动**）
- **来源 = 名称 + 任意多个目录，不再是单目录**：每个来源（含 `pi` 等内置来源）都可以追加日志目录，例如项目内 `.pi/sessions` 可以并进 `~/.pi/agent/sessions` 同属一个来源，而不是被塞进独立来源。持久化状态在读取时把旧单 `path` 幂等迁移为 `paths` 数组（内置 / 自定义条目一视同仁），不丢用户已添加的自定义来源
- **目录级增删 + 归属规则**：新增 `add-usage-source-path` / `remove-usage-source-path` 两命令。目录在**全来源间全局唯一**（同一目录挂两个来源会让同一批会话文件以两个适配器名各入一次库）；内置来源的默认路径不可移除（在其上追加的用户目录可移除）；自定义来源的最后一条目录拒绝移除（应整体移除来源）；sqlite 源（opencode）仍是单文件只读。所有校验（`~/` 展开、绝对路径与扫描脚本安全）与 `add-usage-source` 共用纯函数并单测覆盖
- **扫描枚举全部目录**：`scan_sections` 改为按（来源, 目录）出段、同一来源的多个目录用**同名分段**——`parse_listing` 会把它们全部归到适配器名下，水位键 / 适配器键照旧；内置默认路径仍按当前 home 展开，同一来源的多个目录合并成一条扫描计数
- **来源面板渲染目录清单**：每个来源下缩进列出目录行（内置默认路径带标记不可移除，用户追加目录逐行可移除），并有每来源的**「添加目录」**按钮——复用新增来源同款 fs:pick 选择器流程（无名称输入，名已定）；整来源新增流程文案改为「添加日志来源」以区分两者
- 成本：WIT/ABI 不动、宿主模块未动、无新权限；新增两条插件命令；插件 `dist/index.js` 增至 457 KB（gzip 125 KB）

#### 「添加日志来源」表单加来源名下拉，重复目录在选中时就拦下（桌面端，wasm 应用 `com.bedcode.agent-hub`；**WIT/ABI 不动，宿主未动**）
- **一份表单两种动作，由名称决定走哪条**：来源名称现在是**已添加来源名的下拉 + 可手输**。下拉选中已有来源（如 `pi`）→ 目录追加到该来源下；输入的名字不在列表里 → 新建来源。走哪条**只看最终名称是否已存在**，所以手输把名字打对成 `pi` 与下拉选中效果完全一致——不存在第二种模式需要用户分辨。sqlite 源（opencode）刻意不进下拉：它是单库只读文件，guest 本来也会拒绝给它加目录
- **重复目录在选中时就地提示，不等提交**：fs:pick 选中的目录若已被**任一**来源登记，立即在路径框旁显示就地错误「该目录已添加过，重复无法添加」（`aria-describedby` 关联），且「确认添加」保持禁用，不让用户走一趟注定失败的提交。guest 侧 `error.pathTaken` 仍是最终仲裁，这里只是更早的反馈——与每来源「添加目录」流程原本依赖的规则同源
- 成本：WIT/ABI 不动、宿主模块未动、无新插件命令（两种动作分别复用 `add-usage-source` / `add-usage-source-path`）；新增 3 个 i18n key（zh-CN + en）；dev-shell mock 补上此前缺失的两条目录级命令，其重复检查也从「比较状态里早已不存在的 `path` 字段（恒不成立，形同虚设）」改为真正按 `paths` 数组比对

#### 插件调用模型升级 —— 事件循环属主（可灰度）+ 按需 async 化评估退役（桌面端；**默认值不变，ABI 不变**）
- 宿主侧插件调用从「每次调用抢一把实例锁 + `spawn_blocking` + `block_on_async`」收敛为**装配条目**形态：`WasmInstanceEntry { meta, call_model, slot }` 是 Store 的**唯一**宿主，调用一律经门面 `PluginHost::call_guest`（异步）/ `call_guest_blocking`（同步桥）；`mutex` 分支**逐字保留原实现**（存量插件返回值、错误串、trap 恢复逐字节等价），`event-loop` 分支为每实例一个常驻 `run_concurrent` 属主循环（`start_call_concurrent` + oneshot 结算 + `select! { biased }` 保「启动顺序 = 入队顺序」）
- 灰度开关 `CoreConfig.call_model`（`mutex`（默认，回退窗口）/ `event-loop`，`wasm-core.json` 可按名覆盖，非法值加载即报错）；实例级快照，**reload 即切换**；**默认值不变**，回退窗口保留（切默认待真机复验）
- 停机语义（fail-visible）：停用 / 卸载 / 重载**先停属主丢 store 再回收资源**；`event-loop` 实例的 guest `on_shutdown` / `deactivate` 显性跳过并计数（`owner_cleanup_skipped` + warn，不静默）；trap = 整实例不可用（与既有语义逐字等价），在等请求逐条显式失败；能力转发加 5 s 超时兜底（环依赖从「永久死锁」降级为「有界失败」）
- **按需 async 化评估结论：退役**（ADR 0029 / `.scratch/2026-09-26-plugin-concurrency-model/`）——WIT 层 `async func` 在当前锁定工具链不可用（async-lifted 导出入口即 abort）；改走「宿主实现侧 async（`func_wrap_async`）」后实测证明**收益不成立**：async import 挂起期间，同实例第二条显式调用**零进展**（实例级门，与 import 是否 async 无关），且该期间**属主闭包完全不被调度**（属主停摆）⇒ 「慢调用堵死同插件交互」无法用异步化解决；两条结论已固化为边界锁（`runtime/tests/p3_async_host_import.rs`，转红即重评）。可行替代方向为**非等待形态**（立即返回句柄 + 事件回调，参考 `host-process.run` / 流式 `fetch`），待单独立项

#### 移动端 WS 面重新锚定插件端点 —— 控制面迁 HTTP、session-control 事件通道、终端流重写、WS 帧级加密退役（移动端；桌面 wasm 应用仅补最小事件广播）
- 移动端 WS 面对齐桌面插件端点（旧 `/ws/event` 与 `/ws/terminal/session/{id}` 现已 404）：**事件通道**
  （`/ws/plugin/com.bedcode.terminal-session/session-control`）改极简认证帧 `{"type":"auth","token":"<jwt>"}`
  + 事件帧 `{"type":"event",…}`，经插件广播恢复 `ws_sync_*` 事件面；事件**不重放**——重连后前端经
  `ws_event_channel_ready` 触发 HTTP 对账（`loadActiveSessions` + 活动会话队列按需拉取）。**终端流**
  （`/ws/plugin/.../terminal`）重写为插件新协议（fresh-subscribe 回放 + 裸字节 + 本地字节计数 +
  `ring_resync` 唯一重锚 + `session_stopped`）；TB v3 解析退役
- 会话控制 / 会话与配置加载 / 终端输入 / 插件 API（`session.list`、`terminal.sendInput`）全部迁 HTTP
  （`/api/sessions/*`、`/api/configs`）；旧 `Message` 信封裁剪为 5 变体仅服务 WS legacy 认证集成场景；
  任务面旧前缀 `com.bedcode.auto-task` → `com.bedcode.terminal-session`
- **WS 帧级链路加密退役**（桌面 `TrafficChannel::WsPlugin => false`）：`ws-terminal` / `ws-event`
  设置开关、`useLinkEncryption` 的 WS 字段、`install_event_crypto` 与 `WsClient` 加解密管道全部删除；
  HTTP 信封加密保留（开关 / strict / pinning 语义不变）
- 收敛：配置增删改推送与设备上下线事件**不再推送**（移动端改按需 / 对账）；会话状态字面量统一到桌面
  wire 值 `waitingInput`（原漂移为 `waiting_input`）并加用例锁
- 每条退役面均落结构锁（事件通道零信封、生产零 WS 加密残留、会话状态比较零 `waiting_input`），变异自检通过

#### 终端输出背压回归 — 交付即账 ack + 双水位迟滞（桌面端，插件侧；宿主零改动）
- 下沉前的机制（宿主 per-subscriber 窗口：私有 `acked_offset`、双水位、park 等待、30 s 僵尸回收）回归到 `com.bedcode.terminal-session` 应用内部——宿主 PTY 环不感知消费者：`session.output.ack`（`{sessionId, offset}`）推进会话单调水位；`session.output.pull` 在未确认窗口达上沿（128 KiB）时抑制，降到下沿（64 KiB）以下才放行——取值与迁移前一致，含编译期自检的不变量 `ack ≤ low < high`
- 前端按**交付进写入管线**（而非「渲染完成」）记账，按累计 64 KiB 或空闲 250 ms 节流回发，与迁移前 `useTerminalOutputStreamChannel` 同款；被抑制的响应让拉取循环不推进游标、不写入，退避 200 ms（= 旧 `park_poll_ms`）后重试；`truncated`/resync 同时重锚游标与水位
- 驻留超 30 s 打一次告警（旧语义是回收僵尸连接；拉取模型无连接可回收）并继续拉取，不静默卡死
- 新增只读诊断命令 `session.output.watermarks`（`{sessionId?}` → pushed/acked/unacked/parked + 驻留/截断计数，对齐旧 `SubscriberStats`）；前端在驻留退出时取一次快照打日志，真机复验无需翻引擎日志即可判定「背压是否真发生过」。计数**刻意无时钟**（wasm32 无系统时钟，驻留时长由前端计时）
- **拉取节奏对齐旧引擎**（spec F5）：快/慢档 100/500 ms → 50/250 ms，单 tick 预算 4 × 16 KiB = 64 KiB（= 旧 `FETCH_BUDGET`）——吞吐上限不变、输出可见延迟减半；节奏裁决抽成可单测的纯函数 `terminalPullPolicy`，并修正了一处行为不一致（驻留若发生在慢档期会滞留在慢档，现驻留恒定快档）
- **宿主零改动**——不动 WIT/ABI；真正的 push 引擎事件（毫秒唤醒）仍是已登记的后续项（P2）

#### PTY 输出可用通知 —— 宿主限频唤醒（桌面端；**不改 WIT/ABI，只加一条总线 topic**）
- 拉取模型留了一个延迟洞：会话停在慢档（250 ms）时，首个新字节要等下一轮轮询才发现。宿主现在在输出环
  出现新字节时，向属主私有总线 topic `<owner>::pty:output` **限频**发布提示（同一句柄 ≥50 ms 一条，
  载荷只有 `{ ptyId }`，不带字节）——形态是下沉前 `OutputNotifySink` 写侧装饰器包住 `PtyRingSink`，
  因此 `pty/` 引擎保持**零**总线依赖
- **数据面一字未改**：字节仍按游标拉取（`ring-fetch`），`truncated` resync 语义逐字不变，提示**按设计
  可丢**（无订阅者 / 队列满 / 插件未激活 / 被限频合并，且从不重放）。正确性因此**仍然完全**落在前端
  节奏 + resync 上，事件只买延迟（空闲后首个字节：慢档 250 ms → ≈0）
- 插件侧只当加速器：`terminal-session` 在 `activate` 期订阅（失败**降级**为纯轮询、绝不阻断激活
  ——与 `pty:exit` 的强门相反），并把 `ptyId → sessionId` 转发成前端事件 `session:output-available`；
  命中即立刻拉一轮，非本会话 id 与脏载荷一律忽略，卸载释放订阅。SDK 侧与 `PTY_EXIT` 并列导出常量
  `PTY_OUTPUT`
- 落锁：漂移锁把宿主事件名钉在 SDK 常量上（`output_event_name_matches_sdk_subscription_helper`）——单侧
  改名会静默退化成「订阅成功但永远收不到」；限频本身用真 PTY 突发验证（毫秒内二十余次 push ⇒ 通知数
  远少于 push 数，即装饰器确实生效、通知没退化成逐块推送），限频裁决纯函数另设单测（首次必放行 /
  恰好等于间隔放行 / 窗口内丢弃 / 时钟回拨按未到处理）

#### 对等传输编排整体下沉插件 — 宿主 `peer_engine_*` 收敛为句柄表 + 引擎事件桥（桌面端，**v30 + v31**）
- 传输任务编排此前仍留宿主（活跃任务状态机、发送并发闸门 1..=8 默认 3、历史封顶 200/100、
  serve 供流双端记账、`peer_name` 展示名解析、取消原因码映射、pull 任务行预登记 + 并发信号量）
  ——与「宿主回查内核拿会话」漂移同型，整体下沉 `com.bedcode.file-transfer` 插件**事件归约
  状态机**（`transfer_store.rs`：建行 / 进度 / 终态按方向映射原因码 / 封顶 / 重试回放——
  唯一任务真源在插件私有库）
- **票 1（v30，纯增量）**：`host-peer.active-transfers`（活跃批投影，首屏兜底查询）+
  `collect-outgoing`（目录递归源枚举 + 批内同名去重）；引擎原始事件直推
  `peer:transfer-event` / `peer:receive-event`（载荷带 `tsMs`——wasm32 无时钟），过渡期与
  旧快照 topic 双写
- **票 2（插件侧）**：事件归约消费原始流（快照 merge 退化为校正 + 对账——偏差经
  `reconcile_diff` warn 留痕）；插件侧**发送并发闸门**（`PENDING_SENDS` 队列，终态空出
  槽位后放行）；`peer_name` 从插件自持设备快照解析（短指纹兜底）；activate 期经
  `peer_active_transfers` 首屏重建
- **票 3（v31，破坏性）**：`host-peer.resume-all-transfers` **退役删除**（「全部恢复」编排归
  插件遍历自身暂停批逐个调用；`file-transfer.resume-all` 命令保留、实现改为逐批恢复）；
  `send-files` 收窄为「一次调用 = 一个会话立即发起」——宿主并发闸门删除，旧载荷
  `concurrency` 字段**调用期显性报错**（点名重建）；旧快照 topic `peer:transfer` /
  `peer:receive` 从总线映射退役；`pull-files` 删并发信号量与任务行预登记（逐文件会话立即
  发起，`pull-started` 引擎事实事件喂给插件归约）；`active-transfers` 实现改三处句柄面投影
  （wire 形状不变）；宿主残面删除——`peer_engine_transfer.rs` 以 `SendSessionHandle`
  （CancelToken / PauseSlot / epoch / sources，sources 为 redial 续传必需）重写、源收集剥离
  `source_collect.rs`、`peer_engine_receive.rs` 只留询问回执表 + 事件桥 + 策略闸门
  （concurrency 设置字段删除）；15 符号防回接锁
  `retired_peer_transfer_orchestration_is_not_reintroduced` 变异自检通过（注入 → 转红 →
  还原 → 回绿）
- 旧 v31 前产物**实例化期**失败并附点名 `resume-all-transfers` 的重建指引；仍发送
  `concurrency` 字段的旧产物在 `send-files` 调用期拿到同一指针（fail-visible 双层）
- 移动端零改动且明确不在兼容范围（`host-peer` 为桌面独有；wire 数据面协议不变）——受损清单：
  `.scratch/2026-09-25-peer-transfer-orchestration-downsink/mobile-impact.md`

#### WS 动作词表声明式化 — `contributes.wsEndpoints` + 插件侧分派（桌面端）
- expand–contract（票 09a/09b/09c）：会话/终端 WS 动作词表从「宿主硬编码 match 表」改为「插件声明端点 +
  插件侧分派」。SDK `PluginContributes.wsEndpoints`（形态同 httpEndpoints 两式）在**激活期**登记到
  `/ws/plugin/<id>/<path>`（端点路径**单段约束**，与 `host-websocket.register-endpoint` 同口径；deactivate
  回收、激活期重登记）
- `com.bedcode.terminal-session` 声明 `session-control`（auth=jwt）；新 `ws_control` 域承接动作词表解释
  （list / start / stop / remove / resize）——互调 api `session-ws-control`（宿主 `/ws/event` 转发路径）与
  `events-ws.on-client-message` 直连帧协议共用同一实现（插件为此补 `ws:server` 权限位，回包判据位）
- 宿主 `services/session_control.rs` 重写为**传输面转发层**：声明闸门（端点已声明且插件激活，否则显性报错）
  → 原始动作 JSON 转发插件互调 api（宿主不解动作名语义）→ 响应动作 JSON 套回 `Message::SessionControl`
  信封（原 message_id；信封 `session_id` 取自响应动作的 `session_id` 字段 = 新建会话 id，与旧宿主路径逐字一致）
- 旧 `handle_control` 业务 switch 删除——宿主 WS 层不再内联任何业务动作名语义（grep 断言）；
  `Message` / `SessionControlAction` / `SessionSummary` 保留为宿主传输面契约。终端输出订阅/输入
  （数据面，H1）留在宿主引擎
- 移动端 wire 逐字不变：旧 `/ws/event` `Message::SessionControl` 请求/响应形状零改动
  （`pty_session_chain` 集成测试经转发层全绿）；声明端点是新路由，供未来客户端直连

#### 终端会话中心插件 — 设备 / 会话 / 任务合并为单一内置插件（桌面端）
- 新内置插件 `com.bedcode.terminal-session`（Application 形态、`rust-ts`、wasip3 组件）承接原先散在 内核、`com.bedcode.devices`、`com.bedcode.auto-task` 三处的产品域：配对与信任与首连确认编排、会话配置 CRUD 与生命周期编排、Agent 任务域（队列状态机、定时任务、agent hook 安装）。插件 id 由 `com.bedcode.session` 改名（票 06）；旧 HTTP 前缀与旧互调 api 名在过渡期经双投窗口别名仍可达（票 07）
- `com.bedcode.devices` 与 `com.bedcode.auto-task` 退役；其模块、侧边栏视图、终端工具栏按钮、任务弹窗、文案表与随包 hook 脚本并入合并插件，按域重组而非逐文件平移
- 贡献式 UI「界面维持、贡献方换人」（spec D6）：四个侧边栏目录（设备配对 100 / 连接历史 101 / 终端会话 200 / Agent任务 210）、经新扩展点 `ui.registerSettingsSection` 贡献的设置分组、终端工具栏按钮；宿主内置入口在插件 `Activated` 时让位，未激活 / error / 停用时由宿主兜底壳接管
- 内核去业务化落地：会话结构体的四个任务字段摘除，换为不透明注解槽（`session-id -> map<string,string>`，内核只搬运透传、绝不解释键名）；线协议形状（`taskStatus` 等）不变，老客户端零改动
- 升级后任务历史一条不丢：宿主侧一次性、best-effort、幂等迁移，把六张任务表从退役插件的私有库搬进合并插件私有库，按列名交集拷贝并落账本戳，重启不重复插入
- **终端窗口域整体下沉插件（票 01–05）**：xterm 渲染 / 写入管线 / scroll/resize / IME 守卫迁入 `plugins/terminal-session`；宿主只留引擎原语（窗口编排、设置/背景图桥、PTY 引擎）。宿主降级终端实现（`TerminalPreview.vue` / `composables/terminal` / `utils/terminal` / Tauri Channel 输出传输）摘除；插件输出经 WIT 二进制原语 `host-session.output-ring-fetch`（`session.output.pull` 命令，自适应轮询，无 Channel 桥）。停用插件后终端入口消失且宿主窗口 API 过激活门禁显性报错
- **私有库随 id 迁移（票 07）**：宿主一次性幂等迁移（`plugin/session_db_migration.rs`）把插件私有库从 `plugins/com.bedcode.session/plugin.db` 搬到 `plugins/com.bedcode.terminal-session/plugin.db`——逐表按列名交集拷贝（task 域改名表经共享字典对齐），改名后插件从未激活过时走纯文件重命名；`plugin_meta` 账本戳保证只跑一次

### 基础建设

#### 移动端 wasmtime 47 → 48 —— ADR 0019 双端锁死恢复（移动端；零代码适配）
- 2026-09-18 桌面端升 48 时为规避 47 线 EOL 风险**故意分叉**（桌面 48.0.2 / 移动 47.0.3）；
  移动端本次补齐 `wasmtime = "48"`（lock 解析 48.0.3，MSRV 1.94 → 1.95），关闭 ADR 0019
  偏离。方法完全复用桌面端 spec：把 48.0.0 的每条变更逐项过一遍移动端宿主，再由
  `cargo check` / `cargo test` 驱动适配
- **零代码适配**——且原因与桌面端不同（桌面改了 2 处：宿主依赖 `wasmtime-wasi`，48 把
  `DirPerms`/`FilePerms` 收敛为二态 `FsPerms`，wasi-filesystem #14010）；移动端宿主**根本没有
  WASI 面**（无 `wasmtime-wasi` 依赖、无 preopen、三个插件零 wasi import），48.0.0 里与 wasi
  相关的半边（socket 默认 deny #13936、文件系统权限、wasip2/p3 统一）在移动端没有对应面。
  燃料看门狗语义、`ResourceLimiter` 上限、AOT `.cwasm` 缓存在 48 下均不变
- `cargo check --lib` 干净；移动端 `cargo test` 全量全绿（lib 347 + 集成 target），其中
  `wasm_runtime` 29 项真编译真加载组件（往返、燃料 trap、limiter 拒绝、AOT 缓存命中、
  abi 协商、SDK 宏产物加载并 activate）
- ADR 0019 重写：补版本沿革表（47 → 桌面先行 48 → 重新对齐），并显式写清「锁」到底锁什么——
  **声明范围**必须一致，**lock 解析出的 patch 不必**（两端 `Cargo.lock` 相互独立，`.cwasm`
  写在各自设备的宿主 cache 目录、从不跨端复用）；构建链 target（`wasm32-wasip3` vs
  `wasm32-unknown-unknown`）明确不受本 ADR 约束
- **移动端 wasip3 不在本次范围**：探针 crate 实证桌面 A0-3 的接线形态（CM_ASYNC +
  `p3::add_to_linker` + 同步 `Store` + `instantiate_async`）在 48 上编译通过，且**不需要 p2
  兼容垫片**（既有 unknown-unknown 组件零 wasi import）。但当前三个移动插件对 wasi 零需求，
  收益主要是构建链简化，而代价是**破坏已发布的 `@binblink/bedcode-plugin-sdk-mobile` CLI 构建链**
  + 把 `wasmtime-wasi` 拉进 APK。票拆分 B-1..B-4 与代价评估见
  `.scratch/2026-09-26-mobile-wasmtime-48-wasip3/spec.md` §4，**待决策**
- 文档同步：AGENTS §2 版本表、`docs/knowledge/wasip3-toolchain.md`、ADR 0019、四个 README
  的 wasmtime 徽标与 MSRV 行（桌面端 README 仍写 47，是桌面单端升级遗留的文档债，本次一并纠正）

#### 插件→宿主的同步事件 wire 就是出站 wire —— 一份格式，ABI 不 bump（桌面端；移动端 wire 逐字节不变，但第三方插件须重建）
- `bedcode_plugin_api::events::SyncEvent` 此前走「内部标签 PascalCase + 字段平铺」，宿主转发给移动端时再改写成出站 `SyncPayload` 的形状（adjacently tagged snake_case + `data`）——同一个事件两份格式、三段转换。现在两跳**合成一份**：`SyncEvent` 直接携带 `{"type":"<snake_case 变体>","data":{…}}`，`session` 字段是类型化 `wire::SessionSummary`、状态是 wire 字符串，宿主不再用另一种形状复述会话事件。唯一不对称是 `session_stopped` / `session_removed` 的 `source_device`：给宿主做「排除发起设备」的信封字段，**不出站**
- **WIT `host-events.broadcast-sync` 签名不变（仍是 `event-json: string`），因此插件 ABI 不动**——变的是插件产出的 JSON 形状与字段类型。随包四个插件产物已重建；**第三方插件必须按当前 `plugin-sdk-desktop` 重建**。未重建的旧产物在解析期被点名拒绝，而不是被当成「没有事件」——AGENTS §8 的 fail-visible 判据这次落在格式变更上而非 interface 删除上
- 跨端 wire 形状自此有单一事实源：SDK `bedcode-plugin-api::wire` 定义 `SyncPayload` / `SessionSummary` / `SessionControl*` / `Terminal*` / `KeyCombo`，宿主 `enums/{sync,summary,control,special_key}.rs` 缩为 re-export 垫片（导入路径零改动、运行行为零变化），移动端保留平行副本并由 `mobile_parallel_copy_shape_lock` 逐变体钉住与真源一致
- 补掉门禁空档：形状锁主战场迁进 SDK 后，`test.yml` 只在两端 `src-tauri` 跑 `cargo test`（`sdk-publish.yml` 对 Rust 侧只 `cargo check`），迁过去的锁等于没人执行——桌面 job 现新增 SDK crate 的 `cargo test` 一步

#### 破坏性插件 ABI v27 —— 旧产物必须按新 SDK 重建（仅桌面端；移动端零改动）
- ABI **26 → 27**，本项目迄今第一次破坏性契约变更：WIT 删除 import 两个 interface（`host-session` 12 函数、`host-terminal` 的 `send`）与 export 两个（`terminal-hooks` 整 interface、`events` 的 `on-session-lifecycle` / `on-input-submitted`）；权限位 `session:write` / `terminal:observe` 退役（词汇 34 → 32），`session:read` 判据面收缩为宿主终端窗口事实。四个随包插件产物均已按新 SDK 重建
- **旧 SDK 产物在实例化期即失败**（早于 ABI 版本协商），宿主在报错后附加「缺失 interface 名 + 需按当前 SDK 重建」的指引（`LoadedWasmPlugin::stale_artifact_rebuild_hint`）——是可诊断的失败，不是 trap 也不是静默降级。若分发第三方插件，升级前须用当前 `plugin-sdk-desktop` 重建
- 版本号规则未被本专项触及：桌面独有接口不 bump 移动端 ABI（移动端仍 11），移动端与其 SDK 无需重建

#### 插件 ABI 桌面端 16 → 19（既有 interface 的函数级追加；移动端仍 11）
- v18 `host-auth` 认证记录面：`trusted-devices-list` / `trusted-device-revoke` / `connection-history-list` / `auth-setting-set` 返回内核原始记录，排序 / 过滤 / 派生视图归插件
- v19 `host-session` 会话语义面：配置 CRUD（新权限位 `session:config`）、`create-with-spec`（插件算好 launch spec，宿主只做 shell 包装 / WSL 转换 / 尺寸缺省 / ID 预生成）、`restart` / `remove` / `rename` / `resize`（裁决规则在插件、正统端登记在内核）、`annotate`、`connections-list`；另有 `host-platform.wsl-distros`
- 未新开任何通道：三域全部落在既有 20 组 `host-*` 原语内；输出订阅/ack 原语被显式否决（逐帧输出不进 WASM）
- `manifest-gen` 命令面口径收紧：manifest 已声明 `contributes.commands` 即视为人工裁剪过的用户可见面，生成器只报告「臂 / 声明」差集而不覆写（该插件的 release 构建现已幂等）

### 改进

#### 桌面端
- **`AppEvent` 成为发送协议，宿主不再镜像会话事件（桌面端，会话事件下沉专项票 01–04）**：`AppEvent` 原本是个空 marker trait，事件发送靠专用 `sync_tx` 与处理器内的业务 match。现在它带三个方法——`source_device()` / `validate()` / `to_sync_payload()`——并只经统一入口 `events::publish()`（校验 → 查源 → 投递）：载荷被拒、或该事件类型没注册事件源，都返回 `Err`，不再静报成功；`to_sync_payload` **故意不给默认实现**，新增事件类型必须显式回答走不走同步通道。插件事件只经一个薄适配 `HostSyncEvent` 进入，其载荷折算是同一 wire 的机械转换而非逐变体 match——机械 match 落在宿主就是解释权重回宿主的第一块跳板，变体面一致性改由 SDK 的锁钉住。`SyncEventHandler` 只剩传输面三件事（折载荷、排除发起设备、广播），八个 `handle_*` 方法与 `format!("{:?}").to_lowercase()` 的状态重格式化随镜像枚举 `DesktopSyncEvent` 及其穷尽 `From` 一并删除。`Message::SessionEvent` 同批退役：两端零生产调用方、历史上也从未有生产发送点，会话变更通知自此只有一个面——由插件发布的 `SyncPayload::session_created / session_stopped / session_removed / session_status_changed`。防回接锁两把（`retired_session_event_mirror_is_not_reintroduced`：`src/events/**` 实现段不得再现镜像枚举 / 逐变体构造 `SyncPayload::` / 解读 `SessionStatus`；`sync_handler_does_not_interpret_session_variants`：处理器实现段零变体分支），均做变异自检
- **会话引擎下沉收官：宿主已「零会话对象」（桌面端，2026-09-24）**：内核会话目录 `src-tauri/src/session/` 整目录删除（登记 / 状态机 / 属主表 / 注解槽 / 业务输出环 / 配置管理器，约 5.0k 行）。会话真源只有一处——`com.bedcode.terminal-session` 的登记域（私有库 `sessions` / `session_annotations` 两表）；宿主侧与会话相关的只剩三样且都无业务语义：PTY 引擎（`host-pty`）、宿主 server 在册连接清单（`host-connection`，票 04 已迁独立原语、判据 `connection:read`）、互调窄转发层（`utils/session_gateway.rs`，插件未激活即显性报错）。移动端线路上输出面随之收敛为唯一一条：订阅 / 退订 / ack / 历史快照 / 会话停止通知全部直读引擎 `PtyRing`（票 06 形态 B），内核兜底分支删除；关停回收与关窗守卫也只引用引擎事实。源码扫描锁 `retired_kernel_session_domain_is_not_reintroduced` 会在任何内核会话符号回接时让构建失败
- **每插件 PTY 配额改为 manifest 声明（`ptyQuota`），取代单一内核常量（桌面端，会话引擎下沉 P1-b 前置 / H1）**：宿主此前对一切插件统一封顶「在册 `host-pty` 句柄 8 条」。业务会话改走 `host-pty` 之后，这个数字会静默变成「用户能开几个终端」——那是产品档位，不是内核该定的。现在由插件自己声明并发额度，宿主分两层仲裁：构建链只校形态（正整数，避免把内核常量复刻进 JS），加载期拒绝越界声明（`0` 或超 `PLUGIN_PTY_SESSIONS_CEILING_PER_PLUGIN` = 64）且**不夹取**（静默降级等于让插件按拿不到的深度规划业务）。未声明者沿用默认 8 条，既有插件零迁移；配额登记与权限授权同漏斗（声明面只有一个入口），`spawn` 越界文案点名的是**该插件的声明值**
- **`host-app.plugin-resource-dir` 原语 —— 插件不经生命周期事件即可取自身资源目录（桌面端，会话引擎下沉 P1-b 前置；ABI 仍 v25）**：插件安装目录此前只能经 `on-session-lifecycle(Creating)` 的 `resource_dir` 字段拿到，而创建编排整体移交插件后该事件不再产生，Agent 集成 hook 脚本源会失去输入。新原语返回**调用方自己**的安装目录（与旧事件 payload 同值：`extension_path` 剥离 verbatim 前缀），**不设权限门**（无可授予的权力：不含跨插件信息、零业务语义，同 `host-platform` 口径），未知插件显性报错而非返回空串。`com.bedcode.terminal-session` 改为自取该目录，取不到时显性告警并跳过集成注入
- **WASM 内核模块 `plugin` 改名 `wasm_core` + 结构规整（桌面端）**：`src-tauri/src/plugin` 改名为 `src-tauri/src/wasm_core`（对齐 wasm-core spec 命名），全仓 `crate::wasm_core` 路径替换 `crate::plugin`。宿主对外接口归入新增 `host_api` 模块（`wasm_core/host_api/`）：全部 host-* 原语实现（原 `manager/wasm_runtime/host_impl/`，21 组能力域）+ 前端命令桥 `api_bridge`。`manager/wasm_runtime` 改名 `manager/runtime`；四个一次性宿主侧迁移（auth-records / quick-actions / session-db / task-data）归组到 `wasm_core/legacy/`。facade（`wasm_core.rs`）仍是唯一组合点；`host_api` 移出 `runtime` 子树后 `WasmHostContext` 字段改 `pub(crate)` 供内核内部访问。纯改名/归位重构，无行为变化；桌面 `cargo test` 1153 单测 + 集成全绿
- 旧 HTTP 前缀 `com.bedcode.auto-task/*` 由宿主显式别名表在旧插件缺席时应答；「保留 vs 切断」的成本对比与裁决记在票面而非留为隐含
- 插件私有库表名按域前缀统一（`task_*` / `session_*`），配可逆改名账本与回滚路径，并加源码扫描护栏（漏改一处 SQL 即编译期红）
- **引擎层 PTY 生命面：关停全量回收 + 存活计数（桌面端，会话引擎下沉 P1 前置）**：`host_api/pty.rs` 新增两个非 WIT 引擎原语——`kill_all_registered`（跨属主 kill 并摘除全部在册插件私有 PTY，逐条**按属主**补发 `<owner>::pty:exit`）与 `live_count`（在册即存活的计数），两者与既有按属主回收**共用同一实现**（「摘除成功者才发布事件」的单一发布者不变量不得有第二份拷贝）。`system/lifecycle.rs` 关停钩子（优先级 10）在业务线 `SessionManager::shutdown()` 之后接上全量回收——**插件已停用 / 超时 / trap 时仍能回收孤儿进程**（按属主回收依赖插件停用流程被调到，关机不保证）。另加 `SessionManager::live_pty_count()`：判据 `is_running() && !output_terminated()`，语义与今日关窗守卫口径逐格对齐（含只建不启的 Starting、含 Running、不含已 kill / 已自然退出）——它是「关窗守卫判据改为引擎事实」的落地件（真源切换后守卫不再依赖会话记录）
- **宿主侧会话窄转发层：所有会话操作收口到一处（桌面端，会话引擎下沉 P1 宿主侧）**：新增 `utils/session_gateway.rs` 作为宿主调用会话的**唯一入口**（查询 / 创建 / 停止 / 移除 / 尺寸 / 输入 / 历史快照 / 输出订阅取消），桌面 Tauri 命令 5 项、移动端 HTTP 6 端点 + history、移动端 WS 控制 5 动作与 WS 终端输入全部改经它——此前同一条规则散在三处各自直连 `SessionManager`（尺寸裁决的「插件优先 + 内核降级」只在桌面命令面存在，移动端两线直连内核）。**行为零变化**：创建仍走插件编排（插件必需）、停止/移除/输入仍内核执行器、尺寸桌面路径插件优先否则内核兜底、移动端信号路径仍内核裁决（该函数签名不含宿主上下文，故「移动端零改动」是结构性保证）；模块文档给出「今日策略 ↔ 真源切换后」逐行对照表，后续把实现换成纯插件互调 api 时消费面零改动
- **会话登记域在插件侧落地（双写期，桌面端，会话引擎下沉 P1-a）**：`plugins/terminal-session/rust/src/session/` 新增会话登记域——`SessionStatus` 与宿主 enum 的 serde 形状**逐字同形**（含 `{"error": …}` 形态）、状态机（终态不可复活 / 同态幂等不刷时间戳）、私有库 `sessions` / `session_annotations` 两表、内存镜像**写库先行**（惰性载入 + 稳定读序）、活跃判据与宿主 `filter_active_by_config` 同判据。宿主仍是会话权威：本批次只按同一事实**双写**（创建 / 移除 / 重启 / 改名 / 正统端归属登记 / 注解槽 / 生命周期状态迁移），镜像失败一律 `warn` 留痕、不阻断用户路径，**行为零变化**；`session.status` 命令面新增 `sessionRegistry: {count, active}` 诊断字段观测镜像规模。激活时清空上一进程遗留的会话行（会话与其 PTY 同生命周期，宿主真源同为进程内存）。已知缺口：宿主不经插件的路径（移动端 HTTP/WS 的 `remove` 直连 `SessionManager`、不派发生命周期事件）不进镜像，留待 P1-b 真源切换时随「这类路径改经插件」闭合
- **PTY 引擎去业务化 —— 宿主 shell 包装退役，`pty` 只收算好的 argv（桌面端，PTY 下沉 P0）**：
  - 删除 `pty/command.rs`（`build_command`：`bash -lic` / PowerShell `-Command` / CMD `/K` 包装、cwd 兜底、WSL 路径转换）与 `pty/wsl.rs::windows_to_wsl_path` / `execute_command`；shell 包装的唯一实现现在是插件 `terminal-session/rust/src/launch.rs::build_argv`（pty 票 1 已先行落地）
  - `PtyCommandSource`（`Business` / `Raw`）与 `pty_handler` 工厂 trait 退役：引擎只收调用方算好的 `CommandBuilder` —— `PtySession::with_command`（业务会话线）/ `with_private_command`（host-pty 插件私有 PTY）。`pty/` 不再 import `SessionLaunchConfig` / `ExecutionEnvironment` / `BEDCODE_SESSION_ID`，从类型上无法再包装 / 转换 / 注入
  - 业务翻译收敛为业务层单点 `session/session_manager.rs::launch_command`（argv / cwd 仅原生环境显式设置 / env 透传 / `BEDCODE_SESSION_ID` 注入）。业务输出汇实现 `SessionOutputSink` 同批归位：`pty/output_sink.rs` 只留 `PtyOutputSink` 抽象，实现移到 `session/session_output.rs`（与 `GlobalOutputManager` 同居）
  - `SessionLaunchConfig.command_args` 变为**必需** `Vec<String>`（原 `Option`），`create-with-spec` 对缺省 / 空 `commandArgs` **显性拒绝**——旧插件产物直接报错，不再静默走已退役的 shell 包装路径；旧 `args` 字段（空格拼接交 shell 解释）一并拒绝，`command` 保留为纯诊断串
  - 接受的能力收窄：CMD 分支不可达（插件 environment 词表为 `linux|wsl2|windows`，只映射 PowerShell），其危险字符拒绝随宿主实现一并退役
  - 记入既有发现（非本次引入）：`pty_reader` 在读线程**入队**尾帧后即标记 reader closed，而真正 `sink.on_bytes` 在独立消费者任务里异步执行 → 终态事件到达**不等于** sink 已收到尾帧（原注释表述相反，已按事实更正；消费方含测试须有界轮询）

### 修复

#### 桌面端
- **燃料看门狗漏掉三条可选导出回调，累积烧穿后触发幽灵重载** —— 单次调用预算在 `LoadedWasmPlugin::exports()` 内续费，但 `on_message_binary`（v11）、`on_ws_frame`（v14，客户端域与服务端域两条分支）、`on_task_event`（v20）是直接持探测到的 `TypedFunc` 句柄 `call_async`，**不经过 `exports()`**。这三条入口因此拿不到续费，燃料变成**跳调用累积**扣减，与 `config.rs` 里写死的契约（`FUEL_PER_CALL`：“每次导出调用前重置燃料，预算只约束单次调用内 guest 计算量”）相左。余额归零后 guest `all fuel consumed` trap → Store 中毒 → `schedule_plugin_reload_after_trap` 重载插件。主调用面恰好是这三条回调的插件首当其冲：file-transfer（WebSocket 帧）、以及任何持续收宿主任务事件的插件。同一处缺口还让 `fuel_consumed_total` 监控少计（`record_fuel_consumed` 只在续费时落账）
  - **影响面**：默认 `mutex` 调用模型。`event-loop` 模型全部 op 经 `start_op` → `refill_fuel`，本就不受影响——这也正是既有 `test_component_fuel_watchdog`（只跑 `invoke_command`，走 `exports()`）一直绿的原因
  - **修法**：三个方法各自在入口自带续费，理由写在调用点。`on_ws_frame` 选在 `match` 之前续费而非各分支内，使降级路径的消耗记账也保持流动
  - **护栏**：`test_optional_export_callbacks_refill_fuel` 先把燃料排干到预算一成以下，再断言三条回调各自把它补回——连续三轮（杀“只在首次续费”的变异）、`events-task` 用自己的夹具单独覆盖，以及反例：未导出 `events-ws` 的产物仍恰为 `Ok(false)` 且不被扰动。变异自检已验：删掉任一条续费、或把续费砍半均测红；存活的那条变异（把续费挪到无导出的提前返回之后）语义等价，如实记录而不粉饰
- **Agent Hub 卸载 npm-global CLI 不再假成功——登录 shell 现在带上用户 PATH**（桌面 wasm 应用 `com.bedcode.agent-hub`；**不动 WIT/ABI，宿主未动**）
  - **症状**：「卸载完成」后卡片仍显示「已安装」。落盘真源（`plugin_storage` `install.last`）是 `exitCode:127` / `npm: 未找到命令`——卸载脚本走 `bash -lc`，nvm 的 PATH 注入藏在交互守卫（`case $- in *i*)`）后面，登录 shell 看不到 npm；命令从未执行，完成后自动重探测仍发现 codex
  - **修法**：`exec_script()` 前缀带上 `detect.rs` 探测早已在用的交互 shell PATH 引导（`path_bootstrap_unix` 提升为 `pub(crate)`；仅 unix——Windows `cmd /C` 读注册表 PATH 无需）；install / uninstall 两个入口共用，回归锁断言引导串进入命令行
  - **卸载失败不再静默**：概览卡片对进程级卸载失败（last ok=false 且非取消）上屏「卸载失败，详情见『安装』页控制台」——`failedUninstallFromLast` 纯函数 + watch + i18n key（zh/en 同步）
- **终端会话中心插件的 Tailwind 工具类根本没被编译** —— 插件目录改名后 `bedcode-desktop/tailwind.config.js` 仍写 `./plugins/session/src/**`（另有一条指向已退役插件的 `./plugins/scheduler/src/**`），而缺 `./plugins/terminal-session/src/**`。Tailwind 只在宿主编译（无第二处 content 注入点，插件产物也不携带编译后的 Tailwind），于是**仅本插件使用**的类全部拿不到规则：插件 405 个 class 令牌中 165 个为插件独有，其中 158 个无 CSS —— 9 个 Vue 文件 / 77 处引用丢失间距、固定尺寸（`w-96`、`max-h-[440px]`、`h-[168px]`）、栅格（`grid-cols-[auto_1fr]`）、z-index/定位与状态色。修法：content 改指 `terminal-session` 并删两条死路径；新增护栏用例 `src/__tests__/plugin/tailwindContentCoverage.test.ts` 双向锁住 `plugins/*` 与 content 清单
- **插件引用的两个宿主设计类全仓不存在**：`wb-select`（终端头部 / 设置面板 4 处原生 select）与 `wb-btn-secondary`（背景图选择按钮）无任何定义——宿主样式表只有 `wb-btn-ghost` / `wb-btn-primary` / `wb-mono` / `wb-section-title` / `wb-sidebar-section` / `wb-toolbar`。死类名已清除：select 改带 `cursor-pointer` + `focus:border-brand`（与宿主表单控件同一套焦点反馈），选择按钮改用迁移前就在用的 `wb-btn-ghost`
- **`.plugin-icon` 是宿主工具栏组件的 scoped 类**（scoped 不外泄），插件复刻的三处扩展点图标槽因此没有任何规则；插件现于自身 scoped 块内定义同款 `font-size: calc(14px * var(--ui-scale)); line-height: 1`

### 安全

#### 桌面端
- 用户目录副本不再能顶替同 id 的随包插件：启动时的两次目录扫描（随包 `resources/plugins/desktop` → 用户目录 `app_data/plugins`）各持一份 `seen_ids`，同 id 的用户副本在合并时静默覆盖了内置条目（「重复 id 被拒绝」的注释只在单次扫描内成立；由于强制「目录名 = manifest id」，该判据此前实为死代码）。被顶替的插件随后按 `UserInstalled` 读取——信任档从应用构建信任域降级——激活被审批门禁拒绝（`requires user approval before activation`），这正是 `app_data/plugins` 下 `com.bedcode.agent-hub` / `com.bedcode.ai-chatbox` 的陈旧 `file-install` 副本卡住两个随包插件的原因。两次扫描现在共享同一份去重集合（`PluginLoader::load_builtin_and_user`）：内置条目胜出，用户副本被拒绝并留日志
- `fs_auth` 内置受信任插件白名单种子已改指 `com.bedcode.terminal-session`（随票 06 改名；写用户项目 agent 集成的就是合并插件；不改指会把它静默降级成逐目录弹窗授权）
- 认证分层成文且单点仲裁：配对码 / QR 的编排在插件，签发、验签执行点、密钥托管（host-auth secret-store）与 `pairings` / `connection_history` 表留宿主；插件未激活时宿主桥接回退到宿主实现并 `warn` 留痕，这是设计内降级路径，不算旁路
- 凭据仍只记长度不落明文；插件权限清单收口为 15 项且每位都能追到真实消费点（spec 表格里两位查无调用点的位刻意不声明）

### 测试与质量

#### 桌面端
- **新增 `bench/` 桥接基准工程，量化 wasm ↔ 宿主 ↔ 前端 的成本分布（21 场景 / 7 组，不动任何产品代码）** —— 既有性能探针各测一条链路的一段（`terminal_output_perf.rs` 测 PTY 输出消费、`ws_output_perf.rs` 测 WS 帧吞吐），且全部无头运行，「一次完整往返」的成本分布此前没有数据支撑。新工程含：一个自带 `main()` 的 `cargo test` target（`harness = false`，`src-tauri/tests/wasm_bridge_bench/`）、一个仅供测试的 guest 夹具（`packages/plugin-bench-test`，wasm32-wasip3，不进打包链）、一层真 webview e2e（`e2e/specs/bench.spec.ts`）。宿主层走**生产同形**路径（`PluginHost::new` → `activate_plugin` → `invoke_rust_command`，即 `plugin_invoke` 的内核），并把同一夹具实例化为两个属主，使总线与互调流量端到端可观测；每个测点都带行为断言（收讫字节 / 调用次数 / 计数增量），门禁只取数量级——机器差异不该让基准变红，慢了 10 倍必须变红。首轮基线（宿主 debug，每测点 5 次取中位数）：`nop` 往返 **56 µs**（16 ms 帧预算的 0.3% —— 命令面不是瓶颈）、1 MiB 同步回传 **1.77 ms**（~1.7 ns/B）、单次 `api_call` **359 µs**（64 KiB reply 638 µs）、1 MiB `ring-fetch` 拉取 **19 ms**（guest 内仅占 31%）、`execute-batch` 64 条 343 µs。两条发现另立项而非默默吸收：**512 条突发总线消息只送达 20.7%**（订阅队列容量 64，丢弃只有 warn，发布方与订阅方都不可见）；**`host-storage` 1 MiB 往返 26 ms**（~25 ns/B，比跨越 WASM 边界还慢一个量级 —— 不能当大对象缓存用）。完整表格与推导见 `.scratch/2026-09-26-wasm-bridge-bench/report.md`
- webview 层顺带补上一个既有 e2e 缺口：`wdio.conf.ts` 声明了 `runner: 'local'`，但 devDependencies 里从未装 `@wdio/local-runner`，e2e 此前根本起不来；现已补装（tauri-driver 本就由既有 `autoInstallTauriDriver` 自动装）
- **同工程后续实测：贵的是 Tauri IPC 那一段，不是 WASM 边界；慢调用会堵死整个插件实例** —— webview 层（`e2e/specs/bench.spec.ts`，已全绿：真应用窗口内 2 场景）用**应用自己的**前端封装（`src/plugin/commands.ts` / `src/plugin/events.ts`）测同一批命令。结果：`nop` 往返在宿主内 54 µs、从**真前端**看是 **560 µs**（IPC + 前端桥接 ≈ 0.5 ms）；1 MiB 命令返回 1.87 ms → **27.7 ms**（≈ +24.6 ns/B）——**载荷走 webview↔宿主一趟，比穿一次 WASM 边界贵一个数量级**，任何「大对象经命令面回前端」的设计要按 25 ms/MiB 算预算。事件侧，分块在前端**也没有**优势：1 MiB 单帧 37 ms、16 KiB×64 27 ms、4 KiB×256 33 ms —— 分块的理由应是平滑渲染 / 可 ack，不是吞吐（补上 output-ack 议题缺的那块数据）。新增场景 **G2** 把并发模型专项的动机量化：一条 26 ms 的 `storage` 调用在途时，同实例一条 50 µs 的 `nop` 要 **26.1 ms（167×）**；4 路并发慢调用 = 单次的 4.04×（完全串行）。建议把 G2 作为该专项的**验收基准**（事件循环属主落地后应降到 ~1×）。webview 层能跑起来需三处修正（已写进 spec）：debug 二进制走 `devUrl` 故必须起 vite dev server；`browser.tauri.execute` 只带走函数源码，Node 侧取值必须插值进脚本体；loader 凭证首调用者生效，只能借应用自己已缓存的那份
- **新增 `bench_channel.rs`（仅 debug 的宿主命令）测第三条 Tauri 传输面 `tauri::ipc::Channel` —— raw 快 4×，但「顺手写法」有 6× 的坑** —— 产品当前只用两条传输面（命令返回值、`app_handle.emit`），**Channel 零使用**；而桌面端曾经有过 Channel 传输的终端输出流（`commands/terminal_stream.rs`，2026-09-22 随会话下沉退役，退役原因是**架构**不是性能）。既然 output-ack 的 P2「宿主侧 push」落地时要在 emit 与 Channel 之间选，基准把三条面在同一批 1 MiB 上并测：**Channel `Response`/raw 7.0 ms**（JS 侧 `ArrayBuffer`；≥1024 B 的 body 走 tauri 2.11 的 `ChannelDataIpcQueue` + webview `fetch` 通路）vs 事件 emit 33 ms vs 命令返回 28 ms。**坑**：`Channel<Vec<u8>>` 是 **100 ms** —— `IpcResponse` 有泛型 blanket impl（`ipc/mod.rs:181`），`Vec<u8>` 被序列化成**JSON 数字数组**，webview 收到的是 `[object Array]` 而不是字节；要真字节必须 `Channel<Response>` + `InvokeResponseBody::Raw`。第二个数据点：**块大小有下限** —— 同为 1 MiB，4 KiB×256（52 ms）比 16 KiB×64（17 ms）慢 3×，流式推送的块不应低于 16 KiB。该命令在**声明处与 `generate_handler!` 注册处**双 `#[cfg(debug_assertions)]` 门，并有源码级闸门锁测试（删门即红，已做变异自检），参数契约有单测；它是本基准**唯一新增的产品代码面**，零业务语义

- 端到端等价回归**零改动断言**通过：`pty_session_chain`、`ws_session_route`、`ws_auth_rules`、`http_auth_biometric`、`server_integration`、`link_crypto_http`、`broadcast_shutdown`、`build_manifest_smoke`（S2 接缝文件与本批次前基线零 diff）
- 五个集成测试 target 恢复为可编译可跑绿（审计票 13）：它们此前引用已退役符号（`pairing_service` / `QrTokenManager` / `SessionManager::from_database` / `restart_session`）。现改为真实驱动插件侧路径——`/api/auth/*` 已无宿主实现，故各套件在测试内激活随包 `com.bedcode.terminal-session` 产物（产物缺失显性失败，不静默跳过）；会话创建的编排读插件私有库（无头集成二进制不可达），改驱动内核执行端 `create_session_from_spec`（= 插件经 `host-session.create-with-spec` 到达的同一入口）。`cargo test` 重新成为全 target 门禁：lib 1134 + 八个集成 target 全绿，`[skip]` 计数 0
- 恢复后的生物认证套件当场抓出一条真实回归（同票）：插件把连接历史的 `auth_method` / `result` 写成大写（`BIOMETRIC` / `SUCCESS`），而内核规范取值与连接历史页（i18n key 映射 + `result === 'success'` 计数）都是小写——设备历史页会把认证方式显示成「未知」且成功计数错误。已在插件侧以 `history_value` 常量模块修正，测试断言一字未改
- 插件构建链恢复（审计票 15）：`manifest-gen.js` 手写的 Rust 权限映射表既没跟演 v23 退役的 `session:config`（插件仍在合法调用改挂 `session:read` 的 legacy 读取通道），也没跟演主库 SQL 面的 `database:main` 拆分，于是给插件注入未知权限、`plugin-build.js` 直接拒绝构建。现两处均已跟演，并加了一道加载期护栏：映射项指向词汇表外权限即抛错
- 故障半径行为测试补齐：`Error` 态贡献面整组摘除、再激活整组恢复、内置入口让位与摘除共用同一判据、设置分组回落纯内置形态、配对桥接降级到宿主服务
- 迁入的任务 UI 首次获得前端测试面（旧插件本来为零）：弹窗按需取数、入队/清空/开关的命令契约、历史视图加载与筛选一致性，外加两条同源护栏——前端调用的每条命令必有 Rust 分派臂、`t()` 用到的每个文案 key 必在两语言表内

### 文档

- ADR 0022 v8：会话语义下沉批次（v18/v19 函数表、注解槽、设置分组扩展点、15 项权限清单、双端偏离表加本批次号）
- ADR 0022 补订（2026-09-22，v9）：插件 id 变更登记（`com.bedcode.session` → `com.bedcode.terminal-session`）、`output-ring-fetch` 二进制原语落地（v22 内函数级追加不 bump）、私有库路径迁移与双投窗口别名
- 路线图更新：阶段 2 标已落地并记形态改判、阶段 3 标部分落地（会话已做、终端刻意未动）、补记「合并执行」决策与主动打破渐进原则的理由与代价表，移动端受影响清单 M1–M5 从单个 spec 目录挂进路线图、（2026-09-22）阶段 3 终端部分也标 ✅
- AGENTS.md §7 修 ABI 计数与宿主能力清单条目、§8 认证语义措辞按插宿主分层改写；`docs/knowledge/plugin-http-endpoint-trust.md` 记旧前缀判定；桌面 code-map 与命令文档改指
- 明确不在范围：移动端适配、`com.bedcode.terminal`、终端窗口与输出管线进插件 —— **末项现已完成（票 01–05）**：终端窗口壳与输出消费归 `plugins/terminal-session`，宿主只剩引擎原语

## [2.1.1] - 2026-09-18

> 功能 / Features · 基础建设 / Platform & Infrastructure · 改进 / Improvements · 修复 / Fixes · 安全 / Security · 测试 / Tests & Quality · 文档 / Documentation

### 功能

#### 终端输出管线 — TB v3 字节流与环形缓存（双端）
- 桌面端 PTY 输出重写为字节连续管线（TB v3）：bytes 块队列 / v3 帧 / 字节游标与双速传播；慢消费者从环形缓存按订阅者游标拉取，背压不再阻塞生产者；`session_output` 链路调试统计（产出/ack 节流打点）
- 桌面端：一次性历史接口 `GET /api/sessions/{id}/history`；前端终端输出流适配 v3 字节游标（WS + Channel 双路径）
- 移动端：终端输出链路迁入 Rust 后端（`terminal_link` + 前端接线）；移除 TB v2 帧与旧 `ws_event` 通道终端残留死代码
- 移动端：段2 背压改为 ack 驱动补投，输出帧改为页面级 Channel
- 桌面端：删除 WS 环回终端链路（`local_token` / 环回 WS / 旧输出流）

#### 桌面端插件管理 — zip 加载与卸载
- 插件详情页对**所有来源**的插件都提供卸载（内置随包 / 文件扫描 / zip 安装）；卸载要求插件处于**未启用**状态（运行中按钮禁用并提示先停用），且清空插件全部数据：安装目录（取自 `extension_path`，含同目录私有 `plugin.db`）、键值存储、持久化文件系统授权（`fs_granted_paths` / `preauth_paths`）、持久化审批记录（`__system__` 空间的 `plugin_approvals` 条目：批准权限集 + 内容哈希钉扎）、持久化激活状态、数据库缓存连接与运行时限频记录。内置插件位于随包资源目录：只读安装下删除会如实报错，下次构建/更新后随包副本会重新出现
- 支持从本地 zip 包加载插件（解压到用户插件目录，来源标记 `user-installed`）
- 插件列表布局：加载插件按钮（主题色）移到「未启用」分区标题右侧，无未启用插件时依然可达；刷新按钮移到工具栏最右
- 卸载完整性：卸载时撤销持久化审批记录、按加载插件入口定位；清理插件孤儿残留目录，修复卸载后重装被磁盘查重卡死
- `fs_auth` 授权粒度精确化为三态（目录 / 文件 / 父目录）

#### 移动端 HTTP — Rust 统一代理与 fail-closed Egress 策略
- 移动端 HTTP 全部收束到单个 Rust 代理，配 fail-closed 三层 Egress 策略；前端 HTTP（`useHttpApi` / UpdateChecker / LinkEncryption）全部经此代理，移除 `@tauri-apps/plugin-http` JS 依赖
- Egress 授权弹窗 + 设置页授权查看 / 撤销
- 跳转重校验防 SSRF（移动端 Egress 与桌面端插件 HTTP 双端落地）
- SDK：`link-crypto` 增加 HTTP 密钥派生；SDK manifest `preauthUrls` 声明

#### file-transfer 插件 — 显式暂停/续传与并发控制
- 宿主并发闸门 + 显式暂停/续传（双端同构）；插件 `paused` 语义 + 前端暂停/继续/全部继续与并发设置
- 显式暂停/续传 wire 协议与数据面门控（双端 + `peer-net`）
- 桌面接收队列：接收卡速率 / ETA、清空历史二次确认、面板合计速率
- 移动端任务卡进度条活跃态用主题色（暂停/排队不再灰色误导）
- 双端暂停/恢复/取消状态同步（wire + 数据面门控 + persist 单事务）

#### 移动端终端体验
- 终端体验优化：扫码整合 / 新手引导 / 键盘避让 / 输入条 / 主题 / 帮助文档
- 字体档位跨度加大 + 会话数量限制
- TUI 鼠标上报嗅探支持多参数 DECSET 与真实上报开关
- TerminalView 退化为编排层，业务按域拆分（行尾静态裁切、网格写入收口）
- 移除 `peer_pick_folder` 命令（SAF 树 URI 统一文件夹选择）

#### 插件 SDK
- `host-peer` 传输控制三原语（pause / resume / resume-all）进入 ABI 契约；`plugin-component-test` fixture ABI 同步至 9
- `MarkdownEditor` 组件（marked 渲染 + raw HTML 转义 + 语法高亮）
- 两端 SDK 打包为 GitHub Release 附件（npm tarball / crate / 聚合 zip + SHA256SUMS）

### 基础建设

- 构建资源自适应包装器 `adaptive-run` + `build-profile`：采样 CPU 负载 / 可用内存 / swap 压力并注入编译并行度（`CARGO_BUILD_JOBS`、Gradle `workers.max`、`NODE_OPTIONS` 堆）；用法见 `docs/commands.md`
- CI 发布流水线为每个插件打 zip 分发包 + 双语 release body
- 移动端 dev 日志默认 verbose（logcat 含 Rust `debug!`）
- 移动端应用名统一为 **BedCode**；停用静态首屏动画，Android 开屏纯色化
- pi session 归档脚本（默认阈值 15 → 10 天）；文档跟踪策略：`docs/` 全分支正常跟踪，受保护路径缩减为受保护配置文件

### 改进

#### 桌面端
- 巨型前端组件拆分：TerminalPreview → 编排层 + 终端域 composable，SettingsView → 分组设置子组件，useDesktopCommands → 域命令模块（会话 / 设备 / 设置 / 事件）

#### 移动端
- dev 日志过滤非业务噪音，控制台与落盘同规则

### 修复

- **终端**：输出管理器注册时序（PTY 启动前注册 + 启动失败回滚）；订阅激活竞态；pty 链路历史/实时拼接竞态与重进游标语义；重复进入终端页作废上一代段2 推送通道；显示区与输入栏贴合间距；`terminal_link` 静默吞错点补日志；特殊按键字节非 UTF-8 时丢弃并告警
- **桌面端插件**：用户安装的 rust-ts 插件执行完整 guest 生命周期；插件并发闸门脉冲失败不再静默吞错（补 warn）；正式版禁用右键原生菜单；`tauri-build.js` DEB 重命名正则转义修正
- **file-transfer**：暂停/恢复/取消双端状态不同步；issue 16/17 缺陷修复（暂停恢复/速率计算 + 桌面前端静默失败）
- **移动端**：`terminalRowClip` 闭包内 `rowsEl` 非空断言（vue-tsc 收窄失效）；`http_auth_flow` 全局 token 用例加串行闸（消除默认并行 flake）；触摸滚动单元格高度兜底重算；running 状态广播不再复位存活会话缓冲

### 安全

- 双端跳转重校验（桌面插件 HTTP `redirect_decision`、移动端 Egress）封堵 SSRF 路径
- 移动端 HTTP 在 fail-closed 三层 Egress 策略下默认拒绝

### 测试与质量

- 桌面端 unit-test audit：32 张票据全落地（lib 基线 615 → 784）
- 移动端 mobile-test-audit：3 个 P0 lane 全落地 + P1 部分
- 新增测试：段2 通道补帧解析、file-transfer 清空历史二次确认弹窗（驱动 + 失败/取消反例）、任务卡活跃态主题色
- `unit-test-discipline` skill 接入 AGENTS.md 任务路由

### 文档

- `docs/commands.md`：自适应构建章节（`adaptive-run` 用法）+ 构建 profile 动态覆盖指引
- pty 输出管线 TB v3 架构文档（`docs/knowledge/pty-output-pipeline.md`）+ pty-byte-history 任务记录
- 移动端终端链路 code-map 增补（TB v3 标注）；mobile-ws-rust 方案 / 审查 / 遗留记录
- 架构图迁移至 `docs/diagrams`（archify 交付物 + README 链接）
- feature-branch 隔离 spec（task-scheduler / OCR / code-viewer）与 file-transfer 并发/暂停续传实施记录
- 移除过时的 implementation-plans 与 skills-course 学习文档

## [2.1.0] - 2026-09-10

> 功能 / Features · 基础建设 / Platform & Infrastructure · 改进 / Improvements · 修复 / Fixes · 安全 / Security · 测试 / Tests & Quality · 文档 / Documentation

### 功能

#### 对等网络 — P2P 直连链路（`packages/peer-net`）
- 新增双端共享的 Rust crate：节点身份、绑定公钥的自签证书、TLS 1.3 mTLS 直连、信任存储、专用 mDNS 发现、共享目录浏览与批级断点续传传输引擎
- 节点电源总线：集中的节点生命周期管理，双端完成插件生命周期接线与入站连接事件桥接
- 专用 `_bedcode-peer` mDNS 服务，带 TTL 在线缓存与可测试的注入缝
- TCP keepalive 活性探测、mDNS 周期性重广播/重查询实现可靠互发现、移除首帧超时
- 首连确认闸门、撤销并重确认流程、节点生命周期闸门消除 TOCTOU 竞态

#### 对等网络 — 加密核心（`packages/link-crypto`）
- 新增共享加密 crate：出站密钥 miss 观测、缓存容量护栏、GET 指标校正、query 路径归一（跨端）
- 移动端 pin 写入校验与指纹清理；HTTP GET/HEAD 空 body 协商；`X-BedCode-Crypto` 响应标记
- 生物凭证绑定/解绑迁移到 HTTP（脱离 WebSocket）

#### file-transfer 插件 — 对等化重写
- 旧的局域网文件服务整体退役，文件传输全链路改由对等栈接管
- V2 三段式主视图（移动端），业务逻辑由插件自持（Phase 3，双端）
- 设备发现刷新、端点记忆、历史快照、首连确认超时
- 双端共享目录多选；桌面端设置显示完整路径
- 端点清理/校验、设备单例、确认超时定点结算、根缓存重置
- 传输面板终态归档：进度 / 原因 / 打开所在文件夹 / 双端记账（两端各自记录同一次传输）
- 分区存储下的 SAF `content://` URI 中转复制兜底（解决 `EACCES`）

#### 认证与传输架构重做
- HTTP 生物认证与事件通道原语（桌面端）
- WebSocket 首消息 JWT 认证，以及专用的 `/ws/event` 事件通道
- 移动端常驻事件 WebSocket：认证后建连，断线自动自愈
- 移动端 HTTP 认证客户端；终端直连 WebSocket（`useTerminalSocket` + 状态机 store）

#### 终端输出管线
- 字节流 PTY 管线重写：基于序号的输出队列 + 快照订阅（废除字节偏移契约）
- 每会话终端路由，远端通道使用 TB v2 二进制帧；本地通道迁移到 TB v2 并支持快照重订阅
- 删除旧广播兼容通道与死代码
- 移动端终端写入管线让出主线程：128 KB 分块让出 + `flushing` 重入守卫
- `get_terminal_ws_info` 替代已移除的移动端输出链路；移动端支持 ack / yield / history 缓存

#### auto-task 插件
- 任务历史状态筛选由 chips 改为下拉 Select（默认「全部」）
- 各 Agent 的终端输入提交符统一为 `\r` Enter 字节

#### ai-chatbox 插件
- 供应商限流自动重试（双端）

### 基础建设

- 两端 `package.json` 与 Tauri 配置版本升级至 **2.1.0**；新增 `.deb` 构建支持
- CI：在 Windows / macOS 之外新增 Linux 构建目标；cargo 编译资源限制
- CI：新增双端 Rust + vitest 回归测试门禁（分层独立 job），合并到 `master` / `uat` 时阻断
- CI：`release.yml` working-directory 相对仓库根解析；SDK 构建步骤由 `pnpm filter` 语法改为 working-directory 模式
- CI：verify-latest.json 改用 draft release 资产查询；签名密钥步骤注入 `TAURI_SIGNING_PRIVATE_KEY`
- **E2E 基础设施（桌面端）**：WebdriverIO + `tauri-plugin-wdio`（仅 debug 隔离）+ 外部 `tauri-driver`；smoke 断言验证 execute/IPC 链路。移除遗留的 `@playwright/test` 依赖
- **插件 SDK 更名并发布**：`@binblink/plugin-sdk-*` → `@binblink/bedcode-plugin-sdk-*`（桌面 + 移动），v0.1.1 发布到 npm 与 crates.io；SDK 包文件精简（dev-shell 排除 `node_modules` 与构建产物），补充 `.npmignore`
- 共享 Rust crate 迁入 `packages/`（`peer-net`、`link-crypto`）
- 前端包管理器全仓从 npm 迁移到 pnpm，各项目各自保留独立 lockfile
- `dev-run` 进程组回收（向整组发信号，而非只杀父进程）+ plugin-watch 孙进程自清理
- `doc-tracking.sh` 行尾修复，恢复 pre-commit 保护逻辑；`PROTECTED_PATHS` 扩展覆盖两端 `docs` 子目录
- AI 工具配置入库；`.agents/skills/` 统一供 pi / OpenCode / Codex / Claude Code 共用
- 会话产物不入库（pi-lens 备份、compactions）
- Rust MSRV 1.94（wasmtime 47）；插件 WASM 以 `--release` 构建并保留 names section

### 改进

#### 桌面端
- 终端：xterm 残影收敛 — 背压滞回、Channel 传输、渲染器决策、有序写入队列
- Linux 终端渲染优化；WebKitGTK IME 防护重构为单一 attach 点
- 登录 PTY 以 `-lic` 启动，使其继承 Linux 用户 PATH
- Windows 熄屏 / 休眠唤醒后 WebView2 黑屏自愈
- 启动屏 footer 版本号改为读取单一真源；调整窗口尺寸与启动背景色
- 插件列表简介字号 12px → 11px

#### 移动端
- 设置页拆为 `views/settings/` 子视图：外观、认证、连接、通知、关于（主视图由约 1130 行降至约 400 行）
- Android 启动主题简化 — 移除系统开屏定制；开屏深浅色跟随 + 启动窗口背景统一
- SplashScreen 开屏页暂时下线（注释保留，可一行恢复）
- 手势滑动不再与导航栏切换动画互相打断
- 键盘收起时退出终端输入编辑态
- 通知后台语义收口；死代码清理；导航图标对齐

#### 日志与可观测性
- **前端**：引入 loglevel 作为统一前端日志框架，替代直接 `console` 调用；仅 dev 构建转发到 `frontend.*.log`（桌面端）与 logcat（移动端），release 自动剥离
- **桌面宿主**：HTTP 请求全路径日志、JSON span 链、启动 bootstrap 通道、存量日志字段化迁移
- **桌面插件（WASM）**：启用 `wasm_backtrace_max_frames(32)`，使 trap 携带 WASM 调用栈；trap 在宿主侧留日志；调试模式（`BEDCODE_PLUGIN_DEBUG=1`）以 DWARF 构建插件并支持行号解析、燃料预算放大 32 倍；通过 `BEDCODE_PLUGIN_LOG=id=level` 按插件设定级别
- **移动端**：release 日志级别收敛；dev 日志保留治理（14 天滚动清理）
- 宿主错误处理加固：自描述 `io` 错误包装、span 插桩、spawn 错误边界

### 修复

- **对等网络**：停机 busy-loop、keepalive 留痕、目录 `size` 契约跨端归一；节点生命周期 TOCTOU 闸门；连接态重发补充 `deviceName`
- **文件传输**：移动端共享目录多选、mDNS 单守护恢复互发现、连接感知；桌面端启用死锁改为「启用先行」并把预授权弹窗前置于 loading；插件动态 UI 严格跟随启用状态（生命周期对称拆解 + mDNS 多播锁去重入）
- **终端**：opencode TUI 滚动残影 — rAF 合并写入默认开启 + 滚动停止补刷；移动端 `touchmove` cancelable 守卫；zoom 移除后同步 zoom-compensation 注释
- **移动端**：`SafPickerPlugin` 构造参数恢复为精确的 `android.app.Activity`（JNI 签名查找返回 `null` 导致 NPE）；platform-tools 升级后 adb fd0 shim 自愈；插件授权弹窗不再与 loading 同现；真机发布包联调缺陷
- **桌面端**：VerifyCode 配对成功的 `Authenticated` 响应补充 `device_name`；开屏 caret 残留清理；全局通知去重
- **构建 / 开发**：`plugin-build` 的 `JSON.parse` / `execSync` 补充错误上下文；V2 合并后的 file-transfer 组件导入路径修正

### 安全

- **对等身份**：首启纯随机生成 Ed25519 节点身份并原子持久化，绑定进 rcgen 自签证书，通过 ring 校验 `CertificateVerify` — 伪造身份无法通过握手
- **信任存储**：首连确认闸门，支持撤销并重确认，使已信任的对端可被撤销后重新受审
- **传输**：WebSocket 首消息 JWT 认证 + 专用认证后的 `/ws/event` 通道；移动端生物凭证绑定/解绑由 WebSocket 迁移到 HTTP
- **沿用 2.0.0**：插件身份校验与权限审批、生物认证链路加固、为 Agent hooks 保留本地旁路的 JWT 网关

### 测试与质量

- **桌面端 Rust**：583 个测试；集成覆盖 HTTP 契约、WS 配对认证、PTY 会话链路、多客户端广播 + 优雅停机、契约 fixture 漂移对齐
- **移动端 Rust**：251 个测试；L1/L2 集成套件落地，并修复断线重连缺陷
- **peer-net**：102 个测试，含双节点 harness（发现注入、共享目录、传输会话）
- **前端**：桌面端 61 个文件 569 个测试；移动端 42 个文件 360 个测试（views / stores / composables / integration）
- **插件 SDK**：桌面端契约测试 5 → 85；移动端 2 → 79
- E2E smoke 套件，验证 WebdriverIO → tauri-driver → IPC 链路
- CI 回归门禁：任一层失败即阻断合并到 `master` / `uat`

### 文档

- README 按 2.1.0 重写：版本徽标（含 wasmtime 47）、平台补充 Linux、Claude Code → Pi/Opencode 文案、移除过时截图；`README_en.md` 同步
- 两端目录级代码地图（`bedcode-desktop/docs/code-map.md`、`bedcode-mobile/docs/code-map.md`）作为模块查找索引
- ai-chatbox / auto-task / file-transfer 双端插件架构图（Archify）
- 知识库：`pty-output-pipeline`、`mobile-terminal-optimization-reference`、`release-workflow`、`sdk-publish`、`github-actions-setup`、`build-process`、`feature-branch-isolation`
- ADR：移动端文件服务退役（对等栈接管）、代码查看器设计
- auto-task DAG 编排 spec；xterm 透明模式残影收敛 spec 与票据；desktop-e2e-webdriver spec 与票据；插件 WASM 日志 spec
- 特性分支隔离落地：桌面端定时调度插件 → `feature/task-scheduler`，移动端 OCR 插件 → `feature/ocr-plugin`，代码查看器设计 → `feature/code-viewer`

---

## [2.0.0] - 2026-08-16

### 新增

#### 插件系统 — WASM 平台
- 插件运行时迁移到 WASM Component Model（wasmtime）；移除 cdylib 动态加载，Component 成为唯一支持的形态
- ABI 演进 v2 → v6：类型化宿主 API、参数绑定 SQL、内存回收、out_ptr、签名校验、插件状态上报、InputSubmitted 观测扩展点
- 运行时加固：epoch 中断 + 资源限制、燃料看门狗、trap 自动重载恢复、AOT 缓存、wasmtime 47
- 安全：插件身份校验与权限审批（防冒充）
- 工具链：bedcode-plugin CLI（create / build / dev / validate / doctor / manifest）、Dev Shell 浏览器开发环境（双端，`--host` 供手机访问）、manifest-gen
- 生命周期：动态激活/停用并持久化状态、热重载、安装/卸载、loading 遮罩
- 能力：每插件独立 SQLite 数据库、插件间消息总线、host_notify、fs_auth 批量目录授权、文件服务挂载/传输、WSL 文件系统桥接
- SDK 内置共享 UI 组件库（Rust + TS，双端）

#### auto-task 插件
- 多 Agent 支持：Claude Code / pi / opencode / Codex（注册表驱动的 Agent 适配架构）
- 任务队列：调度、自动执行、自动应答、预设任务（一次性）、定时任务状态机、任务历史与统计、筛选与重试
- 移动工具箱：任务历史 / 定时任务面板
- TUI-agent 首次派发兜底（15s 宽限）与按 Agent 的终端 hooks

#### file-transfer 插件
- 局域网文件传输插件（WASM 核心 + 桌面/移动 UI + 打包分发）
- 双向传输：发送到手机（上行）、带策略审批的接收、异步批量审批、传输历史、断点重新入队、专用 downloads_dir
- Android SAF 流式传输：共享目录、带 pfd 强引用的 SAF 选择器、全盘访问授权引导

#### ai-chatbox 插件
- 纯 AI 对话重写（双端）：多供应商、流式 SSE 解析、thinking 模式、Shiki 高亮、代码渲染配置、JSONL 持久化

#### 移动端
- 插件系统启用：动态路由、插件管理页、工具箱入口、插件导航页签
- Android SAF 文件/目录选择器（startActivityForResult）
- 生物认证：密钥、认证设置页、质询-响应、设备身份持久化
- 终端：会话预载、游标式输出订阅、TUI 滚动兼容（SGR）、Agent CLI 命令预设、16 键默认快捷键
- 强调色色板（与桌面端同源）

#### 桌面端
- 终端 PTY 回放与历史播放（Rust 侧恢复窗口关闭期间丢失的输出）
- 字节流 PTY 输出管线：字节偏移契约、游标增量重订阅
- 四套主题色板（forest / ocean / sunset / violet）
- 生物质询-响应与连接历史
- SystemInfo 采集与设备名广播、通用加密工具模块

### 变更

- 插件后端完全 WASM 化（移除 cdylib）；Rust MSRV 提升至 1.94（wasmtime 47）
- Toast 迁移到 vue-sonner（双端）
- 桌面 UI 基于 Warm Workbench 设计重建；移动 UI 统一为分组卡片风格；字号 token 化
- 终端尺寸控制改为远端优先，移动端支持暂停/恢复订阅
- 输出管线迁移到本地 WS 单通道（桌面端）；游标式订阅取代 2MB 前端环形缓冲（移动端）
- 构建：rust-lld 链接器、thin LTO、版本升级脚本、安装包 release 后缀重命名、CI 以 wasm32 目标构建插件产物 + Windows 签名指纹注入
- Skills 统一至 `.agents/skills/`，供 pi / OpenCode / Codex / Claude Code 共享

### 修复

- 终端：输出连续性（游标增量重订阅解决重放风暴）、长时间运行页面崩溃、丢帧（异步插件回调、drain/reset）、重连后尺寸同步
- 文件传输：文件名冲突（409 + 拒绝原因）、通知风暴、任务竞态、Windows 路径分隔符、资源管理器定位、`.part` 残留清理
- 插件：多插件 PluginContext 污染导致 i18n 失效、WASM trap 恢复、燃料耗尽 trap、loader 句柄释放、WSL 子进程超时
- 移动端：心跳 blocking_write panic、Activity 重建后选择器失效（EBADF）、订阅泄漏、重连状态不一致
- 桌面端：设置保存循环（内容快照比对）、端口输入、删除后会话命名回退

### 安全

- 插件身份校验与权限审批（防冒充）
- 生物认证链路加固：IPC 序列化、DER 解析、绑定守卫自检
- 为 Agent hooks 保留本地旁路的 JWT 网关（token 从 hook 脚本中移除）

### 测试

- 前端 +175，桌面端 Rust +204，移动端 +116，SDK 契约测试（桌面 5→85，移动 2→79），file-transfer 宿主单元测试

---

## [1.1.0] - 2026-07-05

### 新增

#### 插件系统
- 支持 cdylib 动态加载的 Rust 插件 API crate
- 插件清单类型与权限系统
- PluginHost 与 API 桥接 Tauri 命令
- UI 插槽的扩展点注册表
- 插件加载器、存储与 `AppError::Plugin` 变体
- 完整的前端插件系统（PluginRegistry）
- 自动生成配置表单的 PluginConfigView 页面
- 带列表、开关、可展开详情的 PluginsView 页面
- usePluginManager composable
- PluginTerminalToolbar 与 PluginTitleBarItems 渲染组件
- registerTerminalToolbarItem 与 registerTitleBarItem 代理 API
- AI chatbox 插件重写为独立 cdylib 插件
- 资源目录插件加载与 API 安全
- 插件侧边栏 / 工具箱视图路由与导航
- 插件页面 i18n key

#### 移动端
- 面向性能的 Buffer-Only 终端架构
- mDNS 服务发现与广播
- 按会话的任务通知系统
- 自动执行任务引擎与终端集成
- WebSocket 心跳保活与重连改进
- 侧边栏 + 代码显示布局的 CodeExplorerView
- 支持行级着色的 Diff 渲染
- 带 diff 模式的 FileViewerModal
- 带类型徽标、状态与操作菜单的 PresetTaskCard 组件
- 基于 localStorage 持久化的 usePresetTasks composable
- 快捷键配置弹窗与终端输入栏无限轮播
- loading 遮罩与交互改进
- 快捷栏按钮颜色与快捷键面板一致
- 所有弹窗的平滑开合动画
- 集成带通配符 scope 权限的 tauri-plugin-http
- ForegroundService 中的 WakeLock

#### 桌面端
- Actix Web 服务器的高级网络配置
- 带配置迁移到 properties 格式的服务器管理页
- 用于设备识别的指纹跟踪
- 启动时端口可用性检查
- FileSidebar 标题栏中的 Git 分支切换器
- 电源管理功能
- Claude Code hooks 从全局配置迁移到项目级配置
- 带 session ID 绑定的全局化 hooks

#### 服务端 / 后端
- 在现有 WsServer 旁新增 Actix Web HTTP 服务器
- Actix Web HTTP 控制器、DTO 与中间件
- 用于终端 I/O 的 Actix WS actor
- WS 指标与配置端点
- HTTP + WS 双协议支持
- 文件内容 / diff 树 HTTP API
- 终端输出缓冲区以减少 WebSocket 消息数
- 当前行输入跟踪与插件事件响应

#### 国际化
- 带语言持久化的 vue-i18n 基础设施
- 全部视图、组件、composable 的 i18n + 错误码系统
- 带语言切换 UI 的 i18n 设置页
- 导航、布局与共享组件的 i18n
- 终端视图与输入栏的 i18n
- BottomSheet 与 PairingInput 组件的 i18n
- 桌面端 SessionManager、SessionsConfig 与组件文件的 i18n

#### 代码查看器
- useCodeHighlight 多主题支持
- 用于代码查看器设置的 useCodeViewerStore
- CodeViewerSettingsModal 组件
- 在 FileViewerModal 与 CodeExplorerView 中集成代码查看器设置

### 变更

- 插件重构为任务状态管理器，引入 KeyCombo 系统与自动审批模式
- 用进程内 Actix Web 取代 IPC 子进程
- 将 event/ 合并进 events/，修复 IPC 运行时
- 移除 desktop/ 与 shared/ 层级，Rust 模块按领域扁平化
- 移动端模块结构扁平化并迁移 Android 包名
- 移动端：移除 auto-executor、抽出 FileExplorer、新增浅色代码主题
- 移动端：TerminalView 重构并简化预设任务
- 桌面端：重组 Rust 模块、新增 mDNS、基于设计 token 重做 UI
- 桌面端：服务器重置默认值 + UI 打磨
- 移动端通知迁移
- 任务选择器重构

### 修复

- 移动端连接错误处理与状态一致性
- 路径分隔符归一为正斜杠
- 侧边栏动画改进
- 重连处理与特殊键修饰符
- 返回会话列表后移动端终端滑回问题
- 插件状态类型处理与表头
- PluginViewHost props 路由
- IPC reader 实现与 sysinfo 指标
- 按钮符号清理

---

## [1.0.0] - 2026-06-30

### 新增

#### 核心架构
- 多项目 monorepo：bedcode-desktop + bedcode-mobile 作为独立项目
- 桌面端与移动端之间的 WebSocket + HTTP 双协议通信
- 用于设备配对的 X25519 密钥交换
- 所有通信的 AES-GCM 加密
- 基于系统 keychain / secret service 的安全存储
- 带 60 秒过期时间的 6 位配对码认证

#### 桌面端
- 会话管理界面（创建、编辑、删除会话）
- 带二维码显示的设备配对界面
- 集成 xterm.js 的终端预览
- 带快捷操作的系统托盘
- 网络与外观配置的设置页
- 面向 Windows 与 WSL2 的 PTY（伪终端）管理
- 基于 SQLite 持久化的会话配置管理
- 面向移动端的 WebSocket 服务器
- mDNS 设备发现服务
- Tmux 会话集成

#### 移动端
- 设备发现与配对流程
- 支持增强 / 原始模式切换的终端输出显示
- 带特殊键（Tab、Ctrl+C、Esc 等）的输入栏
- 可自定义命令的快捷操作网格
- 带搜索功能的历史记录
- 带通知偏好的设置页

#### 后端（Rust）
- 基于 SQLite 的数据层（pairings、sessions、messages、quick actions）
- 基于 portable-pty 的 PTY 进程管理
- 带路径转换的 WSL2 支持
- WebSocket 消息协议
- ANSI 转义序列解析器
- Markdown 代码块提取器
- 带等待输入检测的输出解析器
- 带免打扰时段的通知服务

### 安全
- 所有 WebSocket 通信经 WSS 加密
- 配对码 60 秒后过期
- 连接时校验设备指纹

---

## [0.1.0] - 2026-04-30

### 新增

#### 核心功能
- 基于 Tauri 2.0 + Vue 3 + TypeScript 的初始项目结构
- 面向 Windows 与 WSL2 的 PTY（伪终端）管理
- 基于 SQLite 持久化的会话配置管理
- 面向移动端的 WebSocket 服务器
- mDNS 设备发现服务
- 6 位配对码认证

#### 桌面 UI
- 会话管理界面（创建、编辑、删除会话）
- 带二维码显示的设备配对界面
- 集成 xterm.js 的终端预览
- 带快捷操作的系统托盘
- 网络与外观配置的设置页

#### 移动 UI
- 设备发现与配对流程
- 支持增强 / 原始模式切换的终端输出显示
- 带特殊键（Tab、Ctrl+C、Esc 等）的输入栏
- 可自定义命令的快捷操作网格
- 带搜索功能的历史记录
- 带通知偏好的设置页

#### 后端（Rust）
- 基于 SQLite 的数据层（pairings、sessions、messages、quick actions）
- 基于 portable-pty 的 PTY 进程管理
- WSL2 支持（含路径转换）
- Tmux 会话集成
- WebSocket 消息协议
- ANSI 转义序列解析器
- Markdown 代码块提取器
- 带等待输入检测的输出解析器
- 带免打扰时段的通知服务

#### 安全
- 用于设备配对的 X25519 密钥交换
- 通信的 AES-GCM 加密
- 基于系统 keychain / secret service 的安全存储

### 变更
- 无（首次发布）

### 修复
- 无（首次发布）

### 安全
- 所有 WebSocket 通信经 WSS 加密
- 配对码 60 秒后过期
- 连接时校验设备指纹

---

## 版本历史

| 版本 | 日期 | 说明 |
|---------|------|-------------|
| 2.1.1 | 2026-09-18 | 终端输出管线 TB v3 + 环形缓存、插件 zip 加载/卸载、移动端 Rust HTTP 代理 + fail-closed Egress、file-transfer 暂停/续传、移动端终端体验、SDK host-peer 原语、自适应构建包装器 |
| 2.1.0 | 2026-09-10 | 对等网络（`packages/peer-net` + `link-crypto`）含 TLS 1.3 mTLS 与信任存储、file-transfer 对等化重写、JWT + 常驻事件 WebSocket、终端输出管线重写、统一前端日志、WASM trap 日志、E2E + CI 门禁、SDK 更名为 `@binblink/bedcode-plugin-sdk-*` |
| 2.0.0 | 2026-08-16 | WASM Component Model 插件平台、auto-task / file-transfer / ai-chatbox 插件、移动端插件系统、生物认证、UI 重做 |
| 1.1.0 | 2026-07-05 | 插件系统、移动端终端重构、国际化、Actix Web 服务器 |
| 1.0.0 | 2026-06-30 | 多项目 monorepo，桌面 + 移动稳定版发布 |
| 0.1.0 | 2026-04-30 | 含核心功能的首次发布 |
