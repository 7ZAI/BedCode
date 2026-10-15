# 全仓工程审计缺陷台账

日期：2026-10-15 · 审计基线：`dev` @ `033b3e0e`（工作区另有 68 个未提交文件，审计只读不改）
方法：静态核查 + 逐条 `ls` / `rg` / `sed` 复核；每条证据均已亲自回验，**不采信未经复核的二手结论**（见 §6 已澄清的误报）。
定位：工程成熟度 **4/5**、产品可交付性 **2.5/5** 的结论来源。

台账只登记**可证伪的缺陷**与**待裁决的边界疑点**；做得好但值得保持的部分记在 §7，不算缺陷。

严重度定义：

| 级 | 含义 |
| --- | --- |
| **P0** | 门禁 / 事实源失效，导致架构承诺无法在合并时强制 |
| **P1** | 架构红线破口或叙事与实现裂缝，需用户裁决归属 |
| **P2** | 机制存在但捕获能力不足（形式化锁定、夹具遮蔽） |
| **P3** | 文档漂移——自称最高硬约束的文档带死引用 |
| **P4** | 产品收敛与对外可交付性 |

---

## P0-1 [最重] 82,142 行 / 952 个 Rust 用例不在 CI 门禁上

- **状态**：未修复
- **事实**：`.github/workflows/test.yml:127-130` 的 server-libs 循环只遍历 6 个 crate
  （`bedcode-server-base` / `bedcode-crypto-engine` / `bedcode-server-core` /
  `bedcode-server-http` / `bedcode-server-websocket` / `bedcode-server-peer-net`）。
  以下 crate 的测试**从不在合并时执行**：

  | crate | 行数 | 用例 |
  | --- | --- | --- |
  | `packages/bedcode-wasm-core` | 47,435 | 525 |
  | `packages/peer-net` | 13,220 | 118 |
  | `packages/bedcode-pty-engine` | 6,378 | 99 |
  | `packages/bedcode-file-transfer-core` | 3,906 | 76 |
  | `packages/bedcode-host-api-core` | 2,437 | 40 |
  | `packages/bedcode-ws-client-engine` | 2,918 | 18 |
  | `packages/bedcode-host-kit` | 2,671 | 34 |
  | `packages/bedcode-discovery-engine` | 2,338 | 35 |
  | `packages/link-crypto` | 752 | 5 |
  | `packages/bedcode-headless-host-probe` | 87 | 2 |
  | **合计** | **82,142** | **952** |

  占全仓 Rust 用例（3,269）的 **29.1%**。
- **精确边界（不要夸大）**：这些 crate 是两端 `src-tauri` 的 path 依赖
  （`bedcode-desktop/src-tauri/Cargo.toml` 引用 `bedcode-wasm-core` / `bedcode-pty-engine` /
  `bedcode-host-kit`），所以**编译错误仍被门禁捕获**——丢的是**行为断言**，不是构建。
- **影响**：`bedcode-desktop/docs/code-map.md` 文末的防回接锁索引把「机制内核 fail-visible 三形态 /
  强制引用」的证据指向 `packages/bedcode-host-kit/tests/registry.rs`（482 行）与
  `packages/bedcode-ws-client-engine/src/boundary_lock.rs`（669 行）——**这两处永不执行**。
  架构投入最大的那一半（wasm-core 单形态 + 纯度锁的落点）恰好在门禁盲区。
- **修复建议**：把上述 10 个 crate 并入 `test.yml:128` 的同一个 `for` 循环（各 crate 已有
  `.cargo/config.toml` 把 target 指向 `target/server-libs`，与现有 6 个同构，循环体可原样复用）。
  成本近乎为零，收益是 952 个已有测试从「写了不看」变成真门禁。
- **复验**：`grep -c 'cd "packages/$lib"' .github/workflows/test.yml` 的清单长度 ≥ 16，
  且本地逐 crate `cd packages/<c> && cargo test` 全绿。

