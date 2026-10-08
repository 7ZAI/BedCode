# AGENTS.md

## 0. 本文件的定位与冲突裁决

**定位**：只给**方向、判据与导航**。机制、清单、模块图、ABI 计数、历史与踩坑都在单一事实源里（代码本身、code-map、ADR、`docs/knowledge/`、`CHANGELOG*.md`）——**不在本文件复制**，需要时按指针去读。

冲突按下述优先级裁决（高者胜）：

1. **用户当前明确指令**（最新指令优先于一切文档规则）
2. **安全与架构红线**（§5 宿主/插件边界、§8 安全红线）——不可被普通任务越过；确需越线时停下向用户确认，禁止自行放松
3. **本文档硬约束**（"必须 / 禁止"字样，下同）
4. **强制 skill**（§4 / §6 标注：`frontend-styles`、`unit-test-discipline`）
5. **code-map / 领域文档**（含 `docs/adr/`）
6. **通用工程经验**

- **路径基准**：不带端前缀的 Rust 路径相对 `bedcode-desktop/src-tauri/src/`（wasm_core 整核抽出后，机制与引擎面真源在 `packages/bedcode-wasm-core/src/`——2026-10-08 由 `bedcode-desktop/packages/` 迁根至仓库根，宿主侧只剩 `pub use` 垫片——spec 2026-10-06-wasm-core-whole-crate / ADR 0037）；**能力域 / 传输面 / 引擎 crate（含插件机制整核）一律落仓库根 `packages/bedcode-*`**（2026-10-07 能力域迁根，2026-10-08 整核本体迁根，与机制内核 `bedcode-host-kit` 同族，ADR 0035 D6 部分撤销；**能力域默认形态 = 纯引擎 + 端口抽象，零 WIT / 零桌面 SDK 依赖，桌面 WIT 绑定层收进 `desktop-host` feature——ADR 0035 脱绑条目，长期门禁 `packages/bedcode-headless-host-probe`**），`bedcode-desktop/packages/` 只剩插件契约 / 夹具 crate（`plugin-*`）；`wasm-apps/`、前端 `src/` 相对**所属端根目录**（双端业务应用源码目录均名 `wasm-apps/`）；`docs/`、`scripts/`、`.scratch/` 相对**仓库根**。
- **文档字面 ≠ 事实**：引用任何路径 / 命令 / 锁名 / 版本前先用 `ls` / `rg` 核对；与事实不符**先修文档**再继续（命令字眼以 `docs/commands.md` 为准）。
- **最小改动原则**：只改任务必要文件；禁止顺手重构相邻代码、擅自升级依赖（升级先做双端影响评估，如 wasmtime / SDK）；设计取舍不猜，先问用户。

---

## 1. 项目速览

**Tech Stack**：Tauri 2 + Vue 3 + TypeScript + TailwindCSS v3 + Rust (Tokio) + SQLite + vue-i18n@9 + WASM（wasmtime 48 组件模型，目标 `wasm32-wasip3`）

**结构**：`bedcode-desktop/`（桌面主机）/ `bedcode-mobile/`（移动远程终端），各带独立前端 + `src-tauri/` + 独立锁文件；业务代码在端内插件工程（双端 `wasm-apps/<app-id>/`），插件契约 = 双端 `packages/plugin-sdk-*/`。**Rust 无根 workspace**，构建与测试各自 crate 根跑（§3）。目录树与模块职责见两端 `docs/code-map.md`。

---

## 2. 环境与工具链（只列与业界默认相反或双端锁步的项）

| 项 | 要求 |
| --- | --- |
| 包管理器 | **pnpm**，全局禁止 npm |
| Tauri | **2**——写 API 前确认是 v2 而非 v1 |
| wasmtime | **双端 48**，升级必须双端同步（ADR 0019） |
| 版本号 | 桌面/移动同步维护；变更记根 `CHANGELOG.md` + `CHANGELOG_zh.md` |
| Android | JDK/Gradle/SDK/NDK 由 `gen/android` 分发包管理，不自行安装 |
| sccache | **全仓 Rust 构建前置**（根 `.cargo/config.toml` 强制 `rustc-wrapper`），未装则一切 cargo 命令直接失败——这是显性失败设计，不要加降级绕过 |

