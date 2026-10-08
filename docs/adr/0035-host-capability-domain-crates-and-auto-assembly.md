# 插件宿主能力域的 crate 化与自动装配（能力实现出内核 + 机制内核双端锚点）

## 状态

**已实施**（2026-10-04 ~ 2026-10-05，桌面端；**ABI 未变**，`world plugin` 的 22 个 import 一个未动）。
spec：`.scratch/2026-10-04-wasm-core-lib-split/spec.md`（9 票，实施记录见文末；后续票 10 已立项）。
边界单一事实源仍是 **ADR 0022**；本 ADR 不改 B1–B6 判据，只定**机制面怎么装**。

> **部分撤销（2026-10-05）**：本 ADR 的**机制内核 + 自动注册表 + 四个能力域**
> （mdns / websocket / peer / http）全部保留；**票 07/08 的 sqlite 能力域 crate 已由
> [ADR 0036](./0036-sqlite-capability-domain-stays-in-kernel.md) 整体撤销**——
> `host-database` / `host-plugin-database` / `host-storage` 与 SQLite 引擎面都留在
> `wasm_core`（判据：这三 interface 的真源与授权判定本来就在宿主，机制面出内核会让
> 归属出现两个答案）。下文「实施记录」表中 07 / 08 两行、以及「落点与纪律」里的
> `sqlite-engine` 提及均以 ADR 0036 为准。
>
> **自然后继（2026-10-06）**：本 ADR 把**能力实现**出内核；[ADR 0037](./0037-wasm-core-whole-crate.md)
> 把**机制整核本体**（`wasm_core/` 整目录 + 引擎面 db/pty/enums + 宿主胶水）整体抽出为
> 可复用 crate `bedcode-wasm-core`（54,394 行 / 119 文件，`bedcode-desktop/packages/`；
> 2026-10-08 迁根至仓库根 `packages/`），
> 宿主只剩 `pub use` 垫片 + 组合根。边界裁决（ADR 0022）与能力域归属均不变。
>
> **部分撤销（2026-10-07，用户指令「能力域 lib 迁到 packages 下」）**：**D6 的前半句
> 被推翻**——八个能力域 / 传输面 crate（`bedcode-server-{base,core,http,websocket,peer-net}` /
> `bedcode-{crypto,discovery,pty}-engine`）自 `bedcode-desktop/packages/` 迁到**仓库根
> `packages/`**，与机制内核 `bedcode-host-kit` 同族同处。**D6 的两条理由同时消解**：
> 它们依赖的「桌面基础层」`bedcode-server-base` 也一并迁根，于是「根目录 crate 反向拉
> 桌面基础层」这一陷阱的成因不再存在；保留在桌面端的那条跨树依赖只有一条，且方向单一
> （能力域 crate → `bedcode-desktop/packages/plugin-sdk-desktop/rust` 的 WIT 契约），
> 它恰好把「本 crate 绑死桌面 WIT」这一事实写在路径上而非藏在文档里。
> 布局后果：`bedcode-desktop/packages/` 只剩插件契约 / 夹具 crate（`plugin-*`）与整核本体
> `bedcode-wasm-core`；仓库根 `packages/` 成为**全部引擎 / lib crate** 的单一落点。
> **边界裁决（ADR 0022）、ABI / WIT / world 均未动**；spec：`.scratch/2026-10-07-capability-crates-to-root-packages/spec.md`。
>
> **部分撤销（2026-10-08，用户指令「wasm-core 也迁出来」）**：**整核本体 `bedcode-wasm-core`**
> 自 `bedcode-desktop/packages/` 迁至仓库根 `packages/`，与全部能力域 / 传输面 crate 同族同处。
> `bedcode-desktop/packages/` 至此只剩插件契约 / 夹具 crate（`plugin-*`）；仓库根 `packages/`
> 成为**全部引擎 / lib crate（含插件机制整核）**的单一落点。D6 的剩余顾虑（整核绑死桌面
> WIT）改由路径显式写清：本 crate 对 WIT 的依赖经
> `../../bedcode-desktop/packages/plugin-sdk-desktop/rust`，跨树边方向单一且可见。
> **ABI / WIT / world 均未动**；spec：`.scratch/2026-10-08-wasm-core-to-root-packages/spec.md`。

## 背景

`wasm_core` 一度长到 57,592 行，其中 `host_api/` 15,014 行。实测**跨模块耦合并不高**
（出边只有 5 个落点、反向引用只有 8 个文件），真正的病灶是**接线耦合**：

