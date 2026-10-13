# 契约归位：core 收窄为机制必需能力 + 交集归能力域 + 端差异归宿主

> **用户裁决（2026-10-13，原文要点）**：
> 1. 「core 内不应该提供业务基础能力契约，而是提供一套可组合的机制和整套 core 机制必须的能力（比如 db、fs 等）；各个能力域才是交集的地方；差异应该在各自的宿主侧。」
> 2. 端独有扩展实现走**变体 B**：扩展的 `impl Host` 与桌面专属引擎面整体迁桌面宿主，能力域 crate 端中立。
> 3. 「双端的交集应该按照**功能**来划分，而不是具体的函数方法；如果功能一致，应该统一成一个函数并视为交集。」
>
> 冲突优先级：用户裁决（AGENTS §0 优先级 1）> 本文 > ADR 0045 D3（「core = 交集切片」被本裁决修订）。
> 决策落文：`docs/adr/0052-core-mechanism-only-and-domain-owned-contracts.md`。
> 本文件是实施计划真源；逐票记录进 `issues/`（格式见 `docs/agents/issue-tracker.md`）。

**状态（2026-10-13）**：T-01 resolved（`audit.md` + 用户裁决 Q1–Q5）→ T-02 resolved（core 16→12、四域契约迁能力域、union 全等）→ T-03 resolved（端扩展 impl 归桌面宿主，双端门禁绿）→ **T-04 resolved（通知/目录选择功能级统一 + minor bump 0.2.0 + 双端产物全量重建 + fail-visible 证据）** → **T-05 resolved（三把契约结构锁 + 文档口径全扫 + 全量门禁收口）**。逐票记录见 `issues/`（含全量门禁结果与基线红清单）。ADR 0052 已落文。

---

## 1. 目标终态

```text
core.wit（内核绑定的唯一契约，业务零词）
  import（8，内核自实现）：host-storage / host-plugin-database / host-log /
      host-config / host-events / host-bus / host-fs / host-platform
  export（4）：command / lifecycle / manifest / events-binary
        │  双端 compose.json 直接消费域契约（pty 先例推广）
        ▼
能力域 crate wit/<domain>.wit（域共享契约 = 功能级交集，唯一真源）
  pty            → packages/bedcode-pty-engine/wit/pty.wit
  mdns           → packages/bedcode-discovery-engine/wit/mdns.wit
  peer           → packages/bedcode-server-peer-net/wit/peer.wit
  http（出站）   → packages/bedcode-server-http/wit/http.wit
  ws（客户端）   → 待 T-01 冻结：bedcode-ws-client-engine 新增 wit/ 或
                   bedcode-server-websocket 保留客户端面
  notify（候选） → 待 T-01 裁决：新建轻量域 crate
        │  只有一端有的功能 → 端 cap（声明与实现都在端宿主）
        ▼
端 cap（各端宿主 rust/wit-src/）
  desktop: cap-desktop（auth-center / task / crypto / process / app / timer /
           api-call / connection-registry / fs-desktop / platform-desktop /
           events-desktop …）+ cap-http-endpoint + cap-ws-server
  mobile:  cap-mobile（notify 端扩展 / fs-mobile / terminal-stream /
           connection-target / auth-client / events）
```

## 2. 规则（判据；今后增改契约逐条对照）

- **R1 · core.wit 收录判据**（三条件全满足）：① 内核自己实现（`manager/runtime/core_binding.rs`）；② 双端同源语义；③ 内核机制必需或全部插件共同需要。业务能力契约（设备发现 / 配对 / 文件传输 / HTTP / WS / 通知 / 终端流…）一律不落 core。
- **R2 · 交集 = 功能级**：同一功能两端都提供 → 进能力域共享契约；形状不一致 → **统一为一个函数/签名**，不保留两端变体；只有一端提供 → 端 cap。判定单位是功能，不是现有函数名/函数集。
- **R3 · 端差异归宿主**：端独有 interface 的 WIT（端 cap）与实现（`impl Host` + 专属引擎面）都住端宿主；能力域 crate 端中立（默认形态零端 WIT / 零桌面专属引擎，`common-host` 只装共享契约绑定）。
- **R4 · 契约房 = 共享机制所在 crate**：域共享契约住该域共享引擎 crate（`wit/<domain>.wit`）；无共享引擎的域可新建轻量域 crate（notify 候选），禁止把业务契约挂回 core。
- **R5 · 变更形态**：契约搬家（interface 在文件间移动、union world 不变）→ 无 bump；功能统一（签名变 / interface 增删）→ minor bump（ADR 0051 D4）+ 双端同步（file-transfer 共享产物对齐，ADR 0051 D3）+ 产物重建 + 旧产物实例化期点名（复用 `stale_artifact_rebuild_hint`）。

## 3. 现状事实（2026-10-13 核对，全部在盘）

