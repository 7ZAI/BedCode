# AGENTS.md

## 0. 规则优先级与冲突裁决

当文档/规则冲突时，按下述优先级裁决：

1. **用户当前明确指令**（最新指令优先于一切文档规则）
2. **安全与架构红线**（§8 安全红线、§5.1 宿主侧无业务代码）——不可被普通任务越过；确实需要越线时停下向用户确认，禁止自行放松
3. **本文档硬约束**（"必须/禁止"字眼，下同）
4. **skill 规范**（§4 / §6 标注「强制」的 skill——`frontend-styles`、`unit-test-discipline`——在本文件内与第 3 级同级）
5. **code-map / 领域文档**（含 docs/、docs/adr/）
6. **通用工程经验**

**路径基准（读本文档所有路径前先记住）**：不带端前缀的 Rust 路径均相对 `bedcode-desktop/src-tauri/src/`（如 `server/http/dtos/` = `bedcode-desktop/src-tauri/src/server/http/dtos/`）；`wasm-apps/`、`plugins/`、前端 `src/` 相对**所属端根目录**；`docs/`、`scripts/`、`.scratch/` 相对**仓库根**。

**文档字面 ≠ 事实**：引用任何文档（含本文件）的具体路径 / 命令 / 锁名 / 版本前，先用 `ls` / `rg` 核对；发现不一致**先修文档再继续**（§3 的命令字眼条款是本条在命令维度的特例）。

**最小改动原则**：只改任务必要文件；禁止顺手重构相邻代码、擅自升级依赖版本（升级先做双端影响评估，如 wasmtime / SDK）；不确定的设计取舍先问用户，不猜。

---

## 1. 项目速览

**Tech Stack:** Tauri 2.0 + Vue 3 + TypeScript + TailwindCSS v3 + Rust (Tokio) + SQLite + vue-i18n@9 + WASM（wasmtime 48 组件模型，目标 `wasm32-wasip3`）

**Monorepo 结构（目录树看两端 `docs/code-map.md`）**：`bedcode-desktop/`（桌面主机）/ `bedcode-mobile/`（移动远程终端），各自独立 `src/`（前端）+ `src-tauri/`（Rust 后端）+ `pnpm-lock.yaml`；业务代码在端内插件工程——桌面 `wasm-apps/<app-id>/`（**wasm 应用**，2026-09-25 术语变更，内部实现 / 插件 ID 契约 / 运行时 `app_data_dir/plugins` 均不变）、移动 `plugins/<plugin-id>/`；插件 SDK（WIT 契约）= `bedcode-desktop/packages/plugin-sdk-desktop/` + `bedcode-mobile/packages/plugin-sdk-mobile/`；**Rust 无根 workspace**（两端 30+ 个独立 `Cargo.toml`，测试与构建各自 crate 根跑，§3）；仓库根另有 `cross-end-tests/`（跨端真实互连集成测试，同时依赖两端 lib，§3）

---

## 2. 环境与工具链

只列**与业界默认相反**或**双端必须锁步**的项（Node / Rust / Gradle 等版本以 CI workflow 与 `Cargo.toml` 为准，不在此重述）：

| 项 | 要求 |
| --- | --- |
| 包管理器 | **pnpm**（根与两端 `packageManager: pnpm@12.2.1`），全局禁止 npm |
| Tauri | **2**（两端）——写 API 前确认是 v2 而非 v1 |
| wasmtime | **双端 48**，升级必须双端同步（ADR 0019）；wasip3 构建链仍仅桌面（`docs/knowledge/wasip3-toolchain.md` §7） |
| 版本号 | 桌面/移动 package.json 与 Cargo.toml **同步维护**；变更记录根 `CHANGELOG.md` + `CHANGELOG_zh.md` |
| Android | JDK/Gradle 由 `bedcode-mobile/src-tauri/gen/android` 分发包维护，SDK/NDK 随其管理（不自行安装） |

---

## 3. 黄金命令（构建 / 测试 / 运行）

```bash
# Desktop Development / Build
cd bedcode-desktop && pnpm run tauri:dev
cd bedcode-desktop && pnpm run tauri:build

# 关插件前端 watch 的 dev（需 app 不被反复重启时；默认仍开 watch，不带 flag 行为不变）
cd bedcode-desktop && pnpm run tauri:dev -- --no-watch
#   必须经 pnpm 转发（裸 `node scripts/dev-run.js` 在无 pnpm_execpath 时 Linux ENOENT）；关掉原因：插件 watch 把
#   vite 产物复制进 src-tauri/ → tauri dev 全量重启宿主并清当日日志。关掉后改插件前端自行 `cd wasm-apps/<id> && node scripts/build.js`

# Mobile Development / Build
cd bedcode-mobile && pnpm run tauri:android:dev        # 开发
cd bedcode-mobile && pnpm run tauri:android:dev:log    # 落盘日志（见 logging.md）
cd bedcode-mobile && pnpm run tauri:android:build      # 发布（aarch64）

# Frontend Test —— 统一 pnpm run test:run（= vitest run，跑完退出）
# 禁止 pnpm run test（vitest watch 挂起不退出）；vite 不执行测试
cd bedcode-desktop && pnpm run test:run
cd bedcode-mobile && pnpm run test:run

# Rust Test（桌面/移动各在各自 src-tauri 下）
cd bedcode-desktop/src-tauri && cargo test
cd bedcode-mobile/src-tauri && cargo test

# 跨端互连测试（仓库根第三个 Rust 包：桌面真实服务器 + 移动真实客户端代码 同进程互连，零 mock）
# 前置：桌面随包 wasm 产物（认证中心 com.bedcode.terminal-session）须已构建——
# 缺产物时测试**显性失败**（不静默 skip）；开发期重建：cd bedcode-desktop && pnpm run plugins:build
cd cross-end-tests && cargo test
cd cross-end-tests && cargo test --test terminal_ws_flow   # 针对性过滤（每个场景 = 独立测试二进制）

# wasm 应用 / 移动插件 crate（独立 workspace，命令必须在各自 crate 根执行）
cd bedcode-desktop/wasm-apps/<app-id>/rust && cargo test   # agent-hub / ai-chatbox / file-transfer / terminal-session
cd bedcode-mobile/plugins/<plugin-id>/rust && cargo test    # ai-chatbox / auto-task / file-transfer
cd bedcode-desktop/wasm-apps/<app-id> && pnpm run build    # 前端 + wasm32-wasip3 release 产物 + wasmHash 注入

# Kotlin/Gradle 编译（改 gen/android 下 Kotlin 代码后必跑；离线加 --offline）
cd bedcode-mobile/src-tauri/gen/android && ./gradlew :app:compileUniversalDebugKotlin

# Lint（根目录，覆盖两端前端；0 error 门禁，warning 暂不计入）
pnpm exec eslint .
```