## P0-2 AGENTS.md 点名的「长期门禁」crate 零接线、从不运行

- **状态**：未修复
- **事实**：`packages/bedcode-headless-host-probe`（87 行 / 2 个测试）在全仓
  `*.toml` / `*.yml` / `*.mjs` / `*.sh` 中**只出现在它自己的 `Cargo.toml:2`**，无任何消费者、
  不在任何 workflow 或 script 里。
- **影响**：AGENTS.md §5.4.1 与 ADR 0035 把「`bedcode-headless-host-probe` 长期门禁」写成了能力域
  crate 形态的合规依据。一个从不运行的门禁不构成依据——这条承诺目前是空的。
- **修复建议**：二选一——① 接进 CI（与 P0-1 同一循环）；② 若判定不再需要，从 AGENTS.md §5.4.1
  与 ADR 0035 删除该承诺。**不可保留现状**（文档承诺 ≠ 事实，AGENTS.md §0）。

## P1-1 「单形态 = 一份机制」未覆盖生命周期编排层，两份实现靠散文同步

- **状态**：未修复（需用户裁决是否属 ADR 0049 D1 范围）
- **事实**：移动 `bedcode-mobile/src-tauri/src/plugin/manager.rs` **2,149 行**
  （`activate_inner` / `deactivate_inner` 主体在 :707-963 / :963-1230），
  与内核 `packages/bedcode-wasm-core/src/manager/host.rs`（972 行）+
  `manager/host/activation.rs`（1,120 行）**平行存在**。移动侧的同步靠注释散文维持：
  `manager.rs:726`「生效权限集（桌面同款口径，`PluginHost::activate_plugin`）」、
  `:816`「必须可见，否则插件作者只能靠运行时 denied 反推（桌面同款告警）」。
- **影响**：ADR 0049 D1 想消灭的「两份机制」在这一层仍然活着，而唯一的对齐机制是人写的注释——
  注释不会编译失败、不会红测。桌面改口径时移动静默漂移。
- **修复建议**：先由用户裁决——这层编排是否属于「机制」而应收进 wasm-core（则移动 2,149 行
  变薄转调用），或属于「端装配」而应改 ADR 0049 明示不覆盖。**不要默认按 ADR 字面理解强行搬迁**，
  按 AGENTS.md §0 优先级 2 属越线，需确认。

## P1-2 `HostEnginePorts` 单实现者 + 恒失败桩（YAGNI）

- **状态**：未修复
- **事实**：`packages/bedcode-wasm-core/src/host_api/ports.rs`（827 行）定义
  `HostEnginePorts`；全仓 `impl HostEnginePorts` 只有三处——
  `:509` `UnimplementedPorts`（**31 个方法全部 `Err(port_err(...))` 恒失败**）、
  `src/test_support/bare.rs:416` `MockPorts`（测试替身）、
  `bedcode-mobile/src-tauri/src/plugin/host_ports.rs:167` `HostPorts`（**唯一生产实现**）。
  **桌面从不实现**，走恒失败桩。
- **旁证**：同文件 `:69-83` 的 `SafIoPort` 是 Android SAF 概念却住在自称「零端名」的中立端口文件；
  `:200-201` 自陈桌面「不经本端口」——半数 trait 表面实为移动专属词汇。
- **对比（同类抽象中做对的）**：`HostFactsPort`（`ports.rs:145-166`）有**两个生产实现**
  ——`bedcode-desktop/src-tauri/src/plugin/host_facts.rs:22` + 移动 `host_ports.rs:153`，
  加 fail-visible 桩，是真边界。判据可复用：**数生产消费者，1 个就不是边界。**
- **修复建议**：把移动专属方法拆成端差异 port trait（ADR 0052 D4 的正确用法），内核只留双端
  共识面；或删掉桌面恒失败路径改 `#[cfg]` 门控，减少死面。

## P1-3 `bedcode-componentize` 双 crate 同名重复实现

