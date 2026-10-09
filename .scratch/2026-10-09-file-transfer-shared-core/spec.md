# file-transfer 双端共享业务核（抽 trait，双端各自实现差异面）

> 用户指令（2026-10-09）：「移动端文件传输 wasm app 用和桌面端一样的 wasm 来替换现在移动端的
> 实现；保留移动端文件传输页面不变，内部实现重构」+「双端不同的部分抽象成 trait，双端有不同
> 的文件系统实现」。
>
> 落点：新建根 `packages/bedcode-file-transfer-core`（业务核）+ 双端 wasm app 退化为薄适配器。
> 先例：ADR 0040「先 fork 对齐、再抽共享核」两步走的第二步（与 `bedcode-host-api-core` 同形）。

## 0. 事实基线（实测，非文档推论）

- 双端 file-transfer **都已经是 wasm app**，产物分别在
  `bedcode-desktop/src-tauri/resources/plugins/desktop/com.bedcode.file-transfer/bedcode_plugin_file_transfer.wasm`
  与 `bedcode-mobile/src-tauri/resources/plugins/mobile/com.bedcode.file-transfer/bedcode_plugin_file_transfer.wasm`。
  **不需要「从零改造成 wasm」**，需要的是把双份业务实现收敛成一份。
- 移动端页面（`bedcode-mobile/wasm-apps/file-transfer/src/components/*.vue` 14 个）与前端 composables
  **整体不动**——本次只动 `rust/`。
- 双端 rust 规模：桌面 4,453 行 / 7 文件，移动 3,417 行 / 6 文件（不含测试子树）。

## 1. 双端差异矩阵（逐面实测）

| 面 | 桌面 | 移动 | 归类 |
| --- | --- | --- | --- |
| SDK crate | `bedcode_plugin_api` | `bedcode_plugin_api_mobile` | **适配器**（不可共享） |
| 共享根持久化 | `host-plugin-database` 表 `shared_roots` | `host-storage` 键 `shared_roots` | **差异面 → trait** |
| 任务台账持久化 | `host-plugin-database` 表 `transfer_entries` | `host-storage` 键（`ENTRIES_KEY`） | **差异面 → 留在各端编排层**（台账本体是纯函数，见 §3 修正） |
| 共享根 wire 形状 | `{ id, name, path }` | `{ id, name, safTreeUri }` | **差异面 → trait**（字段名编解码） |
| 节点电源 | `peer_start_node` / `peer_stop_node` 由插件 activate/deactivate 显式请求 | 宿主外壳驱动，插件不调 | **差异面 → trait**（默认 no-op） |
| 接收落点 | `peer_set_download_dir` + `pick-download-dir` 命令 | 固定 MediaStore.Downloads，命令报 unsupported | **差异面 → trait**（默认报 unsupported） |
| 目录/文件选择 | `pick_files` / `pick_folder` / `pick_folders` | `pick_files` / `pick_folder` | **差异面 → trait**（可选方法默认报错） |
| 打开所在目录 | `platform_reveal_in_dir` + `reveal-in-dir` 命令 | 无（移动端无此命令） | **差异面 → trait**（默认报 unsupported） |
| 信任 / consent 决策 | 经互调认证中心（`auth_center.rs` 940 行，ADR 0031/0033） | 直答宿主原语 `peer_respond_consent` | **差异面 → trait**（认证中心接入是桌面独有） |
| 快照 merge 通道 | 仍订阅 `peer:transfer` / `peer:receive` / `peer:devices`（双写期对账） | 已整条退役 | **差异面 → 端口能力位** |
| mDNS topic 构造 | `mdns_event_topic(MDNS_FOUND, PLUGIN_ID)` | `format!("{PLUGIN_ID}::mdns:found")` | 同值，核内统一 |
| 其余（transfer_store 归约 / roots 纯函数 / settings 纯函数 / sessions 句柄表 / peer 编排） | —— | —— | **可共享** |

结论：**可共享面 ≈ 70%**（纯函数 + 编排骨架 + 领域类型），差异面收敛为 9 个端口方法组。

## 2. 目标架构

