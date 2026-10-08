# 票 01 · wasm-core 复用形态：机制核抽取边界清单（事实底座）

> 状态：**核验完成（2026-10-07 工作区实测）**。本清单是 D1 三选项共用的
> 事实底座——无论选 A（抽共享核）/ B（fork）/ C（两步走），先要回答
> 「wasm-core 里哪些是 WIT 无关机制、哪些沾桌面绑定」。
> 数据全部取自当前工作区（`dev` 分支 2026-10-07），勿凭记忆。

## 1. 桌面 wasm-core 模块盘点（49,093 行 / 136 文件，ADR 0037 整核后形态）

| 模块 | 行数 | 形态 | WIT/bindgen 依赖 | 归类 |
| --- | --- | --- | --- | --- |
| `manager/` | 23,894 | 目录 | **绑定面集中在 `runtime/component.rs`（`bedcode::plugin` 引用 37 处 = bindgen trait impl）；PluginKind 窄引用 12 处（host.rs / activation.rs / boot.rs + 测试）** | 机制主体（绑定面各端自持） |
| `security/` | 8,965 | 目录 | framework.rs 有 bindgen 引用（具体见 §3） | 机制（fs_auth / approval / validation 镜像移动端同构件） |
| `host_api/` | 9,909 | 目录 | **桌面域**（21 域 impl bedcode::plugin::host_X::Host） | 各端自持（移动端 13 域） |
| `bus.rs` | 1,073 | 单文件 | 无 bindgen 依赖（MessageBus 属物） | ✅ 可抽共享 |
| `config.rs` | 766 | 单文件 | 无（AppConfig 引擎级配置） | ✅ 可抽共享 |
| `monitor.rs` | 442 | 单文件 | 无 | ✅ 可抽共享 |
| `permission.rs` | 387 | 单文件 | 无 | ✅ 可抽共享 |
| `runtime_util.rs` | 223 | 单文件 | 无 | ✅ 可抽共享 |
| `storage.rs` | 235 | 单文件 | 无 | ✅ 可抽共享 |
| `intercall.rs` | 98 | 单文件 | 无 | ✅ 可抽共享 |
| `host_context_registry.rs` | 32 | 单文件 | 无 | ✅ 可抽共享 |
| `db/` | 387 | 目录 | 无（SQLite 真源 schema.sql） | ✅ 可抽共享（移动端若对齐 13 原语则连 db 机制一并复用） |
| `enums.rs` | 102 | 单文件 | 桌面 enums（含 PluginKind 等） | 边界待定（§4） |
| `system/` | 1,414 | 目录 | 引擎面（config/opener/process/constants 垫片） | 桌面独有（pty 无关部分待核） |
| `utils/` | 149 | 目录 | auth_center / session_gateway / test_tokens | 桌面宿主胶水，不抽 |
| `crypto.rs` / `db.rs` / `system.rs` / `manager.rs` / `security.rs` / `enums.rs` / `host_api.rs` / `utils.rs` / `host_harness.rs` / `test_support.rs` | 顶层垫片 | `pub use` 或小模块 | — | 不抽（垫片面） |

## 2. 可抽共享核候选白名单（≈ 3,543 行，全部单文件、零 bindgen）

`bus.rs` + `config.rs` + `monitor.rs` + `permission.rs` + `runtime_util.rs` +
`intercall.rs` + `storage.rs` + `host_context_registry.rs` + `db/`（机制端口面）
≈ 1,073 + 766 + 442 + 387 + 223 + 98 + 235 + 32 + 387 ≈ **3,643 行**
（不含 host_harness / test_support，测试 harness 属各端）。

> 对照 spec §4 D1 选项 A 的抽取面清单（manager 生命周期/loader/registry/storage/
> downloader/approval/validation、permission、bus、security/fs_auth、monitor、config、
> runtime_util、intercall、host_context_registry、db 机制端口）——**实测修正**：
> manager/ 主体（23,894 行）不能整目录白名单化，其绑定面在 component.rs，抽取
> 需以「manager 机制逻辑 + 各端 component.rs 绑定 adapter」两层化（见 §3）。

## 3. 绑定面实测细节（决定「两层化」的难度）

