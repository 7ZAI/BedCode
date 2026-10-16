# 仓地图与依赖契约

> ⚠️ **作废（2026-10-16）**：仓数与各仓内容清单已改为**三仓**
> （平台 / 能力 / 应用），见 `.scratch/2026-10-16-three-repo-split/spec.md` §2/§3。
> 本文的 §2 依赖与版本契约、§4 门禁归属、§5 脚本归属三张表仍可参考（三仓版本已在新文重写）。

> 定位：本文件定「**每个仓装什么、依赖谁、版本怎么钉、门禁归谁**」。
> `spec.md` 定为什么这么分（判据与阶段），`naming-rules.md` 定字面量怎么改。
>
> Status: **文档阶段（未实施）**——本文件是目标形态的清单，不是已完成的现状。

---

## 0. 工作区布局（本地磁盘）

```text
~/project/tauriProject/WasmNest/     ← 工作区根（磁盘目录，不是 git 仓）
  wasmnest.code-workspace            ← 多根工作区文件（现 BedCode.code-workspace 升级版）
  wasmnest/                          ← 元仓：ADR / 领域模型 / CHANGELOG / 命令 / 跨仓任务 /
                                       品牌资产 / release manifest / 跨仓装配校验（零代码）
  wasmnest-lib/                      ← 机制内核 + 契约 + WIT 真源（16 crate）
  wasmnest-desktop/                  ← 桌面宿主 + 前端 + 桌面 SDK + 端 cap 分片
  wasmnest-mobile/                   ← 移动宿主 + 前端 + 移动 SDK + 端 cap 分片 + 签名库
  wasmnest-cross-end/                ← 跨端真实互连测试
  wasmnest-app-bedcode/              ← 每 app 一仓（本仓保留 BedCode 名）
  wasmnest-app-file-transfer/
  wasmnest-app-ai-chatbox/
  wasmnest-app-agent-hub/
```

**路径前缀规则**（写文档 / 读 ADR 时用）：

- `wasmnest-<repo>/…` → 仓名开头，路径相对**该仓根**；
- `wasmnest-lib/packages/…` → lib 仓内保留 `packages/` 层级（不改成 `crates/`），
  使旧文档里的 `packages/bedcode-*` 引用在改名后仍可机械映射；
- 元仓内的引用一律用**工作区根相对路径**（含仓名前缀），保证在本地布局下可直接 `ls` 核对。

---

## 0.1 拆分阶段的性质（用户裁决 2026-10-15：先改名、后拆分）

**全部名称替换已在 BedCode 单仓内完成并验证**（见 `naming-rules.md`「执行位置与顺序」）。
因此本文件的拆分阶段是**纯文件移动 + 依赖/CI 接线**：

| 拆分阶段做 | 拆分阶段不做 |
| --- | --- |
| 搬运文件、建仓、建 workspace、接依赖与 CI | 不改任何字面量（品牌名 / crate 名 / 标识符已定） |
| 接 lib（钉 tag）、接 SDK（registry）、接跨端依赖 | 不改业务逻辑、不重构代码 |
| 显式化双端产物矩阵、补各仓 AGENTS.md 与锁索引 | 不新建机制、不删功能 |

**验证重点因此转移**：不再担心「名字改错」（已在单仓被抓住），而是担心**接线接错**
（依赖版本对不上、CI 没触发、锁因路径失效而空转、装配装不起来）——对应 Seam 2 / Seam 1。

---

## 1. 各仓内容清单

### 1.1 `wasmnest`（元仓，零代码）

| 内容 | 说明 |
| --- | --- |
| `docs/adr/` | 46 篇架构决策（单一事实源） |
| `CONTEXT.md` | 领域模型 / 术语 |
| `CHANGELOG.md` + `CHANGELOG_zh.md` | 产品级发布记录（双语，发布前一次性总结） |
| `README.md` + `README_en.md` | 产品门面（**需重写定位**：现首句仍称「局域网远程终端」，与「WASM 应用宿主」不符） |
| `docs/commands.md` | 跨仓命令参考 |
| `docs/knowledge/` | 插件开发清单 / 跨端协议 / 发布流程 / 构建治理 / 日志排障 |
| `docs/agents/` | issue tracker 与 triage 约定 |
| `brand/` | 品牌规范 + 标志几何真源 + 交付资产 |
| `tools/` | 跨仓脚本（见 §5） |
| `release-manifest.json` | 跨仓版本唯一表达（见 §3） |
| `wasmnest.code-workspace` | 多根工作区文件 |
| `.scratch/` | **跨仓**任务（如 ABI bump、本迁移）；各仓自己的任务留各仓 |

### 1.2 `wasmnest-lib`（16 crate + 契约）

| 分组 | crate |
| --- | --- |
| 机制内核 | `wasm-core`（含 core 绑定层）、`host-kit`（双端共享锚点）、`host-api-core` |
| 传输 / 网络 | `peer-net`、`link-crypto`、`server-base`、`server-core`、`server-http`、`server-websocket`、`server-peer-net` |
| 能力域引擎 | `pty-engine`、`discovery-engine`、`crypto-engine`、`notify-engine`、`ws-client-engine` |
| 门禁 | `headless-host-probe`（长期门禁：能力域 crate 纯净性） |