```
packages/bedcode-file-transfer-core/         ← 新：业务核（零 SDK / 零 WIT / 零平台依赖）
├── Cargo.toml          依赖仅 serde + serde_json
└── src/
    ├── lib.rs          facade + 文档
    ├── domain.rs       双端同形领域类型（TransferSettings / SharedRoot / DialEndpoint /
    │                   DeviceSnapshotEntry / TransferEntry / 批与事件归约产物…）
    ├── ports.rs        端口 trait 集合（差异面唯一落点）
    ├── identity.rs     运行期注入的插件身份（id / 短名），核内**零产品 id 字面量**
    ├── settings.rs     接收策略与落点设置（纯函数 + 存储/推送编排）
    ├── roots.rs        共享根注册表（纯函数 + 存取编排）
    ├── sessions.rs     endpoint memo + session 句柄表（进程内状态机）
    └── transfer.rs     任务台账归约（reduce / merge / retry / 闸门 / 归档封顶）

bedcode-desktop/wasm-apps/file-transfer/rust/src/
├── adapters.rs         ← 新：桌面端口实现（newtype over WasmHost，1:1 委派 SDK）
├── store_db.rs         ← 新：桌面 plugin-db 版 RootsStore / EntryStore
└── lib.rs / peer.rs …  ← 缩为「WasmPlugin 外壳 + 端口装配 + 命令面透传」

bedcode-mobile/wasm-apps/file-transfer/rust/src/
├── adapters.rs         ← 新：移动端口实现
├── store_kv.rs         ← 新：移动 KV 版 RootsStore / EntryStore
└── lib.rs / peer.rs …  ← 同上
```

**为什么把核放根 `packages/`**：与 `bedcode-host-api-core`（ADR 0040 第二步产物）同族——
双端共享的、无 WIT / 无 SDK 依赖的实现层。命名沿用组织前缀 `bedcode-`。

**与 `capability_crates_no_product_ids` 锁的关系（必须处理）**：根 `packages/` 下每个
`bedcode-*` 目录必须登记进 `SCANNED_CRATES` 或 `PENDING_SCAN_CRATES`。本 crate 登记进
`SCANNED_CRATES` 并**遵守 C-4**：核内生产代码零 `com.bedcode.<产品段>` 字面量——插件 id /
事件命名空间经 `identity.rs` 运行期注入。这条对核是**正面收益**（业务核不得硬编码产品身份）。

## 3. 端口清单（差异面唯一落点）

```rust
pub trait LogPort      { fn info(&self, msg: &str); fn error(&self, msg: &str); }
pub trait BusPort      { publish / subscribe / unsubscribe }
pub trait EventPort    { fn emit(&self, name: &str, payload: &Value); }   // 不可失败（对齐双端 SDK）
pub trait KvStore      { get / set / delete }                       // 双端同名同形
pub trait RootsStore   { load / save }                              // 差异面①：SQL 表 vs KV 键
pub trait RootWireCodec{ fn to_push_payload(&self, roots) -> Vec<Value>; }  // 差异面③：path vs safTreeUri
pub trait PeerPort     { dial/close/send/respond/pause/resume/set-policy/set-download-dir/
                         set-shared-roots/list-shared-roots/browse-directory/pull-files/
                         list-trusted/revoke-trusted/respond-consent/active-transfers/collect-outgoing }
pub trait NodePower    { start/stop }                               // 差异面④：桌面显式，移动 no-op 默认
pub trait PlatformPort { pick-files→Vec<String> / pick-folder→String / pick-folders* / reveal-in-dir* }
pub trait MdnsPort     { browse→String / stop-browse→bool }
pub trait ConsentGate  { decide_consent / evaluate_consent / list_trusted }  // 差异面⑦：认证中心 vs 直答
pub trait PluginProfile{
    fn uses_legacy_snapshot(&self) -> bool;              // 差异面⑧：桌面双写期对账
    fn supports_custom_download_dir(&self) -> bool;      // 差异面⑤：桌面可选落点 / 移动固定
}
```
（`*` = 默认实现显性 unsupported；T4 实测修正了三处签名与 SDK 的出入，另补一处形态位。）

**T2 修正（2026-10-09）**：差异面②（任务台账持久化）**不落端口**。理由：`transfer` 模块全部
是纯函数与领域类型、无宿主 I/O，持久化调用点在各自端编排层（`peer.rs`）内；只有当编排层
本身也共享时，`load/persist` 才需要端口。故 `EntryStore` 不在本轮端口面内（引入即无人消费
的抽象），随「编排层共享」票据再评估。

