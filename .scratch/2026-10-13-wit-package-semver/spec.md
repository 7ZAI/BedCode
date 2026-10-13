# WIT 包 semver 化评估（ABI 版本管理整改方向）

> 来源：2026-10-13 用户问询链——「ABI 版本号是不是兼容性管理 → 组件模型下是否最佳实践
> → wasm-app 不算独立分发吗 → 评估换 WIT 包最佳实践的影响/可行性/范围/量」。
> 本规格是**评估与决策文档**（未实施、未裁决），落地时按 §7 拆票。
>
> 修订：2026-10-13 已按代码实测全面核对并修正事实基线（详见 §9 首条）——其中
> **R2 范围（探测点 5 → 约 32 处）与 bump 政策两处为影响量级的实质修正**。
> 修订二：同日用户裁决 **R6 = 不做派生、wit 版本独立从 0 起算、整数 ABI 同票退役**
> （详见 §2.1 与 §9 第二条）。
>
> 前序结论（同日问询，已落 `.codebuddy/memory/2026-10-13.md` 会话三/三续/四）：
> ① 整数 ABI 的硬兼容性执行已由组件模型结构校验承担，整数独有价值 = 「产物新于宿主时
> 给 upgrade BedCode 提示」+ 对第三方 SDK 使用者的逐版台账；② 独立分发机制双端真实
> 存在（桌面 zip / 移动远程下载 + 审批钉扎，ADR 0020）⇒ 整数**保留**，不议存废。

## 0. 事实基线（实测，2026-10-13）

1. **现状协商机制**：WIT `interface abi { version: func() -> u32 }`
   （`packages/bedcode-wasm-core/wit/core.wit:343-345`）；宿主装配注入
   `AbiPolicy { version, form }`（`manager/runtime.rs:137-153`：struct 137-144、
   `UNCONSTRAINED` 145-153；桌面 38 + form=1 / 移动 21 + None；wasm-core 零端 ABI
   知识）；实例化期 `verify_abi` 探测（`manager/runtime/component.rs:416` 函数起始，
   超限拒绝在 `445-452`）——**单向协商**：仅拒 `version > host`
   （"please upgrade BedCode"），低版本产物照常加载；反方向（旧产物缺新 import/export）
   由 wasmtime 结构校验 + `stale_artifact_rebuild_hint` 通用重建指引兜底
   （定义与判据在 `component.rs:399-413`，判据锚 = `has not been defined` /
   `not found in the linker` / `component does not export` 三种缺失形态；
   2026-10 起已删除按版本枚举的判据矩阵）。
   ⚠️ 既有漂移：`runtime.rs:133-134` 的文档注释仍写「桌面 37 / 移动 20」，与双端 SDK
   实际 `ABI_VERSION`（38 / 21）不符——属注释滞后（AGENTS.md §6 注释同步强制），
   本规格不修，D1 落地时随探测点改造同票修正。
2. **bump 政策（实测，与直觉相反）**：现行实为「**任何契约变更都 bump**」，无
   「纯增量免 bump」豁免——桌面 `abi.rs` 逐版记账明证：v29（host-http 新增
   register-endpoint / unregister-endpoint，注释自述「函数级追加」）、v30（host-peer
   追加 active-transfers / collect-outgoing）、v32（host-auth 追加 4 函数）、v38
   （core host-platform 加 platform + 桌面 events 扩并集）全部 bump。逐版语义记账 =
   双端 SDK `abi.rs` 头注释（桌面 38 / 移动 21 两条独立序列，ADR 0018）。
   ⇒ 任何「改为只数破坏性变更」的收敛都是**行为变更 + 双端台账改写**，不是免票项。
   ※ 该政策随 §2.1 的 ABI 退役**一并失效**，退役后由新 bump 政策接替（待裁）。
3. **独立分发三层图景**：在仓 `wasm-apps/*`（第一方，随包内置，可信域直通）/
   第三方安装 zip（桌面 `user_plugins_dir` = `app_data/plugins` + file-install +
   approval 门禁 + 哈希钉扎，ADR 0020:40）/ 移动远程下载
   （`bedcode-mobile/src-tauri/src/plugin/downloader.rs`，**无 ADR 覆盖**，属实现面）
   / SDK（发布物，ADR 0020:104 按 0.x breaking 记账）。签名链为 ADR 0020:111-113
   明列遗留项（宿主内置受信公钥 + 插件包签名替代哈希钉扎 + 人工审批）。