- 22 个 interface 的接线全部硬编码在一个装配函数里（`component.rs` 逐行
  `add_to_linker::<WasmPluginState, HasSelf<..>>`）；
- 「实现搬出内核」与「装配」被绑死，于是 `host_api/{ws,http,peer,mdns,database}.rs`
  的实现只能继续长在 `wasm_core` 里——**能力实现的位置成了历史偶然，不是边界**；
- `packages/` 下已有 6 个 `bedcode-server-*` + crypto 引擎，却没有一个能自己声明
  「我提供这些 interface」。

判据用**客观外部基线**而不是主观分类：WASI p3（wasmtime 48）只提供
`cli / clocks / filesystem / random / sockets`，这就是「POSIX 原生面」；按此二分，
55 条原语 / 7 个 interface（peer / websocket / database / plugin-database / mdns /
storage / http）属非原生，实现进能力 crate；其余 65 条（pty / fs / platform / crypto /
bus / events / api-call / log / task / process / timer / app / config / connection /
auth）留内核。

## 决定

**D1｜一个机制内核 crate（`packages/bedcode-host-kit`）+ 能力实现 crate 化 + 自动注册表。**

机制内核只装三样与产品无关的东西：插件实例状态（`WasmPluginState`）、能力模块契约
（`HostModule` / 描述符 / 提交类型）、自动注册表（收集 → 排序 → 白名单校验 → linker 装配）。

**D2｜机制内核必须独立成 crate，两条硬约束都实测过，不是风格选择。**

- **被链接性**：`inventory::submit!` 展开为 linker-section 静态，未被引用的 rlib
  不进最终二进制、静态不执行 ⇒ 注册丢失。已固化为两个自动化用例
  （`tests/forced_link.rs` 引用 ⇒ 收集到；`tests/forced_link_absent.rs` 不引用 ⇒ 为空）。
- **Cargo 环路**：能力 crate 必须能命名 `collect!` 的提交类型与 `WasmPluginState`
  （`add_to_linker::<S, D>` 的 `S` 是单态的）。这两样若住在 bin crate 内，能力 crate
  就得依赖宿主，而宿主又必须依赖能力 crate ⇒ Cargo 硬拒（实测退出码 101）。

**D3｜自动注册 + 白名单锁，不用「不用 inventory」。**

inventory 的固有风险是能力集随链接到的 crate 漂移——某个 crate 被误删依赖，它提供的
interface 就静默从插件 import 集消失，而 guest 编译期照常 import。治理手段是白名单锁：
收集结果与树内白名单**双向**比对（多出 ⇒ 有能力未经 review；少了 ⇒ 依赖没链上），
且「强制引用行」与白名单常量**同处**，不漂移。能力域缺失在**实例化期**点名报错
（fail-visible 形态①），不给「静默降级为该能力不存在」的余地。

**D4｜描述符只描述机制：接口路径 / 权限位 / ABI 下界三类，禁带任何产品名词。**

模块注册表必须是 ADR 0022 §四类薄壳里的「通用注册表与寻址」；一旦描述符出现产品名词
（会话 / 终端 / 配对 / 设备 / 传输 / AI…），它就退化成业务容器，命中 B1 / B5。该红线由
一条词段级锁守住（**不**用子串匹配——`database:main` 里的 `ai` 会把纯机制权限位判成
产品名词，票 08 实测踩过）。

**D5｜能力域 crate 自带 provider 侧 `bindgen!`（spec D8 的前提已被实测证伪）。**

原设想「沿用 SDK 已 `pub use` 的 guest 侧绑定」不成立：宿主自己的 `bedcode` 模块是
**guest 视角**（import 是调用函数，不是 `Host` trait + `add_to_linker`），能力 crate 要
自己装配就必须生成 provider 侧。两侧生成的是**同名但不同类型**的 trait ⇒ 宿主必须同时
删掉自己的该域 `Host` impl 与 `add_to_linker` 行，否则同一 interface 注册两次。
代价是每域一份 `bindgen!`；收益是绑定层与实现在同一 crate 内、装配自报在同一处。

**D6｜落地位置：机制内核与能力域 crate 一律在仓库根 `packages/`。**
（原决议：机制内核在根 `packages/`，能力域 crate 在 `bedcode-desktop/packages/`；
**前半句已于 2026-10-07 被推翻**，见状态段的「部分撤销」。）