**传输模块同样无端口**（`transfer.rs`）：领域类型 + 纯函数判据（归约 / 重试 / 闸门 / 意图
队列）。桌面端仍订阅旧快照 topic 这一差异由 `PluginProfile::uses_legacy_snapshot` 表达，
`prune_absent` / `reconcile_diff` 两个桌面专用纯函数留在核内但不参与共享编排。

`PortError`：轻量 `String` 包装 + `Display`，双端转 `anyhow` 零成本。

## 4. 票据拆分（每票独立可验证）

| 票 | 内容 | 门禁 | 状态 |
| --- | --- | --- | --- |
| T1 | 建核 crate：`domain` + `ports` + `identity` + `settings` + `roots` + `sessions`；单测全绿 | `cd packages/bedcode-file-transfer-core && cargo test` | **done** |
| T2 | `transfer` 模块迁入（任务台账归约 / retry / 闸门 / 归档封顶） | 同上 + 双端 `transfer_store` 函数级等价校验 | **done** |
| T3 | 桌面适配器接线：`adapters.rs` + 各模块改为委派核（含 T7=B：桌面获得 `pull-started` 建行锚点） | `cd bedcode-desktop/wasm-apps/file-transfer/rust && cargo test` + 产物重建 | **done（产物重建被在途基线阻塞，见 T3 记录）** |
| T4 | 移动适配器接线：`adapters.rs` + 各模块改为委派核 | `cd bedcode-mobile/wasm-apps/file-transfer/rust && cargo test` + 产物重建 + import 面核对 | **done** |
| T5 | 锁与文档：宿主语义锁登记 + 接线防漂移锁 + 两端 code-map + CHANGELOG 双语 | 核 `cargo test` + 等价预检 + 治理锁合规核对 | **done** |
| T6 | 端到端复验：双端 wasm 产物重建 + 真机（桌面↔移动）互传回归 | 手工项，交付说明逐条写「跑了 / 没跑」 | 机器部分 done（见 `t6-manual-regression.md` §0）；真机部分待人工执行 |
| T7（新） | 编排层共享裁决：桌面 `peer.rs` 缺票 08 三项修正（重试判据前置 / 发送闸门 / 排队批派发失败落终态行）与 `pull-started` 建行锚点——**这是桌面行为变更，须用户裁决后再动** | 裁决 + 真机复验 | **已裁决：以移动修正版为准，已落地**（见 §8 票 T7 记录） |

## 5. 零 ABI / 零 WIT / 零协议承诺

本次是**插件内部实现重构**：不动 `bedcode.wit`（双端）、不动宿主、不动权限位、不动前端
命令面与事件名、不动 `plugin.json`。移动端页面零改动。故移动 ABI 仍为 18、桌面 31。

## 6. 风险与对策

| 风险 | 对策 |
| --- | --- |
| 行为漂移（移动端此前无门禁的部分被核语义改变） | 每票先钉「行为契约」再迁；差异面只走端口默认实现，不改可共享面语义 |
| 核被反向污染（核里塞进某端专属逻辑） | 核内零 SDK 依赖 + 边界锁（needle `plugin_api` / `wasm_core` / `tauri` 跳注释） |
| 双端测试夹具 mock 需实现端口 trait 全集 | 端口方法给默认实现（`unsupported`），夹具只覆写被测面 |
| 根 `packages/` 目录体量与磁盘 | 核为零依赖小 crate（< 2 MB 源码），不影响 target 体量 |

## 7. 交付说明模板（每票收尾必填）

实际运行的命令与结果、无法运行项及原因、变异自检、i18n / 文档同步、残留进程清理。

---

## 8. 实施记录

### 票 T1（2026-10-09，已落地）

**新增** `packages/bedcode-file-transfer-core`：`Cargo.toml`（serde / serde_json；
dev-dep tempfile）、`src/{lib,identity,ports,domain,settings,roots,sessions}.rs`、
`src/boundary_lock.rs`（5 例）。

**门禁**：

