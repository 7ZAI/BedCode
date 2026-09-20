# 认证中心插件（Auth Center）实施规格 —— 设备连接认证语义下沉

Status: ready-for-agent（2026-09-18 用户确认方向：**wasm32-wasip3（WASI 0.3）**编译路径 + wasi2→wasi3 全量升级；实施前终审清单见 §10）
Date: 2026-09-18
方向来源: 用户指示——「先实现认证中心插件（存功能、无界面），包含远程连接与文件传输插件的设备连接认证语义；device 最终只调用 http / mdns / websocket 基础服务」+「**明确使用 wasi3，既有 wasi2 也升级为 wasi3**」（经查证：WASI 0.3.0 于 2026-06-11 正式稳定发布，wasmtime 46+ 默认启用 WASI 0.3 + component-model-async）。
关联: `.scratch/2026-09-18-devices-plugin-scope/mapping.md`（两套设备语境摸底）；`docs/adr/0022`（裁剪线）；`docs/adr/0017`（互调）；`docs/adr/0019`（双端锁版）；AGENTS.md §8（认证链路红线，配套修订见 §10.3）

---

## 1. 目标形态

**headless 内置插件 `com.bedcode.devices`（认证中心）**，无用户界面，收敛两套设备连接认证语义为单一权威：

- **远程连接认证语义**（现宿主 `utils/auth/` + `commands/`）：配对码、QR token、JWT 签发/校验策略（claims 结构、TTL、设备信任列表）
- **文件传输 peer 认证语义**（现 file-transfer + peer-net）：首连确认（consent）决策、可信对端管理、身份/能力位通告中的认证部分

认证中心**只消费内核基础服务**：`host-mdns`（发现）/ `host-http` / `host-ws`（宿主引擎）/ **WASI 0.3**（熵、时钟、流/异步）/ `host-auth.secret-store`（密钥托管）/ 存储原语。自身不碰传输引擎、不持有任何业务 UI。

## 2. 技术底座决策（2026-09-18 确认：明确采用 wasi3）

### 2.1 编译路径：wasm32-wasip3（WASI 0.3）——全量升级

**目标**：认证中心插件以 **wasm32-wasip3** 编译；宿主运行时与既有 wasi2 用法**全部升级为 wasi3**。

**WASI 0.3 查证事实（2026-09-18）**：

- **规范已稳定**：WASI 0.3.0 于 2026-06-11 由 WASI Subgroup 批准发布（0.3.1 = 2026-08-11）；async 原生进 Component Model（`wasi:io` 的 streams 改为组件级 async primitives，未来化）；WASI 1.0 已在规划。
- **wasmtime 引擎层成熟**：wasmtime 46+ 默认启用 WASI 0.3 + component-model-async（引擎级 async：Store 可并发多 async 操作，区别于旧 async 单操作锁定）。
- **⚠️ wasmtime-wasi p3 模块仍标注实验性**：`wasmtime_wasi::p3` 文档标注 *"Experimental, unstable and incomplete… not ready for production use"*（47/48 一致）；48 起 wasip2/p3 宿主 trait 统一（#13810/#13812），渐进成熟中——**必须先过验证门禁（§4.1 A1）才能承诺**。
- **⚠️ wasmtime 版本线**：项目锁 47（非 LTS，48 发布后支持期至 ~2026-10 将尽）；**48.0.1（2026-08-24）是最新稳定且为 LTS**（支持 24 个月）——升级目标为 **wasmtime 47 → 48（双端同步，ADR 0019）**，不在 47 上硬啃 p3 实验模块。
- **⚠️ 工具链缺口**：wasm32-wasip3 为 tier 2 target（需 LLVM 23 + 更新 rustup 才提供预编译 rust-std；本机 rustc 1.98.1 / rustup 1.29.1 当前无预编译产物）——A0 需先落地工具链（rustup update 或源码构建 target），或暂以 wasm32-wasip2 产物跑在 async store 上作过渡（见 A0 备选）。

**WASI 0.3 能力对应（认证中心所需）**：

- **熵**：`wasi:random`（0.3 下 `get-random-bytes` 为 **async**）→ getrandom/ed25519-dalek OsRng 经 wasip3 target 可用；
- **时钟**：`wasi:clocks`（0.3 下 `now`/`resolution` 为 async）；
- **流/异步**：streams 为组件级 async primitives——认证中心可编写 async 编排（配对流程状态机天然契合）；
- 宿主 `p2::add_to_linker_sync` 需升级为 **p3 async linker + async Store**（见 A0）。