其余版本以 CI workflow 与 `Cargo.toml` 为准；构建资源治理见 `docs/knowledge/build-process.md`。

---

## 3. 命令

**命令与字眼的唯一事实源：[`docs/commands.md`](docs/commands.md)**（速查表 / 通用约定 / 双端开发与打包 / wasm 应用与插件 / 测试 / Rust 与代码质量 / 构建资源与 target 治理 / 端口与遗留进程）。写或执行任何命令前先读它；文档与实际不符**先修文档**再执行，不要在本文件另立一份。

与本文件相关的两个判断：**禁 `pnpm run test`**（watch 不退出，一律 `test:run`，且必须在端目录执行）；**Rust 无根 workspace**，宿主、wasm 应用、移动插件、`cross-end-tests` 各自在自己的 crate 根跑测试，宿主 `cargo test` 不覆盖插件 crate。

---

## 4. 任务路由：改 X 先读 Y

| 任务 | 动手前必读 |
| --- | --- |
| 改前端 UI / 样式 / 布局（组件、CSS、token、动画、主题、响应式） | **先加载 `frontend-styles` skill**（强制）+ 对应端 code-map |
| 改移动端前端（新功能 / 重构 / 样式，含宿主壳） | **§6「移动端前端：优先对接宿主壳」（强制）**——默认落点 `bedcode-mobile/src/shell/**`（新界面）；旧 `src/{components,views,composables,stores}` 只接受缺陷修复 |
| 写 / 改 / 审查单元测试 | **先加载 `unit-test-discipline` skill**（强制） |
| 改 Rust 后端 | 对应端 code-map → 模块目录 → §6 Rust 规范 + 相关 ADR |
| 在宿主侧新增/修改任何能力、类型、状态、存储、路由 | **§5.1（六条判据 + 三问裁决 + 自检）+ ADR 0022**——先判归属再动手；越线必须停下问用户。插件分类 / 加载顺序 / 生命周期形态另读 ADR 0032 |
| 改插件 | `docs/knowledge/plugin-development-checklist.md`（全文）+ 双端 WIT + ADR 0017/0019/0022 |
| 改 wasm 应用 / 移动插件（业务代码主场） | 该应用自己的 crate + 自有测试命令（§3）；**不要拿宿主 `cargo test` 当它的验证** |
| 改数据库 / schema | §9 + `packages/bedcode-wasm-core/src/db/`（schema / 迁移真源，ADR 0037 随整核迁入 crate）与同 crate `host_api/{database,storage}.rs`（插件面机制） |
| 改跨端协议（HTTP/WS/QR/认证） | §9 + `docs/knowledge/mobile-desktop-auth.md`，两端同步评估；改完跑 `cross-end-tests` |
| 排查日志 / 无日志问题 | `docs/knowledge/logging.md`、`adb-fd0-bug.md` |
| 启动多任务 / 需要规划 | `.scratch/<task>/` 记录（项目未设计 GitHub PR 流程，开发过程文档走这里） |
| 改文档 / 改本文件规则 | 改**源文件**而非引用方（单一事实源见 §13），改完核对全仓引用是否失效 |
| 定位代码 | §12 代码查找纪律 |

---

## 5. 架构硬约束：无业务内核

**一句话**：底座内核只含「应用无关的通用引擎」（进程 / 网络 / 存储 / 安全 / 通信 + wasmtime 运行时），一切产品概念（会话、终端、设备连接、文件传输、AI……）都是插件。

**方向**：新增能力先问「这是机制还是产品语义」，机制归内核、产品归插件。演进路线与终态愿景见 `docs/knowledge/plugin-kernel-roadmap.md` 与 `businessless-kernel-vision.md`；边界裁决的单一事实源是 **ADR 0022**，本节只是它的可执行摘要 + 门禁。技术决策一律走 `docs/adr/`。

### 5.1 宿主侧无业务代码（强制红线）

**判定：宿主只回答「机制怎么做」，不回答「这件事是什么、给谁用、怎么组织」。**

对桌面端宿主（`src-tauri/src/` 与 `src/`）生效；移动端形态不同但**同一判据**适用（§5.4）。

#### 5.1.1 口径：六条判据（命中任一即业务代码）