| 项 | 命令 | 结果 |
| --- | --- | --- |
| 核内全量 | `cd packages/bedcode-file-transfer-core && cargo test` | **32 全绿**（27 lib + 5 边界锁） |
| 格式 | `cargo fmt` | 已施加，复跑仍绿 |
| 变异自检 | 注入 SDK 针脚字符串 → SDK 锁红；注入产品 id 字面量 → 身份锁红 | **2/2**，两条还原后 sha256 一致、复跑绿 |
| 宿主语义锁 | 桌面 `cargo test --test capability_crates_no_product_ids` | **未实跑**（桌面 host target 已清空，全量重建 10G+）；改用等价预检脚本 → 新增 crate 的 C-3/C-4/C-6 全 PASS |
| lint | IDE linter | 0 error 0 warning |

**踩坑（值得记）**：首次变异用 `use bedcode_plugin_api::…` 注入，被**编译期** E0433 拦下
（依赖不存在），根本没走到文本锁 ⇒ 看上去「锁红了」其实是编译失败。**文本锁的变异必须
可编译**（改用字符串常量才真正验证到判据）。

**如实上报（非本票引入）**：等价预检复现 `bedcode-host-api-core` 与
`bedcode-headless-host-probe` 未登记进两桶 ⇒ 桌面语义锁 C-3 **在现有 HEAD 即为红**。
按「各归各自票据」不代为登记，但需知悉：该锁在两条登记补上之前无法作为有效门禁。

### 票 T2（2026-10-09，已落地）

**新增** `packages/bedcode-file-transfer-core/src/transfer.rs` +
`src/transfer/tests.rs`（34 例）。

**基线裁决（实测支撑，非偏好）**：核内传输模块以**移动端实现为基线**——引擎侧的 `pull-started`
事件由两端共用的 `packages/bedcode-server-peer-net/src/peer_engine_remote.rs` 发出，其注释明写
「**任务行由插件归约自建**」；桌面插件缺该建行锚点与自有契约不符。移动端同时含票 08 三项修正
（重试判据前置、发送闸门、排队批派发失败落终态行），桌面 `peer.rs` 无对应物。

**核内函数面（23 个）**：领域类型 `TransferEntry`（含移动端票 07 的 `local_path` 加法字段）/
`RetryMeta` / `PullFileSpec`；判据 `is_terminal` / `is_active` / `entry_from_dto`；归约
`reduce_event`（三类建行锚点 + progress/paused/resumed/terminal 推进）/ `merge_snapshot`；
视图与结算 `insert_active_projections` / `mark_active_interrupted` / `evict_overflow` /
`clear_terminal` / `active_{send,receive}_entries` / `mark_cancelled` / `mark_paused` /
`apply_retry`；票 08 三项 `retry_source` / `send_slot_open` / `push_pull_intent` /
`take_pull_intent`；桌面旧快照通路 `prune_absent` / `reconcile_diff`。

**门禁**：

| 项 | 结果 |
| --- | --- |
| `cargo test` | **66 全绿**（61 lib + 5 边界锁） |
| `cargo fmt --check` | OK |
| IDE linter | 0 error 0 warning |
| **函数级等价校验**（`equiv-check-transfer.py`） | **PASS**：23 个函数全部与各自基线等价，其中 22 个连空白与花括号分组都逐字相同；桌面端 3 处差异全部在允许清单内且写有理由 |
| 校验器自查 | 4 项：判据改写/条件取反/字段改名/语句顺序被抓；分号与 match 块分支两种 rustfmt 规范化不误判；引号感知；括号内逗号不误删 |

**校验器口径（刻意取舍，必须记）**：等价判据 = 「忽略空白 + 花括号分组 + 分支逗号」的 token
序列相等。**rustfmt 会按所属 crate 上下文重排 match 分支与 let-else 块**（`=> { f(x) }` ⇄
`=> f(x),`、`else { continue; }` ⇄ `else { continue }`），同一份逻辑搬运后必然两种写法都出现；
判红会产出噪音锁，而噪音锁会被忽略——比「判据略松」严重得多。代价是花括号分组变化不再被捕获，
但标识符/字面量/运算符序列仍逐 token 比对（自查用例证明：加 `!` / 改字段名 / 换语句序都会红）。

