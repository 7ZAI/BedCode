# 07: 文档与 ADR 收口——能力登记 + 双端偏离 + 抽取候选

**What to build:** 让后来者在契约、边界、双端关系三处不迷路：host-pty 作为内核基础能力服务被正式登记，桌面独有的偏离被如实记录，本期刻意「不做」的两个抽取决策留下可检索的注记。摆位与 ws-base-service 文档收口票同构。

**Blocked by:** 06（契约固化后文档才与实现一致）

**Status:** done（2026-09-19；ADR 0022 新增 host-pty 节 + 首次成文「双端偏离」与「抽象提取候选」两节并落 v6 修订记录；AGENTS §7 补 host-pty 与 ABI v16/v15 偏离；桌面 code-map 新增 host-pty 能力域节并改写 PTY 引擎节；roadmap 加进程（PTY）原语面条目；票 01-06 状态归位、spec D4/D9 补实施定形；顺带修好 AGENTS 里 4 条失效 `.scratch/` 路径）

## 要同步的文档

- **ADR 0022（裁剪线）**：增 `host-pty` 记录——裁剪线判定依据（WASI 0.2 无 PTY、运行时默认 deny 设备访问、便携 PTY 依赖为宿主独占 → 插件物理上不可能自建；参数面零业务语义）、终态原语（6 函数 + 输出拉取模型）、与 host-process / host-terminal / host-session 的边界划界。
- **AGENTS.md §7**：宿主能力清单补 host-pty；WIT 双端同步的**文档化偏离**记录（桌面 ABI **v16**、v15 归 host-auth 认证中心线、移动端 WIT/ABI/SDK 不跟演，同 ws-base-service / wasmtime 48 分叉先例，含恢复条件）。
- **code-map**（桌面端）：宿主能力表补 `host-pty` 一行（目录层级即可）。
- **roadmap / platform-kernel**：阶段对照——「进程（PTY）」内核能力首次以 host 原语形态对插件开放，业务编排仍留插件层。
- **抽象提取候选登记**（本期明确不做，只记不写码）：① PtyRing ↔ 业务会话输出环的代码级合并；② PTY 引擎与业务会话线的进一步解耦边界。
- **本目录收口**：票 01–06 的 Status / Comments 补齐（含 exitCode 降级原因、限额实际取值、与本 spec 的偏差）；文档内命令字眼与 AGENTS.md §3 一致（无旧 npm 字眼）。

## 验收

- [x] ADR 0022 出现 host-pty 节且判定链条完整（离宿主无法实现 + 零业务语义 + 边界划界）
- [x] AGENTS.md §7 能力清单含 host-pty，ABI v16 / v15 归属 / 移动端偏离与恢复条件记录在案
- [x] code-map 与 roadmap 同步；描述与实现一致（不一致处顺手修正）
- [x] PtyRing 抽取候选在 ADR 或 spec 中有可检索记录
- [x] 票 01–06 Status 收口，spec 与实现的偏差如实登记
- [x] 文档一致性自查通过（含 §3 命令字眼、双端 ABI 表述不再写「锁死 47」等已过期措辞）

## Comments

### 2026-09-19 收口落地

**1. `docs/adr/0022-plugin-host-interface-primitive-boundary.md`**

- 新增 **「新增 host-pty（插件私有伪终端基础能力服务，2026-09-19）」** 节：裁剪线判定链条（WASI 0.2 无 PTY 接口 + wasmtime 默认 deny 设备访问 + `portable-pty` 宿主独占 → 插件物理上不可能自建；参数面零业务语义）、7 条裁决要点（裸 argv 与「明确不做」清单、与 host-process / host-terminal / host-session / terminal-hooks 的划界、纯拉取输出面及 push 模型的两条否决理由、属主隔离与停用回收、**单一发布者不变量**、权限两域 + 五同步点漂移锁、限额分级与声明式环容量）。
- 首次成文 **「双端偏离」** 节：`host-websocket`（v14）/ `host-auth`（v15）/ `host-pty`（v16）为桌面独有接口，mobile WIT/ABI/SDK 不跟演（现状 desktop v16、mobile 11），含**恢复条件**与「双端同步硬约束仅适用于共有接口」的适用范围界定。
- 新增 **「抽象提取候选」** 节 4 条：① `PtyRing` ↔ 业务 `UnifiedOutputQueue` 代码级合并；② PTY 引擎与业务会话线进一步解耦（策略三元组收敛为装配参数结构，替代构造器家族）；③ `PtyTerminationGate` 缺「终态事件是否已发出」访问器（即票 04 变异 M2 存量的根治入口）；④ `is-running` 判据归属（host 层组合，勿下沉引擎）。
- 修订记录追加 **v6（当前）**；顺手把 v3 / v5 遗留的陈旧「（当前）」标记归位。

