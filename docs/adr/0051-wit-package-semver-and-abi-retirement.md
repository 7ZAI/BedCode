# WIT 包 semver 化与整数 ABI 退役

## 状态

**已裁决并落文（2026-10-13）**。R6 / R7 为用户裁决；bump 政策与起算值在票 01 实施时
确认（见 D4）。本 ADR 记录**决策与纪律**；实施进度以规格文档的「实施记录」节为准——
规格与逐票记录：`.scratch/2026-10-13-wit-package-semver/spec.md` 与
`.scratch/2026-10-13-wit-package-semver/issues/01…06`。

## 背景

现行契约版本管理是**全局单一整数**：WIT `interface abi { version: func() -> u32 }`
（`packages/bedcode-wasm-core/wit/core.wit`）由插件导出、宿主 `verify_abi` 单向校验
（仅拒「产物 > 宿主」，低版本照常加载）；版本上限经 `AbiPolicy` 由各端宿主装配时注入
（wasm-core 零端 ABI 知识，ADR 0049）。双端各一条独立序列：桌面 v1…v38 / 移动
v1…v21（ADR 0018）。

三个实测事实决定了本决策：

1. **硬兼容性执行已由组件模型结构校验承担**——产物缺 import / 缺 export 在实例化期
   即被 wasmtime 拒绝（诊断经 `stale_artifact_rebuild_hint` 转成重建指引）。整数独有价值
   只剩「给 upgrade BedCode 提示」与逐版台账。
2. **整数 bump 政策实为「任何契约变更都 bump」**，无「纯增量免 bump」豁免（桌面 v29 /
   v30 / v32 / v38 均为函数级追加仍 bump）⇒ 「兼容低版本产物」这条职能在现行政策下
   **已是空壳**：能被它救回的只有「纯加法且不 bump」的产物，而这类产物不存在。
3. **版本化包在仓内已有运行先例**：`package bedcode:p3-async-host-probe@0.1.0;`
   （wit-bindgen 0.60 + wasmtime 48 + wasip3 全链在用），且**版本不进 Rust 模块路径**
   ⇒ D1 对 148 处 `bedcode::plugin::` 引用近零侵入。

代价：版本号不在 wire 上（外部工具 `wasm-tools` / `wkg` 读不到）、诊断文案要自造解释
层、四方对齐不变量（产物 `abi.version()` == `ABI_VERSION` == 双端 `compose.json`
`abi.version`）四方必测且已漂移过一次（`runtime.rs` 文档注释写「桌面 37 / 移动 20」
而实际 38 / 21）。

## 决策

### D1 · 契约版本搬上 wire（`package bedcode:plugin@0.Y.Z;`）

组件 import / export 名变为**含版本的完整名**。**这是破坏性改名，不是「多一份兼容余地」**：
wasmtime component linker 对 import 名做含版本的精确匹配，版本号是键而非提示——宿主与
guest 版本相等即照常加载，不等即实例化期被拒（`unknown import` /
`matching implementation was not found in the linker`）。收益是**显式化 + wire 可诊断 +
外部工具可读版本**，不是并存兼容（并存兼容只属于按能力域拆包的 D2，见规格 §2）。

### D2 · 整数 ABI 一次性退役（R6，用户裁决）

> 用户裁决原文（2026-10-13）：**不做 1:1 派生，wit 版本从头从 0 起算，不考虑旧兼容，
> 因项目在研发阶段无用户。**

wit 包版本**独立从 `0` 起算**（不与 `ABI_VERSION` 派生）；`ABI_VERSION` 与 `interface
abi` **一次性退役**，不分两阶段——项目在研发起步期、无用户、无对外 ABI 接口，不承担
向后兼容义务（AGENTS.md §0），「先加版本、再退役」无过渡收益。

`ABI_VERSION` 现有 5 项职能的归属：① 「产物新于宿主」拒绝 + upgrade 提示 → **由
linker 的包名匹配接管（本已冗余）**；② 向后兼容加载 → **不接管，且在现行 bump 政策下
已是空壳**；③ 形态标识 `abi-form.form = 1` → **与 version 正交，原样保留**（form 检查
不在退役范围）；④ 逐版语义记账 → **迁 CHANGELOG 双语台账（D5）**；⑤ 共享产物协商下界
`SHARED_ABI_VERSION` → **整体退役**，改由 D3 的版本对齐承担。

退役净收益：消掉一个四方必测不变量（收敛为「compose 版本 == 源文件 package 行」两方，
票 02 新锁承担）、删运行期冗余协商代码、失败诊断由 linker 按包名 + 版本直接报出、版本
上 wire、为 D2 多包 semver 铺路。

### D3 · 版本真源单点化（R7）

