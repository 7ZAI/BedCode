# file-transfer 双端共享业务核：`packages/bedcode-file-transfer-core`

## 状态

**已立项，分票实施中（2026-10-09）**。规格与逐票记录：
`.scratch/2026-10-09-file-transfer-shared-core/spec.md`。

- 票 T1（核 crate 骨架 + 边界锁）**已落地**：`domain` / `ports` / `identity` / `settings` /
  `roots` / `sessions` + 5 例边界锁。
- 票 T2（`transfer` 模块迁入）**已落地**：领域类型 + 23 个函数（归约 / 视图 / 生命周期标注 /
  重试判据 / 发送闸门 / 拉取意图队列 + 桌面旧快照通路两个专用纯函数），核内 **66 测试全绿**，
  函数级等价校验 PASS（见本文「验证」）。
- 票 T4（移动端接线）**已落地**：新增 `MobilePorts` 适配器；`transfer_store` / `settings_store` /
  `roots_registry` / `device_bridge` 收窄为薄包装（对外签名不变 ⇒ `peer.rs` 与前端零改动）；
  插件 crate **29 测试全绿**、产物重建并同步、产物 host import 面与改动前**同集合**。
- 票 T3（桌面端接线）**已落地**：新增 `DesktopPorts` 适配器；四个模块收窄为薄包装，
  `peer.rs` 仅 2 处改动、前端零改动；插件 crate **47 测试全绿**。按用户裁决 **B**，
  桌面随接线获得 `pull-started` 建行锚点（旧快照通路原样保留，继续承担对账/校正）。
- 票 T5（锁与文档）**已落地**：新增接线防漂移锁 `src/wiring_lock.rs`（零回流 / 台账纯转出 /
  适配器在场 / 端口面登记，5 例 + 变异自检 4/4）；两条锁按治理锁要求迁入 `src/` 由
  `#[cfg(test)] mod` 引入（crate 根 `tests/` 被 `capability_crates_unit_tests_only.rs` 禁止）；
  两端 code-map、CHANGELOG 双语、本 ADR 同批更新。核 crate **72 全绿**。
- 票 T7（编排层共享裁决）**已裁决并落地**（用户裁决 B = 以移动端票 08 修正版为准）：桌面
  `peer.rs` 切核后获得 `pull-started` 建行锚点 + 重试判据前置 / 发送闸门 / 排队批派发失败
  落终态行（spec §8 记录；真机复验留 T6）。
- 票 T6（真机复验）机器部分 done（见 spec `t6-manual-regression.md` §0），真机互传待人工执行；
  桌面 wasm 产物重建被**在途基线**阻塞
  （`manifest-gen` 权限表未跟演，未改动的 `ai-chatbox` 同样失败）与本机缺 wasip3
  工具链，留 T6。

本任务**零 ABI / 零 WIT / 零协议变更**：不动双端 `bedcode.wit`、不动宿主、不动权限位、
不动前端命令面与事件名（移动端页面零改动）。移动 ABI 仍 19、桌面 35（同期双端 ABI 演进：
移动 18→19 = `host-database` 退役，桌面 31→35 = 认证中心下沉 + `host-database` 退役）。

## 背景

`com.bedcode.file-transfer` 在双端各有一份 wasm 应用实现（桌面 4.4k 行 / 移动 3.4k 行）。
两端是**同一产品在两种宿主形态上的两个 app**，结构同源（`peer.rs` / `transfer_store.rs` /
`settings_store.rs` / `roots_registry.rs` / `device_bridge.rs` 一一对应），可共享面约 70%，
其余 30% 的差异集中在九处（持久化原语、wire 字段名、节点电源、落点策略、平台选择面、
信任决策路径、旧快照通道）。双份实现的代价是**每次修复两处同步税**，且已经开始分叉
（移动端 roots 走 KV 但缺显式回滚路径的年代差等）。

这与 ADR 0040 描述的 wasm-core 处境同构，落点手法也相同：**先 fork 对齐、再抽共享核**。
本例的 fork 对齐阶段在历史上已自然完成（两端本就是同源复制），本轮直接进入第二步。

## 决策

### D1 · 新建 `packages/bedcode-file-transfer-core`，双端 app 退化为适配器

落点与 `bedcode-host-api-core`（ADR 0040 第二步产物）同族：**双端共享的实现层**，
落仓库根 `packages/`，命名沿用组织前缀。

- **核内**：可共享的领域类型、纯函数判据、编排骨架。
- **端内**：端口实现（`adapters.rs`）+ 持久化实现（桌面 `store_db.rs` / 移动 `store_kv.rs`）
  + `WasmPlugin` 外壳 + 命令面透传。