机制内核放根 `packages/` 的理由不变：它是「双端将来的共用对象」，有先例（`peer-net` /
`link-crypto`）。能力域 crate 当初留在 `bedcode-desktop/packages/` 的理由有两条——① 必须
绑死桌面 WIT 且依赖 `bedcode-server-base` 的共享错误类型；② 「一个必然依赖桌面基础层的
crate 放根 `packages/` 是陷阱」。**两条在迁根后都不成立**：`bedcode-server-base` 自己也在
根 `packages/`，陷阱的成因（跨树反向依赖）消失；剩下的唯一跨树依赖是能力域 crate →
`bedcode-desktop/packages/plugin-sdk-desktop/rust`，而那不是「陷阱」而是**契约面的正确
显式化**——它让「此 crate 绑定桌面 WIT 契约」写在 Cargo 清单里，读者不必猜。

**D7｜本期不动移动端，且不推翻 ADR 0018 的「能力面有意不对称」。**

ADR 0018 否决的是「共享超集 world」，本方案连**能力面**都不共享：机制内核是共享的锚点，
能力面按端各自装配。ADR 0019 的显式偏离（双端 wasmtime 版本）不因本 ADR 归零——
共享机制内核前须先把它归零，那是独立阶段。

## 顺带定下的两件事

**运行期能力路由表扩表（`host-storage` → `+ host-mdns`），但路由表与路由方法同表同步。**

`ROUTABLE_CAPABILITIES` 是闭表：`(能力名, 路由方法前缀, 组件须导出的全部函数)`。
方法族跨三层同名——能力域端口 `forward_<prefix>_*`、宿主转发函数 `forward_<prefix>_*`、
提供者窄端口 `<prefix>_*`——并由一条锁**逐项**比对（表 ↔ `CapabilityTarget` ↔ 能力域
端口）。漏改任一层时不会静默：最危险的形态是「能力域端口声明了转发、宿主没有对应
转发函数」，那时注册表认为能力可路由、实际调用永远返回 `None`，能力**无声**退回宿主原语。

**但扩表的边界要说清：转发链路当前不传调用方身份。**

`CapabilityTarget` 的方法只带参数、不带 `caller_plugin_id`（该值在路由层**只**用于
自调用判定）。系统组件代持某能力后，真源按**调用方** `plugin_id` 分区这件事就丢了：
存储落到提供者的分区、句柄属主记成提供者、发现事件投到提供者的 topic。对全部能力域
皆如此 ⇒ 运行期替换目前是「机制面就绪、实际不可用」。

因此本次扩表**只接通机制、不开放入口**：没有给 `world plugin-system` 增加 `export
host-mdns`，任何组件都无法提供那五个函数，路由在构造上不可达。缺陷取证、三个候选方案
与「不得顺手改 WIT」的约束见 `issues/10-capability-forward-caller-identity.md`（已立项）。

## 不做什么

- **不推翻 ADR 0022**：能力 crate 化后，宿主侧四类薄壳判定不变。
- **不触发 ABI bump**：guest import 面一个字节未动（`world plugin` 不变）。
- **不引入 `dylib` 热插拔**：宿主进程内任意代码 = 绕过 WASM 沙箱，权限门退化为自证。
- **不做平台后端 trait**（`FsBackend` 等）：桌面 3,209 行 vs 移动 1,600 行，差异是产品
  形态差异，双端共享阶段的独立立项。

## 实施记录（2026-10-05 结案）

| 票 | 交付 |
| --- | --- |
| 01 | 修三处文档失真（WIT `host-auth` 头注释 / peer-net 发现模块注释 / inventory 判据澄清） |
| 02 | 认领工作区在途改动（另一会话的测试目录化拆分，与本票正交），R6 降级为开工自检项 |
| 03 | 机制内核 crate + mdns 域全链路（tracer bullet），并独立修掉「server 端口层反向依赖插件绑定模块」的方向倒置 |
| 04 | ws 域 15 原语 → `bedcode-server-websocket::plugin_binding` |
| 05 | peer 域 19 原语 → `bedcode-server-peer-net::plugin_binding`，解 20 处反向耦合 |
| 06 | http 域（入站 2 + 出站 1）→ `bedcode-server-http::plugin_binding`（D10：不出独立 egress crate） |
| 07 | ~~建 `bedcode-sqlite-engine`，`db/` 整体搬迁（expand，留转发层）~~ **已由 ADR 0036 撤销（2026-10-05）**：crate 整体删除，`db/` 回到 `src-tauri/src/db/` |
| 08 | ~~db 10 + kv 3 绑定层迁入、删转发层（contract，20 个引用点改指）~~ **已由 ADR 0036 撤销（2026-10-05）**：三 interface 实现回到 `wasm_core/host_api/{database,storage}.rs`，`Host` impl 回到 `component.rs` |
| 09 | 契约收口：跨 crate 强制引用实证（两个测试二进制）、白名单锁双向断言与 fail-visible 报错文案、运行期路由表扩到 `host-mdns` + 闭表锁、crate 边界锁并入机制内核与能力域 crate、本 ADR 与双语文案 |