- **状态**：未修复
- **事实**：`packages/bedcode-componentize/Cargo.toml` 与
  `bedcode-desktop/packages/plugin-sdk-desktop/rust/tools/componentize/Cargo.toml` **同名并存**，
  后者约 94 行重复实现。
- **影响**：工程卫生问题；同名 crate 在 `cargo tree` 与错误信息里会互相遮蔽，违反 AGENTS.md §12
  「文档字面 ≠ 事实，先核对再引用」的同类风险。
- **修复建议**：确认哪份是真源，删另一份或改名去重。

## P2-1 「防回接锁」多数是文本扫描，抓不到改名复活

- **状态**：未修复
- **事实**：`bedcode-mobile/src-tauri/tests/retired_mobile_receive_orchestration_lock.rs:23-39`
  的 12 条 needle 中，`"fn publish(app"`、`"tasks: Vec<"` 与接收编排的**架构语义无关**——
  任何 `fn publish(app_handle…)` 都会误红，把逻辑改名成 `ingest_batch` 则全绿。
  该锁锁的是词表，不是架构。
- **能力边界（写下来免得下轮重评）**：这类锁只能可靠抓「同名复活」，**抓不到「改名复活」**——
  用新函数名在 `host_domains/ws.rs` 里重写一遍接收编排，全部锁仍绿。
- **修复建议**：退役面锁改为**结构化断言**（解析函数签名 / AST / 模块导出集），
  或至少为每条 needle 补一句「为什么这条词能代表该架构面」；无法给出理由的 needle 删掉。

## P2-2 purity lock 判据 4 是行数上限，可被 rustfmt 绕过

- **状态**：未修复
- **事实**：`bedcode-mobile/src-tauri/tests/mobile_wasm_core_purity_lock.rs:282-285`
  `SHELL_FILE_LINE_LIMIT = 1300` / `SHELL_TOTAL_LINE_LIMIT = 1800`；判据 4 的注释（`:25`）自陈
  目的是「防止有人把域逻辑改写成壳里的…」。
- **影响**：行数与格式强相关——`rustfmt` 展开或压缩注释即可在不改任何逻辑的前提下过门禁。
  形式上像护栏，实则不构成约束。
- **修复建议**：改为**符号级断言**（壳里不得出现某函数名 / 某 struct / 某 mod），
  行数上限降级为辅助信号或直接删。

## P2-3 端装配缺陷被夹具替身遮蔽（已发生一次真实事故）

- **状态**：部分修复（本次事故已修 + 已补回归锁），**模式未修**
- **事实**：`bedcode-mobile/src-tauri/src/plugin/host_ports.rs` 的 `impl HostEnginePorts for HostPorts`
  曾**整体漏掉 `app_downloads_dir`**，落到 trait 缺省 `None`，真机上 ai-chatbox `activate()` 失败。
  之所以上一轮没测到：`/tmp` 无关——e2e 用的是 `MockPorts::with_app_downloads_dir(...)`，
  **夹具替身自带该键，把端装配缺陷遮住了**（见 `.scratch/2026-10-15-mobile-ai-chatbox-sigill/spec.md:135-137`）。
- **影响**：`MockPorts` / `UnimplementedPorts` 覆盖度一旦高于真实端装配，`cargo test` 全绿反而
  成为**遮蔽器**。这是比「测试不够」更危险的失败模式。
- **修复建议**：为**每个** `HostEnginePorts` / `HostFactsPort` 生产实现加一条「字段完整性」断言
  （实现侧声明它投影了哪些键，测试逐键比对），使替身与真实装配无法静默分叉。

## P2-4 core.wit 收录判据与 ADR 0052 声明的 export 数不一致

