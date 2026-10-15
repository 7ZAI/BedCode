# 改名规则（BedCode → WasmNest）

> 定位：本文件是拆仓迁移的**改名规则单一事实源**。`spec.md` 定「为什么这么分、分几个仓」，
> 本文件定「**每一个字面量怎么改、哪些不改、改完怎么验**」。
>
> Status: **文档阶段（未实施）**——本文件只描述规则，不含已执行的改动。

---

## 执行位置与顺序（用户裁决 2026-10-15）

改名**只在 BedCode 单仓内执行一次**，在拆分之前完成：

| 序 | 步骤 | 说明 |
| --- | --- | --- |
| P0 | 发布 BedCode **最后一个版本 3.0.0** | `v3.0.0` tag → `release.yml`（桌面 + Android + 插件 + SDK） |
| P1 | **`dev` 提交干净** | 在途改动全部认领并提交，工作区零未提交改动 |
| P2 | 开 `feature/wasmnest-rename` | `dev` 留作回退点 |
| **1** | **在本仓执行本文件全部规则** | 用单仓全量测试套件验证（§6 V5/V5b） |
| 2+ | 拆分迁移 | **纯文件移动，不再改任何字面量**（`repo-map.md`） |

**为什么必须先改名再拆分**：

- **验证缝现成**：单仓下两端 `cargo test` + 两端 `pnpm test:run` + `cross-end-tests` + `eslint` +
  WIT 漂移锁 + 全部防回接锁一应俱全；拆完再改，这些门禁已被拆散到各仓，改名出错时很难归因。
- **回退点干净**：P1 后的 `dev` commit 同时就是「BedCode 3.0.0 的代码状态」，
  改名出错直接回退该 commit 即可，无额外代价。
- **拆分退化为纯移动**：拆分阶段只需搬运文件与接好依赖/CI，不叠加命名变更。
- **不先改名就拆**：改名 diff（10063 处）会与拆分 diff 纠缠，且两仓可能改得不一致。

---

## 0. 前置事实（2026-10-15 实测，可复现）

| # | 事实 | 数值 / 证据 |
| --- | --- | --- |
| N1 | 改名面总规模 | **10063 处 / 1323 文件**（排除 `target/` `node_modules/` `dist/` `gen/android/` `.git/`） |
| N2 | 大小写三形态分布 | `bedcode` 9133 / `BedCode` 633 / `BEDCODE` 297 |
| N3 | **旧仓 `wasmnest` 命中 0 次** | 替换是单向的，不存在撞车或二次改名风险 |
| N4 | token 边界全谱（前后字符） | `.` `_` `-` `/` `` ` `` `"` `'` 空格 `:` `(` `[` `&` `<` → **子串替换即完备**，不需要词边界匹配 |
| N5 | 豁免串规模 | `com.bedcode.terminal-session` **647 处**、`bedcode_plugin_terminal_session` **42 处** |
| N6 | 已发布名（不可回收） | `bedcode-plugin-api` 136 处 / `bedcode-plugin-api-mobile` 30 处 / `@binblink/bedcode-plugin-sdk-*` 372 处 |
| N7 | 仓库 URL 引用 | `7ZAI/BedCode` 47 处、`github.com/7ZAI` 35 处 |

---

## 1. 三层模型（决定「改不改」）

| 层 | 范围 | 处置 | 依据 |
| --- | --- | --- | --- |
| **品牌层** | 仓名、crate 名、rust lib 名、npm 包名、CLI 名、环境变量、文档文案、`productName`、WIT 包名、工作区文件名 | **机械替换为 `wasmnest` / `WasmNest`** | 全新项目，无兼容义务（AGENTS §0） |
| **身份层** | 应用 `identifier`、plugin id、签名库文件名、内部数据路径、DB 文件名 | **替换 + 连带迁移动作**（§5），不可只做字符串替换 | 改动会迁移数据目录 / 存储命名空间 |
| **不可回收层** | 已发布到 crates.io / npm 的名字 | **不可改名**，只能新开名 + 废弃旧包 | registry 名称不可回收 |

**唯一豁免 = BedCode 应用**（原 `terminal-session`，双端同一 plugin id）：其**仓名、目录名、
plugin id、rust lib 名、显示名**一律保留 `bedcode` / `BedCode`（§4）。

---

## 2. 替换规则集（唯一实现，长串优先）