- **测试两段式（强制）**：开发中每次改动后**只跑针对性单元测试**（下方过滤命令）自验，红了立即修——禁止每步改动跑全量；**集成测试编写 + 全量套件回归统一留到任务收尾**（§10）
- **前端可单测过滤**：`cd <端目录> && pnpm exec vitest run <测试文件路径>`——**必须在端目录执行**（仓库根无 vitest 配置）；测试文件在宿主 `src/__tests__/` 与 wasm 应用 `wasm-apps/<app-id>/src/__tests__/` 两处
- **Rust 可单测过滤**：`cargo test <名称前缀>`（cwd = 被测 crate 根：宿主 `src-tauri` 或插件 `*/rust`）
- **测试覆盖面现状（别把它当全量门禁）**：`bedcode-desktop/vitest.config.ts` 的 include 目前只含宿主 `src/__tests__`、`packages/plugin-sdk-desktop/__tests__` 与两个 wasm 应用（`agent-hub` / `terminal-session`）；`wasm-apps/ai-chatbox`、`wasm-apps/file-transfer`、`bedcode-mobile/plugins/*` 的前端测试**不在任何门禁内**——改动落在这些目录时须自行运行并在 §10 说明
- **Kotlin 独立工具链**：上述 gradlew 命令是 `gen/android` 下 Kotlin 改动的唯一验证（`cargo test` 与前端测试均不覆盖）
- **文档里的命令字眼必须跟本节一致**（`pnpm run test:run` / `pnpm run tauri:dev` / `cargo test`；禁 `npm`、禁 `pnpm run test` 监听）；`docs/commands.md` 是全量参考（含端口管理、遗留进程清理），**与本节冲突时以本节为准**
- 构建前检查 `src-tauri/target` 大小，超 15GB 执行 `cargo clean`（`pnpm run target:size` 额外列出共享 / 遗留 target 目录大小）
│ **构建产物落点**：夹具 `bedcode-desktop/target/fixtures`、wasm 应用 `bedcode-desktop/target/wasm-apps`、移动夹具 `bedcode-mobile/target/fixtures`（真源与决策见 `docs/knowledge/build-process.md`「Target 目录管理」节 + 各 `.cargo/config.toml` 注释）。**新增 crate / 脚手架时不得写死 `<crate>/target/`**；`fixtures` 与 `wasm-apps` 不得合并（profile 参与产物指纹，会产出两份依赖产物）
- **测试内 fixture 构建依赖 rustup shim**：宿主 wasm 闭环用例会在测试内 `cargo build --target wasm32-wasip3` 构建 fixture（component/sdk/pty/ws/wasip3-test），并显式注入 `RUSTUP_TOOLCHAIN=nightly-2026-09-16`（单一事实来源 `scripts/wasip3-toolchain.sh`）。因此**必须用 rustup shim 的 `cargo`（`~/.cargo/bin/cargo`）跑测试**，禁止把 `~/.rustup/toolchains/*/bin` 前置进 PATH——绕过 shim 会让注入的 `RUSTUP_TOOLCHAIN` 失效（raw toolchain cargo 忽略该变量）→ 依赖 fixture 的用例成批失败（现象：`Test component WASM build failed` / `WASI test component WASM build failed`，一次红约 39 项，与代码无关）
- **测试后清理进程**：每次跑完测试（`cargo test` / `pnpm run test:run` / `gradlew` 等）后，必须检查并关闭测试开启的后台进程/监听端口（如 cargo 测试 spawn 的 mock server、vitest worker 残留、gradle daemon 等），避免残留进程占用端口或 CPU
- 桌面 `tauri:build` 自动解析 updater 签名密钥（`TAURI_SIGNING_PRIVATE_KEY(_FILE)` / `.env`），未配置时自动禁用升级包，本地构建无需私钥；正式发布由 GitHub Actions Secrets 签名（`docs/knowledge/release-workflow.md`）

---

## 4. 任务路由：改 X 先读 Y

| 任务 | 动手前必读 |
| --- | --- |
| 改前端 UI / 样式 / 布局（组件、CSS、token、动画、主题、响应式） | **先加载 `frontend-styles` skill**（`.agents/skills/frontend-styles/SKILL.md`，强制）+ 对应端 code-map |
| 写 / 改 / 审查单元测试 | **先加载 `unit-test-discipline` skill**（`.agents/skills/unit-test-discipline/SKILL.md`，强制） |
| 改 Rust 后端（任意模块） | 对应端 code-map → 模块目录 → §6 Rust 规范 + 相关 ADR（docs/adr/） |
| 在宿主侧新增/修改任何能力、类型、状态、存储、路由 | **§5.1 宿主侧无业务代码（六条判据 B1-B6 + 三问裁决 + 提交前自检 3 问）+ §5.2 桌面端架构 + ADR 0022**——先判归属再动手；越线必须停下向用户确认。新增**基础服务 / 内部统一业务**角色或**即用即弃 worker** 形态前，另读 ADR 0032 |
| 改插件 | `docs/knowledge/plugin-development-checklist.md`（全文）+ WIT（`bedcode-desktop/packages/plugin-sdk-desktop/rust/wit/bedcode.wit`，移动端另有一份）+ ADR 0017/0019/0022；插件**分类/加载顺序/生命周期形态**问题读 ADR 0032 |
| 改 wasm 应用 / 移动插件（业务代码主场） | 该应用自己的 crate（`wasm-apps/<app-id>/rust` 或 `plugins/<plugin-id>/rust`）+ 自有测试命令（§3 插件块）；**不要拿宿主 `cargo test` 当它的验证** |
| 改数据库 / schema | §9 数据规范 + `bedcode-desktop/src-tauri/src/db/` |
| 改跨端协议（HTTP/WS/QR/认证） | §9 协议规范 + `docs/knowledge/mobile-desktop-auth.md`（含「跨端真实互连测试」章），两端同步评估；改完**跑 `cross-end-tests`**（§3） |
| 排查日志 / 无日志问题 | `docs/knowledge/logging.md` + `docs/knowledge/adb-fd0-bug.md`（adb fd0 根因与 shim 维护要点） |
| 启动多任务 / 需要规划 | `.scratch/<task>/` 记录（项目未设计 GitHub PR 流程，开发过程文档走这里） |
| 改文档 / 改本文件规则 | 先读 §13 文档索引找到**单一事实源**（命令 → §3；边界 → ADR 0022；契约 → WIT / code-map），改源文件而非改引用方；改完核对引用是否失效 |
| 定位代码 | §12 代码查找纪律 |