**行为影响**：**本轮零**——核 crate 尚无任何消费方，双端 wasm 应用一行未改。
下文「桌面端差异」在 T3 接线时才会变成实际行为变化，届时须真机复验（见 T7）。

**如实上报：桌面端唯一的实质行为差异候选（待裁决，不在本轮改动范围）**

| 差异 | 桌面现状 | 核内（= 移动端修正版） | 影响 |
| --- | --- | --- | --- |
| `pull-started` 建行锚点 | 无（靠旧快照 merge 补行） | 有（事件即建行） | 桌面拉取任务行出现更早；与引擎契约一致 |
| 票 08 三项编排修正 | 无 | 有（`peer.rs` 已有对应调用） | 桌面将获得「不可重试先判、发送闸门、派发失败不丢用户意图」 |

两者都**不是**「双端不同的部分抽象成 trait」能消化的差异——它们是桌面侧落后的实现，
统一即等于给桌面加行为。**故不在 T2/T3 内顺手做**，立 T7 待用户裁决（选桌面为准 / 选移动
修正版为准 / 分端保留）。

### 票 T4（2026-10-09，已落地）—— 移动端接线

**新增** `bedcode-mobile/wasm-apps/file-transfer/rust/src/adapters.rs`（`MobilePorts<H>`
实现核端口 → 1:1 委派移动 SDK trait，零判据；含 4 例适配器用例）。

**收窄为包装的模块**（保留全部对外签名 ⇒ `peer.rs` / 前端零改动）：

| 模块 | 现形态 |
| --- | --- |
| `transfer_store.rs` | 纯转出：`pub(crate) use bedcode_file_transfer_core::transfer::*` |
| `settings_store.rs` | 三个包装（`load` / `load_or_migrate` / `save_and_push`）+ 既有 mock 用例（身份变为**适配器接线测试**） |
| `roots_registry.rs` | 两个包装（`load_all` / `apply_and_push`）+ 纯函数转出 |
| `device_bridge.rs` | `SessionTable` 实例 + `OnceLock<Mutex<_>>` 持有，函数面不变 |
| `lib.rs` | `mod adapters;` + `remember-peer-endpoint` 改用核内校验入口（删掉端内重复判据） |

**门禁**：

| 项 | 结果 |
| --- | --- |
| 插件 crate `cargo test` | **29 全绿**（0 warning） |
| 产物重建 | `node scripts/plugin-build.js --plugin com.bedcode.file-transfer` → 735,636 字节，已同步 `src-tauri/resources/plugins/mobile/` |
| **契约面核对** | 产物内 host import 面与改动前**同集合**（host-bus / events / log / mdns / peer / platform / storage），**未新增任何 import**；核逻辑字符串确认进入产物 |
| 前端 | 未改（移动端页面零改动，用户硬要求）⇒ 不跑 `test:run` |

**未运行项（如实上报）**：移动宿主 `cargo test` 未跑——`bedcode-mobile/src-tauri/target` 已被清空，
全量宿主重建需 10G+ 与数十分钟；本票未触碰宿主代码 / WIT / 权限 / plugin.json，契约面已用
产物 import 集合核对兜底。**残余风险**：宿主侧「插件实例化 + 真实组件全链路」用例未复跑。

**端口面修正（本票实测驱动，三处）**：

1. `EventPort::emit_event` 改为**不可失败**（双端 SDK 的 `emit_event` 返回 `()`；给不可失败调用
   套 `PortResult` 会诱使适配器写 `Ok(())` 式假分支）。
2. `PlatformPort` 改为 `Vec<String>` / `String` / `Vec<String>` / `()`（原写 `Value`，与双端 SDK 不符）。
3. `MdnsPort::mdns_stop_browse` 返回 `PortResult<bool>`（SDK 返回 bool）。

**新增形态位 `PluginProfile::supports_custom_download_dir`（差异面⑤落在核内的唯一闸门）**：
桌面 UI 可选接收落点并推 `set-download-dir`；移动端落点固定 `MediaStore.Downloads`。
不用「原语有无」表达——移动 SDK **也有**该原语（票 04 对齐），差的不是能力而是**产品形态**；
写成原语有无会把「移动端误推了落点」变成静默生效的路径。核内 `save_and_push` 据此闸门，
两端行为与改动前**逐字一致**（桌面 true / 移动 false）。