| 另持 | 说明 |
| --- | --- |
| `contracts/wasmnest-plugin-package.rs` | 契约包名与版本**全仓唯一手写真源**（`const` 上下文可用，供 `inventory` 静态自报） |
| WIT 真源 | `wasm-core/wit/core.wit` + 6 个能力域分片（pty / mdns / http / ws / peer / notify，各在所属 crate 的 `wit/` 下） |
| lib 侧 compose | 拼 core + 共享分片 → 产出**带哈希的 WIT bundle** |

**迁出**：`file-transfer-core` → file-transfer app 仓；`componentize` → 桌面 SDK 的构建工具面。
两者各只有 1 个消费者，不是平台机制（判据见 `spec.md` §D2）。

### 1.3 `wasmnest-desktop`

`src-tauri/`（宿主）、`src/`（前端）、`packages/plugin-sdk-desktop/`（SDK，pnpm workspace 成员）、
端 cap 分片（`wit-src/cap-http.wit` / `cap-ws.wit` / `cap-desktop.wit`）、端侧 `compose.json` 与合并脚本、
`docs/code-map.md`（含防回接锁索引）。

**易漏**：`packages/` 下另有 5 个测试夹具包 —— `plugin-bench-test`、`plugin-p3-async-host-import-test`、
`plugin-sdk-fixtures`、`plugin-system-test`、`plugin-wasi-test`（它们测宿主，随端走）。

### 1.4 `wasmnest-mobile`

`src-tauri/`（宿主）、`src/`（前端，含新界面 `src/shell/**`）、`packages/plugin-sdk-mobile/`（SDK）、
`packages/plugin-component-test`（夹具）、端 cap 分片（`wit-src/cap-mobile.wit`）、端侧 `compose.json` 与合并脚本、
签名库（`wasmnest.keystore`）、`src-tauri/gen/android`（手写 Kotlin + 分发包配置，重建后需手工恢复）、
`docs/code-map.md`。

### 1.5 `wasmnest-cross-end`

`cross-end-tests/`（跨端真实互连：移动端真实客户端 ↔ 桌面端真实服务器，同进程零 mock）。
依赖两端与 lib，**全部改为钉 tag**（不再 path 依赖兄弟目录）。

### 1.6 app 仓 ×4（内部布局统一）

```text
wasmnest-app-<slug>/
  rust/            ← 双端共享业务核（有则在此）
  desktop/         ← 桌面侧前端 + manifest + 构建脚本
  mobile/          ← 移动侧前端 + manifest + 构建脚本
  README.md        ← 显式声明本 app 的「双端产物矩阵」
```

| app 仓 | desktop 侧来源 | mobile 侧来源 | 共享业务核 |
| --- | --- | --- | --- |
| `wasmnest-app-bedcode`（**保留 BedCode 名**） | 原 `terminal-session` | 原 `bedcode` | 无（双端各自实现：桌面=权威侧，移动=控制侧） |
| `wasmnest-app-file-transfer` | 原 `file-transfer`（含 `rust/`） | 原 `file-transfer`（仅前端） | **有**：`file-transfer-core` 从桌面树迁入 `rust/` |
| `wasmnest-app-ai-chatbox` | 原 `ai-chatbox` | 原 `ai-chatbox` | 无 |
| `wasmnest-app-agent-hub` | 原 `agent-hub` | —（桌面独有） | 无 |

**双端产物矩阵必须显式声明**（哪些 world / 哪些 target / 哪端加载哪份产物 / 字节一致性判据），
不得再用跨端路径偷渡 —— 现状移动 file-transfer 的 `sharedRustManifest` 指向桌面 manifest，由此消灭。

---

## 2. 依赖与版本契约

| 边 | 机制 | 钉法 | 备注 |
| --- | --- | --- | --- |
| app → SDK（端仓） | npm registry + crates.io | 发布版依赖；本地联调用 `{ path, version }` 双声明 | `version` 在场 ⇒ `cargo publish` 合法；npm 侧用 `file:` 兄弟路径或 `overrides` |
| app → lib 共享核 | 同 SDK 模式 | 同上 | **仅白名单 crate**（纯引擎 + 端口抽象、零 WIT、零桌面 SDK 依赖），且必须 `default-features = false` |
| 端 → lib | 子仓引用 | 钉 **tag**（submodule 或 git 依赖，待裁决） | 不得跟分支 |
| 端 → lib 的 WIT | lib 发布的 WIT bundle | 钉 tag + **哈希校验** | 端仓不 checkout 元仓、不 checkout 对方端仓 |
| cross-end → 两端 + lib | 子仓引用 | 钉 tag | 从「PR 阶段」退到「发布阶段」，故 Seam 1 必须补上 |
| 元仓 → 全部 | 无代码依赖 | release manifest 记录 tag | 元仓不装代码 |