---

## 5. 架构硬约束

**目标：无业务内核（Businessless Kernel）**——底座内核只含「应用无关的通用引擎」：进程（PTY）、网络（HTTP/WS/mDNS）、存储（SQLite/文件）、安全（JWT/密钥/TLS/信任）、通信（消息总线/插件互调）+ wasmtime 运行时。一切产品概念（会话、终端、设备连接、文件传输、AI……）都是插件。演进路线的阶段划分见 `docs/knowledge/plugin-kernel-roadmap.md`；终态愿景见 `docs/knowledge/businessless-kernel-vision.md`。

边界裁决的单一事实源是 **ADR 0022**（`docs/adr/0022-plugin-host-interface-primitive-boundary.md`）；
本节是它的**可执行摘要 + 门禁**，两者冲突以 ADR 为准，ADR 未覆盖处以本节为准。

### 5.1 宿主侧无业务代码（强制红线）

**一句话判定：宿主只回答「机制怎么做」，不回答「这件事是什么、给谁用、怎么组织」。**

本红线对**桌面端 `bedcode-desktop/src-tauri/src/` 与 `bedcode-desktop/src/`（宿主前端）**同时生效；
两端形态不同（移动端仍是自持业务 App），但**同一判据**适用，见 §5.4。

#### 5.1.1 口径：六条判据（命中任一即业务代码，禁止进宿主）

| # | 判据 | 典型形态（正例：应归插件） |
| --- | --- | --- |
| **B1** | **产品类型 / 字段**：宿主以业务名词定义数据结构、枚举、状态、常量 | `SessionInfo` / `SessionStatus` / 传输任务 / 配对记录 / 供应商配置；`protocol/` 整目录已因此删除 |
| **B2** | **业务编排 / 状态机**：按产品语义推进的多步流程、队列、重试、封顶、状态迁移表 | 会话状态机、任务队列归约、传输历史封顶与重试编排 |
| **B3** | **业务真源**：宿主持有产品事实的权威存储（表 / 注册表 / 缓存）并对外读写 | 业务表、任务表、设置项真源、断点位置（业务表 `pairings` / `connection_history` / `session_configs` 已退役） |
| **B4** | **业务投影 / DTO 翻译**：把原语结果翻译成产品 wire 形状再对外供业务消费 | `server/http/dtos/` 业务组、会话视图 DTO、peer 快照 DTO |
| **B5** | **业务默认值 / 策略**：宿主替插件决定「业务上该怎样」 | 命名唯一化、默认重试次数、默认接收策略、排序与保留条数 |
| **B6** | **业务生命周期挂钩**：宿主解释产品事件并主动回调插件 | 已退役的 `on-session-lifecycle` / `on-input-submitted` / `terminal-hooks` |

**B4 合法残留（现存唯一例外，防误删）**：`server/http/dtos/` 内只有两类内容合法——① `common_dto`（通用 API 信封）；② 会话 / 配置 / 文件 / git 业务 DTO，**仅供 `#[cfg(test)]` 黄金形状锁**（`server/http/gateway.rs::business_endpoint_shapes_are_locked`）。锁的是「移动端看到的字节」这一 wire 契约，路由真身已在插件侧（ABI v29 sessions REST 下沉）。**生产路径不得构造这些类型**；改动它们 = 改跨端 wire 契约，必须两端同步评估（§9）。

**合法（引擎原语）判据**：与产品概念无关、任意第三方可按同一形状复用、返回**句柄 / 字节 / 计数 / 原始 JSON**而不返回业务判断。
判不准时按 B 系列从严判，并停下向用户确认（§0 优先级 2：红线不可被普通任务越过）。

#### 5.1.2 归属裁决：新增能力放宿主还是放插件（三问）

1. **离宿主能实现吗？** 能（插件已有 `host-storage` / `host-plugin-database` / `host-bus` / `host-events` / `host-task` 自建）→ **放插件**。
2. **携带产品语义吗？** 命中 B1-B6 任一 → **放插件**。
3. 都不命中 → **放宿主**，但必须满足：WIT `host-*` 纯增量（或走 ABI bump 流程）、权限位有门禁落点、停用可回收。

**顺序不可颠倒**：先答 1/2 再动手。「宿主已经能拿到这些数据」**不是**留在宿主的理由——
真源搬迁必须走「插件自持 + 宿主原语化」，禁止宿主做兼容回查（这正是 2026-09-24 会话下沉踩过的坑，
见 §8 fail-visible 三形态）。

#### 5.1.3 宿主允许存在的四类薄壳

引擎实现（`pty` / `server` / `peer_net` 引擎控制面 / `db` / `crypto` / `mdns`）、安全闸门
（权限判定、fail-safe 默认如「无应答/超时即拒」、配额仲裁）、**通用**注册表与寻址
（`server/http/registry.rs` 端点表、`server/websocket/registry.rs` 连接表）、零解析窄转发
（`utils/session_gateway.rs`：全接口 `serde_json::Value` 原样透传插件 reply，**零解析零解释**，
插件未激活显性报错）。这四类之外的任何「顺手加的」业务逻辑都是越线。

#### 5.1.4 强制执行机制（不是口号）

- **防回接锁（已有，随回归运行）**——越线回接会直接测红：

  | 锁 | 锁住的事 | 锁所在文件 |
  | --- | --- | --- |
  | `retired_kernel_session_domain_is_not_reintroduced` | 内核会话域（`src-tauri/src/session/`、`host-session` / `host-terminal`、权限位 `session:write` / `terminal:observe`） | `wasm_core/manager/host/tests/wasm_flow_test.rs` |
  | `retired_session_command_surface_is_not_reintroduced` | 宿主侧会话命令面回流 | `wasm_core/manager/host/api_bridge.rs` |
  | `retired_session_observation_surface_is_not_reintroduced` | 宿主侧会话观察面回流 | `wasm_core/manager/host/tests/wasm_flow_test.rs` |
  | `retired_peer_transfer_orchestration_is_not_reintroduced` | 宿主持有传输任务 / 设置 / 历史真源 | `wasm_core/manager/host/tests/wasm_flow_test.rs`（扫描面 `server/peer_net/`） |
  | `retired_tables_are_not_created` | 已退役业务表不在宿主主库重建 | `db/database.rs` |
  | `stale_artifact_rebuild_hint` | 破坏性契约变更后旧产物**实例化期**点名重建（fail-visible 形态 ②） | `wasm_core/manager/runtime/component.rs` |
  | 权限词汇表自检 | 映射表含词汇表外条目即**加载即抛**（fail-visible 形态 ③） | `bedcode-desktop/packages/plugin-sdk-desktop/bin/manifest-gen.js` |

