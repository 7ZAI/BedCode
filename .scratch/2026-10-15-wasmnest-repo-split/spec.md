# WasmNest 平台改名 + 多仓拆分（lib / 两端 / app / 跨端 / 元仓）

> 用户指令（2026-10-15）：
> 1. 新建 `WasmNest` 目录与多个 git 仓库，把 BedCode 下的内容拆分进入新目录与新仓库；
> 2. 整体仓库 BedCode 全部改名为 WasmNest；
> 3. **例外**：BedCode wasm-app 保留原名不覆盖——BedCode 退化成一个 wasm-app 应用，对应现在的
>    `terminal-session` wasm-app，改名为 BedCode；
> 4. 不需要保留 git 历史。
> 5. **执行顺序（2026-10-15 追加）**：这一切在 BedCode 发布**最后一个版本 3.0.0** 之后进行；
>    先把 `dev` 提交干净，**在 BedCode 单仓内完成全部名称替换**，然后再拆分迁移。

Status: **文档阶段（未实施）**（2026-10-15 立项）——用户裁决：**当前不实施，只写文档**。
落地顺序见 §D9（**先改名、后拆分**），待开工时逐阶段执行。

**配套文档（同一任务目录）**

| 文件 | 定位 |
| --- | --- |
| `naming-rules.md` | 改名规则单一事实源：三层模型 / 替换规则集 / 豁免清单 / 连带动作 / 验证判据 |
| `repo-map.md` | 仓地图与依赖契约：各仓内容清单 / 版本钉法 / 门禁与脚本归属 / 易漏项清单 |

---

## 0. 事实基线（2026-10-15 代码走读，逐条可复现）

| # | 事实 | 证据 |
| --- | --- | --- |
| F1 | 当前是单仓五类内容：双端宿主 + lib（`packages/**`）+ wasm-app + 跨端测试 + 产品文档；**无根 Cargo workspace**，各 crate 各自为根 | 根目录；`BedCode.code-workspace` 头注「仓库根无 Cargo workspace」 |
| F2 | lib 层 **18 个 crate**，全部 path 依赖，未发布 crates.io；仅两个 SDK 有发布通道 | `packages/*/Cargo.toml` 无 `publish = false` 亦无 registry 发布；`sdk-publish.yml` |
| F3 | lib 内部是 **4 层串行依赖链**，`bedcode-wasm-core` 是汇聚点（依赖 11 个同族 crate） | 依赖矩阵：leaf（link-crypto / peer-net / host-api-core / host-kit）→ server-base → server-core/http/websocket/peer-net + pty/discovery/crypto-engine → wasm-core → 两端 |
| F4 | WIT 真源 + 契约版本锁**跨 7 个 crate / 11 个 WIT 文件**（core + 6 个能力域分片 + 4 个端 cap） | `bedcode-desktop/src-tauri/tests/wit_package_version_lock.rs::WIT_SOURCES`；两端 `compose.json` |
| F5 | 契约包版本真源 = 仓根 `contracts/bedcode-plugin-package.rs`（`PACKAGE_VERSION = "0.2.0"`）；**版本进 import 名，宿主与 guest 版本不等即实例化失败** | 该文件头注（ADR 0051 D3/D4） |
| F6 | 双端 ABI **已分叉**：桌面 38 / 移动 21 | 两端 `compose.json` 的 `abi.version` |
| F7 | SDK 已走独立发布通道（tag `sdk-v0.1.0`，版本已到 0.1.1），且**已内置双模式**：默认相对路径 / `--registry` | `git tag`；`docs/knowledge/sdk-publish.md` |
| F8 | **全部 6 个 wasm-app 的生产依赖只有 SDK**；唯一例外 = file-transfer 依赖 `bedcode-file-transfer-core` | 各 app `rust/Cargo.toml`；`terminal-session` 的 4 个 `server-*` **只在 `[dev-dependencies]`**（D10 跨端契约测试） |
| F9 | 双端同名 app 共用同一 plugin id，但**桌面目录名与移动不一致、显示名也不一致** | 桌面 `terminal-session`（name「Terminal Session Center」）vs 移动 `bedcode`（name「BedCode」），id 同为 `com.bedcode.terminal-session` |
| F10 | 跨端测试**同进程 path 依赖两端 `src-tauri`**（零 mock 真实互连） | `cross-end-tests/Cargo.toml` |
| F11 | 移动 file-transfer 的 Rust 源在**桌面树**（`sharedRustManifest` 指桌面 manifest）；移动宿主有探针验证「加载桌面产物 + 两端同字节」 | 移动 app `package.json`；`bedcode-mobile/src-tauri/tests/shared_artifact_probe.rs` |
| F12 | 跨仓路径引用散落在 **6 个桌面测试 + 6 个移动测试**（锁与探针读对方目录） | `rg 'bedcode-(mobile\|desktop)/\|"packages/'` |
| F13 | 品牌资产已设计完成待落地，其 §13 原则是「**先换视觉资产，不动标识符与包名**」——该原则的**前提**是「只换品牌」，与本次「平台改名 + 拆仓」冲突 | `.scratch/2026-10-14-wasmnest-brand/brand-spec.md` §13 |
| F14 | 项目处于研发起步期，**不承担任何向后兼容义务** | AGENTS.md §0 |