4. **版本化包先例已在仓内运行**：`package bedcode:p3-async-host-probe@0.1.0;`
   （`bedcode-desktop/packages/plugin-p3-async-host-import-test/wit/p3-async-host-probe.wit:1`），
   wit-bindgen `=0.60.0` + wasmtime 48 + wasip3 全链在用；**版本不进 Rust 模块路径**
   （生成 `bedcode::p3_async_host_probe`，见该 crate `src/lib.rs:15`）。
5. **WIT 消费面量化（实测，口径：排除 `target/` 与 `node_modules/`）**：
   - 生产绑定站点 **16**：host 侧 `wasmtime::component::bindgen!` **9**
     （wasm-core `core_binding.rs:40` + 桌面 `bindings.rs:25` + 移动 `bindings.rs:20` +
     移动 `desktop_compat.rs:30` + 能力 crate provider 侧 5：pty / http / ws / peer /
     mdns）；guest 侧 SDK `wit_bindgen::generate!` **7**（桌面 5 worlds、移动 2 worlds）。
   - fixture **5**：`plugin-component-test` ×2 worlds（**移动**侧
     `bedcode-mobile/packages/plugin-component-test/src/lib.rs:14,35`；桌面同名 crate
     不走独立 bindgen）、`plugin-system-test`、p3-async
     （另：wasm-core `runtime/tests/p3_async_host_import.rs:48` 与 `a03_probe.rs` 内联探针）。
   - Rust 侧 `bedcode::plugin::` 引用 **148**（含 `node_modules` 副本则为 192——
     统计须排除，否则重复计数）。
   - `bedcode:plugin/` 形态接口名字符串 **117**，其中带引号完整 `"bedcode:plugin/host-x"`
     **49**（元数据常量）、运行期按名匹配 **约 32**（见事实 6）。
6. **接口名字符串分两类（关键事实，直接决定 D1 量级）**：
   - **元数据（不断裂，仅机械同步）**：`HostModuleDesc.interfaces`
     （host-kit `module.rs:29`）+ 各能力 crate `MODULE_INTERFACES`（pty / peer / http /
     ws / mdns 5 处）。运行期匹配**不走**它——`add_to_linker` 由 wit-bindgen 自动生成
     版本名；`ModuleRegistry::verify_whitelist`（`registry.rs:78-98`）只比对模块
     `desc().name`，不读 `interfaces`。桌面 `bindings.rs:244-248` 覆盖测试按 `/` 取
     尾段（`rsplit('/')`），版本后缀天然免疫。
   - **运行期按名匹配（真断裂点，约 32 处）**：wasm-core `core_binding.rs:310,336`
     （`abi.version` / `abi-form.form`）、`component.rs:478,492,500,517`（4 个可选导出
     探测）、`manager/host/owner.rs:62-68 + 210-214`（12 个导出名，经 `1290` 的
     `get_typed_func` 真调用）、`manager/capability.rs:58-60,73-77,80`（9 个常量，
     进 `STORAGE_EXPORTS` / `PROBE_ONLY_CAPABILITIES` 探测表）、`bedcode-discovery-engine/
     src/routing.rs:39-43`（5 个 mdns 导出名）。另 `capability.rs:778` 有对字面量
     `"bedcode:plugin/host-mdns.` 的 `match_indices` 前缀匹配（测试面，版本化后必断）。
7. **版本锁步面**：end 包 = `core.wit` + 桌面 3 cap-src（cap-http / cap-ws /
   cap-desktop，均在 `plugin-sdk-desktop/rust/wit-src/`）+ 移动 1 cap-src（cap-mobile）
   + 拼装生成的 `bedcode.wit`；另有 5 个能力域自持分片（`pty.wit`/`peer.wit`/
   `mdns.wit`/`http.wit`/`ws.wit`）由能力 crate 做 provider 侧 bindgen + `add_to_linker`
   ——注册名与 guest import 名须逐字一致 ⇒ **10 个源文件 + 9 个生成物文件**
   （每端生成物 = `core.wit` + `cap-*.wit` + `bedcode.wit`：桌面 6 / 移动 3）的 package
   行必须同版本。注意 `cap-pty` **不是**桌面 wit-src，而是 compose 直接引用
   `packages/bedcode-pty-engine/wit/pty.wit`（能力域分片进 end 包）。