| 事实 | 值 | 证据 |
| --- | --- | --- |
| `core.wit` interface 数 | **16**（12 import + 4 export）。文档口径「17」是 ADR 0051 删除 `interface abi`（提交 4c5958a18）后未同步 | `grep -c '^interface ' packages/bedcode-wasm-core/wit/core.wit` |
| `core.wit` 中的能力域交集 | `host-mdns` / `host-peer` / `host-http`（出站 1）/ `host-websocket`（客户端 5） | core.wit L82/L178/L246/L287 |
| 内核实际实现 | 8 域：storage / plugin-database / log / config / events / fs / bus / platform | `core_binding.rs` 8 个 `impl …::Host` |
| 能力域自持分片 | discovery-engine/mdns.wit、server-peer-net/peer.wit、server-http/http.wit（含 `host-http-endpoint` 副本）、server-websocket/ws.wit（含 `host-websocket-server` 副本）、pty-engine/pty.wit | 各 crate `bindgen!(path: "wit/…")` |
| 端组合真源 | 桌面 caps = pty/http/ws/desktop；移动 caps = mobile | 双端 `compose.json` + `scripts/compose-wit.mjs` |
| 端组合实际消费域文件 | **只有 pty**（`caps.pty → pty-engine/wit/pty.wit`）；mdns/peer/http/ws 的域文件只是 provider 侧 bindgen 副本 | 桌面生成物 `cap-pty.wit` 在场 |
| 端独有扩展实现落点 | `host-http-endpoint` 在 `bedcode-server-http`、`host-websocket-server` 在 `bedcode-server-websocket`（`common-host` feature） | 两 crate `plugin_binding.rs` |
| 副本一致性锁 | **无函数集静态比对**；现有锁只查 package 行 / 接口名存在性 / 生成物 sha | `wit_package_version_lock.rs`、桌面 `bindings.rs`、`check:wit` |

## 4. 归位映射（初版；T-01 逐项冻结）

### 4.1 core 保留（8 import + 4 export，不动）

storage / plugin-database / log / config / events / bus / fs / platform + command / lifecycle / manifest / events-binary。
**T-01 复核项**：`host-platform` 的 `pick-*` 是否属「机制必需」——初判保留（内核实现 + 双端同源 + 平台原语非业务），如需外移按 R4 走域 crate。

### 4.2 交集搬移（T-02 实施，ABI 中性）

| interface | 现状 | 目标 |
| --- | --- | --- |
| `host-mdns` | core.wit | 域文件（已存在，内容不动）→ 双端 `caps.mdns` |
| `host-peer` | core.wit | 域文件 → 双端 `caps.peer` |
| `host-http`（出站） | core.wit | `server-http/wit/http.wit` 去端扩展副本后 → 双端 `caps.http` |
| `host-websocket`（客户端） | core.wit | 域文件（T-01 冻结契约房）去端扩展副本后 → 双端 `caps.ws` |

### 4.3 端扩展（T-03 实施，变体 B）

| interface | 现状 | 目标 |
| --- | --- | --- |
| `host-http-endpoint` | 桌面 wit-src + server-http crate 双份，impl 在 crate | 桌面 wit-src 唯一声明（world 改名 `cap-http-endpoint`）；impl + 端点注册表/路由匹配等桌面专属引擎面迁桌面宿主 `plugin/` |
| `host-websocket-server` | 桌面 wit-src + server-websocket crate 双份，impl 在 crate | 同上（world 改名 `cap-ws-server`）；server 专属引擎面迁桌面宿主 |

**world 重名约束**：同 package 内一个 world 名只能定义一次。域文件与端扩展文件同时在桌面生成物时，`cap-http` / `cap-ws` 不能各出现两次 → 端扩展 world 必须改名（见上表）。移动 compose 只消费域交集文件，绝不携带端扩展（否则移动 world 引入无人实现的 import，实例化即炸）。

### 4.4 待 T-01 裁决

- ws 客户端契约房：`bedcode-ws-client-engine`（移动在用，需新增 `wit/`）vs `bedcode-server-websocket`（桌面在用，客户端不再共享它）。
- notify 契约房与统一签名（见 C1）。
- `host-fs-desktop` / `host-fs-mobile` / `host-terminal-stream` / `host-platform-desktop` 等逐项按 R2 复核（见 §5）。

## 5. 功能级合并候选（T-01 逐案裁决，按 R2）

| # | 功能 | 现状（两端形状） | 推荐 |
| --- | --- | --- | --- |
| C1 | 系统通知 | 桌面 `host-events-desktop.notify(title, body)`；移动 `host-notify.notify(title, body, options-json)` | 统一为一个共享 `host-notify.notify(title, body, options-json)`；`options` 含 `vibrate?` / `sound?`（不支持端的处置：忽略 vs 报错，待裁——建议「平台不支持的选项静默忽略 + 文档声明」）；移动的 check/request-permission / vibrate / play-sound 留移动扩展或按裁决提升 |
| C2 | 目录选择 | 桌面 `pick-folders()`（多目录，端扩展）+ 双端交集 `pick-folder()`（单目录） | 统一为 `pick-folders()`（`string[]`，空数组 = 取消）；移动 SAF 返回 0/1 元素；`pick-folder` 退役 |
| C3 | 目录读取 / 元数据 | 桌面 `read-dir` / `canonicalize` / `stat`（端扩展）；移动无 | 核查移动 SAF 等价能力：有 → 统一进交集；无 → 留桌面扩展（推荐后者，内容 URI 无 POSIX canonicalize 语义） |
| C4 | 保存到系统位置 | 移动 `write-media-downloads` / `save-to-document`；桌面无 | 保留移动扩展；桌面「系统保存框」是新增能力，不在本轮 |
| C5 | 终端输出通道 | 移动 `host-terminal-stream.forward-output`；桌面走 `session_gateway` 宿主面 | 核查桌面是否有插件面等价物：有 → 统一；无 → 留移动扩展 |
| C6 | 导出面（events 等） | 桌面 `events` 已按共享产物并集；移动原生 `events` 子集 | 导出不进 R2 交集（宿主按名动态调用，非 import 契约）：保留现状 + 理由记录（共享产物场景由 ADR 0050 承载） |