这是裁决词汇，报错信息与代码注释里直接用编号：

- **B1** 产品类型 / 字段：宿主以业务名词定义数据结构、枚举、状态、常量
- **B2** 业务编排 / 状态机：按产品语义推进的流程、队列、重试、封顶、状态迁移
- **B3** 业务真源：宿主持有产品事实的权威存储并对外读写
- **B4** 业务投影：把原语结果翻译成产品 wire 形状再对外消费
- **B5** 业务默认值 / 策略：宿主替插件决定「业务上该怎样」
- **B6** 业务生命周期挂钩：宿主解释产品事件并主动回调插件

#### 5.1.2 归属裁决（三问，顺序不可颠倒）

1. **离宿主能实现吗？** 能（插件已有 `host-*` 存储 / 总线 / 事件 / 任务原语自建）→ **放插件**。
2. **携带产品语义吗？** 命中 B1-B6 → **放插件**。
3. 都不命中 → **放宿主**，且必须满足：WIT 纯增量（或走 ABI bump 流程）、权限位有门禁落点、停用可回收。

「宿主已经能拿到这些数据」**不是**留在宿主的理由；判不准时从严判并停下问用户（§0 优先级 2）。

#### 5.1.3 宿主允许存在的四类薄壳

① **引擎实现** · ② **安全闸门**（权限判定、fail-safe 默认如「无应答 / 超时即拒」、配额仲裁）· ③ **通用**注册表与寻址 · ④ **零解析窄转发**（`utils/session_gateway.rs`）。四类之外的任何「顺手加的」业务逻辑都是越线。

#### 5.1.4 强制执行机制

- **防回接锁**：越线回接会直接测红；锁名 → 文件的索引见 `bedcode-desktop/docs/code-map.md` 文末，新增退役面时同步补锁与索引。
- **真源搬迁必配 fail-visible 三形态，缺一不可**：① 旧读路径删除或显性报错，**禁止**改成「查不到就返回空」（静默降级会让断链在测试全绿下长期存活）② 旧 ABI 产物在实例化期点名缺失 interface 与重建版本 ③ 退役权限位 / 命令字眼加载即抛。
- **提交前自检**（改动落在宿主侧时逐条回答，答不出就停下问用户）：① 在宿主有第二个消费者吗？② 删掉它，第三方插件能用同一形状的既有原语自建吗？③ 是否新增了对产品事件的解释或回调（= B6）？
- **越线处理**：确需越线时停下向用户确认并记 ADR，禁止自行放松红线。
- **落地顺序**：ABI bump + 双端 WIT 副本同步（ADR 0019 / 0022 双端偏离条款）+ 移动端影响评估 + `CHANGELOG` 双语条目。

#### 5.1.5 高内聚低耦合

内核只做引擎原语与安全边界，业务内聚到各自 wasm 应用；插件间**只经**互调 API（ADR 0017）与消息总线（`host-bus`）通信，**禁止跨插件直接耦合**；新增能力优先评估「放哪个 wasm 应用」而非「改内核」。会话真源已下沉插件终态（`docs/knowledge/session-engine-downsink.md`），宿主只剩 PTY 引擎、在册连接清单、窄转发层三样无业务语义的东西。

### 5.2 桌面端架构（两层 · 四闸门 · 四通道）

```text
wasm 应用层（业务事实面：各自私有库 + 各自前端状态 + 自身命令面）
        ↓ 下行只经四闸门：能力 · 身份 · 隔离 · 生命周期
        ↓ 插件上行只经四通道：WIT host-* · bus · events · 互调
宿主内核层（应用无关引擎）
```

方向：闸门与通道的具体接口、权限位、审批与回收落点见 `bedcode-desktop/docs/code-map.md` Core Modules 段与 `docs/adr/0029-plugin-concurrency-owner-and-on-demand-async.md`。宿主直调命令面（`commands.rs` / `src/composables/`）**只保留外壳**——业务面一律走插件命令面。


### 5.4 双端差异（勿把桌面结论套到移动端）

桌面是「无业务内核 + wasm 应用承载业务」；移动端仍是自持业务 App，插件契约独立（ADR 0018），**不跟演**桌面部分 ABI 破坏性变更。但**判据同源**，跨端协议改动必须两端同步部署（§9）。