### 0.1 app 矩阵（双端归属）

| plugin id | 桌面目录 | 移动目录 | 目标 app 仓 |
| --- | --- | --- | --- |
| `com.bedcode.terminal-session` | `terminal-session` | `bedcode` | `wasmnest-app-bedcode`（**保留 BedCode 名**） |
| `com.bedcode.file-transfer` | `file-transfer` | `file-transfer` | `wasmnest-app-file-transfer` |
| `com.bedcode.ai-chatbox` | `ai-chatbox` | `ai-chatbox` | `wasmnest-app-ai-chatbox` |
| `com.bedcode.agent-hub` | `agent-hub` | — | `wasmnest-app-agent-hub`（桌面独有） |

---

## 1. Problem Statement

平台正在从「一个应用」长成「一个宿主 + 一套机制 + 一批应用」。当前单仓结构把这个演进方向卡住了：

- **两端无法独立发版**：一个 `v*` tag 同时出 Windows / Linux / Android + 全部插件产物 + 全部 SDK，桌面未完成的改动会挡住移动端发版（`release.yml` 单 tag 驱动全部 job）。
- **权限无法按端/按应用隔离**：GitHub 权限是仓级的，单仓里做不到「只给某人移动端 + 某个 app 的写权限」。
- **CI 与克隆体积随应用数线性膨胀**：`.git` 已 348M，两端本地 target 合计 60G+；每新增一个 app，两端 CI 都要为它付出代价。
- **产品名与平台名混用**：`BedCode` 同时指平台、指双端标识符（`com.bedcode.*`）、又指移动端那个终端应用（其显示名就叫 BedCode），语义已经无法自洽。
- **未来的主战场是「做不同的 wasm app」**，而 app 目前长在两端树里，桌面 app 与移动 app 目录名、构建方式、共享业务核的挂法（`sharedRustManifest` 指到另一端的树）都不统一。

---

## 2. Solution

把 `BedCode` 平台改名为 `WasmNest`，并按**依赖方向**拆成独立仓库；`BedCode` 这个名字**降级为其中一个 wasm-app 的名字**（现 `terminal-session` 应用，双端同一个 plugin id）。

最终形态 = **一个不装代码的元仓 + 一层 lib + 两个端 + 每 app 一仓 + 一个跨端测试仓**，本地通过「兄弟目录工作区」组装起来，跨仓版本关系由元仓的一份 **release manifest** 唯一表达。