- **真源搬迁必配 fail-visible**（§8 三形态缺一不可）：旧读路径删除或显性报错 / 旧产物实例化期点名 /
  退役权限位与命令字眼加载即抛。**禁止**把旧读路径改成「查不到就返回空」——静默降级会让
  「线还在、数据永远是空」的断链在测试全绿下长期存活。
- **提交前自检（改动落在宿主侧时逐条回答，答不出就停下问用户）**——只问 B 系列**推不出**的三件事：
  1. 它在宿主有**第二个消费者**吗？（唯一消费者 → 归插件）
  2. 删掉它，任意第三方插件能否用**同一形状**的既有原语自建？（不能 → 归插件）
  3. 是否新增了宿主对产品事件的解释或回调（= B6）？（是 → 归插件）

  （产品名词、排序 / 默认 / 封顶策略等已由 B1 / B5 直接覆盖，不在此重复）
- **越线处理**：确实需要越线（例如平台层缺乏逃生口）时，**停下向用户确认并记 ADR**，禁止自行放松红线。
- **落地顺序硬约束**：`ABI` bump + 双端 WIT 副本同步（ADR 0019 / 0022 双端偏离条款）+ 移动端影响评估
  + `CHANGELOG.md` / `CHANGELOG_zh.md` 条目，缺一不可。

#### 5.1.5 高内聚低耦合（配套红线）

- 内核只做引擎原语与安全边界，禁止携带产品语义；业务代码内聚到各自 wasm 应用工程
- 插件间**只经**互调 API（ADR 0017 `api_registry`）与消息总线（`host-bus`）通信，**禁止跨插件直接耦合**
- 新增能力**优先评估「放哪个 wasm 应用」而非「改内核」**；产品事实面按路线逐步下沉
- **「会话」已到达终态（2026-09-24，P4）**：会话真源（登记 / 状态机 / 生命周期分发 / 输入输出编排）在
  `com.bedcode.terminal-session`（`wasm-apps/terminal-session/rust/src/session/`）；宿主侧**不再有任何会话对象**
  ——`src-tauri/src/session/` 整目录删除、`host-session` 与 `host-terminal` 两 interface 退役（ABI v27）、
  `protocol/` 整目录删除、内核输出环与会话状态机消失。宿主与会话相关的只剩三样**都无业务语义**：
  ① PTY 引擎（`host-pty` + `src-tauri/src/pty/`）② 宿主 server 在册连接清单（`host-connection`）
  ③ 互调窄转发层（`utils/session_gateway.rs`）。实施与实测见
  `docs/knowledge/session-engine-downsink.md`（终端渲染管道、设备连接与认证按同一路线继续下沉）
- **裁剪线（ADR 0022）**：宿主能力只暴露「离宿主无法实现、且无业务语义」的原语；业务编排一律在插件层
- 技术决策记录在 `docs/adr/`（Multi-Project Monorepo / Async Everywhere / Event-Driven / Graceful
  Shutdown / Flat Module Structure / Plugin System / 无业务内核 / 插件 Mock 归属 / 0022 边界），
  新增决策走 ADR

### 5.2 桌面端架构（两层 · 四闸门 · 四通道）

```text
┌─ wasm 应用层（业务事实面，4 个应用：terminal-session / file-transfer / ai-chatbox / agent-hub）
│  真源：各自私有 SQLite 库 + 各自前端状态 + 自身命令面（互不直连，只经互调 / 总线）
└──────────────┬─────────────────────────────────────────────────────────
               ↓ 下行只经四种通道（WIT host-* / bus / events / 互调）
┌──────────────▼─────────────────────────────────────────────────────────
│ 宿主内核层（bedcode-desktop/src-tauri/src/ + src/）：应用无关引擎
│  模块清单见 bedcode-desktop/docs/code-map.md（Project Structure + Core Modules）
└─────────────────────────────────────────────────────────────────────────
```

- **四个闸门（宿主 → 插件）**：能力闸门（WIT `host-*` 22 个原语接口 + 权限位判定，
  `wasm_core/security/framework.rs`）· 身份闸门（通道凭证绑定身份，`security/frontend_channel.rs`，
  参数自报 `plugin_id` 无效）· 隔离闸门（bus 具名 topic `<plugin-id>::<name>`、
  `/api/plugin/<owner>/` 与 `/ws/plugin/<owner>/` 路径命名空间、属主判定）·
  生命周期闸门（`approval_gate` 前置审批 + 停用 `purge_for_plugin` 回收 pty/ws/task/peer/http 全部资源）
- **四种通道（插件 → 宿主）**：`host-*` WIT import（**各端一份** WIT 真源：桌面 `bedcode-desktop/packages/plugin-sdk-desktop/rust/wit/bedcode.wit`、
  移动 `bedcode-mobile/packages/plugin-sdk-mobile/rust/wit/bedcode.wit`；双端偏离条款见 ADR 0022 / 0019）· `host-bus`（topic 命名空间仲裁）· `host-events.emit`（插件自定义 JSON 载荷）·
  互调 API（ADR 0017 `api_registry`）
- **调用模型**：实例装配条目 `WasmInstanceEntry`（宿主侧**唯一**入口：`PluginHost::call_guest`）；
  `CoreConfig.call_model` 灰度 = `mutex`（每实例一把锁，回退窗口）| `event-loop`（每实例一个常驻
  事件循环属主任务）。**异步化按需、不全量**——判据与白名单见
  `docs/adr/0029-plugin-concurrency-owner-and-on-demand-async.md`（含 C1–C4 判据、白名单、实例级门结论；宿主实现侧 async，不改 WIT）
