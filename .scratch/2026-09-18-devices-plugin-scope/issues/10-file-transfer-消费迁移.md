# 10: file-transfer 消费迁移（C1）

**What to build:** file-transfer 的 peer consent / 信任管理改经互调认证中心 API（`auth.decide-consent` / `auth.list-trusted-devices`），移除本地实现；行为与迁移前等价。

**Blocked by:** 09

**Status:** done（2026-09-19；验证：插件 58/58、宿主 lib 997/0 全绿）

- [x] file-transfer 单测 + 对等网络集成测试全绿
- [x] 与迁移前行为等价（对照测试：同一场景同一决策）
- [x] 双轨并存期宿主实现保留作对照基线（无单点，降级兜底）

**实施记录（2026-09-19）：**

- **新模块 `rust/src/auth_center.rs`**：file-transfer 经互调消费认证中心（`com.bedcode.devices`）——
  - wire 契约类型镜像 devices consent/model.rs（serde camelCase/snake_case 逐字段对齐，
    插件不互相依赖 crate，契约唯一真源 = JSON wire）
  - `#[plugin_api(manifest = "../../devices/plugin.json")]` 防漂移声明（构建期与 devices
    manifest api 精确比对；native 构建跳过宏展开，仅 wasm 目标执行比对）
  - **consent 两阶段流**：阶段 1（`peer:consent` 事件到达）`auth.decide-consent` 无意向
    预检——已信任免确认自动放行（不弹窗）；未知 → ask 照旧弹窗。阶段 2（
    `file-transfer.respond-consent` 回传意向）最终 accept/deny → 应答宿主 peer 引擎
    （引擎原语留宿主，插件只做决策映射）
  - **信任列表**：`file-transfer.list-trusted` 经 `auth.list-trusted-devices` 统一视图，
    仅取 kind=peer 段映射回旧 TrustedPeerDto 数组 wire（前端零改动、行为等价）；
    peerError 透出不静默空列表（与迁移前直查宿主报错的场景等价）
  - **待确认登记表**（requestId → 对端信息，阶段 2 决策数据源）：static Mutex + FIFO
    容量封顶（wasip3 thread_local 隔离教训）；核心操作抽纯函数 `remember_in/take_in`
    供 native 直测（并行测试共享静态表，容量/淘汰断言须确定性）
  - **双轨降级（无单点）**：认证中心不可用（未激活/超时/门禁拒绝）或无登记请求 →
    直答宿主 / 直查宿主（迁移前行为）；带显式意向仍返回 ask（协议异常）→ 显性报错
  - 编排逻辑经 `AuthCenterGateway` / `PeerRespondOps` trait 注入假实现 native 直测（
    对照测试：同一对端同一决策——trusted→自动放行、未知→ask、accept/deny 映射、
    降级路径逐场景等价断言）；wasm 真实现经 cfg(wasm32) 分支
- **lib.rs**：`peer:consent` 处理、`respond-consent` / `list-trusted` 命令改走 auth_center；
  `revoke-trusted` 维持宿主原语（认证中心互调 api 未声明 revoke，与 peer 段共享宿主
  trust store 数据一致）
- **宿主闭环 `test_filetransfer_consumes_auth_center_closed_loop`**：真实 file-transfer +
  devices 双产物实例 → 总线发布 peer:consent → wire 层静态捕获断言 JSON-RPC 形状
  （阶段 1 无 userDecision / 阶段 2 accept↔true、deny↔false、requestId/nodeId 沿事件桥
  登记传递）；认证中心注销 → 降级直答宿主且不发出版互调；list-trusted 经统一视图、
  peerError 透出；revoke 不走互调
- **既有测试修复**：file-transfer MockHost 缺 `plugin_db_execute_batch`（SDK 已提交新增
  trait 方法，插件测试此前编译不过）→ 补实现（预存断裂，非本票引入）
- **验证**：插件 **58/58**（auth_center 22 新增）、**0 新 warning**（仅余 pre-existing
  `entry_from_dto` dead code + `tracing` 未用依赖）；wasip3 产物构建 + 防漂移比对通过并
  更新 resources artifact；宿主 lib **997 passed / 0 failed**（含新闭环测试）；其余集成
  测试全绿
- **注意（非本票）**：集成测试 `broadcast_and_shutdown_flow` 失败 = 在途 auth 线
  `utils/auth/jwt.rs` 未提交改动新增 ERROR 日志（secret store 不可用回退），属并发线
  存量问题，本票未触碰；`src-tauri/target` 17G > 15G，下次构建前建议 cargo clean