```
~/project/tauriProject/WasmNest/     ← ① 本地工作区根（磁盘目录，不是 git 仓）
  wasmnest.code-workspace            ←    多根工作区文件（现 BedCode.code-workspace 的升级版）
  wasmnest/                          ← ② 元仓：ADR / 领域模型 / CHANGELOG / 命令 / 跨仓任务 /
                                          品牌资产 / release manifest / 跨仓装配校验（零代码）
  wasmnest-lib/                      ← ③ 机制内核 + 契约 + WIT 真源（16 crate workspace）
  wasmnest-desktop/                  ← ④ 桌面宿主 + 前端 + 桌面 SDK + 端 cap 分片
  wasmnest-mobile/                   ← ⑤ 移动宿主 + 前端 + 移动 SDK + 端 cap + 签名库
  wasmnest-cross-end/                ← ⑥ 跨端真实互连测试
  wasmnest-app-bedcode/              ← ⑦ 每 app 一仓（本仓保留 BedCode 名）
  wasmnest-app-file-transfer/
  wasmnest-app-ai-chatbox/
  wasmnest-app-agent-hub/
```

**每个仓可独立克隆、独立测试、独立发版**；需要联调时按兄弟目录约定落位，用 `{ path, version }` 双声明（本地走 path，发布走 registry 版本）。

---

## 3. User Stories

### 平台与命名

1. 作为平台维护者，我想要把平台名从 BedCode 改成 WasmNest，以便产品名不再同时指代平台、双端标识符和某一个应用。
2. 作为平台维护者，我想要 `BedCode` 这个名字降级为终端会话应用的显示名，以便保住这个已经用了很久、对用户有意义的名字。
3. 作为平台维护者，我想要桌面与移动的同一个应用（`com.bedcode.terminal-session`）在两个仓里目录名与显示名一致，以便不再出现「同一个应用两边叫不同名字」的认知负担。
4. 作为新加入的开发者，我想要一份说明「哪个名字属于品牌层、哪个属于身份层、哪个不可回收」的规则，以便知道改哪个、不改哪个。
5. 作为平台维护者，我想要品牌资产的落地（图标 / 闪屏 / 标题栏）与本次改名**解耦**，以便两者可以分别推进、分别回滚。

### 独立发版

6. 作为桌面端负责人，我想要只推 `desktop-v*` tag 就发布桌面版，以便移动端未完成的改动不再阻塞桌面发版。
7. 作为移动端负责人，我想要只推 `mobile-v*` tag 就发布 Android 版，以便两端版本号彻底解耦。
8. 作为 lib 维护者，我想要 lib 有独立的版本线与 tag，以便两端可以分别钉不同的 lib 版本。
9. 作为 app 作者，我想要每个 app 有自己的版本号与 tag，以便单个应用可以独立迭代与独立回滚。
10. 作为发布负责人，我想要一份 release manifest 记录「某次产品发布由哪些仓的哪个 tag 组成」，以便回答「WasmNest 3.1.0 到底装了什么」。
11. 作为发布负责人，我想要发布流程能按 manifest 校验各仓 tag 的实际内容与声明一致，以便防止「清单写 A、实际发 B」。

### 依赖与联调

12. 作为 app 开发者，我想要在只克隆 app 仓的情况下跑通全部单测与前端测试，以便不必把整个平台拉下来就能改一个应用。
13. 作为 app 开发者，我想要 app 默认依赖已发布的 SDK 版本，以便 app 的构建不依赖任何兄弟仓的本地存在。
14. 作为正在改 WIT 的开发者，我想要能把 app 的 SDK 依赖临时指向本地路径，以便改完契约立刻在 app 里验证，不必先发版。
15. 作为正在改共享业务核的开发者，我想要 app 仓内部就持有它的 Rust 业务核，以便不再跨仓引用另一端的构建清单。
16. 作为 lib 维护者，我想要两端以钉住的 tag（而非分支）引用 lib，以便 lib 的变更不会在两端构建时悄悄漂移。
17. 作为 lib 维护者，我想要一条「哪些 lib crate 允许被 guest 消费」的白名单，以便防止 app 悄悄依赖宿主侧传输层。
18. 作为 app 开发者，我想要 app 的产物声明它编译时所用的契约版本，以便宿主能给出「版本不匹配」的人话报错，而不是 wasmtime 的 `missing import`。

### 契约与门禁

