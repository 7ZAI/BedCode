# 三仓拆分方案（平台仓 / 能力仓 / 应用仓）

> 用户指令（2026-10-16）：
> **「重新调整仓库拆分方案：所有的 wasm-app 项目放一个 git 仓库；移动端、桌面端放一个仓库；
> 多个 crate 能力面放一个仓库 —— 总共三个 git 仓库。」**
>
> Status: **文档阶段（未实施）· 裁决已定**（2026-10-16 用户裁决：§10 十一条按建议默认全部采纳）。
> 本文只描述目标形态、接线与实施边界，不含已执行改动；开工前尚需完成 §8 的前置闸门。

**与旧方案的关系**：本文**取代** `.scratch/2026-10-15-wasmnest-repo-split/` 的
「元仓 + lib + 两端 + 每 app 一仓 + 跨端仓」九仓切法（该文 §5 D1/D2/D6 的仓数结论作废）。
旧案的**改名规则**（`naming-rules.md`）与**迁移纪律**（先改名后拆分）**继续有效**——改名是
正交前置阶段，本文不改其任何字面量裁决，只把「拆分」这一步从九仓重切为三仓。

---

## 0. 用户指令的直译（三条边界，不再多切一个仓）

| # | 仓 | 装什么 | 一句话判据 |
| --- | --- | --- | --- |
| ① | **平台仓** | 桌面端 + 移动端（宿主 `src-tauri` + 前端）+ 跨端测试 + 产品文档/品牌/发布清单/签名库 | 「运行宿主与它的发布面」 |
| ② | **能力仓** | `packages/bedcode-*` 全部机制内核 / 传输层 / 能力域引擎 + `contracts/` + WIT 真源 | 「不含产品语义的引擎与契约」 |
| ③ | **应用仓** | 全部 wasm-app（桌面 4 + 移动 3，按 app 归并到同一目录） | 「业务事实面」 |

**明确不再单独立仓**：元仓（文档并入平台仓）、`wasmnest-lib`（= 能力仓）、
`wasmnest-cross-end`（并入平台仓）、`wasmnest-app-*` ×4（并入应用仓）。

---

## 1. 事实基线（2026-10-16 代码走读，逐条可复现）

| # | 事实 | 证据 |
| --- | --- | --- |
| T1 | 当前单仓五类内容：双端宿主 + `packages/**`（**18 个 crate**）+ 双端 `wasm-apps/**` + `cross-end-tests/` + 产品文档/品牌 | 根目录；`ls packages/` |
| T2 | 桌面 wasm-app **4 个**（`agent-hub` / `ai-chatbox` / `file-transfer` / `terminal-session`）；移动 **3 个**（`ai-chatbox` / `bedcode` / `file-transfer`）；`bedcode`（移动）与 `terminal-session`（桌面）**同 plugin id `com.bedcode.terminal-session`** | `ls bedcode-{desktop,mobile}/wasm-apps/` |
| T3 | **6 个 app 的 Rust 侧全部以相对路径依赖本端 SDK**（`../../../packages/plugin-sdk-{desktop,mobile}/rust`） | 各 app `rust/Cargo.toml` |
| T4 | SDK 是**端内包**（`<end>/packages/plugin-sdk-{desktop,mobile}/rust`），不是 `packages/**` 里的能力域 crate；已走独立发布通道（npm `@binblink/bedcode-plugin-sdk-*` + crates.io `bedcode-plugin-api` / `-mobile`） | `package.json` / `rust/Cargo.toml` / `docs/knowledge/sdk-publish.md` |
| T5 | **能力仓对平台仓有一条构建期反向边**：`packages/bedcode-wasm-core/Cargo.toml:107` → 桌面 SDK（`bedcode-plugin-api`，optional，由 `desktop-assembly` 开启，而 `default = ["desktop-assembly"]`） | 该文件 + `[features]` |
| T6 | **Cargo 在 manifest 装载期就要求所有 path 依赖的路径存在**，即使该 optional 依赖未启用 —— 实测：feature 关闭、路径缺失时 `cargo build` 直接 `failed to load source for dependency` | `/tmp/cargotest` 最小复现 |
| T7 | 契约版本真源以**相对 include** 被内核引用：`packages/bedcode-wasm-core/src/contract.rs` → `include!("../../../contracts/bedcode-plugin-package.rs")`，且 `scripts/compose-wit.mjs` 也按仓根读同一文件 | 两个文件 |
| T8 | `cross-end-tests/` **同进程 path 依赖两端 `src-tauri`**（零 mock 真实互连），并直读 `packages/bedcode-server-base` | `cross-end-tests/Cargo.toml` |
| T9 | 双端宿主对能力仓是**密集 path 依赖**：桌面 9 条、移动 8 条（含 `common-host` feature 开关），且**无根 Cargo workspace**（各 crate 自成根） | 两端 `src-tauri/Cargo.toml` |
| T10 | 桌面 `packages/` 另有 **5 个测试夹具包**，移动 1 个——它们测宿主，**不是**能力域 crate | `ls bedcode-{desktop,mobile}/packages/` |
| T11 | 只有两个 crate 不是平台机制：`bedcode-file-transfer-core`（唯一消费者 = file-transfer 应用）与 `bedcode-componentize`（打包工具，非运行期依赖图） | `rg` 消费者扫描 |
| T12 | 根级共享资产：`docs/`（**48 篇 ADR** + `knowledge/` 20 篇 + `diagrams/`）、`contracts/`、`scripts/`（18 个）、`.github/workflows/`（5 个）、`.cargo/config.toml`（sccache + jobs）、`bedcode.keystore`、`eslint.config.js`、`.husky/`、`BedCode.code-workspace` | 各目录 |
| **T13** | **能力仓不是测试期自足的**：`packages/**` 里至少 **6 处治理锁在测试期读平台仓/应用仓的源文件**（见 §5-E2b 清单） | `rg 'bedcode-(desktop\|mobile)' packages/` = 135 命中 |
| **T14** | **平台侧对能力仓路径有 326 处引用**，其中多条防回接锁把 `path = "../../packages/bedcode-wasm-core"` 这类字符串当**判据 needle**（例：`retired_mobile_wasm_core_fork_lock.rs`） | `rg` 计数 + 该文件 |
| **T15** | 能力仓内部另有路径敏感点：`repo_root()` / `crate_root()` 派生 **6 处**、`.join("packages/<crate>/…")` **5 处**（其中 4 处在 `permission.rs`，指向桌面 SDK 生成物，即 E2b#5）、字符串常量比较 **5 处** | `rg 'fn (repo_root\|crate_root)'` / `rg 'join\("packages/'` |
| **T16** | 文档/发布面没有第三方消费者，是纯产品级信息 | 引用扫描 |