### 2.2 wasi2 → wasi3 全量升级（用户决策，2026-09-18）

既有 wasi2 用法**全部升级为 wasi3**，作为本规格的**前置内核工程 A0**（独立里程碑，不随认证中心绑定）：

- 宿主：`p2::add_to_linker_sync` → **p3 async linker**（`Config` 启用 component-model-async + `Store::new_async` + `call_async`）；既有 host_impl 适配（sync host 函数在 async store 下的兼容性验证；燃料续费、ResourceLimiter 适配 async 语义）
- SDK/构建链：插件编译目标从 wasm32-unknown-unknown / wasm32-wasip2 → **wasm32-wasip3**；`componentize`（wit-component 0.256）对 wasip3 产物的编码验证（或升级 wasm-tools 工具链）
- 既有插件（unknown-unknown 产物）：验证在 async store 下零回归（无 await 点组件应同步运行）；迁移策略：双轨并存 → 逐个重建为 wasip3
- **wasmtime 47 → 48（LTS）双端升级**（ADR 0019 流程：双端 Cargo.toml/Cargo.lock + breaking changes 评估 + 编译测试验证）
- 工具链：rustup/rustc 更新以提供 wasm32-wasip3 target（或源码构建）

> **A0 备选（过渡）**：若 wasm32-wasip3 工具链落地受阻，可先用 **wasm32-wasip2 产物跑在 async store** 过渡（wasi2 接口在 48 与 p3 统一后仍可运行），认证中心先行；wasip3 编译在工具链就绪后切换。**不改变 wasi3 为最终目标**。

### 2.3 wasmtime 版本策略

- **升级目标**：wasmtime **48.0.1（LTS，2026-08-24）**双端同步（ADR 0019）——47 非 LTS 支持期将尽；48 是 wasip2/p3 统一的第一条稳定线。
- 版本升级与 wasi3 启用**解耦实施**：先升 48（低风险，兼容 47 行为）验证双端全绿，再切 p3 async（高风险，独立验证门禁）。

### 2.4 宿主原语增量：仅 `host-auth.secret-store`

WASI 无成熟的密钥托管接口（keyvalue 为 preview、不成熟），JWT 密钥 / 配对种子必须宿主托管：

```
interface host-auth {
    // 密钥托管（权限门：PERMISSION_AUTH；明文只经返回值暴露给授权插件，不落日志）
    secret-set:  func(secret-id: string, value: list<u8>) -> result<_, string>;
    secret-get:  func(secret-id: string) -> result<list<u8>, string>;   // 仅本插件可读（属主校验）
    secret-delete: func(secret-id: string) -> result<bool, string>;
    // 宿主验签执行点（供宿主中间件取策略，非插件热路径）——见 §3「不动」与 §10.2
    verify-hmac256: func(secret-id: string, payload: list<u8>, mac: list<u8>) -> result<bool, string>;
}
```

- 存储实现：数据目录受限文件（0600）+ 属主校验（`secret-id` 按插件隔离命名空间 `{plugin_id}.{secret-id}`）+ 内存缓存（激活期）。
- **顺带治理（宿主内改动）**：JWT 密钥现为硬编码常量（`jwt.rs` `JWT_SECRET`，安全债）——改首启随机生成 + 写入 secret-store 宿主侧托管；宿主 `utils/auth` 读 secret-store 替代常量。该治理是 §10.3 配套修订的一部分，随 A 步骤落地。

## 3. 边界（动 / 不动）

### 动（下沉认证中心——语义/策略）

| 语义 | 现状 | 下沉内容 |
|---|---|---|
| 配对流程 | 宿主 commands/system.rs + PairingService | 配对码生命周期编排、TTL 策略、配对状态机 |
| QR 配对 | commands/qr.rs + QrTokenManager | QR token 语义（用途/TTL/一次性）、展示数据组织 |
| JWT 策略 | utils/auth/jwt.rs（claims/TTL 硬编码） | claims 结构、iss/exp 策略、设备名/指纹元数据、签发/验证编排 |
| 设备信任列表 | DB pairings 表 + peer trust_store | 已配对设备/可信对端统一视图与决策（谁能连、可撤销） |
| peer consent | peer_net.rs 事件桥 + 宿主确认框 | 确认编排、展示数据组织、接受/拒绝决策 |
| 身份通告认证部分 | peer-net TXT/ServiceInfo 构造 | 认证元数据语义（身份关联、指纹） |