19. 作为契约维护者，我想要 WIT 真源与全部 WIT 分片同处一仓，以便契约漂移锁能继续机械生效。
20. 作为契约维护者，我想要 lib 发布一个带哈希的 WIT bundle，以便端仓在拼装时能校验「我拼的是不是我钉的那个版本」。
21. 作为契约维护者，我想要端仓的 compose 只负责合并「lib bundle + 本端 cap 分片」，以便端仓不必 checkout 元仓或对方端仓。
22. 作为契约维护者，我想要内核纯净性判据（零端名 / 零端词汇）在 lib 仓内自跑，以便拆仓后这些判据不退化成空转。
23. 作为契约维护者，我想要一条跨仓装配校验，以便「两端 + lib + 全部 app 产物」这一整套组合在发布前被真实装起来过一次。
24. 作为安全负责人，我想要全部防回接锁在迁仓后仍在场且能跑红，以便拆仓不是「把锁留在旧仓」的静默降级。

### 权限与协作

25. 作为组织管理员，我想要按仓授予写权限，以便外包或合作者只能碰被指派的端或应用。
26. 作为组织管理员，我想要组织级 secrets（签名密钥 / 发布 token）一次配置、多仓复用，以便新增仓库不需要重配一遍。
27. 作为组织管理员，我想要共享的 CI 流程抽成可复用 workflow，以便新增仓库不必复制五份 YAML。
28. 作为贡献者，我想要每个仓有自己的 `AGENTS.md`（共同规范引用元仓），以便在任何仓里开工都拿到正确的路径基准与门禁。

### 本地工作区

29. 作为开发者，我想要一个多根工作区文件覆盖全部仓，以便一次打开就能跨仓跳转，且 rust-analyzer 不会因单实例索引全部 Cargo.toml 而 OOM。
30. 作为开发者，我想要一条命令把「兄弟目录布局」建好（拉仓 / 落位 / 建立本地链接），以便新人第一天就能跑起来。
31. 作为开发者，我想要跨仓任务（如 ABI bump、改名）有一个统一的落点，以便它不属于任何单一仓却仍可被追踪。

### 迁移

32. 作为迁移执行者，我想要迁移全程旧仓保持可构建，以便任何一步出问题都能停下并回退到旧结构。
33. 作为迁移执行者，我想要按「先底座后消费方」的顺序逐仓成形，以便每一步都有可验证的中间态。
34. 作为迁移执行者，我想要一次性冻结旧仓，以便不存在「两边都在改」的双写窗口。

---

## 4. Testing Decisions

**先确认这三个缝（skill 要求）**——`Seam 1` 为主缝且复用现成资产，`Seam 2`/`Seam 3` 覆盖两个不同失效面：

| 缝 | 落点 | 判据 | 复用 |
| --- | --- | --- | --- |
| **Seam 1（主，最高）跨仓装配** | 元仓 → 空目录按 release manifest 拉全仓 → 构建两端 → 跑真实互连 + 加载全部 app 产物 | 「这套版本组合能不能装起来」 | `cross-end-tests`（零 mock 真实互连）+ `shared_artifact_probe`（移动宿主加载桌面产物） |
| **Seam 2 单仓自洽** | 每个仓单独 clone 后跑自己的全量测试 | 「app 仓零兄弟仓依赖」 | 各 app 现有 `vitest` / crate 单测 |
| **Seam 3 锁的在场与不空转** | 迁移后逐条防回接锁/漂移锁**跑红一次**（变异自检） | 「锁没退化成空转」 | 现有全部 lock 测试 |

**什么算好测试**：只断言外部行为——「能不能装起来」「单仓能不能自绿」「改了 A 之后锁会不会红」，不断言实现细节（不写「某个文件里必须出现某行」这类脆断言，除非该行本身是契约声明）。

**要测的模块**：跨仓装配校验（新，元仓）、WIT bundle 哈希校验（新，lib + 端仓）、app 产物与契约版本校验（新，端仓加载期）、以及全部现有 lock 测试的迁仓版本。

**Prior art**：`cross-end-tests`（同进程零 mock 真实互连）、`shared_artifact_probe.rs`（跨端单产物加载 + 同字节断言）、`mobile_wasm_core_purity_lock.rs`（七判据 + 变异自检记录）、`wit_package_version_lock.rs`（真源 + 散落副本的逐字比对）。

---

## 5. Implementation Decisions

### D1 仓清单与职责（6 + N 仓）