---

## 2. 目标形态（三仓 · 本地组装）

```text
~/project/tauriProject/WasmNest/          ← 本地工作区根（磁盘目录，不是 git 仓）
  wasmnest.code-workspace                 ← 多根工作区文件（现 BedCode.code-workspace 升级版）

  wasmnest/                               ← ① 平台仓（双端 + 跨端 + 文档/品牌/发布）
    desktop/                              ←   原 bedcode-desktop/（宿主 src-tauri + 前端 + 端内 SDK + 夹具）
    mobile/                               ←   原 bedcode-mobile/（同上，含 gen/android 手写 Kotlin）
    cross-end-tests/                      ←   跨端真实互连（并入，不再单仓）
    docs/  brand/  scripts/  .github/      ←   ADR / CONTEXT / CHANGELOG / commands / knowledge / diagrams
    .cargo/config.toml  .husky/  eslint.config.js  .prettierrc.json  .gitignore  LICENSE
    bedcode.keystore                      ←   发布签名唯一真源
    release-manifest.json                 ←   跨仓版本唯一表达（见 §6）
    packages/                             ←   ★ git submodule → ② 能力仓（钉 tag）
    .gitmodules

  wasmnest-lib/                           ← ② 能力仓（现 packages/** 整体上移为仓根）
    bedcode-wasm-core/  bedcode-host-kit/  bedcode-server-*/  bedcode-pty-engine/  …
    contracts/bedcode-plugin-package.rs   ←   契约版本真源（T7）
    <各 crate>/wit/*.wit                  ←   WIT 真源（core + 6 能力域分片）
    .cargo/config.toml  .husky/  .gitignore

  wasmnest-apps/                          ← ③ 应用仓（双端 wasm-app 按 app 归并）
    apps/terminal-session/{desktop,mobile}       ← 移动侧即原 bedcode（T2 同 plugin id）
    apps/file-transfer/{rust,desktop,mobile}     ← rust/ 收 `bedcode-file-transfer-core`
    apps/ai-chatbox/{desktop,mobile}
    apps/agent-hub/{desktop}                     ← 桌面独有
    pnpm-workspace.yaml（列全部 app）
    .husky/  eslint.config.js  .gitignore
```

**路径前缀规则**：`<repo>/…` 表示路径相对该仓根；平台仓内引用一律写
`wasmnest/desktop/…`、`wasmnest/packages/…`（含仓名前缀），保证本地布局下可直接 `ls` 核对。

---

## 3. 内容映射表（当前路径 → 目标仓；逐条核对）