- **核的依赖纪律**：只有 `serde` / `serde_json`。零任一端 SDK、零平台（tauri）、零 WIT。
  核内 `src/boundary_lock.rs` 就地钉死（含依赖白名单断言）。

**否决项**：feature-gate 共享（ADR 0042 形态）不适用——两端 SDK 是两个 crate，没有
「同一份实现 + 两个 feature」的位置；双份实现永久保留不采纳（漂移税持续，见 ADR 0040 D1
对选项 B 的否决同理由）。

### D2 · 双端差异面全部经端口 trait（`ports`），差异面索引钉在核 crate 文档

九处差异各归其端口（完整索引见 `src/lib.rs` 模块文档表）：

| 差异面 | 端口 | 桌面 | 移动 |
| --- | --- | --- | --- |
| 共享根持久化 | `RootsStore` | plugin-db 表 `shared_roots` | storage 键 |
| 任务台账持久化 | `EntryStore`（T2） | plugin-db 表 `transfer_entries` | storage 键 |
| 推送 wire 字段名 | `RootWireCodec` | `{ id, name, path }` | `{ id, name, safTreeUri }` |
| 节点电源 | `NodePower` | 插件显式请求起停 | 宿主外壳驱动 |
| 目录多选 / 打开所在目录 | `PlatformPort` 可选方法 | 支持 | 不支持 |
| 接收落点可否自定义 | `PluginProfile::supports_custom_download_dir` | 可（UI 选目录并推送） | 否（固定 MediaStore.Downloads） |
| 信任决策 | `ConsentGate` | 互调认证中心 + 双轨降级 | 直答宿主原语 |
| 旧快照通道 | `PluginProfile` | 双写期仍订阅 | 已整条退役 |

**判据：双端都有的原语不给默认实现（缺失即编译错误）；只在一端成立的能力给默认实现且
默认语义 = 显性 `unsupported`，不是「假装成功」。** 默认 `unsupported` 是 fail-visible：
某端误接线到不存在的原语会在调用点报错，而不是静默走空实现。

### D3 · 核内零产品身份字面量（插件 id 运行期注入）

插件 id 与事件短名经 `identity::PluginIdentity` 注入（属主私有 topic `<id>::<suffix>`、
前端事件名 `plugin:<短名>:<suffix>` 由核生成）。理由有二：

1. 核若硬编码 `com.bedcode.file-transfer`，它就从「共享实现」变成「知道有哪个产品」——
   与宿主侧语义锁 `capability_crates_no_product_ids` 的 C-4 判据直接相关；
2. 该 crate 位于根 `packages/`，**必须登记进该锁的 `SCANNED_CRATES`**（否则 C-3 反向断言红）。
   登记的意义不是形式主义：它让 C-4 成为核的**外部门禁**，核内 `src/boundary_lock.rs`
   是同一判据的**就地版本**（不必等桌面 host 构建就能测）。

### D4 · 状态是实例而非核内全局静态

退役前双端的 endpoint memo / session 句柄表是 `OnceLock<Mutex<…>>` 模块级静态，
跨用例状态互清，只能靠一把 `statics_lock` 串行化测试。核内改为 `sessions::SessionTable`
实例（`&mut self` 方法），由各端适配器自己持 `OnceLock<Mutex<_>>`：核的测试天然并行安全。

### D5 · 边界红线

1. **页面不动**：移动端 `src/components/**`、前端 composables、`plugin.json`、命令 id、
   事件名全部逐字保留（用户指令原话：「保留移动端文件传输页面不变」）。
2. **行为契约随实现搬入并有测试**：判据（retry 前置 / 闸门 / 归档封顶 / 回滚语义 / 幂等性）
   搬到核后由核单测覆盖，双端适配器只做 1:1 委派，不复制判据。
3. **不顺手改语义**：本例是结构重构，任何一处发现两端行为不一致时**停下问用户**
   （选哪一端为准是产品决策，不是重构决策）。

### D6 · 共享基线 = 较新且与引擎契约一致的一端；桌面侧落后项不顺手统一（T7 裁决点）

核内实现以**移动端**为基线，依据是实测而非偏好：

1. `pull-started` 事件由**两端共用**的引擎 crate（`packages/bedcode-server-peer-net/src/
   peer_engine_remote.rs`）发出，其注释明写「任务行由插件归约自建」；桌面插件没有该建行
   锚点，与引擎契约和桌面自有文档（「事件归约是 store 主写、快照 merge 退化为校正」）都不符。