| 仓 | 内容 | 可见性 |
| --- | --- | --- |
| `wasmnest`（元仓） | ADR、领域模型（CONTEXT）、双语 CHANGELOG、README（产品门面）、命令参考、领域知识、跨仓任务 `.scratch/`、品牌资产、release manifest、跨仓装配校验、工作区文件 | 私有 |
| `wasmnest-lib` | 16 个 crate（18 − 2 归位）、`contracts/`、WIT 真源、lib 侧 compose 脚本、`headless-host-probe` | 待裁决（§8-2） |
| `wasmnest-desktop` | 桌面宿主 + 前端 + 桌面 SDK + 端 cap 分片 + 端侧 compose | 私有 |
| `wasmnest-mobile` | 移动宿主 + 前端 + 移动 SDK + 端 cap 分片 + 端侧 compose + 签名库 | 私有 |
| `wasmnest-cross-end` | 跨端真实互连测试 | 私有 |
| `wasmnest-app-*` × 4 | 每个 app 的双端（或单端）产物、前端、manifest、自有测试 | 私有 |

**元仓零代码**（只装文档、矩阵、脚本、任务、清单），因此永远不会出现「第二份实现」。

### D2 两个 crate 归位（不是「独立管理」）

- `bedcode-file-transfer-core` → 迁入 file-transfer app 仓：它只有 1 个消费者（该 app 的 Rust 侧），是应用私有逻辑而非平台机制。
- `bedcode-componentize` → 迁入桌面 SDK 的构建工具面：它只有 1 个消费者，是打包工具，不在运行时依赖图上。

其余 16 个 crate **保持一个大库**。判据见 §0 F3/F4：lib 是 4 层串行链且 WIT 版本锁跨 7 个 crate，「每 crate 一仓」会让一行修复变成 5 跳串行发布，并让漂移锁无法实现。

### D3 命名三层模型（全局 `bedcode` → `wasmnest`，唯一例外 = BedCode 应用）

| 层 | 范围 | 处置 |
| --- | --- | --- |
| **品牌层** | 仓名、crate 名、rust lib 名、npm 包名、CLI 名、环境变量、文档与文案、`productName`、WIT 包名 | 机械替换为 `wasmnest` / `WasmNest` |
| **身份层** | 应用 `identifier`、plugin id、签名库文件名、内部数据路径 | **需连带迁移动作**（见 D4），默认全量替换 |
| **不可回收层** | 已发布到 crates.io / npm 的名字 | 不可改名，只能新开名 + 废弃旧包（§8-1 待裁决） |

**唯一例外**：BedCode 应用（现 `terminal-session`，双端同一 plugin id）的**仓名、目录名、plugin id、rust lib 名、显示名一律保留 `bedcode` / `BedCode`**。其余全部改为 `wasmnest` / `WasmNest`。

### D4 身份层改名的连带动作（不可只做字符串替换）

- 应用 `identifier` 变化 ⇒ 数据目录迁移（旧数据不自动搬，研发期接受丢失）；
- plugin id 变化 ⇒ 插件存储命名空间与权限记录键变化（旧记录成为孤儿）；
- 签名库改名 ⇒ 打包配置与 CI secret 引用同步改；
- WIT 包名变化 ⇒ **进 import 名 ⇒ 破坏性契约变更**，须双端同步 bump 并重建全部 app 产物（AGENTS §5.1.4 顺序）。

### D5 lib 与端仓的契约机制（拆仓后必须重建）

- **lib 侧**：以全部 WIT 真源（core + 6 个能力域分片）产出 **WIT bundle 产物**（含逐文件哈希与契约包版本），随 lib tag 发布。
- **端仓侧**：compose 只做「lib bundle + 本端 cap 分片」的合并，并校验 bundle 哈希；端仓**不 checkout 元仓、不 checkout 对方端仓**。compose 脚本随真源走（lib 侧一半、端侧一半），端仓的漂移锁因此可独立跑。
- **依赖机制**：端仓以钉住的 lib tag 引用 lib（git 依赖或 submodule，二选一，见 §8-3）；app 仓以 registry 版本引用 SDK，本地联调用 `{ path, version }` 双声明（本地走 path、发布走 version）。