| 现路径 | 目标仓 | 目标路径 | 备注 |
| --- | --- | --- | --- |
| `bedcode-desktop/**` | ① 平台仓 | `desktop/**` | 含 `src-tauri/`、`src/`、`packages/`（SDK + 5 夹具）、`scripts/`、`docs/code-map.md` |
| `bedcode-mobile/**` | ① 平台仓 | `mobile/**` | 含 `src-tauri/gen/android`（手写 Kotlin，重建后需手工恢复） |
| `cross-end-tests/**` | ① 平台仓 | `cross-end-tests/**` | 依赖两端改为**仓内相对路径** |
| `docs/**` | ① 平台仓 | `docs/**` | 48 ADR + knowledge + diagrams + agents + commands |
| `CONTEXT.md` / `README*.md` / `CHANGELOG*.md` | ① 平台仓 | 同名 | 产品级文档（无第三方消费者，T16） |
| `scripts/**` | ① / ② / ③ 三向拆 | 见 §5-E6 | 按「服务对象」拆，不按历史位置 |
| `.github/workflows/**` | ① / ② / ③ 三向拆 | 各仓 `.github/workflows/` | 见 §7 |
| `.cargo/config.toml` | **三仓各一份** | 各仓根 | sccache 强制前置（显性失败设计），任何 Rust 仓缺它都直接失败 |
| `eslint.config.js` / `.prettierrc.json` / `.husky/` / `scripts/doc-tracking.sh` | ① + ③ | 各仓根 | 有前端的仓各一份；② 只有 Rust，不需要 eslint |
| `bedcode.keystore` | ① 平台仓 | 根 | 发布签名唯一真源，只服务移动端打包 |
| 品牌资产（`brand-spec.md`、改名工具） | ① 平台仓 | `brand/` | 品牌工程与拆分解耦 |
| `contracts/bedcode-plugin-package.rs` | ② 能力仓 | `contracts/…` | T7 的 `include!` 与 compose 脚本按能力仓根解析 |
| `packages/**`（18 crate） | ② 能力仓 | **仓根直挂**（不留内层 `packages/`） | 理由见 §5-E1 |
| `packages/bedcode-file-transfer-core` | ③ 应用仓 | `apps/file-transfer/rust/` | 唯一消费者是 file-transfer 应用（T11） |
| `packages/bedcode-componentize` | ① 平台仓 | `desktop/packages/plugin-sdk-desktop/rust/tools/componentize` | 只被打包/构建脚本调用（T11），随 SDK 构建工具面 |
| `packages/bedcode-headless-host-probe` | ② 能力仓 | 仓根 | 长期门禁（能力域 crate 纯净性），随被检对象 |
| `bedcode-desktop/wasm-apps/**` | ③ 应用仓 | `apps/<slug>/desktop/**` | `terminal-session` → `apps/terminal-session/desktop` |
| `bedcode-mobile/wasm-apps/**` | ③ 应用仓 | `apps/<slug>/mobile/**` | `bedcode` → `apps/terminal-session/mobile`（同 plugin id 归并） |
| `scripts/plugin-package-list.json` | ① 平台仓 | 并入 `release-manifest.json` | 插件清单是产品级信息 |
| 根 `package.json` / `pnpm-lock.yaml` | ① + ③ | 各仓根 | ② 无 JS，不需要 |
| `target/` `dist/` `.dev-logs/` `node_modules/` | — | — | 不入库（各仓 `.gitignore`） |

---

## 4. 两个隔离档位（先选档，再决定工作量）

三仓可拆，但「拆到哪一步」有本质差别。**用户指令只要求三仓，未要求单仓可独立克隆**，
因此先明确档位：

| 档 | 定义 | 需要做的事 | 代价 |
| --- | --- | --- | --- |
| **L1 组装式（推荐起步）** | 三仓物理分离、权限/CI/发版按仓隔离；**开发与 CI 时三仓同场**（能力仓经 submodule 挂在平台仓 `packages/`） | 纯文件搬运 + submodule 接线 + 少量路径修正 | 低；**能力仓仍不能脱离平台仓单测**（T13） |
| **L2 契约式** | 各仓**单克隆即可自测自构建**；跨仓只经发布产物（SDK 版本 / WIT bundle / release manifest） | 在 L1 基础上追加：§5-E2 的构建边打断 + 6 处跨仓锁归位 + 应用仓 SDK 依赖切版本 | 中高；触及内核装配面重构与治理锁迁移 |

**裁定（2026-10-16，按建议默认采纳）**：本轮落 **L1**（对应用户指令「拆成三个仓」的最小充分解），把 L2 拆成
「E2a 结构性解耦」「E2b 跨仓锁归位」「E3 app 依赖版本化」三张后续票。L1 已能拿到用户
真正要的三样收益：按仓授权、按仓 CI 触发、按仓发版。

---

## 5. 依赖接线与硬边（三仓之间只有 3 条边 + 1 个内部布局问题）

| 边 | 方向 | 机制 | 档位 |
| --- | --- | --- | --- |
| **E1 平台 → 能力** | ① → ② | **git submodule，mount 在 `平台仓/packages/`，钉 tag** | L1 必需 |
| **E2 能力 → 平台（反向）** | ② → ① | L1：靠 submodule 同场即可；L2：必须打断两组耦合（构建边 + 测试期读取边） | L1 容忍 / L2 必解 |
| **E3 应用 → 平台 SDK** | ③ → ① | 发布版本（npm + crates.io）；本地开发 `{ path, version }` 双声明指向兄弟目录 | L1 可保留 path / L2 切版本 |
| **E4 平台装载应用产物** | ① ← ③ 产物 | CI artifact + release manifest 记录 app tag；平台仓**不** checkout 应用仓源码 | 两档同 |