---

## 6. 代码规范

### Rust

- 文件与模块命名：snake_case，模块入口文件与目录同名（`module.rs`），不用 `mod.rs`，测试文件 `*_test.rs`
- 统一错误类型 `AppError`；错误构造 / 转换处必须带操作描述，**禁止裸 `?` 透传无上下文错误**（含 `io::Result` 契约内）；`tokio::spawn` 用 `spawn_with_error_boundary()` 包装；重要路径禁止 `let _ =` 静默忽略
- panic hook 内只用 `eprintln!`，禁止 `tracing::error!`
- 状态共享用 `Arc<Mutex<T>>` / `Arc<RwLock<T>>`
- Tauri Commands 命名：`list_*` / `get_*` / `create_*` / `delete_*` / `start_*` / `stop_*`，用 `// ====================` 分隔注释按领域分组

### Frontend（Vue 3 + TypeScript）

- **UI 改动必须先加载 `frontend-styles` skill**（组件、布局、CSS/Tailwind 类、design token、动画、主题、响应式、安全区、字体），禁止凭通用前端经验自行发挥。
- **前端零资源访问（强制红线）**：前端只做 UI 显示，**不含后端逻辑**；HTTP / WebSocket / 文件访问一律由 Rust 端发起并过权限闸门，前端直连即绕过闸门。三层封锁（源码 ESLint 静态锁 + Tauri ACL 运行期拒 + CSP 引擎级封）已落地且有防回接锁，**改前端代码时不要试图绕过**；确需新能力时把发起权收归 Rust（宿主命令 / `host-*` 原语 + 闸门），前端只拿元数据或事件，**撤 capability 与补宿主命令必须成对提交**。豁免只允许在 ESLint 层逐行写 `eslint-disable-next-line` 并注明理由（新增豁免须在交付说明里点名）；**ACL 与 CSP 两层无豁免机制**——要开就改锁并写清理由。
- **禁止用 viewport 宽度 / UA 字符串推断平台**；平台判断统一走 Tauri API。
- **错误处理**：统一 `logger`（真源 `src/utils/frontendLogger.ts`，带上下文），禁止静默 `catch`；用户可见文案一律走 i18n，禁止硬编码中文。
- **i18n**：两端 `src/locales/{zh-CN,en}/`；新增 / 修改 key 必须**同步**出现在 zh-CN 与 en 两文件，命名跟随既有分组。

### 移动端前端：优先对接宿主壳（强制）

**移动端前端的新功能 / 重构 / 样式调整，默认在宿主壳（新界面）里实现；旧界面只接受缺陷修复——直到旧界面被彻底替换。**

- **新界面（默认落点）** = 宿主壳 `bedcode-mobile/src/shell/**`（路由 `/mobile/shell`）。壳自带两层自足面：公共组件库 `src/shell/components/ui/**`、平台机制 `src/shell/composables/**`——两者都是旧组件 / 旧机制的**壳内副本**，契约逐字一致，迁移映射见 `src/shell/components/ui/index.ts` 头注。
- **旧界面（只接受缺陷修复）** = `bedcode-mobile/src/{components,views,composables,stores}/**` 的既有页面与其 `/mobile/**` 路由。新功能、重构、样式统一不进旧目录。
- **缺什么先复制进壳**：壳内需要旧目录的组件 / 机制时，复制进 `src/shell/**` 再按需改造，**禁止**把壳直接接到旧目录——防回接锁 L7 拦截 `@/components` / `@/composables` / `@/views` 的 import（`src/__tests__/shell/shellConstraintLocks.test.ts`，确需桥接必须在锁内白名单登记并写明理由）。旧目录也不得为了方便壳复用而改造共用形状。
- **业务不落壳**：终端 / 文件 / 会话 / 设备 / AI 等业务页面按 §5.1 归各 wasm-app（`registerSurface` / `registerSlot` 注册运行面），壳只提供挂载、生命周期与权限闸门，壳内不实现业务。
- **共享基础设施例外**：日志 `src/utils/frontendLogger.ts` 与全局设置 store 暂为跨新旧共用（复制会导致双写 / 双攒批），退役旧界面时随批迁入壳；新增第三份共用件前先问用户。
- **迁移面不许缩水**：L7 正面钉住壳内复制面文件在场；新增复制件必须同步登记该清单与两端 code-map 的防回接锁索引。
- **退役顺序**：旧页面只有在其新壳等价物可用之后才可删；新旧入口并存期以新壳为主入口。
- 本规则在旧目录的界面代码归零（彻底替换完成）后自动失效。