### D6 app 仓内部布局与双端产物矩阵

每个 app 仓统一为：

```
wasmnest-app-<id>/
  rust/            ← 双端共享业务核（有则在此；file-transfer 的核从桌面树迁入）
  desktop/         ← 桌面侧前端 + manifest + 构建脚本
  mobile/          ← 移动侧前端 + manifest + 构建脚本
  README.md        ← 显式声明「本 app 的双端产物矩阵」
```

双端产物矩阵必须**显式声明**（哪些 world、哪些 target、哪端加载哪份产物、字节一致性判据），不得再靠跨端路径偷渡（现状 F11 的 `sharedRustManifest` 由此消灭）。

### D7 release manifest（跨仓版本的唯一表达）

形态（决策密集部分，来自本轮方案讨论）：

```jsonc
{
  "product": "3.1.0",              // 产品级版本（元仓 tag）
  "repos": {
    "lib":      { "tag": "lib-v0.3.0" },
    "desktop":  { "tag": "desktop-v3.1.0" },
    "mobile":   { "tag": "mobile-v3.0.4" },
    "crossEnd": { "tag": "crossend-v0.1.0" }
  },
  "contract": { "package": "wasmnest:plugin", "version": "0.2.0", "witSha256": "…" },
  "abi": { "desktop": 38, "mobile": 21 },
  "apps": { "<plugin-id>": { "tag": "app-<slug>-v1.0.0" } }
}
```

它是 Seam 1 的输入，也是「统一根」的现代替身——**没有它，没有任何地方能回答「WasmNest 3.1.0 由哪些版本组成」**。

### D8 跨仓门禁的重建映射

| 现锁 | 现落点 | 拆后落点 |
| --- | --- | --- |
| WIT 拼装漂移锁 | 根脚本 + husky + CI | lib 侧 bundle 生成 + 端侧合并校验（脚本随真源各持一半） |
| 契约包版本锁（11 个 WIT 真源逐字一致） | 桌面测试（硬编码移动路径） | **lib 仓内**（11 个真源全在 lib）+ 端仓校验 bundle 哈希 |
| 内核纯净性锁（零端名 / 零端词汇） | 移动测试（扫 lib 源码 + 移动 SDK 词汇） | 判据 7a/7b 进 lib 仓；7c 的禁用词表**静态化**（词表本身即判据，无需读移动 SDK） |
| 双端单产物探针 | 移动测试 | app 仓内（矩阵自证）+ 跨端仓保留「移动宿主加载桌面产物」 |
| 双端 app 目录锁 | 桌面测试（扫两端 app src） | 各仓自扫 |

### D9 迁移顺序（用户裁决 2026-10-15：**先改名，后拆分**）

#### 前置闸门（缺一不可）

| 闸门 | 内容 | 为什么不可省 |
| --- | --- | --- |
| **P0 · 发布最后一个 BedCode 版本 3.0.0** | 推 `v3.0.0` tag → `release.yml`（桌面 Windows/Linux + Android + 插件产物 + SDK）→ draft 转正式发布 | 这是 BedCode 名字下的最后一次发布；之后平台名与全部标识符都变 |
| **P1 · `dev` 提交干净** | 把在途改动（实测 68 项 + 未跟踪文件）全部认领并提交，工作区零未提交改动 | 改名会触及全仓 10063 处字面量；若工作区有在途改动，改名 diff 会与它们纠缠，**既无法审查也无法回退** |
| **P2 · 开改名分支** | `feature/wasmnest-rename`（AGENTS §11 分支约定） | `dev` 保持为回退点 |

#### P0 检查清单（发布 3.0.0 前，2026-10-15 实测的缺口）