### E1 平台仓如何持能力仓 —— submodule mount 在 `packages/`（方案 V1）

**事实（T6 实测）**：Cargo 在 manifest 装载期就要求 path 依赖路径存在。因此
「三仓克隆成兄弟目录 + 跨仓相对路径」这种玩法**不成立**（任一仓缺失，另一仓连
`cargo metadata` 都跑不了）。必须在平台仓内**物理存在**能力仓树。

**方案 V1（推荐）**：能力仓以 git submodule 挂在 `平台仓/packages/`，**钉 tag**，且
**能力仓仓根直挂 crate**（`wasmnest-lib/bedcode-wasm-core/…`，不留内层 `packages/`）。
mount 后路径为 `平台仓/packages/bedcode-wasm-core/…`，与现状**逐字相同** ⇒

- ✅ **平台侧 326 处 `../../packages/bedcode-*` 引用零改写**（T14），且多条防回接锁
  断言 needle 的正是这个字符串（`retired_mobile_wasm_core_fork_lock.rs` 等）——这是
  选 V1 的**决定性问题**；
- ✅ 平台仓 17 条 Cargo path 依赖（桌面 9 + 移动 8）零改写，`common-host` feature 照旧；
- ⚠️ 能力仓**内部**有约 16 处路径敏感点需机械下调一层（T15）：
  6 处 `repo_root()` / `crate_root()` 派生、5 处 `.join("packages/<crate>/…")`、5 处字符串常量路径，
  以及 `contract.rs` 的 `include!`（见 E5）。其中指向桌面 SDK 的 4 处本就是 E2b 的跨仓读取。

**方案 V2（不推荐）**：保留能力仓内层 `packages/`，平台侧改依赖路径为
`../../packages/packages/<crate>`。代价是平台侧 326 处引用 + 多条锁 needle 全部要改，
远超 V1 的能力仓内部 ~16 处。**据此裁定 V1。**

### E2 能力仓 → 平台仓的反向耦合（L2 的解耦对象）

分两组，性质不同：

#### E2a 构建期边（唯一一条）

`packages/bedcode-wasm-core/Cargo.toml:107` 以 path 依赖桌面 SDK，且 `default = ["desktop-assembly"]`（T5）。
能力仓单克隆时该路径不存在 ⇒ 能力仓 `cargo build` 直接失败（T6 实测为硬错误）。

| 方案 | 做法 | 收益 | 代价 |
| --- | --- | --- | --- |
| **A（终态，推荐）** | 把 `desktop-assembly` 面（`PluginHost` / `api_bridge` / `watcher` / `intercall` / `auth_center` / 能力域端口装配 / WS 帧回灌窄端口）**整体迁入桌面宿主**，能力仓 `default = []` | 能力仓零外依赖；与 ADR 0049（绑定归位）/ 0052 D4（端独有实现住端宿主）方向一致 | 较大重构，需独立任务 + ABI/双端影响评估 |
| **B（过渡）** | wasm-core 对该 SDK 的依赖由 path 改**已发布版本**（`bedcode-plugin-api = { version = "…", optional = true }`），平台仓在桌面 `src-tauri/Cargo.toml`（其 workspace 根）用 `[patch.crates-io]` 指回本地 SDK | 一行改动解锁能力仓单克隆 | 内核出现反向依赖（与 ADR 0047/0049 相悖）；两仓版本需人工对齐 |
| C | 保留 path，要求三仓永远同场 | 零改动 | 能力仓不可单克隆 = 放弃 L2 |

#### E2b 测试期读取边（L2 必须归位的 6 处治理锁）

能力仓内的治理锁在**测试期读平台仓/应用仓源文件**（T13）。这些锁的**主题**本来就属平台侧，
因此 L2 时它们是**迁往平台仓**而非改写：

| # | 现位置 | 读的对象 | 主题（谁的契约） | L2 归位 |
| --- | --- | --- | --- | --- |
| 1 | `packages/bedcode-server-base/src/wire/drift_lock.rs` | 桌面 SDK `rust/src` | `BusMessage` 副本 ↔ SDK 原版逐字一致 | ① 平台仓（跨 SDK 的契约锁） |
| 2 | `packages/bedcode-server-websocket/src/wire/drift_lock.rs` | 桌面 SDK `rust/src` | 同上（ws 分支） | ① 平台仓 |
| 3 | `packages/bedcode-server-http/src/wire/drift_lock.rs` | 桌面 SDK `rust/src` | 同上（http 分支） | ① 平台仓 |
| 4 | `packages/bedcode-ws-client-engine/src/wire/drift_lock.rs` | 移动 SDK `rust/src` | 同上（移动 ws 分支） | ① 平台仓 |
| 5 | `packages/bedcode-wasm-core/src/permission.rs`（`sdk_root()` 测试） | 桌面 SDK 三份生成物 + 桌面 `wasm-apps/*` manifest | 权限词汇三副本一致 + manifest 词汇校验 | ① 平台仓 |
| 6 | `packages/bedcode-file-transfer-core/src/wiring_lock.rs` | 两端 SDK `compose.json` + `cap-desktop.wit` + 双端 file-transfer app 树 | 单产物跨端接线（ADR 0050） | ① 平台仓（且该 crate 本身迁 ③，锁留 ① 的 cross-end 面） |

