# 09: 契约收口 —— 白名单锁、跨 crate 实证、路由表扩展、文档

**What to build:** 把前 8 票建立的能力装配机制**锁死成不可静默漂移的契约**，并补齐收尾面。四个交付：

1. **白名单锁**：自动发现的结果必须与树内白名单常量一致，双向断言（漏进即红 / 白名单有而未实现即红）。
2. **跨 crate 自动注册实证测试**：把 03 票建立机制的实测形态固化成自动化测试 —— 特别是「强制引用缺失 ⇒ 收集为空」这条，这是自动装配最容易静默失效的点。
3. **运行期替换能力扩表**：让插件侧也能提供部分已迁出的能力，补齐「运行期替换」缺口。
4. **文档与决策落档**：新 ADR + 双语变更日志 + 代码地图更新。

**Blocked by:** 08

**Status:** done（2026-10-05；账目见下）

- [x] 白名单锁存在且双向断言成立；强制引用行与白名单常量同处（不漂移）
- [x] 有一条测试钉住「未强制引用的能力 crate 收集结果为空、加上强制引用后才有值」
- [x] 能力模块缺失时**显性失败**：实例化期点名缺哪个能力域与依赖，**禁止**静默降级为「该能力不存在」
- [x] 运行期能力路由表已扩展到迁出的能力域，新增可路由能力与其路由方法**同表同步**（该表是闭表设计，两者必须同时改）
- [x] 扩表前已确认既有缺陷是否修复：路由转发曾只传键、不传调用方身份，导致系统组件代持存储能力时丢失调用方命名空间；若未修则**单独立票**而非顺手带过
- [x] 新 ADR 落档：记录能力模块契约、机制内核 crate 的双端锚点定位、以及**为何不推翻**「移动端能力面有意不对称」这条现行决策
- [x] `CHANGELOG.md` 与 `CHANGELOG_zh.md` 均有条目；桌面代码地图已按目录层级职责变化更新
- [x] 桌面与移动两端全量测试、前端 lint 零 error、防回接锁全套、crate 边界锁全绿
- [x] `cargo fmt` / `cargo clippy` 干净；测试后无残留后台进程与占用端口

## 交付账目（2026-10-05 结案）

### 1. 跨 crate 强制引用实证（第 2 项）

新增探针 crate `packages/bedcode-host-kit/fixtures/forced-link-probe`（只自报一个空壳能力模块）+
两个**测试二进制**（强制引用是链接期属性，同进程无法既「有」又「无」）：

| 文件 | 断言 |
| --- | --- |
| `tests/forced_link.rs` | 顶层 `use ... as _;` ⇒ 收集结果**恰好**等于探针自报那一项；描述符载荷（interface 路径）跨 crate 存活；装进 linker 后**反向取证**（同名函数再声明必报 `defined twice`，证明 `register` 真执行过） |
| `tests/forced_link_absent.rs` | 全篇不提及探针 ⇒ 收集结果**为空**（不是「不含某个名字」）；白名单锁点名缺项且错误串带得上 |

**变异自检**：删掉那行强制引用 → `forced_link.rs` **3 条全红**（此前第三条写成
「`install_all` 不报错」时它恒绿——空注册表也不报错，已改为反向取证）。
`lib.rs` 头注释原写「见 `tests/forced_link.rs`」而该文件**不存在**（文档失真，本次补上）。

### 2. fail-visible（第 3 项）

`verify_whitelist` 的错误串按两个方向分别给修法（missing ⇒ 查依赖与强制引用行；
unlisted ⇒ 补 review 过的白名单项或去掉该依赖）；`verify_host_module_registry(whitelist)`
作为可测入口（`add_to_linker` 的第一段），两条用例走**生产入口**：
`missing_capability_module_fails_loudly_with_remediation` /
`unlisted_capability_module_fails_loudly_with_remediation`。

### 3. 运行期路由扩表（第 4 项）

`host-mdns`（5 条）入表，与 `host-storage` 共用闭表形状 `(能力名, 方法前缀, 导出表)`：

| 层 | host-mdns |
| --- | --- |
| 能力域端口 | `DiscoveryPorts::forward_mdns_{browse,stop_browse,advertise,stop_advertise,is_advertising}` |
| 宿主转发函数 | `manager::capability::forward_mdns_*`（5） |
| 提供者窄端口 | `CapabilityTarget::mdns_*`（5） |
| guest 调用面 | `GuestOp::CapMdns*`（5）+ `OpKind` + 两条 dispatch 路径（mutex / event-loop） |