### 单元测试

规范来源为 `unit-test-discipline` skill（§4 强制加载）：从需求与实现推导行为契约 → 测试矩阵 → 实际运行 → 变异自检。**禁止交付**无断言 / 恒真断言 / 只测 mock / 快照替代行为断言 / 只为覆盖率的用例。时机见 §10。

### 注释与命名

注释解释**为什么**而非是什么，语言中文、技术术语保留英文；注释掉的代码必须标注意图与恢复方式，无说明的陈旧代码一律删除。Vue 组件 PascalCase、composable `use` 前缀 camelCase。

---

## 7. 插件开发检查清单

全文外移：[`docs/knowledge/plugin-development-checklist.md`](docs/knowledge/plugin-development-checklist.md)。开发 / 修改插件前**必须**通读并逐项核对（permissions / WIT·ABI / 能力与权限词汇 / 同实例串行 / 会话真源 / HTTP 面 / 存储隔离 / `wasmHash` / 日志等）。硬约束与本文件同级，冲突按 §0 裁决。

---

## 8. 安全与日志红线

### 安全红线（不可违反）

- **禁止提交密钥 / 凭据**：仓库内唯一例外是 Android 签名真源 `bedcode.keystore`；新增密钥、token、密码禁止入库、禁止进日志、禁止写进文档；泄露按仓库规范删除重建
- **凭据只记长度不落明文**（日志与存储同此）；**认证链路只走既有 auth 模块**，禁止旁路
- **认证 / 配对编排归插件**（ADR 0022 分层）：宿主只剩 `host-auth` 链路身份原语与认证中心桥接，**不得持有任何设备入场密码学与设备侧凭证材料**（入场签发密钥、生物公钥托管与验签执行均归认证中心，ADR 0033），有防回接锁
- **认证裁决一律 fail-closed**：认证中心（ADR 0031 显式注册的单中心）未在册 / 调用失败 / 拒绝 → 一律拒绝，**没有「查不到就放行」「传输失败就放行」的降级路径**；**无宿主代签降级路径**，插件未激活时前端命令面显性报错
- 输入校验与权限仲裁在 **Rust 端**，前端校验仅是 UX；WS / HTTP 接入必须过认证与过滤链
- **`pty:spawn` 是「在宿主机执行任意命令」的高风险面**：只发确有 PTY 需求的第一方插件（argv 由插件算，宿主不包装），并发上限由 manifest 声明 + 加载期仲裁，不在运行期放宽
- 真源搬迁的 fail-visible 三形态见 §5.1.3

### 日志红线

统一 `tracing`，级别语义：运行细节 `debug!` / 关键生命周期 `info!` / 可恢复异常 `warn!` / 影响功能的失败 `error!`。三条硬规则：① panic hook 内只用 `eprintln!` ② 热路径克制（PTY 输出 / WS 每帧不打逐帧日志，高频 API 走 `debug!`）③ guest 自报的可处理错误不升 `error!`。**结构化字段强制**：`session_id` / `device_id` / `plugin_id` / `request_id` / `batch_id` / `node_id` 一律 `key = %value`，**禁止拼进消息字符串**；错误信息必须带操作上下文。落盘与排障见 `docs/knowledge/logging.md`。

---

## 9. 数据、协议与产物

**数据库**：主库 schema 单一事实源在 `packages/bedcode-wasm-core/src/db/`（`db.rs` + `db/{database,models,operations}.rs` + `db/schema.sql`，随整核抽出迁入 crate——ADR 0037；宿主以 `pub use` 垫片引用），迁移**必须幂等**、禁止手改生产库、改 schema 必须补幂等测试。插件面的数据库机制（`host-database` / `host-plugin-database` / `host-storage` 共 13 原语：权限门、表名前缀纵深、护栏、属主分区）**留在 wasm 核心内**（同 crate `host_api/`，ADR 0036）——不拆 crate，机制实现与真源同侧；两域用例随 crate 与宿主 `cargo test` 跑。插件存储隔离见插件开发检查清单。测试数据走临时目录，禁止污染真实数据 / 日志目录。