```text
保护（先换成占位符，避免被子串规则吃掉）
  com.bedcode.terminal-session     → 占位符
  bedcode_plugin_terminal_session  → 占位符

替换
  BedCode → WasmNest
  BEDCODE → WASMNEST
  bedcode → wasmnest

还原（占位符换回原字面量）
```

**三条规则为何足够**（N4 的推论）：所有形态都是 `bedcode` 这一个词干的**大小写变体 + 任意边界字符**：

| 形态 | 示例 | 命中规则 |
| --- | --- | --- |
| kebab（crate / 仓名 / 目录） | `bedcode-wasm-core` | `bedcode → wasmnest` |
| snake（rust lib / ident / 路径） | `bedcode_plugin_api`、`bedcode_mobile` | 同上 |
| WIT 命名空间 | `bedcode:plugin` | 同上 |
| 反域名（identifier / plugin id） | `com.bedcode.app`、`com.bedcode.file-transfer` | 同上 |
| 点号（文件 / 内部路径） | `bedcode.keystore`、`bedcode.db`、`~/.bedcode/` | 同上 |
| 显示名 | `BedCode`、`BedCode.code-workspace` | `BedCode → WasmNest` |
| 全大写 | `BEDCODE_*`（环境变量类） | `BEDCODE → WASMNEST` |

**保护必须在替换之前**：否则 `com.bedcode.terminal-session` 会被 `bedcode → wasmnest` 先吃掉。

---

## 3. 逐类对照表（含实测出现次数）

| 类别 | 现形 | 出现 | 目标形 | 层 |
| --- | --- | --- | --- | --- |
| 桌面 identifier | `com.bedcode.app` | 32 | `com.wasmnest.app` | 身份 |
| 移动 identifier | `com.bedcode.mobile` | 26 | `com.wasmnest.mobile` | 身份 |
| **BedCode 应用 plugin id** | `com.bedcode.terminal-session` | 647 | **不变** | 豁免 |
| file-transfer plugin id | `com.bedcode.file-transfer` | 47 | `com.wasmnest.file-transfer` | 身份 |
| ai-chatbox plugin id | `com.bedcode.ai-chatbox` | 141 | `com.wasmnest.ai-chatbox` | 身份 |
| agent-hub plugin id | `com.bedcode.agent-hub` | 57 | `com.wasmnest.agent-hub` | 身份 |
| WIT 包名 | `bedcode:plugin` | 151 | `wasmnest:plugin` | 品牌（**ABI 破坏性**） |
| lib crate 名 | `bedcode-wasm-core` 等 16 个 | — | `wasmnest-wasm-core` 等 | 品牌 |
| 端 crate / npm 包名 | `bedcode-desktop` 483 / `bedcode-mobile` 287 | — | `wasmnest-desktop` / `wasmnest-mobile` | 品牌 |
| SDK crate（**已发布**） | `bedcode-plugin-api` 136 / `-mobile` 30 | — | `wasmnest-plugin-api` / `-mobile` | **不可回收** |
| SDK npm（**已发布**） | `@binblink/bedcode-plugin-sdk-*` | 372 | `@binblink/wasmnest-plugin-sdk-*` | **不可回收** |
| **BedCode 应用 rust lib** | `bedcode_plugin_terminal_session` | 42 | **不变** | 豁免 |
| 其他应用 rust lib | `bedcode_plugin_file_transfer` 等 | — | `wasmnest_plugin_*` | 品牌 |
| 签名库（发布真源） | `bedcode.keystore` | 10 | `wasmnest.keystore` | 身份 |
| 内部数据路径 | `~/.bedcode/` | 11 | `~/.wasmnest/` | 身份 |
| DB 文件名 | `bedcode.db` | 3 | `wasmnest.db` | 身份 |
| 仓库 URL | `7ZAI/BedCode` 47 / `github.com/7ZAI` 35 | — | 指向新仓 | 品牌 |
| 工作区文件 | `BedCode.code-workspace` | 1 | `wasmnest.code-workspace` | 品牌 |
| 显示名 / 文案 | `BedCode` / `WasmNest` | 633 | `WasmNest` | 品牌（豁免除外） |

---

## 4. 豁免清单（BedCode 应用）

**豁免是字面量级，不是目录级**——该应用对平台的技术引用仍须改名。