**2. `AGENTS.md`**

- §7 WIT bullet：补双端偏离段（v14/v15/v16 归属、mobile 11、恢复条件），保留原有 wasmtime 分叉表述与「双端对齐后恢复锁死表述」的条件句。
- §7 宿主能力 bullet：按 `capability.rs::HOST_PRIMITIVE_CAPABILITIES` **实测 20 组**写实（进程拆 `host-pty`（交互式）/ `host-process`（非交互）），并把「权限按域拆分 + 五同步点漏一处即漂移锁翻红」写进同一条，使 `pty:spawn` / `pty:io` 有落点可查。
- **顺带修正 4 条失效引用**（`[ -e ]` 逐条复核，全部由 MISS → OK）：`.scratch/adb-fd0-bug/` → `.scratch/2026-09-07-adb-fd0-bug/`、`.scratch/plugin-wasm-logging/` → `.scratch/2026-09-09-plugin-wasm-logging/`、`.scratch/plugin-kernel-roadmap/` → `.scratch/2026-09-10-plugin-kernel-roadmap/`、`.scratch/platform-kernel/` → `.scratch/2026-09-10-platform-kernel/`（§4 路由表、§5 架构目标、§13 文档索引三处）。

**3. `bedcode-desktop/docs/code-map.md`**

- 新增 **「宿主能力实现域 · PTY 基础能力服务 — `host-pty`（ABI v16）」** 节（与 host-websocket 节同构：契约位置 / 无可选导出 / 零业务红线 / 创建域 / 数据域 / 输出面 / 生命周期 / 回收 / 属主隔离，常量名与文件路径逐一对应实现）。
- `host_impl/` 模块清单补 `pty`；**改写 `src-tauri/src/pty/` 一节**——原文只写「启动时注入 `BEDCODE_SESSION_ID`」，已不描述现状：现按两条消费线说明引擎参数化（`SessionOutputSink` vs `PtyRingSink`、`PtyCommandSource::Business` vs `Raw`、`Hold` vs `ReleaseOnSpawn`）与 `PtyTerminationGate` 的两路信号语义。

**4. `2026-09-10-plugin-kernel-roadmap/spec.md`**

- 加 2026-09-19 修订段：进程（PTY）内核能力首次以 host 原语形态对插件开放，**不改阶段边界红线**（引擎与输出分发仍留内核，下沉的只是产品语义归属）。
- 「阶段边界总览」表新增一行 **「进程（PTY）对插件的原语面」**：阶段 1 未开放 → 阶段 2 期间已落地（v16）→ 阶段 3 终端业务直接复用 → 阶段 4 冻结。

**5. 本目录收口**

