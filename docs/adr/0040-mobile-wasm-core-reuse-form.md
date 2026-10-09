# 移动端 wasm-core 复用形态：先 fork 对齐、再抽共享核（两步走）

## 状态

**已定案（2026-10-07，阶段 0 票 01）**。本 ADR 是 `.scratch/2026-10-07-mobile-wasm-core-refactor/spec.md`
D1 裁决点的落档：移动端复用 `bedcode-wasm-core` 采用**选项 C 两步走**（先 fork 对齐、
再抽 WIT 无关机制核回共享）。桌面端 ABI / WIT / world **零变动**（本专项只动移动端
+ 根 `packages/` 共享机制的抽取动作，后者不动桌面行为）。实施进度：阶段 0 完成
（本 ADR + 票 02 ABI 规划表）；阶段 1 起按 spec 票序执行。

## 背景

- ADR 0037 把桌面插件机制整核抽为 `packages/bedcode-wasm-core`
  （49,093 行 / 136 文件；2026-10-08 由 `bedcode-desktop/packages/` 迁根至仓库根）。
  其 D1 决策「不放根 `packages/`」的理由 = 必然依赖桌面
  基础层（bedcode-server-base / 桌面 WIT / 桌面能力 crate），移动端永远拉不动
  （ADR 0018 契约独立）。
- **2026-10-07 该理由已部分消解**：`capability-crates-to-root-packages`（同批次）把
  8 个能力域 crate（bedcode-server-base/core/http/ws/peer-net/discovery/pty +
  crypto）上提根 `packages/`，根/桌面 packages 依赖边归零。剩余阻塞 = ① WIT 绑定
  （wasm-core 的 `bindgen!` 绑桌面 `bedcode.wit` + `bedcode-plugin-api` 桌面类型）②
  `host_api` 桌面域（21 域 impl 桌面 WIT host trait）③ PluginKind 等桌面独有枚举
  （ADR 0032）。
- 移动端 `plugin/`（11,640 行）与 wasm-core 机制层同构但独立演进：双端机制双份 =
  每次机制修复/ABI 演进两处同步税。

## 决策

### D1 · 复用形态 = 选项 C 两步走

1. **票 16（第一步，fork 对齐）**：`bedcode-mobile/packages/bedcode-wasm-core` 落地，
   以 wasm-core 为源复制，`bindgen!` 换绑移动 WIT（`plugin-sdk-mobile/rust/wit/bedcode.wit`）、
   删桌面域（pty/auth_center/session_gateway/host_api 桌面域）、host_api 移动 13 域自持。
   兑现「移动端拥有 wasm-core 级机制」，立即消灭双份机制漂移的演进分歧。
2. **票 17/18（第二步，抽共享核）**：把无 WIT 依赖的机制模块（bus/config/monitor/
   permission/runtime_util/intercall/storage/host_context_registry/db 机制端口 ≈ 3,643 行，
   边界清单见 `.scratch/2026-10-07-mobile-wasm-core-refactor/ticket-01-wasm-core-reuse-form/mechanism-core-boundary.md`）
   抽回共享核（并入 `bedcode-host-kit` 或新建 `bedcode-plugin-mechanism`），双端各自保留
   「WIT 绑定 + host_api 域」；fork 面逐步收缩到「WIT 绑定 + host_api」。

**否决项**：
- 选项 A（一步抽共享核）不作首步——绑定解耦是单次大手术（manager 23,894 行须
  「机制逻辑 ↔ 绑定 adapter」分离），风险集中在一次变更；C 每步可验证可回退。
- 选项 B（永久 fork）不作终态——双份漂移税持续。

### D2 · 抽取边界与桌面零回归承诺

- 抽取面 = 纯搬移 + 保 facade，桌面 ABI/WIT/world **一个字节不动**（同 ADR 0037 口径）；
  crate 边界锁先行（`crate_boundary_lock` 登记「共享核不得回接桌面/移动域」）。
- 绑定面（`manager/runtime/component.rs` 37 处 `bedcode::plugin`、PluginKind 12 处窄引用）
  属各端自持，不抽。

### D3 · 移动端 wasm-core crate 骨架

```
bedcode-mobile/packages/bedcode-wasm-core/
├── Cargo.toml    # 依赖 = 共享机制核（票 17 后）+ bedcode-plugin-api-mobile + 移动能力
├── src/
│   ├── lib.rs        # facade：pub use 机制核 + 移动域（同 ADR 0037 D3 垫片先例）
│   ├── component.rs  # bindgen! 绑移动 WIT（11 import / 8 export）——移动端自持
│   ├── manager/      # fork 面（票 17 后 re-export 共享核）
│   ├── host_api/     # 移动 13 域 impl 移动 WIT host trait（现有 host_impl/ 迁移）
│   ├── security/     # fs_auth / approval / validation（移动现有实现对齐）
│   ├── bus.rs        # MessageBus（现有 message_bus.rs 对齐）
│   └── …（对齐桌面模块清单，删桌面域）
```