**闭表锁** `routable_capabilities_and_forward_methods_stay_in_sync` 四条判据：
表内能力名 ∈ 宿主原语能力表 · 导出全部可解析为 `ItemName` · `CapabilityTarget` 方法数
与导出数**逐项相等**（且每个方法都属于某个可路由能力）· 能力域端口的 `forward_<prefix>_*`
方法数 == 导出数。探测面改为「可路由 ∪ 仅探测」派生（原先两份表各列一遍 `host-storage`）。

**变异自检**：从可路由表删掉一条 mdns 导出 ⇒ 锁转红（left: 5, right: 4）。
反向方向（多一个无主方法）**未实测**——那需要对 trait 与 impl 各加一个方法再重编，成本高于
收益；断言本身是对同一份数据的直接扫描。

### 4. 缺陷确认 ⇒ 单独立票（第 5 项）

**未修，且范围比票面假设的更宽**：`CapabilityTarget` 只带参数、不带 `caller_plugin_id`
（该值在路由层**只**用于自调用判定），而每个能力域的真源都按 `plugin_id` 分区 ⇒ 转发后落到
**提供者**的分区 / 句柄属主，mdns 的事件还投到提供者的 topic（调用方根本收不到）。
取证与三个候选方案见 `issues/10-capability-forward-caller-identity.md`。

⇒ 本票因此**只接通机制、不开放入口**：`world plugin-system` **没有**增加 `export host-mdns`，
没有组件能提供那五个函数，路由在构造上不可达（故本期无行为变更、无线上影响）。

### 5. 票 08 结案时移交给本票的三条

- ✅ `crate_boundary_lock` 登记表并入机制内核 + 两个能力域 crate（**并带路径列**：kernel 落仓库根，
  只记名字的表会把它当成「拆分产物缺失」）—— 首轮即查出一条真边：三个传输 crate 都依赖
  `bedcode-host-kit`（各持一份 `plugin_binding`），已登记为允许的向下边 + 生产必需边；
- ⚠️ 描述符名词锁的判据**未放宽**（仍不命中复数 / 连写形式如 `sessions`）。理由：已试过的每种
  加宽规则都有既有的误伤前科（票 08 的 `database:main` 被 `ai` 误判；前缀匹配会误伤 `filesystem`
  一类机制名词）。按 AGENTS §0「不确定的取舍不猜」，此项**留给独立决策**，未在本票顺手改；
- ⚠️ `check-target-size.js` 的 `rootTargetDirs` 仍缺 `target/host-kits`（该文件是另一会话的在途
  改动，按 §11 不碰）。**留待该会话收尾时补一行**。

### 6. 验证账目

| 范围 | 结果 |
| --- | --- |
| 宿主 `cargo test --no-fail-fast` | `--lib` **820 passed / 1 failed / 1 ignored**；9 个集成 target **全绿** |
| 唯一失败 | 既有 `session_e2e::test_session_task_domain_closed_loop`（票 06/07/08 均已记账，陈旧断言，与本票零交集） |
| 与票 08 对账 | 816 → 820 = **+4**（2 条 fail-visible + 2 条路由闭表）✅ 无其他增减 |
| `bedcode-host-kit` | **16 passed**（11 旧 + 5 新：3 强制引用正例 / 2 反例） |
| `bedcode-discovery-engine` | **18 passed**（14 + 4 路由：转发全命中且引擎零副作用 / 未代持仍走引擎 / 提供者失败原样透传不回落 / 权限门先于转发） |
| 移动端 `cargo test --no-fail-fast` | 全绿（366 lib + 集成 target；移动端零改动） |
| `pnpm exec eslint .` | **0 errors**（115 warnings 全为既有，无前端改动） |
| clippy | host-kit / discovery-engine **零告警**；宿主 `--lib` 54 条**全部既有**，触达区间内零命中 |
| fmt | 本票触达的 7 个文件 + 两个 crate **干净**；宿主整树剩余漂移在他人在途文件（import 排序等），按 §11 不碰 |
| 未跑 | `cross-end-tests`（无跨端协议变动）、wasm 应用构建、`gen/android` Kotlin、闭表锁反向变异 |

### 7. 事故与外部干扰（记账）

- **磁盘三次逼近 0**（峰值 1.8G / 99%）：本票开始时 30G 可用，全量桌面测试后降到 1.8G。
  处置= 删 `target/host-kits`（纯缓存桶，票 07 的教训：本桶会被 GTK 栈重编一遍）→ 7.3G →
  跑完移动端与 kit 重编又回到 2.9G。**未删 sccache 缓存**（4.8G）以免拖慢他人重编。
  后续接手者：**跑全量前先看 `df -h`**，本链新增两个 target 落点（`target/host-kits` /
  `target/server-libs`）都在 AGENTS §3 的治理表里。
- 未触碰其他会话的在途改动（`scripts/check-target-size.js`、宿主整树既有的 fmt/clippy 漂移）。

## Comments