### 不动（留内核——密码学引擎/密钥/传输）

- **密钥托管**：secret-store 明文不出宿主；TLS 私钥维持 peer-net 宿主侧托管（rustls 握手用）
- **验签执行点**：server 连接建立时的 token 验签仍在宿主中间件执行（策略经 06 能力注册表取认证中心导出，执行留宿主——红线项 §10.2）
- **TLS/传输引擎**：rustls、ed25519 证书验证（ring）、WS/HTTP 链路加密、连接注册表
- **TrafficFilterChain / 认证中间件**、**DeviceIdentity / NodeIdentity**（生成与持久化真源留宿主）
- **peer-net 引擎**：发现/拨号/传输/共享目录（无业务语义，ADR 0022 终态 13 原语现状不变）

### 认证中心插件内部设计

- 模块：`pairing`（配对码/QR/JWT 策略）、`trust`（设备信任列表/peer 可信对端）、`consent`（首连确认决策）、`identity`（身份元数据组织）
- 密码学：HS256 自实现（hmac + sha2，~50 行，jsonwebtoken 因 ring 不可 wasm 编译）；Ed25519 验签经宿主原语（宿主已有 ring 实现）或 wasip3 下 ed25519-dalek（OsRng 可用）
- 对外服务（manifest `api` 声明，ADR 0017 gate）：
  - `auth.verify-device-token(token) -> claims|err`（消费：宿主 server 中间件经 06 框架取策略）
  - `auth.list-trusted-devices() / auth.revoke-device(id)`（消费：设置面、未来插件）
  - `auth.decide-consent(peer-info) -> accept|deny`（消费：file-transfer）
  - `auth.pairing-status() -> {qr, code, ttl, remaining}`（消费：宿主配对 UI 经命令面桥接）
- 导出能力（06 系统组件装配框架）：`host-auth` 同形导出（若需宿主中间件直接路由）；未导出可路由能力则仅互调消费

## 4. 分步实施（每步独立可验证）

### A0 —— wasi2 → wasi3 内核升级（前置工程，独立里程碑）

| 内容 | 验收 |
|---|---|
| A0-1 工具链：rustup/rustc 更新以提供 wasm32-wasip3（或源码构建 target；受阻则 A0 备选过渡：wasip2 产物跑 async store） | `rustup target add wasm32-wasip3` 可用；SDK fixture 可编译 |
| A0-2 wasmtime 47 → 48（LTS）双端升级（ADR 0019：双端 Cargo.toml/Cargo.lock + breaking changes 评估 + 编译/测试验证） | 双端 cargo test 全绿；升级与 wasi3 解耦（先升版本后切运行时） |
| A0-3 宿主运行时 async 化：component-model-async 启用 + `Store::new_async` + `call_async` + p3 async linker 替换 `p2::add_to_linker_sync`；既有 host_impl 适配（sync host fn 在 async store 下兼容性验证；燃料续费/ResourceLimiter async 语义） | wasip3 fixture 闭环：async 组件调用 `wasi:random`（0.3 async 形态）/`wasi:clocks` 成功；**既有 unknown-unknown 插件零回归** |
| A0-4 构建链：wasm32-wasip3 产物经 componentize/wit-component 编码验证（或升级 wasm-tools）；双端一致 | wasip3 组件可实例化；旧插件双轨并存（unknown-unknown 照常） |
| A0-5 既有 wasi2 用法清理：`p2::add_to_linker_sync`、wasip2 产物引用、相关注释文档 | 全仓无 wasi2 引用残留（除历史档案）；文档同步 |

> **A0 门禁**：A0-3 的 wasip3 fixture 闭环 + 旧插件零回归是硬门槛——wasmtime-wasi p3 模块标注实验性，未过门禁不得承诺 wasi3；失败则按 §10.6 回退策略处置（wasip2 + async store 过渡，wasmtime 48 上 wasip2/p3 接口已统一）。

> **2026-09-19 决策修订（用户指令，覆盖上表双端范围）**：之后所有**桌面端**插件（现有 4 个 + 新建）使用 wasi3；**移动端不变**（wasmtime 47 + p2 sync + wasm32-unknown-unknown 现状，恢复推进后再独立评估）。落地节奏：**先引入依赖/编译接口（工具链 + 构建链 target 支持），不改动宿主与插件代码**；宿主 async 化（A0-3）等运行时切换留待用户另行指示。工具链现状：wasm32-wasip3 在 stable 1.98.1 无预编译产物（需 LLVM 23 + rustup 更新或源码构建），A0-1 仍为前置专项。