| 保留 | 理由 |
| --- | --- |
| plugin id `com.bedcode.terminal-session` | 该应用的**身份**；显示名就叫 BedCode，vendor 段 `bedcode` 与之一致 |
| rust lib 名 `bedcode_plugin_terminal_session` | 与 plugin id 同源，`plugin.json` 的 `rustLibrary` 字段 |
| app slug `bedcode`（仓名 `wasmnest-app-bedcode`、目录名 `apps/bedcode/`） | 应用名，非平台名 |
| 显示名 `BedCode`（`plugin.json` 的 `name`、i18n 文案、README 标题） | 用户可见的产品名 |

**仍须改名**（该应用里的平台引用）：
SDK 依赖名（`bedcode-plugin-api` / `@binblink/bedcode-plugin-sdk-*`）、WIT 包名（`bedcode:plugin`）、
数据路径（`~/.bedcode/`）、`author` 字段（作者是平台 = `WasmNest`）、以及全部品牌层文档引用。

**落地方式**：全局替换后按本清单**显式回改**（比「跳过整个目录」可靠——跳过会漏掉该应用里必须改的平台引用），
并补一条锁断言该应用的 id / name / rustLibrary 三件身份在场。

---

## 5. 连带动作（不可只做字符串替换）

| 改动 | 连带影响 | 处置 |
| --- | --- | --- |
| `identifier`（`com.bedcode.app` / `.mobile`） | 应用数据目录按 identifier 派生 ⇒ 旧数据不再被读取 | 研发期不做迁移工具，接受旧数据孤立（无真实用户） |
| plugin id | 插件存储命名空间 + 权限授权记录键 ⇒ 旧记录成孤儿 | 同上；同时须清理旧 plugin id 的授权记录残留 |
| **WIT 包名 `bedcode:plugin` → `wasmnest:plugin`** | **版本进入 import 名，宿主与 guest 不等即实例化失败** | 走 AGENTS §5.1.4 顺序：ABI bump + 双端 WIT 副本同步 + 重建**全部** app 产物 |
| `bedcode.keystore` → `wasmnest.keystore` | 打包配置与 CI secret 的文件名引用 | 同步改 Android 签名配置 + CI 步骤；改错会签不出包 |
| crate 名（含已发布 SDK） | crates.io / npm 名字不可回收 | 新开 `wasmnest-*` 名 + 旧包 yank / 弃用（spec §8-1 待裁决） |
| 仓库 URL | 文档与 package.json 里的仓库链接 | 指向新仓；旧仓冻结后保留只读 |

---

## 6. 验证判据（改完必须逐条跑）

| # | 判据 | 期望 |
| --- | --- | --- |
| V1 | `rg -i bedcode` 全仓命中 | **等于豁免串的在场数**，且只出现在 BedCode 应用的身份位（§4） |
| V2 | `rg -i bedcode` 命中位置白名单化 | 命中行只能是：plugin id / rustLibrary / 显示名 / app slug 四类 |
| V3 | 身份锁 | BedCode 应用的 id、name、rustLibrary 三件身份有断言在场（防「顺手全改了」） |
| V4 | 契约 | WIT 包名、契约版本、两端 ABI 三处一致；`compose --check` 无漂移 |
| V5 | 构建（**单仓内、改名的唯一验证门**） | 两端 `cargo test` + 两端 `pnpm test:run` + `cross-end-tests` + 根 `eslint` 全绿 |
| V5b | 锁不空转 | WIT 拼装漂移锁 + 全部防回接锁仍绿（改名不得让任何锁因路径失效而空转） |
| V6 | 装配 | release manifest 驱动的跨仓装配校验通过（spec Seam 1） |

---

## 7. 风险与回退

- **最大风险**：`bedcode → wasmnest` 是子串替换，若将来出现含 `bedcode` 的**第三方专有名词**
  （例如外部插件作者的自有标识），会被误改。当前 N3 证明仓内 `wasmnest` 零命中、`bedcode` 全部属于本平台，
  故现阶段安全；跨仓协作开始后须改为「显式豁免清单」模式。
- **回退**：**回退点 = P1 完成后的 `dev` commit**（即 BedCode 3.0.0 的代码状态）。改名在
  `feature/wasmnest-rename` 分支上进行，未合入前不影响 `dev`；合入后若发现问题，回退该 commit 即可。
  拆分阶段另起新仓（无历史），旧仓停写不动，任一步出问题可直接丢弃重建。