- **状态**：待核对（低）
- **事实**：`docs/knowledge/plugin-development-checklist.md` 多处写「core 收窄为
  **8 import + 4 export**」（如 :14），ADR 0052 D1 亦称 8 import + 4 export；
  `packages/bedcode-wasm-core/wit/core.wit:196-212` 的 `world core` 实际为 **8 import + 3 export**
  （导出 `command` / `lifecycle` / `manifest`；`core.wit:196` 自己的注释就写「8 import + 3 必选导出；
  events-binary 是可选出，不在 world 内」）。**真源是对的，错在两份文档**。
- **影响**：低——但 ADR 0052 是被 AGENTS.md §13 点名为「契约归属边界」单一事实源的文档，
  计数偏差会让后来者按 4 export 去核对并判红。
- **修复建议**：以 `core.wit` 为真源订正 ADR 0052 与 checklist 的计数（或补齐 export），二选一并同步。

## P3-1 `plugin-development-checklist.md` 带 2 条死路径（自称与 AGENTS.md 同级）

- **状态**：未修复
- **事实**：该文档 :14 称第一方清单在 `plugin/security/fs_auth.rs::FIRST_PARTY_TRUSTED_DIRS`——
  ① 文件 `bedcode-desktop/src-tauri/src/plugin/security/fs_auth.rs` **不存在**（已迁
  `packages/bedcode-wasm-core/src/security/fs_auth.rs`）；② `FIRST_PARTY_TRUSTED_DIRS` 常量
  **已显式删除**（`wasm-core/src/security/fs_auth.rs:99`：「产品清单不在本 crate」，
  真源为 `bedcode-desktop/src-tauri/src/first_party_dirs.rs:26`）。
  同文档 :29 引 `plugin/manager/capability.rs` 亦不存在（真源
  `packages/bedcode-wasm-core/src/manager/capability.rs`）。
- **影响**：**这是插件开发的第一入口文档**，且 AGENTS.md §7 定其「硬约束与本文件同级」。
  照着它写插件会找不到文件。
- **修复建议**：按 §9 真源改指 `first_party_dirs.rs` 与 wasm-core 的 `manager/capability.rs`。

## P3-2 `build-process.md` 的目录树两端一个都不存在

- **状态**：未修复
- **事实**：`docs/knowledge/build-process.md:19-52` 印的架构树含
  `src/components/{common,desktop,mobile}`、`src/views/{desktop,mobile}`、
  `src-tauri/src/{auth,discovery,pty,session,websocket}`——逐条 `ls` 验证**全部不存在**；
  `:87+` 仍在讲 `src/composables/usePlatform.ts` 的假机机制，该文件已迁 `src/shell/composables/`。
- **影响**：AGENTS.md §3 把 `docs/commands.md` 定为命令唯一事实源，`build-process.md` 是其
  「构建资源治理」章节的引用目标；一棵完全失效的目录树比没有更坏（读者会以为文档描述的是事实）。
- **修复建议**：删掉目录树改为指向两端 code-map（AGENTS.md §12 规定 code-map 才是目录导航真源），
  或按当前结构重写并订正 usePlatform 路径。

## P3-3 `wasm-core/Cargo.toml` 在 20 行内自相矛盾

- **状态**：未修复
- **事实**：`packages/bedcode-wasm-core/Cargo.toml`
  - `:4` 仍称本 crate 含「引擎面（db/**pty**/enums）」
  - `:17-19` 同时称「批次 02 已移出 pty——`bedcode-pty-engine` 依赖与 `portable-pty` 不再出现在本 crate」
  - `:24`（`[package].description`，**进包元数据、会被外部读到**）又写「引擎面（**pty**/db）」
- **影响**：违反 AGENTS.md §6「注释与代码同步维护（强制）」；且 `src/lib.rs:249-315` 还上锁禁止
  `src/pty.rs` 回来——文档、锁、元数据三方不一致。
- **修复建议**：`:4` 与 `:24` 的 `pty` 删掉，与 `:17-19` 对齐。

## P3-4 `auth_center.rs` 头注列 4 个不存在的消费者