替换路径：`bedcode-mobile/src-tauri/src/plugin/`（11,640 行）→ `pub use
bedcode_wasm_core` 垫片 + 移动绑定层。

## 对 ADR 0037 D1 的补充（不是推翻）

ADR 0037 D1 的「移动端拉不动」理由，其桌面基础层反对点（依赖边）已于
2026-10-07 消解；剩余阻塞（WIT 绑定 / host_api 桌面域 / PluginKind）由本 ADR 的
「各端自持绑定面」消解。0037 正文不动（历史实施记录），本条为追加 Comment。

## 验证门禁（票 01）

- 本 ADR 落档（阶段 0 完成标记）。
- 桌面 wasm-core 零改动编译通过：本专项任何阶段不触碰
  `packages/bedcode-wasm-core/` 实现文件；票 17 抽取动作为纯搬移，
  抽取前后桌面 crate 编译 + `cargo test` 基线一致。
- 移动端 wasm-core 落地门禁（票 16）：移动端 `cargo test` 全量 + 插件三方案例回归
  （load/activate/deactivate/权限门）。

## Out of scope

- 桌面端任何行为改动（含桌面独有接口、认证中心、PTY/WS 服务端引擎）。
- wasm-core / 机制核发布为 crates.io 包（仓库内 path 依赖形态，ADR 0037 延续）。
- wasmtime / SDK 版本升级（双端已 48，ADR 0019）。

## Comments

- 2026-10-07：本 ADR 落档于 `.scratch/2026-10-07-mobile-wasm-core-refactor/spec.md`
  阶段 0；同批产票 02 ABI 规划表（`.scratch/.../ticket-02-mobile-abi-plan.md`，
  桌面 ABI 实测 34、移动 11）。
- 2026-10-09：票 18 开工，共享核落点裁决 = **新建 `packages/bedcode-host-api-core`**
  （票 18 §4 选项 A；D2 草案「并入 host-kit 或新建 bedcode-plugin-mechanism」收敛为
  此形态——host-kit 保持装配期机制锚点最小面，新 crate 承载 host_api 运行期实现层，
  依赖纪律 serde_json / tracing + 边界锁）。批次 1 = host-storage 域（实现层上移 +
  双端 adapter + `SYSTEM_PLUGIN_ID` 真源随迁 re-export），桌面门禁全绿；移动门禁因
  并行「双端共享 lib M3」在途暂挂（实施记录见票 18 §8）。
- 2026-10-09：批次 3（database）+ 批次 4（log / events）实施完成，**方向修正 = 以
  桌面 wasm-core 的机制为完整基准**（用户指令「以目前 wasm-core 具有的机制为准
  重构移动端」）：共享核承载全套桌面机制（authorizer 引擎层纵深 / 语句超时护栏 /
  `database:main` 权限位分离 / callsite 缓存与 per-plugin 阈值 / JSON 严格解析），
  移动 adapter 接入后自动补齐此前缺失项。config / fs / http 三域**判定不抽**（实现层
  引用各端 SDK 枚举或属各端平台接入，票 18 §10 记录）。门禁：共享核 34+2、桌面
  669+1 基线、移动 fork 285+8、移动宿主全绿（含并行 ADR 0043 + ABI v18 收口后的
  回归）。「同一个 wasm-core」机制面收口于五域 + 不抽裁决。
- 2026-10-09（同日第二批）：**主库收归 wasm-core（双端统一机制决策）**——
  `host-database`（主库 5 原语）自双端 WIT 面移除（桌面 ABI 34→35 / 移动 18→19，
  破坏性），`database:main` 权限位与主库 authorizer 纵深 / 前缀校验随之退役；
  **插件数据库能力 = 插件私有库**（`host-plugin-database`，`storage` 位声明）。
  双端插件生态实测零主库消费者（桌面 agent-hub 用 plugin_db_*，移动 3 插件零
  引用）——零迁移负担。共享核 `database` 收缩为插件库面（权限门 storage /
  超时护栏 / 行字节护栏 / 批次事务）；`with_main_db_guards` / `authorize_*` /
  `validate_sql_table_prefix` 删除。门禁：共享核 31+2、桌面 648+1 基线、移动
  fork 284+8、移动宿主全绿（A1/A3 锁更新至 v19）。