**注意**：L2 之后能力仓剩下的锁必须是**只读仓内真源**的（例：`mdns_browse_singleton_lock.rs`
的 `packages/<crate>/src` 白名单、`boundary_lock.rs` 的 crate 边界、`headless-host-probe`
的形态门禁），这类锁在 V1 布局下改路径前缀即可。

### E3 SDK 归属 —— 留在平台仓（端内包），不迁能力仓 / 应用仓

理由：SDK 是**端内包**（T4，路径 `<end>/packages/plugin-sdk-*/`），其 `compose.json` 同时引用
能力仓 WIT 真源与**本端 cap 分片**（`wit-src/cap-desktop.wit` / `cap-mobile.wit`），而
ADR 0052 D4 定「端独有功能的声明与实现都住端宿主」⇒ SDK 随端走。用户指令里的
「crate 能力面」指 `packages/bedcode-*` 机制 crate，**不含 SDK**。

由此得到的应用仓契约：**app 只认 SDK 的发布版本 + WIT bundle + ABI 版本**，不认任何宿主源码。
（L1 阶段可不切版本、用 `{ path, version }` 指兄弟目录；L2 才切成 registry 版本。）

### E4 跨端测试归属 —— 并入平台仓

`cross-end-tests/`（T8）同进程 path 依赖两端 `src-tauri`，端点同仓后这条依赖退化为**仓内相对路径**，
比旧案（跨端仓钉两端 tag）更简单。它对能力仓的直读（`packages/bedcode-server-base`）经 E1
的 submodule 路径原样成立。**Seam 1（三仓装配）由它 + 平台仓 release 流程共同承担。**

### E5 `contracts/` 与 WIT 真源 —— 放能力仓，`include!` 相对深度必须下调一层

`contracts/bedcode-plugin-package.rs` 被两处消费（T7）。核对两种形态的相对深度：

| 形态 | `contract.rs` 所在 | `include!("../../../contracts/…")` 解析为 | 需要改成 |
| --- | --- | --- | --- |
| 现状（crate 在 `<root>/packages/` 下） | `<root>/packages/bedcode-wasm-core/src/` | `<root>/contracts` ✔ | — |
| 新·能力仓单克隆（crate 在仓根） | `wasmnest-lib/bedcode-wasm-core/src/` | `wasmnest-lib` 的**上一级**/contracts ✗ | **`../../contracts`** |
| 新·submodule mount 后 | `平台仓/packages/bedcode-wasm-core/src/` | `平台仓/contracts` ✗ | **`../../contracts`** |

⇒ 两种新形态都正确值是 `../../contracts`，**一次改动、两档通用**；对应地，
`compose-wit.mjs` 里按仓根拼 `contracts/…` 与 `packages/<crate>/wit/…` 的常量也要跟着调整
（见 E6：该脚本放平台仓，路径其实与现状一致，故**它不用改**，只有内核那一行要改）。

### E6 脚本与 CI 归属（按「服务对象」拆，不按「历史位置」拆）

| 脚本 / workflow | 归属 | 理由 |
| --- | --- | --- |
| `compose-wit.mjs`（含 `--check`） | **① 平台仓（唯一一份）** | 它要同时看 WIT 真源（经 submodule，路径与现状一致）与**两端 cap 分片 + SDK 输出路径**；放平台仓则**路径常量零改动**。能力仓只保留一条**仓内** contracts↔crate 一致性锁 |
| `package-sdks.mjs` | ① 平台仓 | 产物属于该端 SDK |
| `package-plugins.mjs` / `plugin-package-list.json` | ③ 应用仓 | 产物属于各 app |
| `plugin-wasm-config.mjs` / `replace-console.mjs` | ③ 应用仓 | 只服务 app 构建 |
| `bump-version.mjs` / release manifest 生成 | ① 平台仓 | 跨仓版本一致性 |
| `adaptive-run.mjs` / `build-profile.mjs` / `wasip3-toolchain.sh` | ① 平台仓（真源）+ 三仓引用 | 构建资源治理与工具链必须全仓同版 |
| `audit-rust-tests.mjs` / `split-rust-tests.mjs` / `run-test-split.sh` | 各仓一份（逻辑同源） | 测试组织纪律各仓自跑 |
| `doc-tracking.sh` / `.husky/` | 各仓一份 | 分支级文档跟踪策略各仓各自生效 |
| `check-target-size.js` / `dev-run.js` | 各仓 | 本仓构建入口 |
| `lint.yml` | ① + ③ | 有前端才 lint |
| `test.yml` | **三仓各一份** | 各仓 `cargo test` / `pnpm test:run` 按仓触发 |
| `wit-drift-lock.yml` | **① 平台仓**（真源经 submodule ↔ 两端 SDK 生成物） | 两侧文件都只在平台仓可见 |
| `release.yml` | ① 平台仓（编排）+ ③ 应用仓（产物 job） | 端发布与 app 产物解耦 |
| `sdk-publish.yml` | ① 平台仓 | SDK 住在平台仓 |