8. **门禁面**：`pnpm run check:wit`（compose-wit.mjs sha256 漂移锁）+ husky pre-commit
   + CI `wit-drift-lock.yml`（⚠️ push / pull_request 均 `branches-ignore: [dev]` ⇒
   dev 分支本地提交不触发 CI，仅靠 husky 拦）；`mobile_wit_composition_lock.rs`
   （abi.version 双查 + worlds 存在性）；桌面 `plugin/bindings.rs:93` 测试
   （abi.version==38 + caps↔modules 覆盖双向）；`mobile_wasm_core_purity_lock.rs`
   判据 7（CAP_SLICE_WORDS 五词 needles）；`binding_surface_must_not_return_to_wasm_core`
   （wasm-core lib.rs）。`compose.json` 的 `abi.version` **只被锁读**
   （`bindings.rs:93`、`mobile_wit_composition_lock.rs:40`），`compose-wit.mjs` 完全不读。
9. **桌面产物重建环境限制**：stable 无 `wasm32-wasip3` 预构建产物 ⇒ 依赖随包 wasm 的
   7 个桌面测试目标本环境无法转绿（既有基线，需 nightly 环境补跑）。

## 1. 目标与非目标

**目标**：把「组件模型下的契约版本管理」从全局单一整数，升级为 WIT 包 semver 最佳
实践；明确三种深度的代价与推进顺序，供用户裁决。

**非目标**：

- 不改动任何插件业务语义、协议 wire、事件/命令名（本规格只动契约版本管理层）。
- 不引入包注册表 / 插件市场 / 签名链（ADR 0020 遗留项，另行立项）。
- 不在本规格内退役整数 ABI（D2 才涉及，且需保留为兜底或协商下限）。

## 2. 方案空间（三种深度）

| 深度 | 形态 | 兼容性语义 | 量与风险 |
| --- | --- | --- | --- |
| **D1** | `package bedcode:plugin@0.Y.Z;` 单版本（**独立起算，从 0 开始**）；组件 import/export 名变 `bedcode:plugin@0.Y.Z/host-x`；版本取自 `compose.json` 既有 `package` 字段；**同票退役整数 `ABI_VERSION`**（见 §2.1） | 版本搬上 wire（可诊断性 ↑）+ 消灭第二版本号；**语义上是破坏性改名**：wasmtime linker 按含版本的完整名精确匹配 ⇒ 版本不等即加载失败，「兼容」≡ 版本号完全相等，**不提供并存兼容**（那只有 D2 有） | 机制 ~1 人日 + 探测点改造 2–3 人日 + 全量产物重建；风险集中 R1/R2/R5/R7 |
| **D2** | 按能力域拆包（`bedcode:core@0.x` / `bedcode:pty@0.x` / `bedcode:desktop@0.x` / `bedcode:mobile@0.x`…），compose 生成跨包 `include`，各域独立 semver | **多版本接口共存**（旧版已装插件可在新宿主加载） | 3–6 人周契约级重构；R3 爆炸半径 + POC 前置 |
| **D3** | `wkg wit fetch/build` + 包注册表 + `wac` 组合 | 面向第三方发布/消费流 | 当前零消费者，**不做** |

**D1 的兼容性真相（防止落地时被误读为「安全改名」）**：wasmtime component linker
对 import 名做**含版本的精确匹配**，版本号不是提示而是键。因此 D1 之后：

- 宿主与 guest 版本号相等 ⇒ 照常加载；不等 ⇒ 实例化期被拒（`unknown import` /
  `matching implementation was not found in the linker`）；
- D1 的收益是**显式化 + wire 可诊断 + 外部工具（wasm-tools / wkg）可读版本**，
  不是「多一份兼容余地」；研发起步期不承担旧产物兼容义务（AGENTS.md §0）。

## 2.1 ABI version 退役评估（已裁决：退役，2026-10-13 用户裁决）

**裁决**：wit 包版本**不与 `ABI_VERSION` 派生**，独立从 `0` 起算；`ABI_VERSION` 与
`interface abi` **一次性退役**，不保留兜底、不保留双轨过渡。项目在研发阶段、无用户，
不承担向后兼容义务（AGENTS.md §0），因此「先加版本、再退役」的两阶段**无过渡收益，
合并为一票落地**。

**`ABI_VERSION` 今天的 5 项职能 × 可替代性**：