**跨端协议**：HTTP / WS / QR / 认证改动**必须两端同步部署**，遵循「老端忽略未知字段」的增量原则，禁止破坏性替换；协议现状与端点清单见 `docs/knowledge/mobile-desktop-auth.md`。wasmtime 升级必须两端同步（ADR 0019）。

**产物**：禁止提交 `**/target/`、`node_modules/`、`.dev-logs/`、Android 构建产物；`Cargo.lock` / `pnpm-lock.yaml` 只经包管理器变更，**禁止手工编辑**。`gen/android` 下手写 Kotlin 源码是版本跟踪的一部分，`tauri android init` 重建后需手工恢复，改 Kotlin 后必须跑 gradlew 验证（`docs/commands.md`）。

**发布**：包名 Desktop `com.bedcode.app` / Mobile `com.bedcode.mobile`；**签名唯一真源是仓库根 `bedcode.keystore`**，勿用其他 keystore 签发布版；版本号两端同步（§2），流程见 `docs/knowledge/release-workflow.md`。

---

## 10. 完成定义与验证证据

**测试两段式（强制）**：开发中每次改动后**只跑针对性单元测试**自验（vitest 必须端目录执行、Rust 过滤按 crate 根，见 §3），红了立即修；**集成测试编写 + 全量套件回归统一留到任务收尾**——也就是下面这张清单。以下**必须实际运行并贴出结果**，无法运行要说明原因与风险：

- 集成测试补齐 / 更新（Rust `src-tauri/tests/`、前端 `src/__tests__/integration/`）；与本任务无涉时写明理由跳过
- 改了 Rust → 对应 crate 根 `cargo test` 全量通过（宿主两端各自 `src-tauri`；插件在各自 crate 根）
- 改了跨端协议 / 任一端客户端或插件的认证·会话·终端面 → `cross-end-tests` 全量通过（两端各自的 mock 各自自洽，真实互连才是契约的真正门禁）
- 改了前端 → 对应端 `pnpm run test:run` 全量 + 根 `pnpm exec eslint .` 0 error（warning 不计入）；`cargo fmt` / `cargo clippy` 提交前自查（非 CI 门禁）
- **未纳入自动化门禁的手工验证项**（逐项写「跑了 / 没跑 + 原因」）：未接入 vitest 的目录测试 · wasm 应用完整构建（含 wasmHash 注入）· `gen/android` gradlew 编译（改了 Kotlin 时必跑）· 真机 / 浏览器核验
- 改动落在宿主侧时通过 §5.1 裁决与自检（B1-B6 零命中、未在宿主新增业务类型 / 状态 / 存储 / 路由 / 业务默认值、退役面回接被防回接锁覆盖）；真源搬迁附 fail-visible 三形态证据
- i18n key 双语同步；公开项有文档注释；错误处理用 `AppError`；单元测试改动通过 `unit-test-discipline` 自查（契约 / 正反例 / 变异）

CI 门禁（合并到 master/uat 时）：`lint.yml`（eslint 0 error）+ `test.yml`（两端 cargo test + vitest）。

---

## 11. 提交、回滚与 Git 规则

- **禁止 commit message 出现 AI 协作者标记**（Co-Authored-By 等）；格式 conventional commits `<type>(<scope>): <subject>`
- 分支：`dev` 本地集成主线，`feature/*` 开发，`uat` / `master` 发布线；`origin/dev` 的 push 与 PR 不触发任何 workflow，CI 由合并到 master/uat 时接管
- 开工先 `git status`：工作区已有未提交改动时先认领来源，**非本任务的改动一律不碰、不回滚**
- 远程 dev 与本地 dev 分叉时**立即停手问用户**，禁止自动 `--force` 覆盖
- 推送前过 husky pre-commit（doc-tracking + 暂存前端文件 eslint；钩子正则不含 `wasm-apps/`，全量口径用根 `pnpm exec eslint .`）
- **分支级文档跟踪**：`docs/`、`README*`、`AGENTS.md` 全分支正常跟踪；`CLAUDE.md` / `CONTEXT.md` / `.pi` / `.scratch` 为受保护路径，只在 uat/master 之外入库（uat/master 仅从 index 剔除、不删磁盘文件）——行为细节**以 `scripts/doc-tracking.sh` 为准**，改动前先读它；`.pi/` 被 .gitignore 忽略，新增文件必须 `git add -f .pi/<子路径>` 精确添加