---

## 6. release manifest（跨仓版本的唯一表达，保留旧案 D7）

```jsonc
{
  "product": "3.1.0",                                  // 平台仓 tag
  "repos": {
    "lib":  { "tag": "lib-v0.3.0" },                    // ② 能力仓（= 平台仓 submodule 指针）
    "apps": { "tag": "apps-v1.2.0" }                    // ③ 应用仓（整体）
  },
  "contract": { "package": "wasmnest:plugin", "version": "0.2.0", "witSha256": "…" },
  "abi": { "desktop": 38, "mobile": 21 },
  "apps": { "<plugin-id>": { "path": "apps/<slug>", "artifact": "…" } }
}
```

- **谁写**：平台仓发布流程；**谁读**：Seam 1 装配校验、各仓 release workflow（校验自己在清单里）。
- **插件清单合并**：原 `scripts/plugin-package-list.json` 并入 `apps` 段。
- **一致性判据**：CI 校验 `平台仓` 的 submodule 指针 == manifest 的 `lib.tag`（防指针漂移）。

---

## 7. 门禁归属（按 L1 / L2 分列）

| 门禁 | 现落点 | L1 落点 | L2 落点 |
| --- | --- | --- | --- |
| WIT 真源 ↔ 两端 SDK 生成物（11 份逐字一致） | 桌面测试（硬编码移动路径） | ① 平台仓（真源在 submodule 下，同场可读） | ① 平台仓（不变） |
| WIT 拼装漂移（`check:wit`） | 根脚本 + husky + CI | ① 平台仓 | ① 平台仓 |
| contracts ↔ crate 版本一致 | 内核测试 | ① / ② 均可（同场） | **② 能力仓**（仓内自足） |
| 内核纯净性（零端名 / 零端词汇） | 移动测试 | ① 平台仓 | **② 能力仓**（7c 禁用词表静态化） |
| 能力域 crate 纯净性（`headless-host-probe`） | `packages/` | ② 能力仓 | ② 能力仓 |
| wire 漂移锁（4 处，读对方 SDK） | `packages/*/src/wire/drift_lock.rs` | 原位（submodule 下路径仍成立） | **迁 ① 平台仓**（E2b 1–4） |
| 权限词汇三副本 + manifest 校验 | `bedcode-wasm-core/src/permission.rs` 测试 | 原位 | **迁 ① 平台仓**（E2b 5） |
| file-transfer 单产物跨端接线锁 | `file-transfer-core/src/wiring_lock.rs` | 随 crate 迁 ③（同场可读） | **迁 ① 平台仓 cross-end 面**（E2b 6） |
| 宿主侧无业务代码（反回接 B1–B6） | 各自端测试 | ① 平台仓 | ① 平台仓 |
| 壳约束锁（新界面不接旧目录） | 移动测试 | ① 平台仓 | ① 平台仓 |
| 双端单产物探针（移动宿主加载桌面产物） | 移动测试 | ① 平台仓 `cross-end-tests` | ① 平台仓（输入来自 ③ 构建输出） |
| 跨端真实互连（零 mock） | `cross-end-tests` | ① 平台仓（同仓 path 依赖） | ① 平台仓 |
| app 零宿主源码依赖 | — | 不适用 | **③ 应用仓（新增）** |
| 三仓装配（Seam 1） | — | **① 平台仓（新增）** | **① 平台仓（新增）** |

---

## 8. 迁移顺序（沿用「先改名、后拆分」；拆分重切为三步）

### 前置（不变，旧案 D9 闸门保留）

| 闸门 | 内容 |
| --- | --- |
| P0 | 发布最后一个 BedCode 版本 3.0.0（`CHANGELOG` 双语段落 + `v3.0.0` tag + draft 转正式） |
| P1 | `dev` 提交干净（零未提交改动，否则改名 diff 无法审查/回退） |
| P2 | 开 `feature/<platform>-rename`，`dev` 留作回退点 |
| **步骤 1** | **在单仓内完成全部名称替换**（规则见 `../2026-10-15-wasmnest-repo-split/naming-rules.md`，含该文 §8 的追加项：`bedcode-desktop/`→`desktop/`、`bedcode-mobile/`→`mobile/`、应用目录 slug 改 `terminal-session`），用现成的单仓全量测试套件验证（两端 `cargo test` + 两端 `pnpm test:run` + `cross-end-tests` + eslint + `check:wit` + 全部防回接锁） |