| # | 项 | 现状 |
| --- | --- | --- |
| P0.1 | **CHANGELOG 写 `## [3.0.0]` 双语段落**（从 `## [Unreleased]` 汇总，AGENTS §2） | ❌ **缺**：`CHANGELOG.md` 只有 `## [Unreleased]`；提取不到版本段时 release body 会 fallback 成整个文件 |
| P0.2 | 版本号四处一致（两端 `tauri.conf.json` / `Cargo.toml` / `package.json`） | ✅ 均已是 3.0.0 |
| P0.3 | 合入 `uat` / `master`（`publish-release` job 要求 tag 是 `master` 祖先） | ⏳ 待做 |
| P0.4 | 推 `v3.0.0` tag → `release.yml`（桌面 + Android + 插件产物 + SDK） | ❌ tag 不存在（现有最高为 `v2.1.1`） |
| P0.5 | draft 转正式发布 | ⏳ 待做 |

流程细节以 `docs/knowledge/release-workflow.md` 为准，本表只标出实测缺口。

#### 阶段

1. **在 BedCode 单仓内完成全部名称替换**（规则见 `naming-rules.md`）
   —— 在单仓里做的收益是**用现成的完整测试套件验证**：两端 `cargo test` + 两端 `pnpm test:run` +
   `cross-end-tests` + 根 `eslint` + WIT 拼装漂移锁 + 全部防回接锁。全绿后合入 `dev`。
   **此步之后 BedCode 树的内容就是 WasmNest 的最终内容，拆分阶段不再改任何字面量。**
2. **元仓 + 工作区骨架**：立 release manifest 格式、兄弟目录布局、`wasmnest.code-workspace`。
3. **lib 仓**：16 crate + `contracts/` + WIT 真源 + lib 侧 compose（纯移动，含 2 个 crate 归位）。
4. **两端仓**：端 cap 分片与 SDK 随端走；接入 lib（钉 tag）。
5. **app 仓 ×4**：纯移动 + 显式化双端产物矩阵 + SDK 依赖切 registry 模式。
6. **跨端仓**：path 依赖两端改钉 tag。
7. **元仓补齐**：release manifest + 跨仓装配校验（Seam 1）+ 可复用 workflow。
8. **冻结旧仓**：一次性停写（可选 archive），不存在双写窗口。

#### 两个失效面被彻底分开（本顺序的核心收益）

| 失效面 | 在哪一步被抓住 | 判据 |
| --- | --- | --- |
| 改名改坏了（编译 / 契约 / 数据路径 / 锁） | **步骤 1（单仓）** | 单仓全量测试套件（现成，无需新建） |
| 拆分接错了（依赖 / CI / 锁 / 装配） | **步骤 3–7（各仓 + 元仓）** | 各仓自洽（Seam 2）+ 跨仓装配（Seam 1） |

若按「先拆仓、再逐仓改名」的顺序，两个失效面会在同一步里叠加，一次红灯无法归因到是哪一类问题。

#### 回退点

**改名前的 `dev`**（P1 完成后的 commit）。它同时就是 BedCode 3.0.0 的代码状态——
改名后若发现不可收拾，回退该 commit 即可回到「最后一个 BedCode 版本」，无额外代价。

### D10 组织级收敛

- 组织级 secrets（签名密钥 / 发布 token）一次配置、多仓复用；
- 共享 CI 抽为可复用 workflow，新增仓库只写调用方；
- 每仓一份 `AGENTS.md`（路径基准按该仓重写，共同规范引用元仓），元仓持有跨仓命令与共同规范。

---

## 6. Out of Scope

- **品牌视觉资产落地**（图标 / 闪屏 / 标题栏 / 字体 / 色板）——由品牌工程单独承载，本次只做「改名」不做「换皮」；两者解耦以便分别推进与回滚。
- **第三方插件生态仓库**（外部 app 作者自有仓）——本次只处理第一方 app。
- **SDK 是否公开、lib 是否公开**（§8-1 / §8-2）——属产品叙事决策，本次只要求「拆仓后这两种选择都还能走」。
- **旧数据的自动迁移**（identifier / plugin id 变化后的数据搬迁）——研发期不做迁移工具。
- **CI 运行时长与成本优化**——本次只要求「按仓触发」。

---

## 7. Further Notes