| 职能 | 代码位置 | 能否由 wit 包版本接管 |
| --- | --- | --- |
| ① 「产物新于宿主」拒绝 + "please upgrade BedCode" | `component.rs:445-452` | **能，且本已冗余**：版本上 wire 后不等即 import 名不匹配，结构校验直接拒，且错误自带包名 + 版本 |
| ② 向后兼容加载（低版本产物在新宿主照常跑） | 同上（仅拒 `>`） | **不能**（精确匹配 ⇒ 双向严格）。但**该能力在现行政策下已是空壳**：现行 bump 政策是「任何契约变更都 bump」，能被②救回的只有「纯加法且不 bump」的产物，而这类产物今天不存在 |
| ③ 形态标识 `abi-form.form = 1` | `component.rs:453-471` + `cap-desktop.wit:533`（**仅桌面导出**） | **不能，且不受影响**：form 与 version 正交（`FORM_COMPONENT` 注释明写「语义与 ABI_VERSION 解耦」），退役 `abi` 后 form 检查原样保留 |
| ④ 逐版语义记账（桌面 38 / 移动 21 两条序列） | 双端 SDK `abi.rs` 头注释 | 可迁移（非运行时职能）：迁 CHANGELOG + ADR + 各端 `cap-*.wit` 头注记（`core.wit:5-16` 已声明「WIT 内不复制历史」，删除 `abi.rs` 后**必须显式指定记账去处**，否则历史丢失） |
| ⑤ 共享产物的协商下界（`SHARED_ABI_VERSION = 21`，锁断言 ≤ 双端） | `bedcode-file-transfer-core/src/wiring_lock.rs:375-381`、`bedcode-desktop/wasm-apps/file-transfer/rust/src/lib.rs:63,507` | **不能**（依赖②）：wit 包版本下一份产物只对应一个版本 ⇒ 机制整体退役，改由「共享产物按哪端 world 构建就要求双端版本对齐」承担 |

**退役面（可枚举，双端同步）**：`core.wit` 删 `interface abi`（343-345，该文件由
`core_binding.rs:40` 直接绑真源）+ 双端 cap world 各删 `export abi`
（桌面 `cap-desktop.wit:618,687`、移动 `cap-mobile.wit:203`）+ `compose.json` 删
`abi.version` + `AbiPolicy` 删 `version` 字段（退化为「只管形态」，更贴合「wasm-core
零端 ABI 知识」）+ 删 `verify_abi` 的 version 比较分支与 `core_binding::abi_version`
探测 + 双端 SDK `wasm_entry!` 宏去掉 `abi.version` 导出 + `SHARED_ABI_VERSION`
常量与其锁 + 全部 e2e / fixture 的「产物 abi.version == ABI_VERSION」断言基线。

**退役净收益**：① 消掉一个四方必测不变量（产物 `abi.version()` == `ABI_VERSION`
== 双端 `compose.json` `abi.version`；断言点见 `bindings.rs:93`、
`mobile_wit_composition_lock.rs:40`、双端 `abi.rs` 自测、`test_support/bare.rs:274`
的 e2e 基准）⇒ 收敛为「compose 版本 == 源文件 package 行」两方；② 删运行期冗余协商
代码；③ 失败诊断由 linker 按包名 + 版本报出，撤掉自造解释层；④ 版本上 wire 供外部
工具读取；⑤ 消除双轨漂移（历史前兆：`runtime.rs:133-134` 注释写 37/20 而实际
38/21）；⑥ 为 D2 多包独立 semver 铺路。

**退役后必须补的纪律（否则版本号会失控）**：
- **bump 政策重定**：现行「任何契约变更都 bump」在退役 `abi` 后没有运行期载体，
  必须落成显式规则——建议「任何 WIT 变更（**含函数级追加**）→ patch bump
  （`0.1.0` → `0.1.1`）；interface 增删 / 签名变 → minor bump（`0.1.x` → `0.2.0`）」，
  写入 ADR。理由：组件模型下任一契约变更都要 guest 重建，「加法免 bump」不存在。
- **台账去处**：双端 `abi.rs` 删除前，先把 v1…v38 / v1…v21 的逐版语义迁 CHANGELOG
  双语条目 + ADR 引用（否则契约演进史断档）。
- **起算值**：建议双端同为 `0.1.0`（首个版本化契约），此后各自演进；共享产物场景
  要求双端版本对齐（R7），由 file-transfer 这类「同产物跨端」需求驱动。

## 3. 可行性证据

- **工具链已验证**：先例 `bedcode:p3-async-host-probe@0.1.0` 在 wit-bindgen
  `=0.60.0` + wasmtime 48 + wasip3 全链运行（事实基线 4）；wasmtime component
  linker 对版本化接口名的匹配是 WASI 生产形态，`add_to_linker` 由 wit-bindgen
  自动生成版本名，host/guest 同源即一致。
- **D1 对 Rust 模块面近零侵入**：版本不进 Rust 模块路径 ⇒ 148 处 `bedcode::plugin::`
  引用不动；guest 侧 SDK 用 `wit_bindgen::generate!` 走**目录**合成 package
  （`wasm.rs:27-29`），目录内 package 行带版本即可，仍不生成版本化模块名。