1. `manager/runtime/component.rs`：`bindgen!` 唯一落点；37 处 `bedcode::plugin::`
   引用全部是 host trait impl（`impl bedcode::plugin::host_X::Host for WasmHost`）。
   → 该文件整文件属于「各端绑定层」，机制核引用它只能经 trait 对象或宏参数化。
2. `PluginKind`（ADR 0032 桌面独有分类）仅 12 处引用（host.rs / activation.rs /
   boot.rs + 3 个测试文件）。→ 抽取时用「各端注入的分类枚举」参数化，改动面窄。
3. `security/framework.rs`：命中 bindgen 引用——需逐文件核验是 impl trait 还是类型
   标注（同 component.rs 情形则同为绑定层；若是类型则参数化）。
4. `manager/validation.rs` / `registry.rs` / `loader.rs` / `types.rs`：命中
   `bedcode_plugin_api`（SDK 包名）——需核验是类型导入还是仅注释/文档。

> 结论：**抽取可行性成立，但 manager 层必须做「机制逻辑 ↔ 绑定 adapter」分离**，
> 这是选项 A 的固有成本（spec 已估）；选项 B（fork）绕开此成本但双份漂移。

## 4. 边界待定项（进入 ADR 时给出处置）

| 项 | 现状 | 候选处置 |
| --- | --- | --- |
| `enums.rs`（含 PluginKind） | 桌面 enums | 移动端自持移动 enums（PluginKind 移动版按 ADR 0032 移动端分类自定）；不抽 |
| `db/` 机制端口 | 桌面 db = 真源 + 13 原语机制 | 若 D4 选「统一 13 原语」→ db 机制连 schema 迁移模式一并抽共享；若选「薄库」→ 只抽执行器 |
| `system/config.rs`（AppConfig） | 引擎级配置 | 移动端 config 域自持（移动 config.rs 766 行同构）→ 抽取时按「端口」而非整文件 |
| `crate_boundary_lock.rs` | 登记表 | 抽取后上提共享核（登记「共享核不得回接桌面/移动域」） |

## 5. 移动端 wasm-core crate 骨架规划（目标形态，供票 16 执行）

```
bedcode-mobile/packages/bedcode-wasm-core/
├── Cargo.toml            # 依赖 = 共享机制核（若选 A/C）+ bedcode-plugin-api-mobile + 移动能力
├── src/
│   ├── lib.rs            # facade：pub use 机制核 + 移动域
│   ├── component.rs      # bindgen! 绑移动 WIT（11 import / 8 export）——移动端自持
│   ├── manager/          # 机制逻辑（若选 A：自共享核 re-export；若选 B：fork 自持）
│   ├── host_api/         # 13 域 impl 移动 WIT host trait（现有 host_impl/ 迁移）
│   ├── security/         # fs_auth / approval / validation（移动现有实现对齐）
│   ├── bus.rs            # MessageBus（现有 message_bus.rs 对齐）
│   └── …（对齐桌面模块清单，删桌面域）
└── (tests 随 crate)
```

替换路径：`bedcode-mobile/src-tauri/src/plugin/`（11,640 行）→
`pub use bedcode_wasm_core` 垫片 + 移动绑定层（同 ADR 0037 D3 垫片先例）。

## 6. 门禁证据（本票事实部分）

- 桌面 ABI_VERSION = **34**（`packages/plugin-sdk-desktop/rust/src/abi.rs`，实测；
  spec §1.2 写 31 已过时——桌面在 spec 落档后有过 bump）
- 移动 ABI_VERSION = **11**（`packages/plugin-sdk-mobile/rust/src/abi.rs`，实测）
- 移动 plugin/ = **11,640 行** / 17 个顶层模块 + host_impl 13 域（实测）
- 桌面 wasm-core = 49,093 行口径与 ADR 0037 一致；模块清单见 §1（实测）
- 移动 WIT：host-peer 15 函数（含 resume-all-transfers；缺 5 个桌面已有函数）、
  host-terminal 1 函数、terminal-hooks 2 导出、host-database 2 函数、
  host-storage 3 函数、host-bus 5 函数（**已有 publish-binary/subscribe-binary**）、
  host-events 2 函数、无 host-websocket（实测，与 spec §1.2 一致）