版本值定义为**仓根唯一常量**，双端 `compose.json` 的 `package` 字段各自引用它（禁各写
一份——那只是把第二真源换了个位置）。`compose-wit.mjs` 的 `computeFiles` 把该字段整行
写入生成的 `bedcode.wit`，cap 分片走逐字拷贝 ⇒ **脚本零改动**，源文件带版本即拷贝件
自动带版本（不做「拷贝时重写 package 行」，那会制造第二处版本真源）。

**双端版本对齐的范围界定**：该约束**仅在「同一份产物跨双端加载」时成立**——file-transfer
双端单产物（ADR 0050）使同一份 wasm 被双端加载，产物 import 名只带一个版本，双端版本
不同源即有一端实例化失败。非共享产物场景**双端各自 bump 即可**（各自 host↔guest 自洽，
不受另一端版本影响）⇒ **不是**「双端必须同版本」的全局约束。共享产物场景由
`bedcode-file-transfer-core/src/wiring_lock.rs` 断言双端版本对齐（替换原
`SHARED_ABI_VERSION ≤ 双端 ABI_VERSION` 判据，`SHARED_ABI_VERSION` 机制整体退役）。

移动 `plugin/desktop_compat.rs` 跨端读**桌面生成物目录**（world `desktop-compat`）⇒ 桌面
版本变更会连带移动 crate 的编译面，这正是「共享产物要求双端对齐」的第二个成因。

### D4 · 新 bump 政策与起算值（票 01 实施时确认）

D2 退役后版本纪律**没有运行期载体**（无 `abi.version` 可比），因此必须落成显式规则，
否则版本号会失控：

| 变更类型 | bump | 例 |
| --- | --- | --- |
| 任何 WIT 变更，**含函数级追加**（既有 interface 加函数、新增 interface、载荷字段追加） | **patch**（`0.1.0` → `0.1.1`） | `host-platform` 加 `platform` |
| interface 增删 / 签名变 / 导出面删除 | **minor**（`0.1.x` → `0.2.0`） | `host-session` 整面退役 |

**起算值：双端同为 `0.1.0`**（首个版本化契约），此后各自演进（双端版本相等的范围见 D3）。

规则理由：组件模型下**任一契约变更都要求 guest 重建**（版本不等即加载失败），「加法免
bump」在语义上不存在——把它写成 patch/minor 的区分，是让**破坏面大小**在版本号上可见，
而非换取兼容余地。**禁止**为了少 bump 而把破坏性变更记作追加。

### D5 · 记账去处：CHANGELOG 双语台账

`core.wit` 头注已声明「WIT 内不复制历史」，因此双端 `abi.rs`（桌面 v1…v38 / 移动
v1…v21）的逐版语义台账在文件删除后必须有显式去处，否则契约演进史断档：

- **去处 = `CHANGELOG.md` + `CHANGELOG_zh.md` 的「契约演进台账（整数 ABI）」节**（双语
  同表，见各文件 [未发布] / [Unreleased] 顶部）。
- 本 ADR 是该台账的**索引与解释**（为什么会有两条序列、为什么退役）。
- **先迁后删**：台账迁文完成才允许删除 `abi.rs` 文件（删除动作随契约面同步执行，随票 04
  退役面一并完成）。

### D6 · 双端偏离条款

**ADR 0018（移动插件契约独立演进）维持不撤销**：移动 WIT / ABI / SDK 不跟演桌面部分
破坏性变更（桌面 v17 / v18 / v20 / v25 / v26 / v32–v37 均为 desktop 独有面，移动序列
对应位不 bump——台账里逐条标注了「desktop 独有」）。

版本化后偏离的形态变化：版本号从「各端各自一条整数序列」变为**双端各自的 wit 包版本**，
**偏离条款本身不变**（同 D3：共享产物场景要求对齐，其余各自演进）。bump 政策
（D4）双端同一份规则文本，无偏离空间。

双端**同步**的部分只有共享契约面：`core.wit`（真源）与 file-transfer 这类共享产物的
依赖面，走 ADR 0019 双端同步流程。

## 影响面

- **契约**：10 个 WIT 源文件 package 行带版本（core.wit + 桌面 3 cap-src + 移动 1 cap-src
  + 5 能力域自持分片）+ 9 个生成物文件（桌面 6 / 移动 3）。
- **退役**：`core.wit` 的 `interface abi` + 双端 cap world 的 `export abi`（桌面 2 处 /
  移动 1 处）+ 双端 `compose.json` 的 `abi.version` + `AbiPolicy.version` 字段 +
  `verify_abi` 的 version 分支 + `core_binding::abi_version` 探测 + 双端 SDK
  `wasm_entry!` 的 `abi.version` 导出 + `SHARED_ABI_VERSION` 及其锁 + 全部
  「产物 `abi.version == ABI_VERSION`」断言基线 + 双端 `abi.rs`（台账已迁，见 D5）。
  **`abi-form.form` 检查不在退役范围。**