- **D1 的真实成本面是运行期探测点**：约 32 处按名匹配（事实基线 6）+ 49 处元数据
  常量机械同步，不是「5 个探测点」。

## 4. 硬约束与风险

| # | 风险/约束 | 严重度 | 对策 |
| --- | --- | --- | --- |
| R1 | **版本须跨 10 源 + 9 生成物锁步**：`include` 只认同 package 同版本；5 个能力域自持分片的 provider 侧 `add_to_linker` 注册名与 guest import 名须逐字一致，不能独立于 end 包版本。`compose.json` **已有 `"package": "package bedcode:plugin;"` 字段**，且 `compose-wit.mjs` 的 `computeFiles` 把它整行写入 `bedcode.wit` ⇒ 版本注入点已存在、**脚本零改动**；cap 分片走 `copyFileSync` 逐字拷贝 ⇒ 源文件带版本则拷贝件自动带版本，**不需要「拷贝时重写 package 行」**（那只会制造第二处版本真源）。`core.wit` 被 `core_binding.rs:40` 直接绑真源 ⇒ 源文件必须带版本（不能改绑生成物，违反「绑真源」红线） | 高（D1 即触发） | 版本真源 = 双端 `compose.json` 的 `package` 字段（引用仓根唯一常量）；新增漂移锁：`compose.json` package 版本 == 双端 10 个源文件 package 行（§2.1 退役 `abi.version` 后**不再有第三个对齐物**，不变量由四方收敛为两方） |
| R2 | **顶到「wasm-core 零端 ABI 知识」红线**：运行期按名匹配点约 **32 处**（清单见事实基线 6）全部硬编码无版本接口名——`core_binding.rs:310,336`、`component.rs:478,492,500,517`、`manager/host/owner.rs` 12 处、`manager/capability.rs` 9 处常量、`bedcode-discovery-engine/src/routing.rs:39-43` 5 处；版本化后字符串全部要带 `@x.y.z` | 高（D1 即触发，行为变更） | **禁止「32 处各写后缀匹配」**（那只是把红线风险从「硬编码版本」换成散落字符串手术）。落点取既有对偶结构：`owner.rs:80-84` 注释明写「导出名的词汇真源在能力域自报侧，`const EXPORT_*` 是编译期具名的对偶，值相等由 `capability` 单测钉住」⇒ 改造为**导出名单点派生**（由 world / 接口元数据生成 `ItemName`，版本作为构造参数注入一次），保留现有对偶单测；属行为变更，单独立项 + 变异自检 |
| R3 | **D2 爆炸半径**：148 处 Rust 路径 + 16 生产绑定站点 + 5 fixture 重写；移动 `desktop_compat.rs:30-41` **跨端绑桌面 wit 生成物目录**（world `desktop-compat`），拆包后跨端包引用要保持；wit-bindgen 0.60 对「一个 world 跨包 include」的 guest 生成稳定性未实证（同名 interface `events` 桌面/移动各一份） | 高（D2） | D2 前置 1 周 POC：跨包 include guest 生成 + 同名接口并存 + wasip3 async 全链 + desktop-compat 跨端绑定四选探针；失败则 D2 推迟 |
| R4 | **锁与门禁面**：`mobile_wit_composition_lock`、桌面 `bindings.rs` 双断言、purity 锁判据 7 五词 needles、`binding_surface_must_not_return_to_wasm_core`、`check:wit` sha256（版本注入后生成物内容变，须重拼）+ husky + CI；另 `capability.rs:778` 对 `"bedcode:plugin/host-mdns.` 的 `match_indices` 前缀匹配（版本化后必断，随 R2 同票改） | 中 | 每条 needle/判据跟改；按纪律每把新锁**变异自检**（注入→红→还原→核 sha256）；49 处元数据常量机械替换 |
| R5 | **全量产物重建**：桌面 4 + 移动 3 个 wasm-app 与全部 fixture 组件；桌面本环境无法重建（stable 无 wasm32-wasip3 预构建产物），依赖随包产物的 7 个桌面测试目标继续红 | 中 | 排 nightly 环境补跑；交付说明逐条「跑了/没跑」。**迁移期语义**：版本化后旧产物（unversioned import 名）在新宿主实例化期被拒，且该文案命中 `stale_artifact_rebuild_hint` 的两条判据（`has not been defined` / `not found in the linker`，`component.rs:399-413`）⇒ fail-visible 重建指引生效，**不做双版本并存**（研发起步期无存量第三方产物；用户已装插件按 0.x breaking 预期走审批重装，ADR 0020） |
| R6 | **「第二个版本号」双轨漂移**（D1）：wit 包版本与 `ABI_VERSION` 语义重叠 | 中（**已裁决：退役而非派生**） | 用户裁决 2026-10-13：**不做 1:1 派生**——wit 包版本独立从 `0` 起算，`ABI_VERSION` 与 `interface abi` **一次性退役**（研发起步期无用户，AGENTS.md §0）⇒ 双轨是「过渡形态」而非「并存形态」，D1 与退役**同票落地**（不拆两阶段，见 §2.1）。退役后必测不变量从「产物 `abi.version()` == `ABI_VERSION` == 双端 `compose.json` `abi.version`」**四方**收敛为「compose 版本 == 源文件 package 行」**两方** |
| R7 | **共享产物场景双端 package 版本必须对齐**（D1 约束，2026-10-13 精化）：file-transfer 双端单产物（`.scratch/2026-10-13-file-transfer-single-wasm`）使同一份 wasm 被双端加载，产物 import 名只带一个版本 ⇒ 双端版本不同源即有一端实例化失败；且移动 `desktop_compat.rs:30-41` 跨端读**桌面生成物目录**，桌面版本变更会连带移动 crate 的编译面。**范围界定**：该约束**仅在「同一份产物跨双端加载」时成立**——非共享产物场景双端各自 bump 即可（各自 host↔guest 自洽，不受另一端版本影响），故**不是**「双端必须同版本」的全局约束 | 高（D1 即触发；D2 拆包后按包同此理） | 版本真源单点化：版本值定义为**仓根唯一常量**，双端 `compose.json` 各自引用；新增锁断言 compose 版本 == 源文件 package 行（现有 `bindings.rs:93` 与 `mobile_wit_composition_lock.rs:40` 的 `abi.version` 双查随 §2.1 退役一并替换为版本一致性检查）；共享产物场景另由 `bedcode-file-transfer-core/src/wiring_lock.rs` 断言双端版本对齐（替换现「`SHARED_ABI_VERSION ≤ 双端 ABI_VERSION`」判据，`SHARED_ABI_VERSION` 机制整体退役） |