- **宿主直调命令面只保留外壳**：`src-tauri/src/commands.rs` 只服务宿主页面（外壳 / 诊断 / 引擎事实），
  业务面一律走插件命令面；`src/composables/` 同理
- **域 → 原语接口 → 业务真源**（ABI desktop 32；每域详解见 `bedcode-desktop/docs/code-map.md` Core Modules 段）：
  - 四引擎域：伪终端 `pty/` → `host-pty`（真源：terminal-session 的 `sessions` / `session_annotations` 库）· 传输 `server/http/` + `server/websocket/` → `host-http` / `host-websocket`（真源：terminal-session REST 域、file-transfer WS 域）· 对等网络 `server/peer_net/` + `packages/peer-net` → `host-peer` v31（真源：file-transfer 事件归约状态机）· 认证 `utils/auth/` + `host_api/auth.rs` → `host-auth` v18（真源：terminal-session `auth_records/` 私有库）
  - 其余：存储 `db/` → `host-storage` / `host-database` / `host-plugin-database`（各应用私有库，`plugin_id_` 前缀或独立库）· 通信 `wasm_core/bus` + `api_registry` → `host-bus` / `host-api-call`（通道本身无真源）· 运行时 `wasm_core/` → `host-app` / `host-config` / `host-log` / `host-process` / `host-fs` / `host-timer` / `host-platform` / `host-mdns` / `host-crypto` / `host-task`（各应用自持）· 连接清单 `server/websocket/registry.rs` → `host-connection`（仅在册连接事实）

### 5.3 已退役 · 不得回接（桌面端）

`src-tauri/src/session/`（整目录）· `src-tauri/src/protocol/`（整目录）· `src-tauri/src/events/`
（宿主同步广播面）· `host-session` / `host-terminal` interface · `terminal-hooks` 导出 ·
权限位 `session:write` / `terminal:observe` · 内核输出环 `GlobalOutputManager` ·
manifest 静态声明面 `contributes.httpEndpoints` / `toolProviders` · 宿主主库业务表
`pairings` / `connection_history` / `session_configs`（不兼容旧版本存量用户，旧库滞留表**不读不迁不清理**）·
`host-peer::resume-all-transfers` 与 `concurrency` 字段 · peer 旧快照 topic `peer:transfer` / `peer:receive` ·
`wasm_core/legacy/` 一次性迁移链。回接任一项 = 越 §5.1 红线。

### 5.4 双端差异（勿把桌面结论套到移动端）

- **桌面**：宿主 = 无业务内核 + 4 个 wasm 应用承载业务（§5.2）
- **移动**：仍是自持业务 App（远端终端客户端），插件契约独立（ADR 0018），移动**不跟演**桌面部分 ABI 破坏性变更
  （`host-pty` / `host-task` / `host-peer` v31 等），移动端相关判断以 `docs/knowledge/mobile-desktop-auth.md` 与
  ADR 0018/0019 为准
- 但**判据同源**：移动端宿主同样禁止出现「内核解释产品语义」的代码；跨端协议改动必须两端同步部署（§9）

---

## 6. 代码规范

### Rust

- 文件命名 snake_case；模块入口文件与目录同名（`module.rs`），不用 `mod.rs`；测试文件 `*_test.rs`
- 统一错误类型 `AppError`：`pub type Result<T> = std::result::Result<T, AppError>`
- 关键调用链在错误构造/转换处带操作描述（`AppError::X(format!(...))`、`io::Error` 自描述包装或 `anyhow::Context` 跨桥），**禁止裸 `?` 透传无上下文错误**（含 `io::Result` 契约内）；`tokio::spawn` 用 `spawn_with_error_boundary()` 包装；重要路径禁止 `let _ =` 静默忽略错误
- panic hook 中只用 `eprintln!`，禁止 `tracing::error!`
- 状态共享用 `Arc<Mutex<T>>` / `Arc<RwLock<T>>`
- Tauri Commands 命名：`list_*`（多个）、`get_*`（单个）、`create_*`、`delete_*`、`start_*` / `stop_*`（生命周期）；用 `// ====================` 分隔注释按领域分组

### Frontend（Vue 3 + TypeScript）

- **任何 UI 改动（组件、布局、CSS/Tailwind 类、design token、动画/过渡、主题、响应式、安全区、字体/行高）必须先加载 `frontend-styles` skill 并以其规范为准**，禁止凭通用前端经验自行发挥
- **禁止用 viewport 宽度 / UA 字符串推断平台**；平台判断统一走 Tauri API（如 `@tauri-apps/plugin-os` 的 `platform()`），两端渲染容器不一致时以 API 为准
- **前端错误处理**：统一 `logger`（两端真源 `src/utils/frontendLogger.ts`，`logger.error/info` 带上下文），禁止静默 `catch`；用户可见错误/状态文案一律走 i18n，禁止 composable / 组件内硬编码中文字符串
- **i18n**：文件位于两端 `src/locales/{zh-CN,en}/`；新增/修改 key 必须同步出现在 zh-CN 与 en 两文件，命名跟随既有分组；复数/日期/数字走 vue-i18n 机制

### 单元测试（Unit Test Discipline）

- **规范来源**：`unit-test-discipline` skill（§4 路由强制加载）：行为契约 → 测试矩阵 → 硬性门禁 G1-G6 → 实际运行 → 变异自检
- **禁止交付**无断言 / 恒真断言 / 只测 mock / 快照替代行为断言 / 只为覆盖率的用例（完整反模式清单见 skill）
- **时机（两段式）见 §3 与 §10，本节不重复**

### 注释与命名

- 注释解释**为什么**而非是什么；语言中文，技术术语保留英文；注释掉的代码必须标注意图与恢复方式，无说明的陈旧代码一律删除
- 分隔注释 `// ==================== Section ====================`；Rust 模块级注释用 `//!`、pub 项用 `///`；文件名 snake_case（Rust）、PascalCase（Vue 组件）、camelCase + `use` 前缀（composable）

---

## 7. 插件开发检查清单

**全文外移**：[`docs/knowledge/plugin-development-checklist.md`](docs/knowledge/plugin-development-checklist.md)。

开发 / 修改插件前**必须**通读该文档并逐项核对（permissions / WIT·ABI / `host-*` 能力与权限词汇 / 同实例串行 / 会话真源 / HTTP 面 / 存储隔离 / `wasmHash` / 日志等）。硬约束与本文件同级。

---

## 8. 安全、日志与可观测性红线

### 安全红线（不可违反）