> **2026-09-19 wasip3 编译 spike 验证（A0-1 预演，已通过）**：nightly 1.100.0（2026-09-18）+ `wasm32-wasip3` target 安装成功；最小 fixture（`/tmp/wasip3-spike`，依赖仓库 SDK `bedcode-plugin-api`）与**现有插件 file-transfer 均零代码改动编译通过**。关键发现：① **wasip3 target 的 cdylib 直接输出 Component 组件**（`\0asm` + `0d 00 01 00`，免 componentize/wit-component 步骤，构建链简化）；② wasmtime 48.0.2 成功解析产物：import = `bedcode:plugin/host-log` + 全套 `wasi:cli@0.3.0`/`wasi:clocks@0.3.0`，export 8 接口与 unknown-unknown 同构；③ 插件产物因 import wasi0.3 接口，实例化需宿主 p3 async linker（A0-3），**当前 p2 sync 宿主不可加载——产物仅编译链验证，不得替换 `resources/plugins/` 现行产物**。正式接入路径：等 stable 1.99（预编译产物自 2026-09-12 起 nightly present，预计 2026-10 中发布）→ `rustup update` + `target add wasm32-wasip3` → 构建链切 target。

> **2026-09-21 A0-3 前置验证（P1-P6 全部通过，实施依据 `.scratch/2026-09-21-a0-3-host-async/`）**：探针（`wasm_runtime/tests/a03_probe.rs`，7 用例）+ 报告（`report.md`）。关键结论：
>
> **P1 兼容性（三场景全绿，实测输出见 report.md §1）**：① sync host fn 在 async store 下兼容（P1-a）：同步注册的 bedcode host 原语（`func_wrap` 的 20 组接口）可被 wasip3 组件调用，`block_on_async` 桥三路径（多线程 block_in_place + 重入检测 / current_thread spawn 新线程 / 无 handle 线程 ambient 直接驱动）均不 panic；② wasip3 组件完整闭环（P1-b）：session 产物 `activate → session.status → 终端 hooks → manifest → deactivate` 全链路 OK；③ 生产产物零回归（P1-c）：resources/plugins 四产物**已全部为 wasip3 组件**（magic `0d 00 01 00`），在 async store 下全部加载 + manifest 往返——**无 unknown-unknown 残留可回归**，原「旧插件零回归」门槛落点变为「既有 wasip3 产物在 async store 上行为不变」（已证）。另实证：bindgen `exports: { default: async }` 下同步 `call` 在 async-required store 报错（`requires that *_async functions are used`），统一 async 调用面是唯一路径。
>
> **P2 资源限制 async 语义（探针断言 + 结论写回）**：燃料——默认引擎（consume_fuel=true）下 guest 指令计数在 async 调用内**跨 suspend/resume 累计**（消耗量随 spin iters 缩放，实测 1M→10M 净消耗等比例）；调用前 `exports()` 续费到 `fuel_budget`，`set_fuel(0)` 后调用仍成功；`consume_fuel=false` 引擎下 `set_fuel` 显性报错。内存——`ResourceLimiter` 在 async store 下强制生效：上限低于组件最小内存（fixture 17 页）时**实例化阶段被拒**（`memory minimum size of 17 pages exceeds memory limits`）；上限高于最小内存、低于工作集时**调用期 `memory_growing` 拒绝 → guest allocator abort → trap**（实测 4MiB 分配被 17 页+1KiB 上限拒绝）。
>
> **P3 同实例串行红线（主体实施硬约束，随 A0-3-main 立项时机械落实）**：① A0-3 主体实施后，**每插件实例同一时刻仍只允许一个 guest 调用在执行**——async 化只改变「宿主线程在等待时让出」，不引入同实例并发进入 guest；② `host.rs` 的 `Arc<Mutex<LoadedWasmPlugin>>`（std Mutex）async 化时改为 **tokio `Mutex`（await 持锁、不因等待释放）**，锁语义与现在等价（串行）；③ **禁止**改成「await 点释放锁」的细粒度锁（第二个调用会与第一个交错 → 插件静态状态竞态（配对码/QR/挑战注册表/config 缓存/私有库）+ wasmtime Store 重入 panic）。
>
> **P4/P5/P6**：13 个 sync 入口的 async 化影响面清单（调用方线程 / hot path / 成本）、测试适配计划 + 性能基线（端到端 guest 短调用 ~40µs/op，桥开销 ~0.14-0.22µs/op）、风险表复核无新增 blocker——全部见报告 §3/§4/§5。