- **状态**：未修复
- **事实**：`bedcode-desktop/src-tauri/src/utils/auth/auth_center.rs:19-22` 列出的消费者
  （`commands/system.rs` / `commands/qr.rs` / `controllers/auth_controller.rs`）与 `:40-42` 列出的
  （`events/sync_handler.rs` / `events/forwarder.rs`）**全部不存在**。
- **影响**：AGENTS.md §6 明令「对已删除、改名的符号与已迁移 / 退役的路径，注释里的引用必须同步改为
  当前真源或删去，不留读者无法对照的死引用」。认证中心是安全红线模块（§8），其消费点图失准会误导
  安全审计。
- **修复建议**：逐条订正为当前真实调用方，或删去无法对照的条目。

## P3-5 ADR 0022 已从决策记录腐化为追加式变更日志

- **状态**：未修复
- **事实**：`docs/adr/0022-plugin-host-interface-primitive-boundary.md` 共 991 行 / 118 KB；
  1-565 行是正常的 Context / Decision（28 子节）/ Considered Options / Consequences，
  **565-991 行是 426 行的追加式「修订记录」（v32→v35…）**。
- **影响**：ADR 应当「决策时冻结」。而 AGENTS.md §0 把 0022 指定为宿主/插件边界裁决的**单一事实源**，
  腐化会向下传染到 §4 / §5.1 路由表与插件开发检查清单。修一份漂移文档的成本会随引用面放大。
- **修复建议**：把修订记录外迁到 CHANGELOG 或独立 ABI 台账，0022 只留当前有效决策 + 指向外迁处的指针。

## P3-6 `packages/bedcode-wasm-core/Cargo.toml` 与 `plugin.rs` 对 `terminal_stream_gateway` 归属的说法冲突

- **状态**：待核对（低）
- **事实**：`bedcode-mobile/src-tauri/src/plugin.rs:9` 把 `terminal_stream_gateway` 列为
  wasm-core 机制面；而 `mobile_wasm_core_purity_lock.rs:320-330` 正是在禁止它出现在内核。
- **修复建议**：核对后统一口径。

## P4-1 v3.0.0 只 bump 版本号，无 tag、无用户功能章节

- **状态**：未修复
- **事实**：两端 `package.json:3` 均为 `3.0.0`，但最新 tag 是 **v2.1.1（2026-09-18）**。
  `CHANGELOG.md:11` 只有 `[Unreleased]` 一段。
- **公允说明**：该段**并非纯重构**——`### Desktop`（:58）与 `### Mobile`（:40）段确有用户可见
  修复（插件状态 wire camelCase 词汇、运行期 Pinia store、划屏检测修复、零编译警告等）。
  但**没有新增用户功能**，且整段以架构重构为主。
- **影响**：版本号与可交付物脱节；按 AGENTS.md §2「CHANGELOG 只在发布版本前一次性总结」，
  当前处于「总结已写、发布未发生」的空档。
- **修复建议**：要么补 tag 发布，要么把版本号回退到与实际可交付状态相符。

## P4-2 README 与代码事实不符

- **状态**：未修复
- **事实**（逐条已回验）：
  - `:19` / `:41` / `:214` 三处把 **Auto Task** 列为内置插件，实际对应
    `bedcode-desktop/wasm-apps/terminal-session/rust/src/task/`，**名为 `terminal-session`**，无此插件 id
  - `:19` / `:214` 称「内置 AI Chatbox / Auto Task / File Transfer 三个插件」、
    「双端内置相同插件集」——实际 `agent-hub` 是**桌面独有**，README 通篇未提
    `terminal-session` / 移动端 `bedcode` 应用
  - `:202` 称 agent-hub「正在开发（未发布）」——它已在 `scripts/plugin-package-list.json`
    并打进 `src-tauri/resources/plugins/desktop/`
- **影响**：README 是对外第一门面，也是双端能力的事实介绍，错误直接误导用户与潜在贡献者。
- **修复建议**：按 `scripts/plugin-package-list.json` + 两端 `wasm-apps/` 实况重写插件表与平台表。