- **禁止提交密钥/凭据**：仓库内唯一例外是签名真源 `bedcode.keystore`（私有仓库设计，见 §9 Android）；新增的任何密钥、token、密码禁止入库、禁止进日志、禁止写进文档/备注；API token 泄露按仓库规范删除重建
- **认证链路只走既有 auth 模块**（JWT / 设备指纹 / 二维码 / 生物凭证），禁止旁路；**日志与存储中凭据只记长度不落明文**（`token.length()` 模式）。**入场签发密钥与验签执行归认证中心自持**（ADR 0033，desktop ABI v33）：`host-auth` 的 `device-token-issue` / `device-token-verify` 两原语**已退役**，宿主 `utils/auth/jwt.rs`（`JwtService`）与 `host_secrets.rs` **整模块删除**——**宿主不得持有任何设备入场密码学**（防回接锁 `host_has_no_entry_token_crypto`）。**生物凭证面（公钥托管 + 验签执行）已随 v34 下沉认证中心**（B-downsink，desktop ABI 34）：`host-auth` 的 `biometric-credential-bound` / `biometric-verify-signature` / `biometric-credential-bind` 三原语**已退役**，宿主 `utils/auth/biometric.rs` **整模块删除**，生物公钥真源在中心插件私有库 `auth_biometric_keys`（WASM 内 p256 验签）——**宿主不再托管任何设备侧凭证材料**。密钥环（最多两代）在中心插件的 `pairing/keys`，`kid` 只是**诊断标签不是授权门**（真闸门是「签名能否用环内某把密钥验过」）；轮换上一代在宽限期（7 天）内继续可验签，**轮换不撤销既有 token**。
- **配对 / 认证的编排归插件**（ADR 0022 分层）：配对码与 QR 的编排、签发、验签全在 `com.bedcode.terminal-session`（`pairing/` / `qr/` / `auth_http`），认证记录真源在该插件私有库 `auth_records` 域；宿主只剩 `host-auth` **链路身份**原语 / 认证中心桥接（`link-identity-parts` 只含公开材料）。**认证中心 = 单一显式注册的中心**（ADR 0031，v32 已实施）：中心激活时调 `host-auth.auth-center-register` 登记进宿主单中心注册表（第二注册者被拒并点名在册属主），停用时注销 / 宿主 `purge_for_plugin` 回收；`auth-policy` 的发现方式**不再是能力探测 + 排序取首个**。**裁决一律 fail-closed**：无中心在册 / 中心调用失败 / 中心拒绝 → 一律拒绝（`deny_kind` 分 `no_center` / `unavailable` / `policy`），**没有「查不到中心就放行」「传输失败就放行」的降级路径**。宿主主库 `pairings` / `connection_history` / `session_configs` 三表与存量迁移链已退役（旧库滞留表不读不迁不清理；v34 起宿主 `plugin_secrets` 的 `biometric:*` 死行由迁移幂等清扫）。**无宿主代签降级路径**——插件未激活时前端命令面显性报错，新代码不得绕过插件自行签发或验签（删除清单与日期见 ADR 0022）
- 输入校验与权限仲裁在 Rust 端，前端校验仅是 UX；WebSocket/HTTP 接入必须过认证与过滤链（TrafficFilterChain）
- **真源换了地方就要 fail-visible**（通用判据）：事实真源迁走后，**旧读路径必须显性失败，
  禁止静默降级成「无数据」**——静默降级会让「线还在、数据永远是空」的断链在测试全绿的情况下
  长期存活。三种具体形态，缺一不可：① **宿主侧回查**：旧读路径要么删掉、要么对真源外的对象
  显性报错，不得返回空 / `NotFound` 让调用方当「无数据」吞掉；② **旧 ABI 产物**：破坏性契约
  变更后旧产物要在**实例化期**拿到点名缺失 interface + 「按哪个版本重建」的错误，不是 trap
  也不是静默降级；③ **退役的权限位 / 命令字眼**：构建链映射表含词汇表外条目时**加载即抛**，
  而不是注入一个永远过不了门的权限。
  **先例（本判据的来源）**：2026-09-24 会话下沉专项——宿主「回查内核拿会话」曾造成桌面终端按键丢失、任务队列被批量标中断而测试全绿；三形态各已落锁（防回接锁 / `LoadedWasmPlugin::stale_artifact_rebuild_hint` / `manifest-gen.js` 加载期词汇自检），详见 `docs/knowledge/session-engine-downsink.md` §4
- **`pty:spawn` 是「在宿主机执行任意命令」的高风险面**：只发确有 PTY
  需求的第一方插件（会话插件经 `host-pty.spawn` 自产会话、argv 由插件算，宿主不包装），
  并发上限由 `ptyQuota` 声明 + 加载期区间仲裁，不在运行期放宽

### 日志红线

统一 `tracing`（Android 自动转发 logcat），级别语义按「运行细节 `debug!` / 关键生命周期 `info!` / 可恢复异常 `warn!` / 影响功能的失败 `error!`」自判；本仓库硬规则三条：① **panic hook 内只用 `eprintln!`**，禁止 `tracing::error!`；② **热路径克制**（PTY 输出 / WS 每帧不打逐帧日志，高频 API 走 `debug!`）；③ guest 自报的可处理错误不升 `error!`。

- **结构化字段（强制）**：`session_id` / `device_id` / `plugin_id` / `request_id` / `batch_id` / `node_id` 一律 `key = %value` 字段形式，**禁止拼进消息字符串**；消息只写人类可读描述（中文 + 英文术语），错误信息必须带操作上下文
- 落盘机制 / 排障（non_blocking 缓冲、日志路径、移动端无日志排查、插件 WASM 日志细节）见 `docs/knowledge/logging.md`

---

## 9. 数据、协议与产物

### 数据库（SQLite）

- **主库 schema 单一事实源**：`bedcode-desktop/src-tauri/src/db/schema.sql`；列级迁移写在 `database.rs::run_migrations()`，**迁移必须幂等**（可对旧库重跑），禁止手改生产库；改 schema 必须补迁移幂等测试
- 插件存储隔离见 `docs/knowledge/plugin-development-checklist.md`（独立库 / 主库 `plugin_id_` 前缀）
- 测试数据：Rust 走临时目录 + `with_default`（日志），禁止污染真实数据/日志目录

### 网络协议 / 跨端兼容