### A —— wasip3 验证 + host-auth 原语

| 内容 | 验收 |
|---|---|
| A1 wasip3 构建链验证（承接 A0-3/A0-4）：SDK fixture 用 wasm32-wasip3 编译 → 编码 → 宿主 async 实例化，import `wasi:random`（0.3 async）/`wasi:clocks` 解析成功 | fixture 闭环：`get-random-bytes`（async）返回熵、时钟可读；既有插件零回归 |
| A2 WIT 新增 `host-auth`（secret-store 四函数）+ host_impl + 权限门（PERMISSION_AUTH）+ 属主校验 + 内存缓存 | 单测：set/get 属主隔离、越权拒绝、delete、明文不落日志；ABI bump 双端投影（ADR 0019） |
| A3 JWT 密钥治理：宿主 `utils/auth/jwt.rs` 改读 secret-store（首启随机生成） | 既有 jwt 单测全绿；重启后密钥稳定（持久化） |
| A4 wasip3 插件实例化的燃料/内存限额走 07 资源覆盖（async 语义下复核燃料续费） | 与 A1 合并验证 |

### B —— 认证中心插件工程

| 内容 | 验收 |
|---|---|
| B1 新建 `bedcode-desktop/plugins/devices/`（pluginType: rust-only 或 rust-ts 最小设置扩展点；**wasm32-wasip3** target，async 编排） | cargo test 插件单测全绿；manifest 声明 api + permissions（auth/storage/database） |
| B2 pairing 模块：配对码/QR token/JWT 签发校验策略迁移（逻辑从宿主 utils/auth 语义层平移，宿主留引擎） | 与宿主实现行为等价（对照测试：同一输入同输出） |
| B3 trust 模块：设备信任列表统一视图（DB pairings + peer trust_store 映射） | 列表/撤销行为等价 |
| B4 consent 模块：peer 首连确认决策（经互调被 file-transfer 消费） | consent 决策单测 + 互调闭环测试 |

### C —— 消费方迁移

| 内容 | 验收 |
|---|---|
| C1 file-transfer：peer consent/信任改互调 `auth.decide-consent` / `auth.list-trusted-devices` | file-transfer 单测 + 对等网络集成测试全绿 |
| C2 宿主命令面桥接：配对/QR/历史/设备列表命令改经认证中心（命令仍暴露给前端，实现转发） | 前端 pairing-flow.test.ts 迁移后行为等价 |
| C3 server 认证中间件：验签执行留宿主、策略取认证中心导出（06 框架） | jwt_auth 中间件单测全绿；连接建立流程集成测试 |

### D —— 退役与收尾

| 内容 | 验收 |
|---|---|
| D1 宿主命令面退役：system.rs 配对/历史、qr.rs、devices.rs（命令移除或改为薄转发占位） | 全量 cargo test + eslint + 前端测试全绿 |
| D2 行为等价回归：对等网络集成、配对流程、连接认证全链路 | 与 C 步骤基线无回退 |
| D3 文档同步：ADR 0022（认证语义入「可下沉」列）、AGENTS.md §8 修订、code-map、roadmap 阶段 2 标记 | 文档一致性检查 |

## 5. 测试矩阵

| 层 | 用例 |
|---|---|
| 宿主原语（A2） | secret 属主隔离 / 越权拒绝 / delete / 明文不落日志 / 重启持久化 / 权限门 deny |
| wasip3 链路（A0-3/A1） | async `get-random-bytes` 返回熵 / async 时钟可读 / 燃料限额生效（async 语义）/ 旧插件零回归 |
| 插件单测（B） | pairing 策略（TTL 边界、一次性语义）/ claims 组织 / trust 列表 / consent 决策（正反例）/ HS256 向量（RFC 7515 官方 test vector） |
| 互调（B4/C1） | file-transfer → auth.decide-consent 闭环 / 未声明 api 不可调（ADR 0017） |
| 集成（C/D） | 配对流程等价 / 连接认证等价 / 对等网络等价 / 认证中心未激活降级（宿主命令面兜底） |
| 前端 | pairing-flow.test.ts 迁移 / eslint 0 error / i18n 双文件同步 |