## P4-3 移动端两处修复尚未真机复验（主链路在最新 HEAD 上未被端到端验证）

- **状态**：未修复
- **事实**：
  1. **AI Chatbox 激活失败**（`app.downloads_dir` 端装配漏投影）——修复已落地并补回归锁，
     但 §7 自陈是「第二轮真机复验」中**发现**的，修复本身**未再上真机**。
  2. **连接页假认证态死锁**（`isConnected` 被当已认证 → 「正在连接…」永不消失、配对口不可达、
     受保护端点 401）——`.scratch/2026-10-15-mobile-connection-page/notes.md:54` 验证栏写
     「真机：无（设备在 09:52 后从 adb 断开，**未做修复后实机复验**）」。
- **公允说明**：SIGILL 崩溃本身**已复验通过**（`spec.md:125`「真机打开 AI Chatbox 不再 SIGILL」），
  随之暴露的是第二个缺陷。两次都是真机发现、真机未闭环。
- **影响**：「手机远程控制桌面」这条产品主链路，在最新代码上**没有被端到端验证过**。
  这是 P4-1/P4-2 之外最实质的交付风险。
- **修复建议**：解除 `spec.md:109-115` 记录的签名冲突阻塞（release 变体原地升级可保留数据）
  后补一次完整真机走查：配对 → 认证 → 建终端会话 → 传输 → AI Chatbox。
- **阻塞点**：`spec.md:109` 记录设备现装 2.1.1 release 包（release keystore 签名），
  `adb install` debug 包报 `INSTALL_FAILED_UPDATE_INCOMPATIBLE`。需用户决策走哪条路。

## P4-4 iOS / macOS / 内网穿透是承诺而非能力

- **状态**：已知并已在 README 声明（`README.md:143-148` 明确标「预留」并说明聚焦三平台）
- **事实**：`bedcode-mobile/src-tauri/gen/` 只有 `android` + `schemas`；`bedcode-mobile/package.json`
  无任何 `tauri ios` 脚本；`release.yml` 无 iOS / macOS job。
- **评价**：README 的**声明是诚实的**，这一条不按缺陷计。登记在此仅为避免下轮审计重复计分。
  唯一可挑的是 `README.md:214` 把「磁盘清理」「OCR」列入插件演进清单——OCR 仅存
  `feature/ocr-plugin` 分支、磁盘清理已暂停，属规划项混写在能力清单里。

## P4-5 产品身份即将作废 + 主链路刚被重构完

- **状态**：规划中
- **事实**：`.scratch/2026-10-15-wasmnest-repo-split/spec.md` 状态为「文档阶段（未实施）」——
  平台改名 WasmNest 并拆仓，**BedCode 降级为单一 app**；`README.md:209` 的内网穿透亦为纸面。
- **影响**：架构与产品定义同时在动，README 的对外描述随时失效（与 P4-2 同源）。

---

## 6. 已澄清的误报（下轮审计勿重复计分）

三条流传于初步评估、经复核**不成立**或**需降级**的结论：

1. **「wasm-core 内核持有产品字面量 `com.bedcode.ai-chatbox`」——不成立为生产代码缺陷。**
   复核：`manager/watcher.rs:197-198` 是**文档注释举例**；`:228/:232` 与
   `manager/validation.rs:287`、`security/fs_auth.rs:1670` **全部在 `#[cfg(test)] mod tests` 内**
   （`watcher.rs:210`、`validation.rs:278` 是 test 门），属测试夹具。purity lock 判据 7b 只扫
   **声明位**、不扫测试与注释，与此一致。**降级为**：注释含产品字面量，极低危，可顺手清理。
2. **「AI Chatbox SIGILL 未复验」——不准确。** `spec.md:125` 记录真机复验已通过（不再 SIGILL）；
   未闭环的是**复验中发现的下一个缺陷**（见 P4-3）。