## 5. 改动范围与量

### D1（包 semver 命名）

1. **版本真源定义（仓根单点）**：把版本值定为仓内唯一常量，双端 `compose.json` 引用
   而非各写一份（R7）——这是唯一「必须新增」的配置面。
2. 双端 `compose.json`：`"package": "package bedcode:plugin;"` → 追加版本
   （**字段已存在**，`compose-wit.mjs` 的 `computeFiles` 整行写入 `bedcode.wit`
   ⇒ **脚本零改动**；不需要新增 `wit.version` 字段）。
3. **10 个** WIT 源文件 package 行带版本（core.wit + 桌面 3 cap-src + 移动 1 cap-src
   + pty/peer/mdns/http/ws 5 自持分片）。cap 分片由 `copyFileSync` 逐字拷贝 ⇒
   源文件带版本即拷贝件自动带版本，**不需要脚本在拷贝时重写 package 行**。
4. **约 32 处运行期按名匹配点改造**（R2：导出名单点派生 + 版本作构造参数注入一次），
   含 `capability.rs:778` 前缀匹配。**这是 D1 的主要成本项**，属行为变更。
5. **同票退役整数 ABI**（§2.1，用户已裁）：`core.wit` 删 `interface abi`（343-345）+
   双端 cap world 各删 `export abi`（桌面 `cap-desktop.wit:618,687`、移动
   `cap-mobile.wit:203`）+ 双端 `compose.json` 删 `abi.version` + `AbiPolicy` 删
   `version` 字段（退化为只管 form）+ 删 `verify_abi` version 分支与
   `core_binding::abi_version` 探测 + 双端 SDK `wasm_entry!` 去掉 `abi.version` 导出 +
   `SHARED_ABI_VERSION` 常量与其锁（`bedcode-file-transfer-core/src/wiring_lock.rs`）
   + 全部 e2e / fixture 的「产物 abi.version == ABI_VERSION」断言基线（含
   `test_support/bare.rs:274`）。**form 检查不动**（走独立 `abi-form`，仅桌面导出）。
   ⚠️ 前置：先把双端 `abi.rs` 的 v1…v38 / v1…v21 逐版语义迁 CHANGELOG 双语 + ADR，
   否则契约演进史断档（`core.wit:5-16` 已声明「WIT 内不复制历史」，记账去处须显式指定）。
6. 锁 ~6 把 needles 跟改 + 新锁（compose package 版本 ↔ 10 源 package 行）+ 变异自检；
   原两处 `abi.version` 双查（`bindings.rs:93`、`mobile_wit_composition_lock.rs:40`）
   改为版本一致性检查。