- **运行期**：约 32 处按名匹配点（`core_binding.rs` / `component.rs` 可选导出探测 /
  `owner.rs` 12 处 / `capability.rs` 9 常量 / `discovery-engine/routing.rs` 5 处）
  必须**单点派生**导出名（版本作构造参数注入一次）——**禁止**在 32 处各写后缀匹配，
  那是把红线风险从「硬编码版本」换成散落字符串手术，且触碰「wasm-core 零端 ABI 知识」
  红线。
- **锁**：约 6 把 needles 跟改 + 1 把新锁（compose package 版本 == 10 源 package 行）+
  file-transfer 共享产物双端对齐断言；每把按纪律做变异自检。
- **元数据**：49 处带引号的 `bedcode:plugin/...` 常量（`MODULE_INTERFACES` /
  `HostModuleDesc.interfaces` 消费侧）机械同步。

## 与既有 ADR 的关系

- **ADR 0018**（双端契约独立演进）：**维持不撤销**，D6 是其在版本化后的形态。
- **ADR 0019**（wasmtime 双端锁步）：共享契约面（core.wit）改动走双端同步流程。
- **ADR 0020**（插件身份校验与审批）：整数 ABI 退役后，用户已装插件按 0.x breaking 预期
  走审批重装；签名链与受信更新通道立项时把「多版本共存」作为契约子任务。
- **ADR 0049**（wasm-core 单形态 + 绑定归位）：`AbiPolicy` 删 `version` 后退化为「只管
  形态」，更贴合「wasm-core 零端 ABI 知识」。
- **ADR 0050**（file-transfer 双端单产物）：其 D5「协商 ABI = 产物自报双端下界」被本 ADR
  D2 取代为「按哪端 world 构建就要求双端版本对齐」（D3 末段）。

## 验证（决策层）

- R6 / R7 用户裁决记录在案（D2 / D3）。
- bump 政策与起算值在票 01 实施时确认（D4），后续各票无「待裁决」遗留项。
- 台账迁文落位：`CHANGELOG.md` / `CHANGELOG_zh.md` 双语「契约演进台账（整数 ABI）」节，
  桌面 v1…v38 / 移动 v1…v21 逐版语义无断档（D5），删除 `abi.rs` 前核对迁文在场。
- 实施进度与门禁记录：规格 §9「实施记录」逐票追加。

## 实施落点（票 02，2026-10-13）

D3 的「单点化」在 JSON 清单上无法零脚本实现（JSON 无引用机制），经用户裁决取**仓根常量 +
脚本注入**：

- **版本真源**：仓根 `contracts/bedcode-plugin-package.rs`（Rust 常量：`PACKAGE_NAME`
  = `bedcode:plugin` / `PACKAGE_VERSION` = `0.1.0` / `PACKAGE` = 全名）——全仓唯一
  手写处。**票 03 实施期由 JSON 改为 Rust 常量**：能力域 crate 的导出名清单走
  `inventory` 静态自报（`static ROUTE` + `submit_routable_capability!` 要求编译期
  常量），JSON 只能运行期解析 ⇒ 结构性冲突。改后 const 上下文直接可用，`scripts/`
  的 JSON 派生视图不再需要（直接读常量），少一处生成物与一把锁。
- **端清单语义变更**：双端 `compose.json` 的 `package` 从整行声明（`package
  bedcode:plugin;`）退化为**只声明包名**（`bedcode:plugin`），版本由
  `compose-wit.mjs` 注入生成的 `bedcode.wit`；脚本另校验清单包名 == 真源 `name`，
  漂移即抛。
- **脚本改动范围**：`computeFiles` 的 package 行拼装 + 一个 `readPackage()` 读取校验
  函数 + 清单包名校验；cap / core 分片仍是逐字拷贝（源文件带版本即随之进生成物）。
- **门禁**：`bedcode-desktop/src-tauri/tests/wit_package_version_lock.rs` 四判据（真源
  形状 · 端清单只声明包名 · 10 源 package 行 · 9 生成物 package 行），变异自检 3/3
  （源文件漂移 / 清单回写版本 / 生成物漏重拼各红对应判据，还原后 sha256 一致）。
- **编译面**：wasm-core `cargo check`、双端 SDK `cargo check --features wasm`、桌面宿主
  `cargo check --lib`、移动宿主 `cargo check` 全绿——**版本化不改变编译面**（bindgen
  按 WIT 源生成，import 名版本化只影响运行期 linker 匹配）。
- **运行期面按票序留待后续**：约 32 处按名匹配点（票 03）与整数 ABI 退役（票 04）尚未
  落地，故本票之后**端到端实例化路径暂红**（旧产物 import 名无版本），属票 03/04 的
  在途范围，非回归。