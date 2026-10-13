# file-transfer 双端单产物：一份 wasm + 平台分支

## 状态

**已立项，主体已落地（2026-10-13）**。规格与逐票记录：
`.scratch/2026-10-13-file-transfer-single-wasm/spec.md`。本 ADR 记录**决策与判据**；
实施进度以规格文档的「实施记录」节为准。

## 背景

ADR 0044 把双端 file-transfer 的可共享面收敛为共享业务核
（`packages/bedcode-file-transfer-core`），但**产物仍是两份**：双端各有一个 wasm
适配器 crate（桌面链 `bedcode-plugin-api` / 移动链 `bedcode-plugin-api-mobile`），
各自的 `WasmPlugin` 外壳、命令面、事件回调、持久化落点各一份。代价：每次修复两处
同步税；且两端契约面（events 词汇、ABI 版本）已经分叉。

file-transfer 是对等网络传输——**双端是同一产品在两种宿主形态上的两个 app**，
业务逻辑理应只有一份。用户裁决（2026-10-13）：方向 = 单产物；产品差异不消除，
「保留在同一个 trait 的不同实现里，根据 platform 做不同处理」。

实测阻塞（规格 §0）：桌面产物多导入 `host-api-call` / `host-platform-desktop`
（移动宿主不提供 ⇒ 实例化即失败）；双端 `events` 导出词汇不同（宿主要求的导出
函数名不一致）；双端宿主 AbiPolicy 下界不同（桌面 37 / 移动 20）。

## 决策

### D1 · 产物源收敛到桌面端一个 crate，双端加载同一份组件

`bedcode-desktop/wasm-apps/file-transfer/rust/` 是唯一产物源（按桌面
`world plugin` 构建）；移动端 `wasm-apps/file-transfer/rust/` **删除**，只剩前端
与 manifest。移动构建经 `package.json` 的 `bedcode.sharedRustManifest`（构建链
字段，不经宿主 manifest 校验）指向共享 crate，`scripts/plugin-build.js` 先构建
共享 rust 再走前端构建。

否决项：移动端保留一份「转发 crate」——那只是把双份实现换成双份构建，同步税
照收。

### D2 · 产品差异 = 同一端口 trait 的双实现 + 运行期 platform 分支

共享 crate 内 `adapters.rs`（桌面行为）与 `adapters_mobile.rs`（移动行为）并列，
由 `platform.rs` 选择。端值来自 **core.wit 新增的 `host-platform.platform()`**
（无权限门；各端宿主经装配 feature 编译期判定，桌面 `desktop` / 移动 `mobile`）。
激活时探测一次并驻留；**空/未知取值 = 激活显性失败**——猜错平台会把桌面分支
静默跑在移动宿主上，没有任何运行期信号能暴露。

「移动实现」不是第二套 SDK：持久化 KV、`safTreeUri` wire、电源不接线、信任直答
全部是 core interface（双端宿主都提供）上的不同编排，用桌面 SDK 的 trait 即可
完整表达。⇒ 共享 crate **只链桌面 SDK**（锁钉住：出现移动 SDK 依赖即红）。

### D3 · 双端 events 导出取并集，桌面 wit 扩为七函数

共享产物按桌面 world 构建，必须同时导出两端宿主要按名调用的回调：桌面
`on-message` / `on-process-done`，移动 `on-bus-message` / `on-auth-success` /
`on-disconnect` / `on-session-created` / `on-session-stopped`。桌面
`interface events` 扩为七函数并集（签名逐字照 cap-mobile.wit）；wit-bindgen 导出
视图按名懒查找，一端不认识的函数对该端无感 ⇒ 对桌面原生插件是**加法**
（默认空实现，旧宿主按名调用不受影响）。

### D4 · 桌面专属 import 在移动宿主由 fail-visible 桩闭合

组件模型没有「缺省 import」：桌面产物多导入的 `host-api-call` 与
`host-platform-desktop` 必须被移动宿主满足。新增 binding-only
`world desktop-compat`（定义在桌面 wit-src `cap-desktop.wit`，同 `plugin-binary`
先例），移动宿主 bindgen 它并提供**全函数显性 `Err`** 的桩
（`bedcode-mobile/src-tauri/src/plugin/desktop_compat.rs`）。桌面分支在移动端
**不可达**（platform 探测后不选桌面实现）；桩只为 import 闭合，且误调用时错误
文案当场点名——fail-visible，不是「假装成功」。

### D5 · 协商 ABI：产物自报双端下界

`verify_abi` 拒「产物 version > 宿主 policy」。共享产物经桌面 SDK 新宏
`wasm_entry_with_abi!` 显式传 `SHARED_ABI_VERSION`（当前 = 21，= bump 后双端
下界）；桌面 SDK `ABI_VERSION` 高于该下界，默认路径（`wasm_entry!`）不可用于
共享产物。锁断言 `SHARED_ABI_VERSION ≤ 双端 ABI_VERSION`——任一端 bump 后必须
重估（fail-visible）。