### 落点与纪律（后续接手者必读）

- **target 落点跟依赖图对齐，不跟架构族谱对齐**：`host-kit` + `discovery-engine` + 整核
  `bedcode-wasm-core` 落仓库根 `target/host-kits`。（`sqlite-engine` 曾落
  `target/server-libs`——与六个 server crate 同桶；该 crate 已由 ADR 0036 撤销，此桶不再有它。）
  当初把「机制内核 + 能力域」当一族放进 `host-kits`，结果 GTK 栈要在这只桶重编一遍、
  把磁盘打到 0 字节（`No space left` + 链接期 `Bus error`）。
  **2026-10-07 迁根后**：八个能力域 crate 的 `.cargo/config.toml` 里相对路径从
  `../../../target/*` 改为 `../../target/*`（crate 深度少了一层，不改就静默落到仓库根的
  **上级**目录）——每处都要求 `cargo metadata` 核验 `target_directory`，与本文「多一个 `..`
  就换一个桶」的教训同源。
- **「白名单锁存在」不等于「强制引用行有效」**：前者只保证表与收集集一致，后者依赖链接期
  属性 ⇒ 必须有跨 crate 实证（票 09 第 2 项），否则「漏一行注释」这类事故只能靠读代码发现。
- **能力域迁移的隐性回归面是测试装配链**：域的 host function 取宿主能力改经实例级端口，
  任何裸造 `WasmHostContext` 的脚手架都会 panic（fail-visible）。票 08 因此首轮全量 29 红。

## 能力域脱绑（2026-10-08，spec `.scratch/2026-10-08-capability-crates-unbind-desktop`）

五个带桌面 WIT 绑定面的能力域 crate（`discovery-engine` / `server-http` / `server-websocket` /
`server-peer-net` / `pty-engine`）与地基 `server-base` 从桌面端解绑——能力域 crate 默认形态 =
**纯引擎机制 + 端口抽象（零 WIT / 零桌面 SDK 依赖）**，任何宿主（移动端 / 无头 / 第三方）可直接引用：

- **feature 门控**：绑定层（`bindgen!` / `HostModule` / `inventory::submit!` / `impl Host`）收进
  `desktop-host` feature（wasmtime / wit-bindgen / inventory / host-kit 转 optional，依赖按各域
  实际面收窄）。spec C2 风险（cfg 下 `bindgen!` / `inventory::submit!` 是否照常展开）实测解除：
  P1 样板（server-http）双态编译 + 双态测试全绿即证，其后各域同法。
- **wire 契约自持**：各域 / server-base 引用的 plugin-api 词汇（权限位 / WS 事件名 / topic 构造 /
  `EndpointAuth` / `BusMessage`）改为**自持副本 + 漂移锁**（锁读 SDK 源文件按文本块 / 行级比对，
  漂移即红）；SDK 原定义**不删除**（另有消费方：wasm-core host_api 总线 / WIT 契约面）。
  server-base 四个 Claude 常量的死 re-export（全仓零消费者）直接删除。
- **双类型桥接**：`server-base::wire::BusMessage` 自持后与 SDK 原型并存；wasm-core 总线内部流转
  保持 SDK 类型（机制层类型选择，本 ADR「不做什么」不变），`WasmHandlerAdapter`（base handler →
  wasm handler 唯一桥接点）做字段级值转换，形状一致由漂移锁钉死。
- **桌面接线**：wasm-core `default = ["desktop-host"]` 级联各域 feature；`component.rs` 五行强制
  引用逐行 `#[cfg(feature = "desktop-host")]`（无 feature 宿主不注册——它没有插件宿主机制）；
  桌面 src-tauri 五能力域依赖行显式 `features = ["desktop-host"]`。
- **长期门禁**：`packages/bedcode-headless-host-probe`（无头测试宿主探针，非拆分产物、不进宿主
  清单、受 `capability_crates_unit_tests_only` 锁自动治理）以非桌面形态默认引用全部能力域——
  `cargo tree` 零 wasmtime / wit-bindgen / inventory / host-kit / plugin-api；六 crate（五域 +
  server-base）源码非注释行 `rg bedcode_plugin_api` 归零。