2. 票 08 三项修正（重试判据前置、发送闸门、排队批派发失败落终态行）只在移动端存在，
   桌面 `peer.rs` 无对应物——桌面存在「先发后校验铸出无主会话」「派发失败静默丢用户意图」
   这两个已被移动端修掉的缺陷。

**这两处统一到核内即等于给桌面加行为，不属于「把差异抽象成 trait」的范畴**，故本轮
不顺手做：T2 只落纯函数（核内无消费方 ⇒ 双端行为零变化），桌面侧统一立 **T7** 待裁决
（选桌面为准 / 选移动修正版为准 / 分端保留）。裁决前桌面接线不得把这些差异一并带入。

## 实施纪律（本任务踩出来的，对同类重构同样适用）

1. **插件 crate 禁 `cargo fmt` 整 crate**：双端插件源文件既非 rustfmt-clean、行尾还逐文件混用
   （同目录下 `peer.rs`/`lib.rs`/`transfer_store.rs` 是 CRLF，`roots_registry.rs`/
   `settings_store.rs` 是 LF）。整 crate fmt 会让 `peer.rs` 产生 +116 行 / 2880 行的纯格式 diff，
   把真实改动淹没。**只对本次新建 / 重写的文件单独 `rustfmt <file>`**，并按该文件 HEAD 行尾归一。
2. **`rustfmt <单文件>` ≠ 行尾/格式状态判据**：目标文件含 `mod tests { mod <子文件>; }` 时，
   rustfmt 在临时目录下解析不到子模块会**静默跳过**（stderr 被吞看不出），据此会误判
   「HEAD 已 fmt-clean」。判定用直接测量：`git show HEAD:<file> | grep -c $'\r'`。
3. **回退格式噪音用「恢复 HEAD + 逐处重放」而非 `git checkout`**：先用
   `diff <(git show HEAD:f | sed 's/\r$//') <(sed 's/\r$//' f)` 确认非格式差异**只有**本任务的
   预期改动，再恢复 HEAD 并按协议重放（脚本断言每处替换命中数 = 1，CRLF 自适应）。

## 锁的落点约束（新增 crate 必须知道）

`capability_crates_unit_tests_only.rs` 的治理面按 `packages/bedcode-*` 目录约定**自动推导**
（新 crate 自动纳入），判据两条：crate 根不得有 `tests/` / `benches/` / `examples` / `[[test]]`
段（crate 根 `tests/` = 独立测试二进制 = 只能经 `pub` API 访问的对外行为面），
且 `[dev-dependencies]` 不得含 `bedcode*` 前缀的内部 crate。

⇒ **共享核的锁必须写在 `src/` 并由 `#[cfg(test)] mod <name>;` 引入**（本 crate 的
`boundary_lock.rs` / `wiring_lock.rs` 即此形态）。随之而来的两个必要细节：

- **锁文件需自排除**：它们含被禁 needle 的**字面量常量表**，不排除就会扫到自己而恒红；
  同时必须有「真生产文件不得被自排除误伤」的正面断言，防自排除扩大化让锁空转。
- 自排除的变异测试要注入**真实生产文件**（注入锁自身不会红，属预期而非锁失效）。

## 影响面（已落地部分）

- **新增** `packages/bedcode-file-transfer-core`：`Cargo.toml`（serde / serde_json；
  dev-dep tempfile）、`src/{lib,identity,ports,domain,settings,roots,sessions,transfer}.rs`、
  `src/transfer/tests.rs`（传输台账 34 例）、`src/boundary_lock.rs`（5 例）、
  `src/wiring_lock.rs`（接线防漂移 5 例）——两条锁放 `src/` 并由 `#[cfg(test)] mod` 引入：
  治理面按 `packages/bedcode-*` 约定自动推导，crate 根出现 `tests/` 即被宿主侧
  `capability_crates_unit_tests_only.rs` 判红。
- **桌面宿主锁** `bedcode-desktop/src-tauri/tests/capability_crates_no_product_ids.rs`：
  `SCANNED_CRATES` + 1，模块头「9 个」改「10 个」并写明第二类形态（产品业务核）。
- **零 ABI / 零 WIT / 零宿主行为变更**：双端 wasm 应用与宿主代码本票未触碰。

## 验证（已落地部分）

- 新 crate `cargo test`：**66 全绿**（领域/身份/设置/注册表/会话/传输台账 61 + 边界锁 5）；
  `cargo fmt --check` OK；IDE linter 0。