- 协议（HTTP / WS / QR 配对 / 认证）改动**必须两端同步部署**（桌面主机 + 移动端），字段演进遵循「老端忽略未知字段」的增量原则，禁止破坏性替换
- 认证/配对协议文档：`docs/knowledge/mobile-desktop-auth.md`；宿主/插件契约见 `docs/knowledge/plugin-development-checklist.md`（WIT 节）
- wasmtime 版本升级必须两端同步（ADR 0019）
- **移动端 WS 面口径（2026-09-26 硬切）**：移动端 WS 只承载 `com.bedcode.terminal-session` 的两条插件端点连接（事件通道 + 终端流），且**帧永不加解密**（桌面 `TrafficChannel::WsPlugin => false`，WS 帧级链路加密已退役，**不得回接**）；会话控制 / 会话与配置加载 / 终端输入 / 插件 API 一律走 HTTP（`/api/sessions/*`、`/api/configs`），认证走 HTTP `/api/auth/*`。端点清单、帧类型、重连对账流程见 `docs/knowledge/mobile-desktop-auth.md`

### 产物与生成文件

- **禁止提交**：`**/target/`、`node_modules/`、`.dev-logs/`、Android 构建产物（`build/`、`.gradle/`、`.cxx` 等）
- **锁文件**：`Cargo.lock` / `pnpm-lock.yaml` 只经包管理器变更（`pnpm install` / `cargo update`），**禁止手工编辑**
- **`gen/android` 例外**：`app/src/main/java/com/bedcode/mobile/*.kt` 等手写 Kotlin 源码是版本跟踪的一部分，`tauri android init` 重建后需手工恢复；改 Kotlin 后必须跑 gradlew 验证（§3）

### Android 发布

- 包名：Desktop `com.bedcode.app`，Mobile `com.bedcode.mobile`
- **签名唯一真源：仓库根 `bedcode.keystore`**。`bedcode-mobile/src-tauri/gen/android/` 与 `bedcode-mobile/src-tauri/android-backup/` 下的 keystore 必须是其副本；**勿用其他 keystore 签发布版**
- 版本号两端同步维护（§2）；发布流程见 `docs/knowledge/release-workflow.md`

---

## 10. 完成定义与验证证据

**测试两段式**：开发中每次改动后只跑针对性单元测试自验（§3 过滤命令），不跑全量套件；**本节是任务最后一环**——先补齐集成测试编写，再跑全量回归。

收尾验证以下命令**必须实际运行并贴出结果**；无法运行（环境缺失 / 平台限制）必须说明原因与风险：

- 集成测试：补齐/更新相关用例（Rust `src-tauri/tests/`、前端 `src/__tests__/integration/`）；与本任务无涉时写明理由跳过
- 改了 Rust → `cargo test` **全量**通过（宿主：两端各自 `src-tauri`，含集成 target；**插件：在 `wasm-apps/<app-id>/rust` / `plugins/<plugin-id>/rust` 各自 crate 根跑**）
- 改了**跨端协议 / 任一端客户端或插件的认证·会话·终端面** → `cross-end-tests` 全量通过（§3；两端各自的 mock 各自自洽，真实互连才是契约的真正门禁）
- 改了前端 → `pnpm run test:run` **全量**通过（对应端）。注意覆盖面只到 §3 所列 include；`wasm-apps/ai-chatbox`、`wasm-apps/file-transfer`、`bedcode-mobile/plugins/*` 的前端测试**不在门禁内**，改动落在那里须自行运行并说明
- **未纳入自动化门禁的手工验证项**（须在交付说明里逐项写「跑了 / 没跑 + 原因」）：wasm 应用完整构建（`cd wasm-apps/<id> && pnpm run build`，含 wasmHash 注入）· 未接入 vitest 的应用测试 · `gen/android` gradlew 编译 · 真机 / 浏览器核验
- 改了 `gen/android` 下 Kotlin → `./gradlew :app:compileUniversalDebugKotlin` 通过
- 改了前端 → 根目录 `pnpm exec eslint .` 0 error（warning 不计入）；`cargo fmt` / `cargo clippy` 提交前自查（非 CI 门禁）
- i18n key 同步出现在 zh-CN 和 en
- 公开项有文档注释；错误处理用 `AppError` 而非裸字符串
- 前端 UI 改动通过 `frontend-styles` 自查（token-bound、无原生控件外观、无反模式）
- 改动落在宿主侧时通过 §5.1 自检（三问裁决 + 提交前自检 3 问有答案）、B1-B6 判据零命中；未在宿主新增业务类型 / 状态 / 存储 / 路由 / 业务默认值；新引入的退役面回接被防回接锁覆盖
- 真源搬迁类改动附 fail-visible 三形态证据（§8）：旧读路径显性失败 / 旧产物实例化期点名 / 退役词汇加载即抛
- 单元测试改动通过 `unit-test-discipline` 自查（契约 / 正反例 / 变异）

CI 门禁（合并到 master/uat 时）：`lint.yml`（eslint 0 error）+ `test.yml`（两端 cargo test + vitest）。

---

## 11. 提交、回滚与 Git 规则

### 提交与分支

- **禁止 commit message 中出现 AI 协作者标记（Co-Authored-By 等）**
- 格式：conventional commits `<type>(<scope>): <subject>`；type ∈ feat/fix/docs/refactor/chore/test/perf，scope 常用 desktop / mobile / scratch / sdk
- 分支：`dev` 为本地集成主线；`feature/*` 开发；`uat` / `master` 为远程发布线。开发过程通过 `.scratch/<task>/` 文档记录（项目未设计 GitHub PR 流程）
- 开工先 `git status`：工作区已有未提交改动时先认领来源；**非本任务的改动一律不碰、不回滚**（回滚规范见本节末）
- **CI 隔离**：`origin/dev` 的 push 事件与对 `origin/dev` 的 PR 不触发任何 workflow（lint/test/release/sdk-publish 均忽略 dev）；CI 验证由合并到 master/uat 时的 lint.yml / test.yml 接管。PR / 合并目标基线为 `master`
- 远程 dev 与本地 dev 出现分叉时，**立即停手与用户确认处理方式，禁止自动 `--force` 覆盖**
- 推送前过 pre-commit 钩子（husky）：`scripts/doc-tracking.sh untrack` + **对暂存前端文件跑 eslint**。钩子的 eslint 正则只覆盖 `bedcode-(desktop|mobile)/(src|packages|plugins)/`——**`wasm-apps/` 不在内**（四个 wasm 应用前端改动不过钩子），要全量口径用根 `pnpm exec eslint .`