## 6. 票与依赖

```text
T-01 功能级盘点与裁决（issues/01）        ← 产出审计表 + 最小问题集，用户逐项确认
   └─→ T-02 core 收窄 + 交集契约搬域 crate（issues/02，ABI 中性）
          └─→ T-03 端扩展实现归位（变体 B）（issues/03）
                 └─→ T-04 功能统一签名落地（issues/04，minor bump + 双端同步 + 产物重建）
                        └─→ T-05 锁/文档/CHANGELOG + 全量门禁收口（issues/05）
```

**测试节奏（AGENTS §10 两段式）**：每票内只做针对性自验（cargo check + 直接相关单元测试 + 锁变异自检）；全量回归统一在 T-05 收尾跑。

## 7. 版本与失败可见（R5 细化）

- **T-02（搬家）零 bump**：`world plugin` 的 import/export 并集不变、package 名与版本不变 → 插件产物不重建。需在票内**证明并集等价**（改造前记录双端 world 的 import/export 名单，改造后逐项比对；证据入票）。
- **T-04（统一签名）minor bump**：`0.1.0 → 0.2.0`，版本真源 `contracts/bedcode-plugin-package.rs` 单点改；因 file-transfer 双端单产物共享同一版本（ADR 0051 D3），**双端必须一起 bump**。
- **fail-visible 三形态**（签名破坏）：① 旧产物 import 名版本不等 → 实例化期被 linker 拒绝并点名；② `stale_artifact_rebuild_hint` 文案含目标版本与重建指引；③ 插件产物全量重建（桌面 + 移动 3 app + file-transfer 共享产物），不允许保留旧路径。
- **台账**：`CHANGELOG.md` / `CHANGELOG_zh.md` 双语条目（契约归位 + 签名统一 + 版本 bump）。

## 8. 门禁

| 票 | 针对性自验 |
| --- | --- |
| T-01 | 无代码改动；审计表每条有证据（文件/行）；用户确认记录入 issue |
| T-02 | `pnpm run check:wit` 绿；双端 `cargo check` 0 error；双端 world import/export 并集比对证据；桌面 `bindings.rs` cfg(test) 更新后通过；移动 `mobile_wit_composition_lock` 通过 |
| T-03 | 桌面 `cargo test`（http_e2e / ws_e2e / 白名单锁 / 回收锁）通过；能力域 crate 无端 WIT 副本与端 impl；移动 `cargo check` 绿 |
| T-04 | 双端 `cargo check` + SDK 编译；3 个移动 app 重建（`--rust-only`）+ 桌面插件产物重建；旧产物拒绝证据；双端 `pnpm run test:run`（相关面） |
| T-05 | 全量：双端 `cargo test`、两端 `pnpm run test:run`、根 `pnpm exec eslint .` 0 error、`pnpm run check:wit`、`cross-end-tests`（涉及共享产物时必跑）、`cargo fmt` / `clippy` 自查 |

## 9. 风险与并行纪律

- **world 重名**：端扩展 world 改名（§4.3），漏改 → `check:wit` 拼装期 duplicate world 即红。
- **双端版本对齐**：T-04 双端必须同步 bump（file-transfer 单产物 + 移动 `desktop_compat.rs` 跨端读桌面生成物）。
- **移动实现缺口**：C3/C5 若裁决为「统一」，移动需补实现（SAF 列目录等），工作量随裁决爆炸——T-01 必须先冻结。
- **工作区在途**：当前有多条并行改动（file-transfer-single-wasm / runtime-single-form / wit-package-semver / desktop-shell-redesign 及 `git status` 既有 M 文件）。各票只碰本票文件，禁整文件回滚（AGENTS §11）。
- **文档口径漂移**：AGENTS §5.4 / ADR 0049 D2 / code-map / Rust 注释的「17 个」与 `wit_package_version_lock.rs` 注释「pty / http / ws 会被双端 compose 拷贝」（实际移动只组合 cap-mobile）在 T-05 一并收口。
- **磁盘 / target**：大编译前 `df -h`；命令字眼以 `docs/commands.md` 为准。

## 10. 越线条款

本改造是 ADR 级红线调整（修订 ADR 0045 D3 判据），以 ADR 0052 落文为前提实施。实施中发现需要新增/放松其他红线（如移动端补实现面、为共享产物保留端并集导出）时**停下问用户**，禁止自行扩大范围。