## 6. 风险与控制

- **启动顺序**：连接建立时认证中心必须已激活——06 框架保证系统/内置组件先于应用插件激活 + **降级**：认证中心未激活时宿主命令面兜底（双轨并存期 C2 前无单点）
- **wasmtime-wasi p3 实验性风险**：p3 模块标注 not ready for production——A0-3 验证门禁硬性把关；失败走 §10.6（wasip2 + async store 过渡，wasmtime 48 上接口已统一）
- **wasmtime 47 → 48 升级风险**：跨 minor breaking changes——与 wasi3 解耦实施（先升版本验证全绿，再切 p3 async）
- **燃料预算**：WASM 内签名计算消耗燃料，07 资源覆盖为认证中心调高预算（非热路径，握手一次）；async 语义下燃料续费复核（A4）
- **双轨一致性**：B/C 并存期两套实现需行为等价测试（对照向量）
- **wasip2 构建链**：A1 先行验证，失败则回退路线②（宿主 `secure-random` 原语 + unknown-unknown），spec 不受影响（接口面等价）
- **peer-net 私钥**：维持宿主托管，认证中心只做语义决策，不改 rustls 数据流
- **移动端**：仅 WIT/ABI 投影同步（A2），不消费不实现；wasip2 构建链延后

## 7. 依赖

- wasmtime **48（LTS，双端升级目标）**（ADR 0019；47 非 LTS 支持期将尽）+ component-model-async（引擎级，46+ 默认启用）
- wasmtime-wasi 48 `p3` 模块（**实验性标注，A0-3 验证门禁**；wasip2/p3 接口已统一）
- 06 系统组件装配框架（能力导出/依赖检查/只停不删）
- 07 资源覆盖（燃料预算，async 语义复核）
- wit-component 0.256（componentize，验证 wasip3 编码；必要时升级 wasm-tools）
- SDK/插件：`hmac` / `sha2` / `ed25519-dalek` / `getrandom`（wasip3 target 可用）加入认证中心插件依赖（SDK 核心不膨胀）

## 8. Out of Scope

- **wasi3 标准（0.3.x）的后续演进 / WASI 1.0**：跟踪，不在本规格范围
- **移动端认证中心 / wasip3 构建链**：延后（移动端仅 A0 的 WIT/ABI 与 wasmtime 48 升级双端同步；wasip3 运行时接线移动端随 A0 一并升级——双端 wasmtime 版本锁 48）
- **认证中心的用户界面**：headless；宿主设置面经命令面桥接读取状态（C2）
- **peer 传输/发现引擎**、**server 引擎**：不动
- **quick_actions（快捷操作）**：归属未定，本次不随迁（mapping.md Q3 遗留，另行裁决）

## 9. 里程碑

- M0（A0 完成）：wasmtime 48 双端升级 + wasip3 fixture 闭环 + 旧插件零回归 + wasi2 清理——双端 cargo test 全绿
- M1（A 完成）：host-auth 原语 + JWT 密钥治理——cargo test 双端全绿
- M2（B 完成）：认证中心插件行为等价宿主实现（对照测试通过）
- M3（C 完成）：file-transfer 与宿主命令面消费迁移，双轨并存
- M4（D 完成）：命令面退役 + 文档同步，roadmap 阶段 2 桌面端 ✅

## 10. 实施前终审清单（红线项，开工前确认）

1. **密钥托管边界**：JWT 密钥 / 配对种子明文不出宿主，secret-store 属主校验——确认
2. **验签执行点**：server 连接建立验签在宿主中间件执行（策略取认证中心）——确认
3. **AGENTS.md §8 配套修订**：「认证链路只走既有 auth 模块」→「密码学引擎/密钥留宿主 + 认证语义归认证中心插件」（§4 D3 落地）——确认
4. **移动端**：仅 WIT/ABI 投影，不消费不实现——确认
5. **插件形态**：headless（rust-only 后端，无 UI；设置面经宿主命令桥接）——确认
6. **A0 门禁/回退**：wasmtime-wasi p3 模块实验性——A0-3 验证门禁硬性把关；失败则回退「wasip2 产物 + async store」过渡（wasmtime 48 上 wasip2/p3 接口已统一，认证中心不受阻），**wasi3 仍为最终目标**——确认