### 拆分阶段

| 步 | 动作 | 中间态判据 |
| --- | --- | --- |
| **S1 能力仓** | 建 ②：把 `packages/**` 上移为仓根（**不保留历史**，按用户旧裁决）；补 `contracts/`、`headless-host-probe`、`.cargo/config.toml`、`.husky/`；机械下调 T15 的 ~16 处路径 + E5 的 `include!` | ② 单克隆 `cargo test`——**L1 下允许跨仓锁因同场而通过；L2 下要求全绿**（需先落 E2a + E2b） |
| **S2 平台仓** | 建 ①：`desktop/` + `mobile/` + `cross-end-tests/` + `docs/` + `brand/` + 脚本/CI/keystore；`packages/` 换为 ② 的 submodule（钉 tag） | ① 两端 `cargo test` + 两端 `pnpm test:run` + eslint + `check:wit` 全绿；平台侧 326 处路径**零改动**得到验证 |
| **S3 应用仓** | 建 ③：7 份 wasm-app 归并到 `apps/<slug>/{desktop,mobile}`；`file-transfer-core` 迁入 `apps/file-transfer/rust/`；`componentize` 归平台仓 SDK 工具面；6 处 SDK path 依赖按档处理（L1 保留 `{path,version}` 指兄弟目录 / L2 切发布版本） | ③ 单克隆（L2）或不拉 ①②（L1 需兄弟目录）跑通全部 app 的 `vitest` + `bedcode-plugin build` |
| **S4 装配与冻结** | ① 补 `release-manifest.json` + Seam 1 装配校验 + 可复用 workflow；submodule 指针 == manifest 校验；冻结旧仓（一次性停写） | Seam 1：按 manifest 从空目录拉起三仓 → 构建两端 → 真实互连 + 加载全部 app 产物 |

---

## 9. 验证（Seam）

| 缝 | 落点 | 判据 |
| --- | --- | --- |
| **Seam 1（主）三仓装配** | ① 平台仓 | 空目录 → 按 release manifest 拉三仓 → 构建两端 → 跑真实互连 + 加载全部 app 产物。回答「这套版本组合能不能装起来」 |
| **Seam 2 单仓自洽** | ② / ③（仅 L2） | ② 能力仓 `cargo test`（含纯净性门禁、无跨仓读取）；③ 应用仓零宿主源码依赖跑通 app 测试与构建 |
| **Seam 3 锁不空转** | ① / ② / ③ | 迁仓后每条防回接锁 / 漂移锁**跑红一次**（变异自检），证明没因路径失效退化成空转。**这是拆仓最大的静默失效面**（旧案 §4 已认定） |

**什么算好测试**：只断言外部行为（能不能装起来 / 单仓能不能自绿 / 改 A 锁会不会红），
不断言实现细节（不写「某文件里必须出现某行」这类脆断言，除非该行本身是契约声明——
现有防回接锁里大量 needle 正是此类**契约声明**，故 L1 下它们必须保持有效）。

---

## 10. 裁决记录（2026-10-16 用户裁决：按建议默认**全部采纳**）

下表 11 条已**定案**，实施时不得自行偏离；确需改动须新开一条裁决并记入 §12。

| # | 议题 | **裁决** | 落地影响 |
| --- | --- | --- | --- |
| 1 | **隔离档位**：L1 还是直接冲 L2 | **先 L1、后 L2**（L1 即满足「三个仓」的权限/CI/发版收益） | 本轮实施只做 L1；L2 拆三张后续票（E2a / E2b / E3） |
| 2 | E2a 解法：先 B（registry + patch）还是一步到位 A（迁 `desktop-assembly`） | **先 B 后 A**（B 一行解锁；A 单独立任务清偿架构债） | B 是 L2 的前置；A 只登记不排期 |
| 3 | 能力仓 crate 是否保留内层 `packages/` 层级 | **不保留**（仓根直挂，方案 V1） | 平台侧 326 处路径零改动；能力仓内部 ~16 处下调一层 |
| 4 | 平台仓内是否把 `bedcode-desktop/` 改名为 `desktop/` | **改**（`wasmnest/desktop` 比 `wasmnest/wasmnest-desktop` 清晰） | 该改名**并入前置的改名阶段**（步骤 1）一并完成，拆分阶段不再改字面量 |
| 5 | `cross-end-tests/` 并入 ① 而非独立 | **并入 ①**（三仓约束下无第四条边，且它依赖两端源码） | 无独立跨端仓 |
| 6 | 文档（48 ADR / CONTEXT / knowledge）留 ① 还是迁 ② | **留 ①**（无第三方消费者；避免第四个仓） | ① 同时是「平台仓 + 产品文档仓」 |
| 7 | E2b 的 6 处跨仓锁：迁 ① 还是留在 ② 改写为只读仓内副本 | **迁 ①**（锁的主题属平台契约） | L2 时执行；L1 下原位保留（submodule 路径仍成立） |
| 8 | 三仓是否公开 | **先全私有**；「SDK 公开 / 能力仓公开」留作后续独立决策 | 不阻碍后续公开 |
| 9 | SDK 已发布名是否随改名新开 `wasmnest-*` | **新开新名 + 旧包 yank/弃用**（沿用旧案 §8-1） | 已发布名不可回收；影响 app 依赖串 |
| 10 | 三仓共享 CI 是否抽可复用 workflow | **抽**（`workflow_call`） | 否则 5 份 YAML 复制在三仓 |
| 11 | 能力仓在平台仓的钉法：tag 还是 branch | **tag**（submodule 指针 + CI 校验 == manifest） | 钉 branch 会让平台仓构建随能力仓 HEAD 漂移 |