### 票 T3（2026-10-09，已落地）—— 桌面端接线（含 T7=B）

**新增** `bedcode-desktop/wasm-apps/file-transfer/rust/src/adapters.rs`（`DesktopPorts<H>`
实现核端口 → 1:1 委派桌面 SDK trait；含 4 例适配器用例，其中 plugin-db 夹具是一个**最小内存
SQL 执行器**，只认适配器实发的三条语句——语句形状变动会立刻测红）。

**收窄为包装的模块**（对外签名不变 ⇒ `peer.rs` 仅 2 处、前端零改动）：

| 模块 | 现形态 |
| --- | --- |
| `transfer_store.rs` | 纯转出（+ 顶部注明 T7=B 的行为变化） |
| `settings_store.rs` | 三个包装 + 既有 mock 用例（身份变为适配器接线测试） |
| `roots_registry.rs` | `ensure_table` / `load_all` / `apply_and_push` 三个包装 + 纯函数转出 |
| `device_bridge.rs` | `SessionTable` 实例 + `OnceLock<Mutex<_>>`，函数面不变 |
| `peer.rs` | 2 处：`mark_interrupted_on_load` → 核内统一名 `mark_active_interrupted`；`TransferEntry` 字面量补 `local_path: None` |
| `lib.rs` | `mod adapters;` + 端点登记改用核内校验入口 |

**T7=B 落地**：桌面 `transfer_store` 切核后获得 `pull-started` 建行锚点（本端拉取批次由引擎
事件即建行）。旧快照通路（`merge_snapshot` / `prune_absent` / `reconcile_diff`）**原样保留**，
继续承担对账与校正——核内三个函数共存即为此设计。

**门禁**：

| 项 | 结果 |
| --- | --- |
| 插件 crate `cargo test` | **47 全绿 / 0 warning** |
| wasm 目标编译 | cargo 报 `Finished release profile`（crate + 共享核在 wasm 目标下编过），**但产物文件未在 `rust/target/` 找到 ⇒ 本项不作为已验证证据**（存疑项，如实记录） |
| 产物重建 + 同步 resources | **未做——两道阻塞，均非本票引入**（见下） |
| 前端 | 未改 ⇒ 不跑 `test:run` |

**阻塞（如实上报，属他人会话在途 / 环境）**：

1. **`manifest-gen` 权限映射表与真源未跟演**：桌面插件构建（`node scripts/plugin-build.js
   --plugin <id>`）抛 `权限映射表含词汇表外权限 'database:main'`。**未改动的 `com.bedcode.ai-chatbox`
   同样失败** ⇒ 与本票无关；来源是桌面 SDK `permission.rs` / `permission-vocabulary.json` 的在途
   改动（工作区已显示为 M），按「非本任务改动一律不碰」不代为修。
2. **本机未安装 wasip3 pinned nightly**（`docs/knowledge/wasip3-toolchain.md`：桌面插件固定
   `nightly-2026-09-16`，经 `scripts/wasip3-toolchain.sh install` 安装）。故 `pnpm run build:rust`
   在本机不可行。

**残余风险**：桌面 wasm 产物（含契约 import 面）与真实桌面宿主加载未复验；移动端同项已在 T4
用产物 import 集合核对过。留 T6。

### 本票两条教训（都已回退，但必须留档）

1. **`cargo fmt` 在插件 crate 里是「整 crate 格式化」= 事故**：双端插件源文件**既非
   rustfmt-clean、行尾还逐文件混用**（同为 `src/` 下：`peer.rs`/`lib.rs`/`transfer_store.rs`
   是 CRLF，`roots_registry.rs`/`settings_store.rs` 是 LF）。`cargo fmt` 一次性把它们全部
   规范化 ⇒ `peer.rs` 出现 +116 行 / 2880 行的纯格式 diff，把真实改动彻底淹没。
   **已全部回退**：`peer.rs`（两端）与移动端 settings 用例文件恢复为 HEAD；`lib.rs`（两端）与
   桌面 `peer.rs` 恢复 HEAD 后**逐处重放**预期改动（脚本按 HEAD 逐文件探测 CRLF 并断言
   替换命中数 = 1）；重写文件按各自 HEAD 行尾归一。
   **纪律**：本项目插件 crate **禁止 `cargo fmt` 整 crate**；要格式化只允许对**本次新建/重写**
   的文件单独跑 `rustfmt <file>`。