- **`.code-workspace` 的存在本身就是「根已不是工程」的证据**：其头注写明根无 Cargo workspace、两端各自独立工作区、rust-analyzer 单实例索引全部 Cargo.toml 曾 OOM。拆仓只是把这个容器里唯一还剩下的代码搬走，工作区文件本身要升级为覆盖全部仓。
- **lib 的提交节奏是「共演化」而非「独立演化」**：全部 18 个 crate 在过去 10 天内都有提交，15 个在过去 4 天内改过；最活跃的 `wasm-core` 38 次、`peer-net` 19 次。独立建仓的前提是各自有独立节奏——机制层没有，只有应用层有。
- **近两周的方向是「收拢」不是「散开」**：整核抽出、单形态收口、能力域 crate 化、绑定归位——拆成每 crate 一仓会逆转这个方向。
- **app 开发体验不由 lib 的仓数决定**，由三件事决定：SDK 的双模式（已有）、契约版本兼容声明（缺）、dev-shell（已有）。第二项是本次应当顺手补上的。

---

## 8. 待裁决（每条给默认建议，用户否决即改）

| # | 议题 | 默认建议 | 影响面 |
| --- | --- | --- | --- |
| 1 | SDK 已发布名（crates.io / npm）是否新开 `wasmnest-plugin-api` / `@binblink/wasmnest-plugin-sdk-*` | **新开新名 + 旧包 yank / 弃用**（研发期无真实用户，成本最低） | 已发布名不可回收；影响插件作者与 `create` 模板 |
| 2 | `wasmnest-lib` 是否公开 | **先私有**，把「公开」留作后续独立决策 | 决定平台叙事与第三方能力域作者的可行性 |
| 3 | lib 引用机制：git 依赖 vs submodule | **submodule 钉 tag**（path 依赖与现有锁零改造，只需路径前缀变化） | 影响端仓的 CI 与开发者操作习惯 |
| 4 | `identifier`（`com.bedcode.app` / `com.bedcode.mobile`）是否改 | **改**（无真实用户，一次做完再无包袱） | 数据目录迁移；与品牌工程 §13 的「明确不改」冲突，需重新裁决 |
| 5 | plugin id 是否改（除 BedCode 应用） | **改**（同上；BedCode 应用保留 `com.bedcode.terminal-session`） | 插件存储命名空间与权限记录键；旧数据成孤儿 |
| 6 | 元仓是否建 | **建**（薄元仓，零代码） | 不建则跨仓文档与跨仓任务无家可归 |
| 7 | 签名库（现 `bedcode.keystore`）是否改名 | **改**，同步改打包配置与 CI 引用 | 发布签名唯一真源，改错会签不出包 |
| 8 | 改名前的 SDK 是否再发一次旧名版本（`sdk-v*`） | **不发**（3.0.0 已是最后版本；SDK 名随平台改名一次处理完） | 与 §8-1 耦合：旧包废弃后，BedCode 3.0.0 的插件开发者只能停留在旧 SDK 版本 |

> 注：§8-4 / §8-5 与品牌工程 §13 的「明确不改标识符与包名」直接冲突。冲突根因是该原则的前提是「只换品牌」；本次是「平台改名 + 拆仓」，前提已变，故须重新裁决而非默认沿用。

---

## 9. 实施记录

- **2026-10-15（追加裁决）**：执行顺序定为 **先发布 3.0.0 → `dev` 提交干净 → 在 BedCode 单仓内完成
  全部名称替换 → 再拆分迁移**（§D9 已按此重写）。关键收益：改名与拆分两个失效面被分开，
  改名用现成的单仓全量测试套件验证，拆分退化为纯文件移动。
- **2026-10-15**：Phase 0（基线记录）+ Phase 1（元仓骨架）曾被**临时执行**，产物在
  `/home/binblink/project/tauriProject/WasmNest/`（168 文件：产品级文档副本 + 品牌资产 +
  改名工具 `tools/brand-rename.mjs` + `PHASE0-BASELINE.md`）。用户随后裁决「当前不实施，
  只写文档」，执行已停在该目录；**旧仓全程只读、零改动**（`git status` 与执行前一致）。
  该目录去留待用户决定（保留作脚手架 / 删除重建）。
- 若将来从该目录续做：`tools/brand-rename.mjs` 已通过 dry-run 验证（保护串生效、
  报告准确），可直接复用于 Phase 2–5 各仓。