**由本裁决直接锁定的实施边界**：

- 本轮只建三仓 + submodule 接线 + 路径修正（L1），**不做** E2a 重构、**不迁**跨仓锁（E2b）、
  **不切** app SDK 依赖版本（E3）——后三项各自立票。
- 三仓**全私有**，`AGENTS.md` 每仓一份（共同规范引用平台仓）。
- 平台仓目录名 `desktop/` / `mobile/` 在**改名阶段**定下，拆分阶段只搬不改名。
- 本方案仍为**文档阶段**：不实施任何搬迁，等前置闸门（P0 发 3.0.0 / P1 dev 干净 / P2 开改名分支 / 步骤 1 改名）完成后才进 S1–S4。

---

## 11. 风险与缓解

| 风险 | 触发条件 | 缓解 |
| --- | --- | --- |
| 跨仓锁因路径失效**静默空转** | 迁仓后锁读不到文件却未 panic / 断言恒真 | Seam 3：每条锁**跑红一次**；锁读文件失败必须 panic（现有 drift_lock 已用 `unwrap_or_else(panic!)`，保持该形态） |
| 平台侧 326 处路径被误改 | 采用 V2 或随手改依赖路径 | 裁定 V1；把「平台侧路径零改动」写成 S2 的中间态判据 |
| E5 `include!` 深度漏改 ⇒ 内核编译失败 | 只改了依赖路径 | 一次改动两档通用（§5-E5 表）；内核 `cargo test` 会立即红 |
| submodule 指针漂移 | 只 `git pull` 未 `submodule update` | 钉 tag + CI 校验指针 == release manifest |
| app 产物与宿主契约版本不匹配 | ③ 与 ① 各自演进、无装配校验 | Seam 1 + 宿主加载期「版本不匹配」人话报错（旧案 User Story 18） |
| L1 的「三仓同场」被误当成 L2 交付 | 未明确档位就宣称拆分完成 | §4 明列档位；S1/S3 判据按档标注 |
| 文档引用在拆仓后失准 | ADR / knowledge 里大量 `packages/…`、`bedcode-desktop/…` 路径 | 命名阶段已机械映射；拆仓后按 §3 映射表补一次文档路径复核 |

---

## 12. 实施记录

- **2026-10-16**：用户指令改为三仓切法（平台 / 能力 / 应用）。本文件取代旧案
  `.scratch/2026-10-15-wasmnest-repo-split/spec.md` 的仓数结论（九仓 → 三仓）；
  旧案 `naming-rules.md` 与「先改名、后拆分」顺序继续有效。
- **2026-10-16 关键实测**：
  1. Cargo 在 manifest 装载期即要求所有 path 依赖路径存在（`/tmp/cargotest` 最小复现）
     ⇒ 跨仓相对路径方案不成立，必须 submodule/git dep/registry 三选一；
  2. 据此裁定 mount 方案 **V1**（能力仓仓根直挂 crate，挂到 `平台仓/packages/`），
     使**平台侧 326 处路径引用零改动**（含多条防回接锁的 needle）；
  3. 定位到能力仓对平台仓的**两组**反向耦合：构建期 1 条边（E2a）+ 测试期 6 处治理锁（E2b），
     并定位到能力仓内部 ~16 处路径敏感点（T15）与唯一的 `include!` 深度点（E5）。
- **2026-10-16 档位裁定**：默认 **L1 组装式**起步，L2 契约式拆为三张后续票（E2a / E2b / E3）。
- **2026-10-16 裁决锁定**：用户裁决「按建议裁决、只写文档」——§10 十一条全部按默认建议采纳
  （档位 L1 起步、E2a 先 B 后 A、能力仓仓根直挂 V1、平台仓目录 `desktop/`/`mobile/`、跨端测试并入
  ①、文档留 ①、6 处跨仓锁 L2 迁 ①、三仓全私有、SDK 新开名、CI 抽 `workflow_call`、submodule 钉 tag），
  并写入 §10 的「实施边界」。**本轮仍不实施任何搬迁**；实施待 §8 前置闸门（P0 发 3.0.0 →
  P1 dev 干净 → P2 开改名分支 → 步骤 1 单仓改名）完成后才进 S1–S4。