**文件回滚规范（强制）**：回滚前先 `git status <file>` + `git diff <file>` 确认不含本次会话之外的未提交改动。

1. 含他人 / 其他任务在途改动的文件，**禁止 `git checkout -- <file>` / `git restore` 整文件回滚**（未提交内容无法从 git 恢复）
2. 正确做法：用 edit 工具逐段逆向替换，只精确还原本次修改的内容
3. 本次新增且非他人创建的独立文件可直接删除
4. 误用 `git checkout` 覆盖在途改动时立即停手上报（恢复源：`.pi/sessions/` 会话日志、`.scratch/` 交接文档），不得猜测重建

---

## 12. 代码查找纪律

探索代码 / 定位模块 / 查功能实现，**必须先读对应端 code-map.md**（桌面 `bedcode-desktop/docs/code-map.md`、移动 `bedcode-mobile/docs/code-map.md`），按其导航到目录后再用 `ls` / `rg` 找文件；禁止未读 code-map 盲目全仓 grep。code-map 只到目录层级、不逐文件列举（写成源码清单会既冗余又漂移），顶层模块增删或职责变化时同步更新。

---

## 13. 文档索引

| 需求 | 入口 |
| --- | --- |
| 命令参考 | `docs/commands.md` |
| 代码地图（含防回接锁索引） | 两端 `docs/code-map.md` |
| 领域模型 / 术语 / 决策 | 根 `CONTEXT.md` + `docs/adr/`（规范见 `docs/agents/domain.md`） |
| Issue tracker / triage | `.scratch/` 下的 markdown（`docs/agents/issue-tracker.md`、`triage-labels.md`） |
| 发布 | `docs/knowledge/release-workflow.md`、`sdk-publish.md` |
| 跨端协议 | `docs/knowledge/mobile-desktop-auth.md` + `cross-end-tests/` |
| 日志 / 排障 | `docs/knowledge/logging.md`、`adb-fd0-bug.md`、`plugin-wasm-logging.md` |
| 插件开发检查清单 | `docs/knowledge/plugin-development-checklist.md`、`plugin-http-endpoint-trust.md` |
| 会话 / 终端下沉 | `docs/knowledge/session-engine-downsink.md` |
| 分支隔离 / CI | `docs/knowledge/feature-branch-isolation.md`、`github-actions-setup.md` |
| 构建与产物治理 | `docs/knowledge/build-process.md`、`wasip3-toolchain.md` |
| 架构路线 | `docs/knowledge/plugin-kernel-roadmap.md`、`businessless-kernel-vision.md` |
| **宿主 / 插件边界裁决（§5 红线的单一事实源）** | `docs/adr/0022-plugin-host-interface-primitive-boundary.md`（+ 0017 互调 / 0018 移动独立契约 / 0019 双端锁版 / 0029 并发 / 0031 认证中心注册 / 0032 插件分类 / 0033 认证中心自持签发验签 / 0035 能力域 crate 化 / 0037 wasm_core 整核抽出 / 0038 wasm-core 引擎面与薄壳纯净性 / 0039 host-pty 能力域整面迁出（含 WIT 接线）） |
| pi 工具手册 | `docs/agents/pi-tools.md` |

---

## 附录：pi 工具专属（仅 pi agent）

pi-lens 代码查询纪律（三阶段漏斗、符号级查询、诊断收尾）、subagents 编排、vision 视觉 subagent、scipq Rust 精确引用 —— 完整手册见 **`docs/agents/pi-tools.md`**。非 pi agent（Claude Code / Codex / Gemini / OpenCode）按 §12 code-map 默认规范执行。

pi agent 专属 DoD：**收尾 `lens_diagnostics mode=all` 无 blocker**（🔴 blocker 未清前不算 done）。