7. 49 处元数据常量（`MODULE_INTERFACES` / `HostModuleDesc.interfaces` 消费侧）机械替换：
   `bedcode:plugin/...` → `bedcode:plugin@0.Y.Z/...`。
8. 全量重拼（`node scripts/compose-wit.mjs --all`）+ 双端 SDK `cargo test` + 全部插件产物重建。
9. CHANGELOG 双语 + ADR（退役决策 + **新 bump 政策**：任何 WIT 变更含函数级追加 →
   patch bump；interface 增删 / 签名变 → minor bump）+ R7 真源单点化。

**量：机制 ≈1 人日（配置 + 源文件 + 锁）+ 探测点改造 2–3 人日 + ABI 退役 ≈1 人日
+ 1 轮全量重建验证。**
收益：组件 import 名自带版本（wire 可诊断性）、外部工具可直接读版本、**消掉第二个
版本号与四方对齐不变量**、为 D2 铺路。注意：D1 是**破坏性改名**（任何版本 bump 都要求
guest 重建），且退役后**不再有任何运行期版本协商**——版本一致性由 linker 的包名匹配
保证，失败诊断直接来自 wasmtime（命中 `stale_artifact_rebuild_hint` 重建指引）。

### D2（按能力域拆包）

D1 全部 +：WIT 拆 6+ 包；compose 生成跨包 `include`；16 绑定站点 + 148 Rust
路径 + 49 元数据字符串重写；**多包版本共存**（宿主按包分别匹配不同版本接口）；
POC 1 周（R3）；双端 WIT 同步流程（ADR 0019）+ CHANGELOG + ADR。

**量：≈3–6 人周**（含 POC、双端同步、锁重写与变异自检、全量重建）。
收益：域间独立演进（修 peer 不动 core）、**真正的多版本共存**（D1 已退役整数 ABI，
此处不再有「整数兜底」议题）。

### D3

0 消费者，不做。等插件市场/签名链生态出现再议。

## 6. 推进顺序（R6 / R7 已裁决 2026-10-13，其余待裁决）

1. ~~先裁决 R6 / R7~~ —— **已裁决**：R6 = 不派生、**退役整数 ABI**（wit 版本独立从
   `0` 起算，无用户不考虑兼容）；R7 = 版本真源仓根单点化 + 共享产物场景双端对齐
   （非共享场景双端各自演进）。两项落 ADR 即为 D1 前置，现已解除阻塞。
2. **待裁决（新增，退役带来的必答题）**：新 bump 政策（建议：任何 WIT 变更含函数级
   追加 → patch bump；interface 增删 / 签名变 → minor bump）与起算值（建议双端同为
   `0.1.0`）。退役 `abi` 后版本纪律没有运行期载体，这两条必须先定，否则版本号会失控。
3. ~~整数收敛（bump 政策改「只数破坏性变更」）~~ —— **作废**：该提案的前提是
   `ABI_VERSION` 继续存在；既已裁决退役，bump 噪音问题随之消失（改为第 2 条的新政策）。
   事实基线 2 仍有效：现行是「任何契约变更都 bump」。
4. **签名链 / 受信更新通道立项时**（ADR 0020:111-113 遗留）：把 D2 作为该票的契约子任务
   —— 那时「多版本共存」有真实消费场景（存量已安装第三方插件 × 新宿主），且签名链
   天然需要包版本作为签名对象。
5. **D1 若做**：搭下一次破坏性契约变更的自然 bump 点（避免单独为改名兴一场），
   版本化 + R2 探测点改造 + ABI 退役同票落地（用户已裁「不派生、不分两阶段」）。

## 7. 落地时的票据拆分（D1 示例，D2 待裁决后重拆）

