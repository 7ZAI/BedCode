# ADR 0052：core 收窄为机制必需能力 + 交集契约归能力域 + 端差异归宿主

- 状态：**已裁决（2026-10-13，用户）**；实施跟踪见
  `.scratch/2026-10-13-domain-owned-contracts/spec.md`（票 01–05）。
  本 ADR 修订 ADR 0045 D3（「核心 world = 交集切片」）的判据；0045 D1/D2/D4
  （单一 crate、同 package 分片拼装、能力域 bindgen 自持）与 ADR 0049 D2
  （绑定归宿主、内核唯一合法绑定 = `core_binding` 绑 `core.wit`）**保留**。
- 日期：2026-10-13
- 相关：ADR 0018（移动契约独立，维持）、ADR 0035（能力域 crate 化）、
  ADR 0045（单一 crate + WIT 分片组合）、ADR 0047（端独有域落宿主）、
  ADR 0049（单形态 + 绑定归位宿主）、ADR 0050（file-transfer 双端单产物）、
  ADR 0051（WIT 包 semver 与 bump 政策）

## 背景

1. **现状判据是「core = 双端交集」**（ADR 0045 D3）：`packages/bedcode-wasm-core/wit/core.wit`
  因此持有 4 个能力域交集面——`host-mdns` / `host-peer` / `host-http`（出站）/
  `host-websocket`（客户端）。实现早已按 ADR 0049 归位（内核只实现 8 个机制域，
  能力域实现在能力域 crate / 端宿主），但**契约面仍是业务词**：内核绑定的
  `world core` 声明着它并不实现的业务能力。
2. **能力域分片当前是 provider 侧副本、端组合不消费**（除 pty）：域文件与 core.wit
  两份文本必须靠注释约定「逐字一致」，而仓内并没有函数集级静态比对锁（现有门禁
  只覆盖 package 行、接口名存在性、生成物 sha）——重复即漂移风险。
3. **同一功能被两端形状差异拆成不同函数**：如系统通知（桌面
   `host-events-desktop.notify(title, body)` vs 移动 `host-notify.notify(title, body,
  options-json)`）、目录选择（`pick-folder` vs `pick-folders`）——按「函数方法」
  而非「功能」划分，交集被切碎在端扩展里。
4. **用户裁决（2026-10-13，原文要点）**：
   - 「core 内不应该提供业务基础能力契约，而是提供一套可组合的机制和整套 core
     机制必须的能力（比如 db、fs 等）；各个能力域才是交集的地方；差异应该在
     各自的宿主侧。」
   - 端独有扩展实现走**变体 B**：`impl Host` 与桌面专属引擎面整体迁桌面宿主。
   - 「双端的交集应该按照功能来划分，而不是具体的函数方法；如果功能一致，应该
     统一成一个函数并视为交集。」

## 决策

### D1 · core.wit 收录判据（三条件全满足）

① 内核自己实现（`manager/runtime/core_binding.rs`）；② 双端同源语义；③ 内核机制必需
或全部插件共同需要。业务能力契约（设备发现 / 配对 / 文件传输 / HTTP / WS / 通知 /
终端流…）一律不落 core。落地后 core = 8 import（storage / plugin-database / log /
config / events / bus / fs / platform）+ 4 export（command / lifecycle / manifest /
events-binary）。

### D2 · 交集契约归能力域 crate，端组合直接消费

域共享契约（双端功能级交集）的**唯一真源** = 该域共享引擎 crate 的
`wit/<domain>.wit`；双端 `compose.json` 的 caps 直接指向它（pty 先例推广）。
能力域 crate 不再持有「core.wit 的私有副本」——契约与实现同属一处。

### D3 · 交集按功能划分；功能一致 → 统一为一个函数

交集判定的单位是**功能**，不是既有函数名/函数集：同一功能两端都提供 → 进域共享
契约；形状不一致 → 统一签名（不保留两端变体）；只有一端提供 → 端 cap。统一签名
属破坏性变更（minor bump，ADR 0051 D4），双端同步（共享产物对齐，ADR 0051 D3）。

### D4 · 端差异归宿主（变体 B）

端独有 interface 的 WIT 声明（端 cap）与实现（`impl Host` + 端专属引擎面）都住端
宿主；能力域 crate 端中立——默认形态零端 WIT / 零端专属引擎，`common-host` feature
只装配共享契约的绑定。`host-http-endpoint` / `host-websocket-server` 的桌面专属
引擎面按此整体外迁。

**补录（2026-10-13，D1 同款判据落到 core 域内部实现）**：core 域的**选择器本体**
也是端差异——`host-platform.pick-files` / `pick-folders` 的桌面对话框实现经
`host_api::ports::PlatformPickerPort` 由宿主装配（桌面
`plugin/platform_picker.rs`，`PluginHost::new` 在实例化 / 激活前注入；移动由
`WasmHostContext::new_with_ports` 用 `EnginePortsPlatformPicker` 适配既有 SAF 引擎
端口），内核只留共享机制（`fs:pick` 门 + `authorize_picked` 结果校验 + JSON 形状）。
原因：`FileDialogBuilder::pick_folders` 是 tauri-plugin-dialog 的 desktop-only API，
实现留内核会让 `aarch64-linux-android` 构建直接失败。

### D5 · 变更形态与失败可见

- 契约搬家（interface 在文件间移动、union world 不变）→ 无 bump、产物免重建
  （须留并集等价证据）；
- 统一签名（interface 增删 / 签名变）→ minor bump、双端同版本、产物全量重建、
  旧产物实例化期被 linker 点名（复用 `stale_artifact_rebuild_hint` fail-visible）。

### D6 · 与既有 ADR 的关系

- ADR 0018（移动契约独立）**维持**：两端的域 caps 仍各自组合、各自实现；
- ADR 0045 **D3 被本 ADR 取代**（core = 交集 → core = 机制必需），D1/D2/D4 保留；
- ADR 0049 **D2 保留**（内核唯一合法绑定 = `core_binding` 绑 `core.wit`），其中
  「17 个双端全等 interface」计数口径随本 ADR 更新（收窄后 12 个）；
- ADR 0051 的 bump 政策 / 版本真源 / 双端对齐条款适用于本 ADR 的 D5。

## 代价与风险

| 项 | 说明 | 处置 |
| --- | --- | --- |
| 端 caps 数量增加 | 桌面 caps 由 4 组增至 7–8 组，compose/锁断言要跟 | 票 02/05 一次性收口 |
| world 重名 | 域文件与端扩展文件同 package 时 `cap-http` / `cap-ws` 会重复定义 | 端扩展 world 改名（`cap-http-endpoint` / `cap-ws-server`） |
| 内核绑定面缩小 | `core_binding` 生成物少了 4 个 trait 模块（预期收益） | 票 02 注释/锁同步 |
| 统一签名的破坏面 | 两端 SDK、宿主、全部产物需重建 | 票 04：minor bump + 双端同步 + fail-visible 三形态 |
| 个别功能是否「一致」有判断空间 | 如移动 SAF 列目录 / 桌面终端流插件面 | 票 01 逐案盘点，交用户裁决后冻结 |

## Out of scope

- 为了「统一」而给任一端**新增**能力（超出两端现状的功能不在本改造内）；
- 导出面（`events` 等）的改造——共享产物场景的并集导出由 ADR 0050 承载，本 ADR
  只记录其豁免理由；
- 插件 ABI 的兼容旧版本路径（项目研发期无兼容义务，AGENTS §0）。
