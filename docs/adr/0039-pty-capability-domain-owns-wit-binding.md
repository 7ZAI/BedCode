# ADR 0039：host-pty 能力域整面迁出 wasm-core（pty-engine 承接 WIT 接线）

- 状态：accepted
- 日期：2026-10-06
- 取代：ADR 0038 的 D1（PTY 只迁引擎体、WIT 绑定面留 wasm-core）与 E3 的部分后果
- 相关：ADR 0035（能力域 crate 化）、ADR 0036（机制与真源同侧）、ADR 0022（宿主/插件边界）、ADR 0037（wasm_core 整核抽出）
- spec：`.scratch/2026-10-06-pty-capability-domain/spec.md`

## 背景

ADR 0038 把 `pty/` 引擎体从 `bedcode-wasm-core` 迁到 `bedcode-pty-engine`，但**保留了
`host_api/{pty,pty_output}.rs`（948 行）在 wasm-core**，理由是「WIT 绑定面与
`component.rs` 的 `impl Host` 同侧」。用户随后终裁（2026-10-06）：

> 不需要绑定。全部迁移所有非 wasm-core 机制的代码。如果 WIT 接口也迁移，通过声明
> trait 静态扫描连接 host api。

即：垫片不要、引擎与 WIT 接线一起走、宿主侧只用「声明 trait + 静态扫描」接线。
用户追问「不是有 `packages/bedcode-host-kit` 包吗？」——那份正是该机制的既有实现，
本 ADR 不新造任何东西，只把 PTY 接到已有的第 5 个位置。

## 既有机制（本 ADR 的复用面，非新发明）

`packages/bedcode-host-kit/`（仓库根，双端共享锚点）已提供：

| 机制 | 落点 | 先例 |
| --- | --- | --- |
| 能力模块自报 + 静态收集 + 白名单双向校验 | `module.rs`（`HostModule` / `HostModuleDesc` / `ModuleEntry` / `submit_module!`）、`registry.rs` | http / ws / peer-net / mdns 四域 |
| 宿主端口下发（实例级 + 进程级兜底） | `ports.rs`（`HostPorts::domain_ports` / `downcast_domain_ports` / `downcast_host`） | 同上 |
| 插件实例状态（`add_to_linker::<S,D>` 的单态 `S`） | `state.rs` | 同上 |

## 决策

- **D1 · pty-engine 升格为能力域 crate**：`bedcode-pty-engine` 承接 host-pty 的
  **WIT 接线**（自带 `bindgen!` provider 侧生成、6 条原语的 `Host` impl、`HostModule`
  自报）+ **全部域机制**（句柄表 / 配额表 / 6 条原语 / 退出事件 / 限频通知）。
  ⇒ **推翻 ADR 0038 D1 的「零 wasm 依赖」**（引擎本体的零业务属性不变）。
- **D2 · 边界 = `PtyPorts` 窄端口 trait**（消费方声明、宿主实现，照 `HttpPorts`）。
  留在宿主的五件事：`check_permission`（安全闸门）/ `publish`（总线投递面）/
  `config`（`AppConfig` 快照）/ `block_on_any`（唯一那份同步↔异步桥）/
  `spawn_task`（ambient runtime 任务派生）。
- **D3 · 三条垫片全删**：`src/pty.rs`、`src/enums/pty_status.rs`、宿主 `lib.rs` 的
  `pub use bedcode_wasm_core::{db, enums, pty}` 中的 `pty`。调用点一律显式路径。
- **D4 · WSL 发行版列举留在 wasm-core**（`system/wsl.rs`，属 `host-platform` 平台事实，
  与 PTY 正交——ADR 0038 E3 判①继续成立；`host_api/platform.rs` 的引用改指真源）。
- **D5 · 测试按「谁的真源」归属**：域行为（含真 PTY 产出、环内部视图
  `watermarks()` / `chunk_count()`）→ pty-engine 单测（夹具 = 迷你宿主，端口逐项
  对应）；**真总线投递语义**留在宿主侧（`wasm-core` 的 `pty_e2e` + `src-tauri` 的
  `pty_session_chain`）；源码文本接线漂移锁留 wasm-core（`host_api/tests/pty_wiring.rs`）。
- **D6 · 反双份锁极性翻转**：`pty_shim_file_contains_no_definitions`（钉「垫片只允许
  re-export」）随垫片退役，换成 `pty_module_must_not_return_to_wasm_core`（反向：内核
  不得再有 PTY 模块 / 文件 / 垫片名）。

## 为什么不推翻「host-pty 是机制」这条判断

判据没有变：6 条原语仍是**机制**（裸 PTY / 字节面 / 句柄属主 / 游标拉取），B1-B6 零命中。
变的只是**它住在哪个 crate**：机制面可以住在能力 crate 而不必住在内核——这正是 ADR
0035 四域已经确立的形态（`bedcode-server-http` 等既实现能力又自带 `bindgen!` 与自报，
内核只剩 adapter）。wasm-core 留下的是 AGENTS §5.1.3 允许的 ②安全闸门 / ③注册寻址
薄壳（权限门 + 端口装配），不是机制本身。

## 代价与已知风险

- **两侧同名不同类型的 `Host` trait**：pty-engine 与 wasm-core 各自 `bindgen!` 生成
  `bedcode::plugin::host_pty::Host`。故**必须同批删除** wasm-core 侧的原
  `impl … Host` 与 `add_to_linker` 行，否则同一 interface 注册两次 → 装配期
  `defined twice`（已随本次迁移一并删除，并由 `HOST_MODULES` 白名单双向校验兜住）。
- **guest 路径多一次端口取用**：`ports_for(state)` 每次原语调用做一次
  `domain_ports` 查表 + 向下转型 + `Arc` 克隆。与 http / ws / mdns 同款，非 PTY 独有。
- **pty-engine 不再是「零 wasm 依赖」**：任何想复用 PTY 引擎面而不接组件模型的程序，
  应只依赖引擎模块（`pty_process` / `pty_ring` / …）——但 Cargo 粒度到 crate，
  故此类程序现在会拖进 `wasmtime`（与四域同款代价，见 ADR 0035）。

## fail-visible 三形态（AGENTS §5.1.4）

1. 旧读路径**删除**：wasm-core 的 `impl host_pty::Host` 与 `host_pty::add_to_linker` 行
   已删（不是「查不到就跳过」）；`crate::pty::*` 路径整体消失，调用点改显式路径。
2. 缺 interface 即红：能力模块不进白名单 → `verify_whitelist` missing 方向立即失败；
   强制引用行 `use bedcode_pty_engine as _;` 缺失同样由该校验抓住（inventory 静态
   不执行 ⇒ 注册丢失）。
3. 反向锁：`pty_module_must_not_return_to_wasm_core` + lib 侧
   `wasm_core_whole_crate_lock` 的 `pty` 反向断言，双侧钉死「内核不得回接 PTY 面」。

## 验收

- `bedcode-pty-engine`：`cargo test --lib` **94 项全绿**（含 39 项迁入的域行为用例 +
  10 项端口/通知装饰器用例 + 45 项既有引擎用例）。
- `bedcode-wasm-core`：`cargo test --lib` 的 pty 相关 **33 项全绿**（含 5 项真组件
  `pty_e2e` + 反向锁 + 接线漂移锁）。
- 宿主锁：`wasm_core_whole_crate_lock` 3/3、`capability_crates_no_product_ids` 6/6、
  `capability_crates_unit_tests_only` 5/5。