| 票 | 内容 | 门禁 | 依赖 |
| --- | --- | --- | --- |
| T1 | 决策与 ADR：**R6 退役决策** + R7 真源单点化 + **新 bump 政策 + 起算值** + 双端偏离条款 + CHANGELOG 双语 + **abi.rs 台账迁移**（v1…v38 / v1…v21 迁 CHANGELOG/ADR） | 用户裁决记录 | — |
| T2 | 契约层：仓根版本常量 + 双端 `compose.json` 的 `package` 行 + 10 源 package 行 + 双端重拼 + 新锁（compose ↔ 源文件） | `pnpm run check:wit` 绿；新锁变异自检 | T1 |
| T3 | 运行期按名匹配点改造（R2）：约 32 处导出名单点派生 + 版本注入 + `capability.rs:778` 前缀匹配 + 单测（版本名 / 旧名双形态） | wasm-core 与能力 crate `cargo test`；变异自检 | T2 |
| T4 | **ABI 退役**（§2.1）：`core.wit` 删 `interface abi` + 双端 cap world 删 `export abi` + compose 删 `abi.version` + `AbiPolicy` 删 `version` + 删 `verify_abi` version 分支与 `abi_version` 探测 + SDK 宏去导出 + `SHARED_ABI_VERSION` 及其锁 + 全部 abi.version 断言基线；**form 检查保持不动** | wasm-core / 双端 SDK / file-transfer core `cargo test`；锁变异自检 | T2/T3 |
| T5 | 锁与元数据：~6 把锁 needles + 49 处常量 + 桌面 `bindings.rs` 断言（原 `abi.version` 双查改版本一致性检查） | 双端 `cargo test`；purity / composition 锁绿 | T2 |
| T6 | 全量重建与回归：7 个 wasm-app + fixtures 重建；双端 `cargo test`、`pnpm run test:run` | 交付说明逐条跑了/没跑（桌面 nightly 项明列） | T4/T5 |

## 8. 纪律与不做项

- **不顺手改语义**：只动版本管理层；发现行为不一致处停下问用户。
- 零 wire / 事件 / 命令名变更；零跨端协议变更。版本搬上 wire 属契约层改名，不豁免
  R2 的「零端 ABI 知识」红线——**禁止内核硬编码版本常量**。
- 不在 32 处各写后缀匹配（R2 对策已定：单点派生）。
- **退役面必须双端同步**（`core.wit` + 双端 cap world + 双端 SDK + compose），
  漏一端即编译红或运行期缺导出；`form` 检查不在退役范围（走独立 `abi-form`）。
- **退役前先迁台账**：双端 `abi.rs` 的逐版语义迁 CHANGELOG / ADR 再删文件。
- 插件 crate 禁 `cargo fmt` 整 crate；只对新写文件单跑 rustfmt。
- 共享单点文件（compose.json / 锁 / 版本常量）写前重读、写后复查（工作区瞬时翻转教训）。
- 命令字眼以 `docs/commands.md` 为准（`pnpm run check:wit`、`node scripts/compose-wit.mjs --all`、`cargo test`、`pnpm run test:run`）。

## 9. 实施记录

- **2026-10-13 · 代码事实审核修订**（本节即修订记录）：按代码实测纠正 8 处事实偏差——
  ① 接口名字符串并非「全为元数据」，运行期按名匹配约 32 处（R2 范围 ×6）；
  ② bump 政策实为「任何变更都 bump」，原「函数级追加不 bump」不成立（E2）；
  ③ `compose.json` 已有 `package` 字段、cap 分片逐字拷贝 ⇒ 脚本零改动、无需重写
  package 行（E3）；④ 生产 WIT 源为 10 个（非 11）、生成物 9 个文件（非 2）；
  ⑤ 补 R7 双端版本相等约束（file-transfer 单产物）；⑥ 行号纠正（`runtime.rs:137`、
  `component.rs:416/445/399`、`bindings.rs:93`、ADR 0020 只覆盖桌面 zip）；⑦ D1 语义改写为
  「破坏性改名」并消除与 R5 的自相矛盾；⑧ 量级重估（1 + 2–3 人日）、建议 1 降级为需裁决。
- **2026-10-13 · R6 裁决落文（用户：不做 1:1 派生，wit 版本从头从 0 起算，不考虑旧
  兼容，因项目在研发阶段无用户）**：新增 §2.1「ABI version 退役评估」——枚举 `ABI_VERSION`
  现有 5 项职能与可替代性（① 超限拒绝可替代且本已冗余、② 向后兼容加载**在现行 bump
  政策下已是空壳**、③ form 与 version 正交不受影响、④ 逐版记账迁 CHANGELOG/ADR、
  ⑤ `SHARED_ABI_VERSION` 协商下界随②一并退役）；裁决落地为**版本化与退役同票**，
  不拆两阶段。连带更新：R1 对策（不变量由四方收敛为两方）、R6（标记已裁）、R7
  **范围精化**（「双端版本相等」仅在共享产物场景成立，非共享场景双端各自演进）、
  §2 方案表（D1 含退役、D2 去掉「整数兜底」议题）、§5 改动项 5–9 与量（+ABI 退役
  ≈1 人日）、§6 推进顺序（R6/R7 阻塞解除；新增待裁：新 bump 政策与起算值；原「整数
  收敛」提案作废）、§7 票据重排为 T1–T6（新增 T4「ABI 退役」）。
- （按票追加）