- 票 01：中间态 Status「code-complete，验证受阻」→ **done**（并发会话阻塞后已解阻，时间线仍在原文）。
- 票 02 / 03：Status 与实施记录齐；票 03 追加「票 04 落地后的连带修正」（载体迁移 + 变异 C-② 证据易主到 `running_verdict` 真值表）。
- 票 04：**done**，含交接事实（对侧在途代码 + 用户裁决接手）、生产级 panic 修复、9 例新断言、变异 M1-M5。
- 票 05：**done**，含用户裁决（声明式环容量）、8 例契约、变异 N1-N4、与票面「同源/均被拒」两条偏离的显式登记。
- 票 06：**done**，含矩阵落点表（六类断言 × 分层，标明每格落在 e2e / 宿主 / 环 / SDK 哪一层）与变异 N6（单函数破口 → 单分格定位）。
- `spec.md`：D4 补「票 04 实施定形——单一发布者不变量（含 ambient runtime 硬约束）」；D9 补「票 05 实施定形——声明式环容量 + 读侧截断／写侧拒绝的分裂」；头部「同类前例」路径补日期前缀；Status → `implemented`。
- **exitCode 未降级**：spec D4 预留的降级分支未触发——票 01 取到真实退出码，票 04 端到端验 `exit 42` / `exit 3` / `exit 0`（0 与「无退出码」以字段有无区分）。
- **限额实际取值**：`PLUGIN_PTY_MAX_SESSIONS_PER_PLUGIN = 8`、`PLUGIN_PTY_RING_BYTES = 256 KiB`（默认）、`PLUGIN_PTY_RING_MAX_BYTES = 4 MiB`、`PLUGIN_PTY_RING_FETCH_MAX_BYTES = 16 KiB`、`PLUGIN_PTY_MAX_WRITE_BYTES = 64 KiB`、`PtyRing::DEFAULT_MAX_CHUNKS = 4096`（条目维度留在环模块，理由见票 05「与票面的偏离」3）。

**6. 发布前实机验证清单（Windows / macOS，本机 Linux 覆盖不到；票 01-05 的注记合并于此）**

- `ReleaseOnSpawn` 在 ConPTY 下的行为（unix 侧 slave fd 正是挡住 EOF 的引用，win 侧 slave 只是 `Arc` 克隆，源码可推但无真机证据）——判据：短命命令自然退出必须观察到 EOF + 恰好一条 `pty:exit`；
- `kill` 的 `taskkill /F /T` 分支与 `reason=killed` 的一致性；
- 裸 argv 经 `CommandBuilder` 在 ConPTY 下是否被二次解析（免 shell 注入的裁剪线证据）；
- `resize` 在 ConPTY 下的生效时序（插件侧以 `mode con` 反查）；
- `stty -echo -onlcr` 是 POSIX 终端语义：本线 4 例字节级比对用例在 Windows 需换载体（票 05 记的「载荷同步点经验」同源）。

**门禁复核（本会话实跑）**

- `cargo test --offline --lib` → **999 passed / 0 failed**；`cargo test --offline --tests --no-fail-fast` → 8 集成目标绿（含业务链路 `pty_session_chain`），唯一红 `broadcast_shutdown`＝认证中心线 `utils/auth/jwt.rs:43` 的 error 级日志（票 02 / 04 / 05 同一登记项，收口时仍未由对侧处理）
- `pnpm run test:run`（bedcode-desktop）→ **70 files / 672 passed**；根 `pnpm exec eslint .` → **0 error**（122 warning，按 §10 不计入门禁）
- SDK：`cargo test` **85 passed** + `wasm32-unknown-unknown`（stable）/ `wasm32-wasip3`（nightly-2026-09-16）双 target check 通过
- 文档一致性：`.scratch` 路径逐条存在性复核（见 2）；`grep` 确认票 01-07 与 spec 中测试/构建命令字眼全为 `cargo test` / `pnpm run test:run` / `pnpm exec eslint`，无旧 `npm` 命令；「锁死 47」等过期措辞未出现（现存表述均为「桌面 48 / 移动 47 分叉中 + 双端对齐后恢复锁死表述」的条件句）
- 测试后进程与端口核查：无 PTY 载体残留、无测试遗留监听

**遗留与后续建议（不属本票范围，登记备查）**

1. `broadcast_shutdown` 红项待认证中心线在其集成夹具中初始化 secret-store。
2. 票 01 点名的 `hold_policy_*` 时间型断言（本目录内翻红过一次，4 次复跑绿）：稳态化两条路径见 ADR「抽象提取候选」③ 与票 05 记录。
3. 票 06 未覆盖「配额打满 × 多插件并发」的端到端组合与 deactivate 的 PluginHost 级行为用例（当前为接线锁 + 行为用例组合）。
4. 移动端若将来需要同类能力，按 ADR「双端偏离」恢复条件补 interface 并对齐 ABI 计数。