**反向禁止**：lib 不依赖任何端；app 不依赖任何端仓的实现（只依赖 SDK 与白名单 crate）；
端不依赖任何 app 仓。

---

## 3. release manifest（跨仓版本的唯一表达）

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

- **谁写**：发布时由元仓的跨仓发布流程写入；
- **谁读**：Seam 1 的跨仓装配校验、各仓 release workflow（校验自己是否在清单里、tag 是否匹配）；
- **为什么必须有**：拆仓后没有任何地方能回答「WasmNest 3.1.0 由哪些版本组成」——单仓时这是免费获得的。

---

## 4. 门禁归属

| 门禁 | 落点 | 变化 |
| --- | --- | --- |
| WIT 真源一致性（11 个 WIT 逐字一致） | **lib 仓** | 从桌面测试迁入（原本硬编码了移动路径） |
| 契约包版本真源与漂移 | lib 仓（`contracts/`）+ 端仓校验 bundle 哈希 | 端仓不再读对方目录 |
| 内核纯净性（零端名 / 零端词汇） | **lib 仓** | 判据 7a/7b 进 lib；7c 的禁用词表**静态化**（词表本身即判据） |
| WIT 拼装漂移 | lib 侧 bundle 生成 + 端侧合并校验 | 脚本随真源各持一半 |
| 双端单产物加载 | app 仓（矩阵自证）+ cross-end 仓（移动宿主加载桌面产物） | 探针随 app 走 |
| 跨端真实互连（零 mock） | **cross-end 仓** | path 依赖改钉 tag |
| 宿主侧无业务代码（反回接锁） | 各自端仓 | 各扫各的 |
| 壳约束锁（新界面不接旧目录等） | 移动仓 | 不变 |
| app 零兄弟仓依赖 | 各 app 仓（Seam 2） | **新增** |
| 跨仓装配 | **元仓**（Seam 1） | **新增** |

---

## 5. 脚本归属

| 脚本 | 归属 | 理由 |
| --- | --- | --- |
| `compose-wit.mjs` | **拆两半**：lib 侧 bundle 生成 / 端侧合并校验 | 端仓必须能独立跑漂移锁 |
| `bump-version.mjs` | 元仓 | 跨仓版本一致性 |
| `package-sdks.mjs` | 各端仓 | 产物属于该端的 SDK |
| `package-plugins.mjs` | 各 app 仓 | 产物属于该 app |
| `doc-tracking.sh` | 每仓一份 | 分支级文档跟踪策略，各仓各自生效 |
| `wasip3-toolchain.sh` | 元仓（单一事实源）+ 各仓引用 | 工具链版本必须双端锁步 |
| `adaptive-run.mjs` / `build-profile.mjs` | 元仓 | 构建资源治理，跨仓共享 |
| `audit-rust-tests.mjs` / `split-rust-tests.mjs` / `run-test-split.sh` | 元仓 | 测试组织纪律，跨仓共享 |
| `sync-skills.sh` / `pi-session-archive.sh` | 元仓 | 开发环境工具 |
| `plugin-wasm-config.mjs` / `replace-console.mjs` | 各自 app 仓 / 端仓 | 只服务本仓构建 |
| `check-target-size.js` / `dev-run.js` | 各仓 | 本仓构建入口 |
| `plugin-package-list.json` | 元仓（并入 release manifest） | 插件清单是产品级信息 |

---

## 6. 易漏项清单（逐项核对，别只搬主目录）

> 改名已在单仓完成，本清单只核对**搬运完整性**与**接线落位**——
> 漏搬一个目录不会编译报错，但会让某个锁或某条产物链静默失效。

- [ ] `contracts/`（契约版本真源）→ lib
- [ ] 6 个能力域 crate 内的 `wit/*.wit`（随 crate 走，别只搬 `core.wit`）
- [ ] 端 cap 分片 `wit-src/*.wit`（桌面 3 个 / 移动 1 个）
- [ ] 端侧 `compose.json` 与合并脚本
- [ ] 桌面 `packages/` 下 5 个测试夹具包
- [ ] 移动 `packages/plugin-component-test`
- [ ] `bedcode.keystore` → 移动仓（发布签名唯一真源）
- [ ] `src-tauri/gen/android`（手写 Kotlin + 分发包配置）
- [ ] 两端 `docs/code-map.md`（含防回接锁索引）
- [ ] 根 `.cargo/config.toml`（sccache 强制前置）+ 各 `Cargo.toml` 的编译资源 profile 段
- [ ] `eslint.config.js`（现根单份 → 每仓一份或抽共享包）
- [ ] `.husky/` + `scripts/doc-tracking.sh`
- [ ] `.code-workspace`（升级为覆盖全部仓）
- [ ] `plugin-package-list.json`
- [ ] SDK 的 `dev-shell/`（随包发布，勿漏 `files` 白名单）
- [ ] 品牌资产（→ 元仓）
- [ ] `.scratch/` 任务文档（跨仓 → 元仓；各仓 → 各仓）