> **已退役（ADR 0051 D2，2026-10-13）**：整数 ABI 与协商机制整体退役，`wasm_entry_with_abi!`
> 宏与 `SHARED_ABI_VERSION` 常量均已删除；共享产物的双端约束改由「双端 wit 包版本对齐」
> 承担（锁在 `packages/bedcode-file-transfer-core/src/wiring_lock.rs`）。本节保留为决策
> 当时的记录。

### D6 · 工具链统一为 unknown-unknown + 组件化（file-transfer 例外）

file-transfer 是双端共享产物源，若桌面用 wasm32-wasip3 直出而移动用
unknown-unknown + componentize，就是两条编码路径。统一为后者：stable 工具链可编
（无需 pinned nightly）、移动宿主加载的组件形态与其既有产物同构、桌面宿主
（wasmtime 48）加载标准组件无差异（T3 探针用例覆盖）。组件化工具自移动 SDK
提升为仓库级 `packages/bedcode-componentize`（双端共用）。桌面其余 wasm 应用
**不受影响**（维持 wasm32-wasip3）。

### D7 · 行为统一以「较正确一边」为准，逐条入真机清单

单产物后桌面随共享获得移动端已有的修正：`node-stopped` 中断标注 + 陈旧句柄摘除
（与引擎契约一致）。命令面取桌面形状（pause/resume 返回 `true` 字面量、
`list-remote` 返回 `{entries, notice}`）——双端前端契约实测一致（移动前端
`r === true` 不消费返回值、`data.entries`/`data.notice` 双端同构）。所有行为
统一点列入 T8 真机互传清单，人工执行确认。

## 影响面

- **新增**：桌面 wit-src `world desktop-compat`；移动宿主 `desktop_compat.rs`
  （桩 + HostModule 自报）；共享 crate `platform.rs` / `adapters_mobile.rs`；
  仓库级 `packages/bedcode-componentize`。
- **契约**：core.wit `host-platform.platform`（双端同步）；桌面 events 七函数
  并集；桌面 ABI 37→38、移动 20→21；`wasm_entry_with_abi!` 宏变体。
- **退役**：移动端 `wasm-apps/file-transfer/rust/` 整目录（锁钉住不得重建）。
- **构建**：file-transfer 双端均走 unknown-unknown + 组件化；移动
  `scripts/plugin-build.js` 增共享 rust 路径；SDK CLI 的组件化工具路径改指
  仓库级。
- **锁**：`wiring_lock.rs` 改写为单产物六判据（零回流 / 纯转出 / 双适配器在场 /
  端口登记 / 产物源退役 + 单 SDK / 协商 ABI 受控）；`capability_crates_no_product_ids`
  登记新 crate。

## 与既有 ADR 的关系

- **ADR 0044**：共享业务核阶段完成；本 ADR 是其延续（编排层与产物层也收敛为
  一份）。0044 的「双端各自薄适配器」形态由本 ADR 终结。
- **ADR 0018**：双端插件契约各自演进不撤销——本 ADR 的双端 WIT 同步（platform
  原语、events 并集）走的就是 0019 双端同步流程；ABI 双端各 bump 一次。
- **ADR 0022**：platform 探测原语是「离宿主无法实现、无业务语义」的引擎原语
  （裁剪线内）；共享 crate 是插件侧业务代码，§5.1 B 判据不适用于它。
- **ADR 0049**：本 ADR 的 events 并集与 desktop-compat world 复用其「绑定归位
  宿主 / 契约面上移」手法——端差异词汇由各端消费方按名派发。

## 验证（主体落地部分）

- 共享 crate `cargo test` **78 全绿 / 0 warning**（双平台分支以 seam 注入覆盖）；
  共享核 `cargo test` **75 全绿**（wiring_lock 六判据，变异自检 5/5）。
- 双端 SDK `cargo test --features wasm`：移动全绿；桌面仅 4 个 `wire::key`
  基线失败（HEAD 同红，与本任务正交）。
- wasm-core 双形态 `cargo check` 通过；桌面宿主 `cargo check --lib` 通过；
  `pnpm run check:wit` 双端 9 项无 diff。
- 产物核验：新构建组件字节形态 `00 61 73 6d 0d 00 01 00`；import 面 = core 子集 +
  `host-api-call` + `host-platform-desktop`；`events` 导出七函数并集；
  command/lifecycle/manifest/abi/abi-form/events-binary 齐全。
- **待办**：移动宿主在并行会话在途改动落地后复跑（T3 桩实例化 + 派发探针）；
  真机（桌面↔移动）互传回归清单（T8）。