### 分支级文档跟踪（Git Hooks）

`docs/`（含两端 `bedcode-{desktop,mobile}/docs`）与 `README*` / `AGENTS.md` **全分支正常跟踪**（含 uat/master，允许随发布分支推送远程）；受保护路径（`scripts/doc-tracking.sh` 的 `PROTECTED_PATHS` = `CLAUDE.md` `CONTEXT.md` `.pi` `.scratch`）只在 uat/master 之外入库——uat/master 仅从 index 剔除、不删磁盘文件。行为细节与冲突处理（dev→uat/master 合并出 modify/delete 冲突时 `sh scripts/doc-tracking.sh untrack && git commit`）**以脚本为准**，改动前先读它。

| 场景 | 行为 |
| --- | --- |
| dev / feature 提交 | 正常跟踪（含 docs/），hooks 不干预 |
| uat / master `pre-commit` | 仅从 index 剔除受保护配置文件（`CLAUDE.md`/`CONTEXT.md`/`.pi`/`.scratch/`，工作区保留）；docs/ 正常入库 |
| 切到 uat/master `post-checkout` | 剔除 index 中受保护配置文件 + 从 dev 恢复工作区副本；docs/ 不干预 |
| 合并落到 uat/master `post-merge` | 剔除合并带入的受保护配置文件，以暂存删除形式待提交；docs/ 正常合并入库 |

- 新增受保护路径：同步改 `scripts/doc-tracking.sh` 的 `PROTECTED_PATHS`；默认忽略类条目（如 `CLAUDE.md` / `CONTEXT.md`）另加 `.gitignore`；env：`DOC_UNTRACKED_BRANCHES`（默认 `uat master`）、`DOC_TRACKING_SOURCE`（默认 `dev`）
- `.pi/` 整目录被根 .gitignore 忽略：新增 .pi 文件必须 `git add -f .pi/<子路径>` 精确添加，**禁止 `git add -f .pi` 整目录**；`.pi/sessions/` 始终不入库

### 文件回滚规范（强制）

回滚/撤销某文件的修改前，先 `git status <file>` + `git diff <file>` 确认其不含本次会话之外的未提交改动：

1. 含他人/其他任务在途改动的文件，**禁止 `git checkout -- <file>` / `git restore` 整文件回滚**（未提交内容无法从 git 恢复）
2. 正确做法：用 edit 工具逐段逆向替换，只精确还原本次修改的内容
3. 本次新增且非他人创建的独立文件可直接删除
4. 误用 `git checkout` 覆盖在途改动时立即停手上报（恢复源：`.pi/sessions/` 会话日志、`.scratch/` 交接文档），不得猜测重建

---

## 12. 代码查找纪律

两端各有一份目录级代码地图：桌面 `bedcode-desktop/docs/code-map.md`、移动 `bedcode-mobile/docs/code-map.md`。

**探索代码 / 定位模块 / 查找功能实现，必须先读对应端 code-map.md**，按 Project Structure → Core Modules → Quick Navigation 定位目标目录，再用 `ls` / `rg` 找具体文件。禁止未读 code-map 盲目全仓 grep。

维护规则：只到目录层级；顶层模块目录增删或核心职责变化时同步更新；描述与实际不符时以实际为准并顺手修正文档。

> pi agent 增强：见附录的 pi-lens 纪律（`docs/agents/pi-tools.md`）。

---

## 13. 文档索引

| 需求 | 入口 |
| --- | --- |
| 命令参考 | `docs/commands.md` |
| 代码地图 | `bedcode-desktop/docs/code-map.md` / `bedcode-mobile/docs/code-map.md` |
| 领域模型 / 术语 | 根 `CONTEXT.md`（单上下文）+ `docs/adr/`，规范见 `docs/agents/domain.md` |
| Issue tracker | issues 为 `.scratch/` 下的 markdown，见 `docs/agents/issue-tracker.md` |
| Triage 标签 | needs-triage / needs-info / ready-for-agent / ready-for-human / wontfix，见 `docs/agents/triage-labels.md` |
| 发布流程 | `docs/knowledge/release-workflow.md`（桌面 updater / 移动发布）、`docs/knowledge/sdk-publish.md`（SDK 发布） |
| 跨端协议测试 | `cross-end-tests/`（§3 命令）+ `docs/knowledge/mobile-desktop-auth.md`「跨端真实互连测试」章 |
| 日志 / 排障 | `docs/knowledge/logging.md`、`docs/knowledge/adb-fd0-bug.md` |
| 插件开发检查清单 | `docs/knowledge/plugin-development-checklist.md`（AGENTS §7 指向的全文） |
| 插件 WASM 日志 | `docs/knowledge/plugin-wasm-logging.md`（dev 调试模式 + trap backtrace + per-plugin 级别） |
| 会话/终端下沉 | `docs/knowledge/session-engine-downsink.md`（会话真源终态、fail-visible 三形态落锁） |
| 分支隔离 / CI 配置 | `docs/knowledge/feature-branch-isolation.md`、`docs/knowledge/github-actions-setup.md` |
| 构建与产物治理 | `docs/knowledge/build-process.md`（target 目录治理）、`docs/knowledge/wasip3-toolchain.md`（§7 移动端待决策项） |
| 插件 HTTP 信任模型 | `docs/knowledge/plugin-http-endpoint-trust.md` |
| 架构路线 | `docs/knowledge/plugin-kernel-roadmap.md`、`docs/knowledge/businessless-kernel-vision.md` |
| **宿主/插件边界裁决（§5 红线的单一事实源）** | `docs/adr/0022-plugin-host-interface-primitive-boundary.md`（+ ADR 0017 互调 / 0018 移动独立契约 / 0019 双端锁版 / 0031 认证中心注册与组合式认证 / 0032 wasm 插件分类体系） |
| pi 工具手册 | `docs/agents/pi-tools.md`（附录） |

---

## 附录：pi 工具专属（仅 pi agent）

pi-lens 代码查询纪律（三阶段漏斗、符号级查询、诊断收尾）、subagents 编排、vision 视觉 subagent、scipq Rust 精确引用 —— 完整手册见 **`docs/agents/pi-tools.md`**。非 pi agent（Claude Code / Codex / Gemini / OpenCode）按 §12 code-map 默认规范执行。

pi agent 专属 DoD：**收尾 `lens_diagnostics mode=all` 无 blocker**（🔴 blocker 未清前不算 done）。