2. **`rustfmt <单文件>` 不能用来判定「HEAD 是否 fmt-clean」**：`peer.rs` 含
   `mod tests { mod redial_and_history; }`，rustfmt 在 `/tmp` 下解析不到子模块会**静默跳过
   格式化**（stderr 被吞时看不出）⇒ 会得出「HEAD 已 fmt-clean」的错误结论。判定行尾/格式
   状态要用 `git show HEAD:<file> | grep -c $'\r'` 这类**直接测量**。

### 并行会话在途基线（T3 收尾时新出现，非本票）

- **移动 SDK 正在加 `host-database` 域**：`bedcode-mobile/packages/plugin-sdk-mobile/rust/src/{wasm_host.rs,
  host/mod.rs,host/database.rs,abi.rs,permission.rs}` 与 `rust/wit/bedcode.wit` 均为 M，
  `src/host/notify.rs` 未跟踪，且当时 `cargo test` 报
  `unresolved import crate::wasm::bedcode::plugin::host_database`（中间态编译红）。
  ⇒ 移动插件 crate 的**复跑门禁受此阻塞**；T4 收尾时（该改动落地前）移动端 29 绿为有效证据。

### 票 T5（2026-10-09，已落地）—— 防漂移锁 / code-map / CHANGELOG

**新增接线防漂移锁** `src/wiring_lock.rs`（5 例）：

| 判据 | 内容 |
| --- | --- |
| 零回流 | 核内 23 个纯逻辑函数名，双端 `rust/src/**` 不得再有同名 `fn` 定义（两份实现的漂移从「顺手补一个判据」开始，且两边单测都会绿） |
| 台账纯转出 | 双端 `transfer_store.rs` 必须逐字含 `pub(crate) use bedcode_file_transfer_core::transfer::*;` |
| 适配器在场 | 双端 `adapters.rs` 必须实现 5 个已消费端口 + 用 `PortError`（错误类型未转换即形态不对） |
| 端口面登记 | 核 `ports.rs` 的 `pub trait` 集合与钉死清单**精确相等** ⇒ 新增/改名/删除差异面必须回锁改清单 |

**锁落点修正（重要）**：两条锁原在 crate 根 `tests/`，被宿主侧治理锁
`capability_crates_unit_tests_only.rs` 拦下（`FORBIDDEN_CRATE_TEST_DIRS`，治理面按
`packages/bedcode-*` 目录约定**自动推导**，新 crate 自动纳入）。已迁入 `src/` 并由
`#[cfg(test)] mod boundary_lock; / mod wiring_lock;` 引入，crate 根 `tests/` 目录已移除。
边界锁相应加**自排除**（锁文件含被禁 needle 的字面量常量表，不自排除会扫到自己而恒红），
并以「真生产文件不得被自排除误伤」的正面断言防自排除扩大化。

**门禁**：

| 项 | 结果 |
| --- | --- |
| 核 crate `cargo test` | **72 全绿**（单 lib 测试目标：领域/身份/设置/注册表/会话/台账 62 + 边界锁 5 + 接线锁 5） |
| 变异自检 | **5/5**：端内复活 `fn reduce_event(` → 接线锁红；台账改非纯转出 → 接线锁红；核新增未登记 `pub trait` → 接线锁红；适配器删 `KvStore for` → 接线锁红；真实生产文件注入 `"tokio"` → 边界锁红。全部还原 + sha256 逐字核对 + 复跑绿。<br>（一次无效变异如实记录：把违禁 needle 注入**锁文件自身**不会红——因为锁自排除；换成注入 `src/ports.rs` 才验证到判据。） |
| 桌面插件 crate `cargo test` | 47 全绿 |
| 宿主语义锁等价预检 | 锁迁入 `src/` 后复跑：新增 crate 的 C-3/C-4/C-6 仍全 PASS（仅 HEAD 既有的两条 C-3 红） |
| 治理锁（单测纯净性）合规 | crate 根无 `tests/`/`benches/`/`examples/`、无 `[[test]]` 段、`[dev-dependencies]` 仅 `tempfile`（非内部 crate）⇒ 判据①②均满足 |
| 文档 | 两端 code-map（wasm-apps 段 + 判据段 + 锁索引 + 快捷导航表）+ CHANGELOG 双语 + ADR 0044 |