3. **「CHANGELOG 全是架构重构」——不准确。** `CHANGELOG.md:40/:58` 两段含用户可见修复。
   准确表述是「无新增用户功能」（见 P4-1）。

---

## 7. 达标项（保持，勿在后续重构中回退）

这些不是缺陷，是当前项目真正的护城河，记录下来以免「大重构」时被无意削平：

- **`bedcode-desktop/src-tauri/tests/domain_contract_structure_locks.rs:46-106`**：解析
  `interface X {` 定义集与 `world core` 的 import/export 成员，并**双向断言**
  （unexpected + missing）。这是语义契约检查，不是文本扫描。
- **`bedcode-mobile/src-tauri/tests/mobile_wasm_core_purity_lock.rs`**：判据 7 配 7d 正向锚点
  （`core_binding.rs` 必须仍含 `bindgen!` + `wit/core.wit`）防「扫空恒绿」；`:401-407` 用**登记表**
  而非静默跳过处理豁免；`:37-42` 甚至诚实记录了「禁 WASI 词汇」这条判据曾被用户裁决作废。
  这三样是本仓锁测试的成熟度上限，其余锁应对齐此标准。
- **ADR 0045 `:92-97`（P3 条目 `:96`）保留了 POC 实测中被证伪的假设 P3**（`cargo tree` 命中 28 处桌面依赖）——
  留下被推翻的决策是这套流程真实性的最强证据。0049 的「遗留项」用删除线标记已收口项，同款做法。
- **`ADR 0052`（104 行）是样本最佳**：背景含编号论据 + 用户原话裁决，D1-D6 决策，
  「代价与风险」表配处置列，D6 点名取代 0045 D3。
- **`cross-end-tests/`** 是独立 crate，同进程内让桌面真实 Actix 服务器 + 真实移动 wasm 产物互连，
  **零 mock、零 adb**，产物缺失时**显性失败不 skip**。
- **真抽象样本**：`HostFactsPort`（`ports.rs:145-166`）有桌面 `host_facts.rs:22` + 移动
  `host_ports.rs:153` 两个生产实现 + fail-visible 桩。可作为判定「一个 port 是不是真边界」的标尺
  （对照 P1-2）。
- **契约层与 schema 层的「无业务内核」是真落实**：`core.wit` 的 `world core` 只收机制面、
  主库 `db/schema.sql:11-73` 只有 5 张通用表零产品表、
  `bedcode-desktop/src-tauri/src/utils/session_gateway.rs:49-56` 是真零解析窄转发。

---

## 8. 建议处置顺序

| 序 | 项 | 理由 |
| --- | --- | --- |
| 1 | **P0-1**（10 个 crate 进 CI 循环） | 成本近零、收益最大：952 个已有测试一步变成真门禁，且不碰任何产品代码 |
| 2 | **P4-3**（真机复验） | 唯一实质交付风险：主链路未端到端验证过。需用户先决策签名冲突出路 |
| 3 | **P0-2 / P3-1 / P3-2**（文档承诺兑现或删除） | 都是「文档承诺 ≠ 事实」，AGENTS.md §0 硬要求先修文档再继续 |
| 4 | **P3-3 / P3-4 / P1-3**（注释与元数据订正） | 小、确定、无风险，随时可做 |
| 5 | **P1-1 / P1-2**（边界裁决） | **需用户确认归属后才能动手**（AGENTS.md §0 优先级 2） |
| 6 | **P2-1 / P2-2 / P2-3**（锁提质） | 建议在 P0-1 落地、门禁真跑之后再改锁，避免又造出「写了不看」的锁 |
| 7 | **P4-1 / P4-2 / P4-5**（发布与 README） | 等 P4-3 真机闭环后一次性处理 |

**第 1 项是本台账的最高杠杆项**：它不写一行产品代码，却让本项目全部架构承诺第一次具备强制执行力。