- **函数级等价校验** `.scratch/.../equiv-check-transfer.py`：23 个函数（21 共享 + 2 桌面专用）
  全部与各自基线等价，其中 22 个连空白与花括号分组都逐字相同；桌面端 3 处差异全部在允许
  清单内并写明理由（`local_path` 加法字段 ×2、`pull-started` 锚点 ×1）。
  校验器自带自查（防假绿 / 防假红 / 引号感知 / 括号内逗号不误删）。
  **口径取舍**：判据忽略「空白 + 花括号分组 + 分支逗号」——rustfmt 会按所属 crate 上下文
  重排 match 分支与 let-else 块，逐字判红会产出会被忽略的噪音锁；标识符/字面量/运算符序列
  仍逐 token 比对，判据改写照旧会红。
- 变异自检 **2/2**（T1）：核内注入 SDK 针脚字符串 → SDK 锁红；注入产品 id 字面量 → 身份锁红；
  两条均还原并核 sha256 一致，复跑绿。
  （首次尝试用 `use bedcode_plugin_api::…` 做变异被**编译期**拦下，未走到文本锁——
  改用可编译的字符串常量变异才真正验证到锁，该坑记入票据。）
- 桌面 `capability_crates_no_product_ids` **未实跑**：桌面 host target 已清空，全量重建需
  10G+ 与数十分钟。改用等价预检脚本
  `.scratch/2026-10-09-file-transfer-shared-core/gate-precheck-desktop-lock.py`
  （从锁源解析登记表，复刻 C-3/C-4/C-5/C-6）：新增 crate 三条判据全 PASS。
  **残余风险**：预检是对 Rust 实现的复刻，偏差需以宿主实跑为准。
- **T2 等价校验**：见上（23 个函数与各自基线等价）。
- **T4 移动端接线**：插件 crate `cargo test` **29 全绿**（0 warning）；
  `node scripts/plugin-build.js --plugin com.bedcode.file-transfer` 重建并同步产物
  （735,636 字节）；产物内 host import 面与改动前**同集合**（未新增任何 WIT import，
  即「零契约变更」的机器可验证据）；移动端前端未改，故不跑 `test:run`。
  **未运行项**：移动宿主 `cargo test`（`src-tauri/target` 已清空，全量重建 10G+）；
  残余风险 = 宿主侧「插件实例化 + 真实组件全链路」用例未复跑。
- **T4 实测驱动的端口面修正**：`EventPort::emit_event` 改不可失败、`PlatformPort` 改
  `Vec<String>`/`String`、`MdnsPort::mdns_stop_browse` 改返回 `bool`（三处原与双端 SDK 不符）；
  新增形态位 `PluginProfile::supports_custom_download_dir`（落点可否自定义**不是**能力差异，
  移动 SDK 同样有 `set-download-dir` 原语——写成原语有无会把「移动端误推落点」变成静默路径）。
- **T3 桌面端接线**：插件 crate `cargo test` **47 全绿 / 0 warning**；`peer.rs` 2 处
  （函数名统一 + 字面量补 `local_path`）、前端零改动；plugin-db 侧适配器由「只认三条语句的
  最小内存 SQL 执行器」夹具钉住语句形状。
  **未完成项**：桌面 wasm 产物重建——阻塞 ① `manifest-gen` 权限映射表与真源未跟演
  （未改动的 `com.bedcode.ai-chatbox` 同样失败，属他人会话在途，不代为修）；
  ② 本机未装 wasip3 pinned nightly。**残余风险**：桌面产物 import 面与真实宿主加载未复验（留 T6）。
- **T7=B 已在 T3 落地**：桌面随接线获得 `pull-started` 建行锚点；票 08 三项编排修正
  （在 `peer.rs` 层）**仍不在本轮**，须单独立票 + 真机复验。
- **如实上报（非本票引入）**：预检复现出 `bedcode-host-api-core` 与
  `bedcode-headless-host-probe` 未登记进两桶 ⇒ 该锁 C-3 在现有 HEAD 即为红；按「各归各自票据」
  不代为登记。

## 与既有 ADR 的关系

- **ADR 0040**：同一两步走手法的第二个落点（第一步 fork 对齐已历史完成）；共享核形态
  （零 WIT / 零 SDK 依赖）与 `bedcode-host-api-core` 逐字同族。
- **ADR 0035 / 0042 / 0043**：端口抽象范式（`Ports` 注入差异面）的延续；本例差异面从
  「平台机制」扩展到「产品形态差异」（节点电源 / 落点策略 / 信任决策路径）。
- **ADR 0018**：双端插件契约仍各自独立（本任务 WIT / ABI 零变更，不构成任何跨端契约推断）。
- **ADR 0022**：本核是**插件侧业务代码**，不是宿主能力面，§5.1 B1–B6 判据不适用于它；
  但与宿主交互仍只经既有原语（核内无任何新增宿主能力诉求）。