### 票 T7（2026-10-09，已裁决 + 已落地）—— 桌面 peer.rs 票 08 三项修正（以移动修正版为准）

**裁决（用户授权「从实际场景出发选择合适者」）**：三项差异在桌面侧均为**实际缺陷场景**（不可重试条目先发后判会铸无主会话、占死槽位；重试在并发满时超发；排队批派发失败静默丢用户意图），故**以移动修正版为准**，桌面 `peer.rs` 对齐核内已统一判据。

**落地改动（桌面 `peer.rs` / `lib.rs`）**：

| 差异面 | 桌面改动 |
| --- | --- |
| 重试判据前置 | `retry_task` 改用 `transfer_store::retry_source`（终态 + 回放凭证先判）；`apply_retry` 返回 false 时显性报错（不留无主会话） |
| 发送闸门 | `send_slot_available`（核内 `send_slot_open`）统一；`retry_task` send 方向先过闸门（满则显性拒绝）；`enqueue` / `dispatch_pending_sends` 同判据 |
| 排队批派发失败落终态行 | `PendingSend` 增 `attempts`；`MAX_SEND_ATTEMPTS = 2` 重排队；用尽后 `insert_failed_send_entry` 落带回放凭证的 failed 行（`send_row` / `next_local_failure_id` / `insert_send_row` 新增） |
| 拉取意图先入队 | `pull_files` / `retry_task` Pull 分支改用 `push_pull_intent` **先于** `peer_pull_files` |
| 事件通路 pull 挂载 | `reduce_and_emit` 增 `is_pull_started && attach_pull_meta`（核内 `take_pull_intent` 单文件匹配；与快照通路 `attach_pending_pull_meta` 共存） |
| 停用清理 | `reset_volatile_intents`（排队发送批 + 待挂载凭证随停用作废）+ `lib.rs` deactivate 接线 |

**门禁**：

| 项 | 结果 |
| --- | --- |
| 桌面插件 crate `cargo test` | **50 全绿**（47 + 3 新增编排级用例） |
| 核 crate `cargo test` | 72 全绿（未改，回归确认） |
| 移动插件 crate `cargo test` | 29 全绿（未改，回归确认） |
| 变异自检 | **3/3**：`send_row` 置 retry_meta=None → 本地失败行不可回放红；`attach_pull_meta` 去 `retry_meta.is_none()` 判据 → 同 rel_path 第二意图被越判据消费红；`reset_volatile_intents` 只清 sends → intents 残留红。还原 + sha256 逐字核对 + 复跑绿 |
| 函数级等价核对 | 桌面 `retry_task` / `attach_pull_meta` / `send_slot_available` / `dispatch_pending_sends` / `send_row` / `next_local_failure_id` / `reset_volatile_intents` 与移动端对应段逐 token 相等（T2 校验器口径） |
| 产物重建 | `cd wasm-apps/file-transfer && pnpm run build` → 1,018,695 B，wasmHash `748b314…` 已注入，已同步 `resources/plugins/desktop/` |
| 接口面 | 新产物 host import 面仍为 9 个 `host-*` 原语（host-api / bus / events / log / mdns / peer / platform / plugin / storage），**无新增**（本次只改编排逻辑，未新增宿主原语调用） |
| 前端 / 移动端 | 零改动 |

**真机复验（T6 清单）**：D3（桌面拉取即出行）与 F 组（重试判据）现在是桌面**已落地**行为，须随 T6 真机清单人工执行确认——见 `t6-manual-regression.md`。

### 待实施

- **T2**：`transfer` 模块迁入（任务台账归约 / retry 判据 / 发送闸门 / 归档封顶 /
  `EntryStore` 端口落点）。
- **T3 / T4**：桌面 / 移动适配器接线（含两端既有 mock 夹具改实现端口 trait 全集）。
- **T5**：防漂移锁（核↔双端接线对照）+ 两端 code-map + CHANGELOG 双语。
- **T6**：双端 wasm 产物重建 + 真机（桌面↔移动）互传回归。
