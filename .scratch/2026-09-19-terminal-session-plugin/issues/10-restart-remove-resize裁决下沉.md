# 10: restart / remove / rename / resize 裁决下沉

**父规格:** `.scratch/2026-09-19-terminal-session-plugin/spec.md` D4 / D8-P2

**What to build:** 插件补齐今天做不到的四个会话动作：重启、移除、改名，以及尺寸冲突裁决。裁决**规则**（谁是正统端、冲突时听谁的、覆盖确认何时出现）进插件，「当前渲染端是谁」的登记事实留内核。用户在桌面重命名/重启/删除会话与多端争用尺寸时的表现与今天一致。

**Blocked by:** 09

**Status:** done（2026-09-20；四项原语 + 插件编排/裁决 + 宿主命令面薄转发 + 真实 wasm 闭环，含「插件路径 vs 内核直连路径」事件与状态对照；全量门禁绿。**与并行实施的票 11 合并后已复验**，双方接线全部在位，见 §5）

## 实施记录（2026-09-20）

### 1. WIT / SDK / 宿主契约（四函数级追加，ABI 不 bump——仍 desktop 19）

`host-session` 追加四原语（函数级追加不 bump）：

| 原语 | 语义 | 执行方式 |
| --- | --- | --- |
| `restart(session-id)` | 宿主执行器 `SessionManager::restart_session`（移除 + **同一 id** 重建并启动，正统端回到启动端）保留 | **异步**（Creating/Created 回灌同实例需锁释放，理由同 `create`/`close`） |
| `remove(session-id)` | 注册表清理（输出管理器 / PTY / 记录 / 正统端归属）+ 删除同步事件；未知 id 幂等成功 | 同步（失败对调用方可见） |
| `rename(session-id, name)` | 改名 → 返回**改名前的名字**；未知 id / 空名显性报错 | 同步 |
| `resize(session-id, cols, rows, requester-json)` | **只登记与执行**（透传 winsize + 置归属为请求方），**不裁决** → 返回 `{previousCanonical, canonical}` | 同步 |

- **登记事实的读取通道**：`host-session.get` 回执**增量追加** `canonicalRenderer`（无归属 `null`，wire 形状同宿主 `RendererSource`）——插件据此判四态，不另开读原语。
- `resize` 实现刻意复用内核执行器并带覆盖信号（`force = true`）：内核裁决分支因此不参与本原语（那是插件职责），而执行 / 登记 / 失败语义（未知会话 `NotFound`、PTY master 不可用报错）与迁移前逐字一致。
- SDK：`HostSession` trait + `WasmHost` 四方法；宿主 `component.rs` 四处接线。

### 2. 插件侧：`plugins/session/rust/src/actions.rs`（新域）

- **裁决规则纯函数** `decide_resize(current, requester, force)` 四态单测：**无渲染端**（首个请求方即位正统）/ **单端**（归属 = 请求方）/ **多端争用**（他端未 force → 需覆盖确认，**零改动**）/ **端接管**（force → 应用并移交）。
  - **「端下线」的落法（有意保守）**：判定原正统端是否已下线需要连接清单（spec D4 归票 11 `connections-list`），本票**不臆测存活状态**——无法判定一律按「仍在渲染」处理（要求确认），绝不静默抢占在线端；票 11 落 `connections-list` 后由同一规则消费该事实（届时端下线 → 直接接管）。第 4 态当前即「显式 force（用户确认覆盖）」。
- **编排**：`restart/remove/rename` 均先 `get` 会话做**存在性预检**——把「会话不存在」变成同步可见的失败（`restart` 原语本身异步，宿主侧失败只能落日志）。
- **wire 模型**：`RendererSource` / `ResizeOutcome` 与宿主 serde 同形（`rename_all_fields = "camelCase"` 必须：漏了 `deviceName` 会以 snake_case 出网，宿主反序列化即失败）。
- api 四项（`session-restart` / `session-remove` / `session-rename` / `session-resize`，15 → **19** 项）+ 命令面四项（`session.action.*`）+ `session.status` 的 domains 增 `session` 域。
- 权限面**不变**（复用已声明的 `session:read` / `session:write`；无新权限位 → 无五同步点新增）。

### 3. 宿主命令面薄转发（行为等价，含降级）

新增 `utils/session_action_bridge.rs`（探活 + JSON-RPC 互调 + 降级 `warn`，与 auth / config / create 桥接同构）；`commands/session.rs` 三条命令改经桥接、失败降级宿主执行器：

| 命令 | 插件可用 | 插件不可用（降级） |
| --- | --- | --- |
| `delete_session` | 插件编排（存在性预检 → `remove`） | `sm.remove_session`（原行为） |
| `restart_session` | 插件编排（→ `restart`） | `sm.restart_session`（插件侧同一执行端） |
| `resize_session` | 插件**裁决**（仅在可应用时 `resize`） | `sm.resize_session`（内核裁决分支） |
| （无命令） | `session-rename` 仅落 api 面——桌面无改名 UI，消费方随票 13 会话视图落地 | — |

- **内核裁决分支保留**：`SessionManager::resize_session` 不动——移动端 HTTP / WS 路径（`server/services/session_control.rs`）与降级轨仍直连它（「宿主执行器保留」口径 + 移动端零改动前提）。两处规则同一（四态对照测试）。
- 命令签名 / 返回形状不变 → 前端零改动（`resize_session` 仍返回 `ResizeOutcome`，由桥接从插件回执反序列化）。

### 4. 测试与验证

- 宿主 `host_impl::session` 新增 7 项：四原语权限门先于参数处理、参数门矩阵（空 id / 空名 / 非正尺寸 / 非法 requester，且零副作用）、移除幂等 + 归属随会话清理、改名闭环 + 未知会话显性错、`resize` 登记与执行（**不裁决**：他端请求照样执行并移交）、`get` 携带 `canonicalRenderer`、重启受理语义。
- **真实 wasm 闭环** `test_session_actions_closed_loop`（真实产物 + 真实原语 + 真实私有库）：改名（回执原名 + 未知会话降级）→ 裁决四态（无渲染端 applied / 单端 applied / 多端争用 `needsConfirmation` **且内核登记零改动** / force 接管并移交）→ 重启（同 id / 名字与 configId 保持 / 归属回启动端 / 生命周期事件序列 `Creating → Created`）→ **与内核直连路径对照等价**（同事件序列 + 同状态字段）→ 移除（记录与归属消失）→ 降级四条（注销互调面后桥接返回 `None`）。
- 插件 crate `actions` 域 11 项：四态裁决矩阵 + 登记事实解析（含形状非法显性报错）+ wire 形状锁定 + 请求解析（含「缺 `requester` 必须报错，不能默认成桌面端冒充请求方」）。
- 引脚同步（能力面增长的合法面变更）：`plugins/session/plugin.json`、插件 Rust 契约用例（19 项）、前端 `plugin-contract.test.ts`、宿主 `test_session_plugin_artifact_lifecycle`（19 项 + 会话动作 api 必须在声明面里）。

### 5. 门禁取证

**票 10 单独（pi 未介入前）**：桌面 `cargo test --no-fail-fast` lib **1041/0**（1033 + 8）+ 8 集成目标全 ok + doc 1 passed/2 ignored；插件 session **119/0**（108 + 11）、file-transfer 58/0、auto-task 24/0；SDK **85/0**（含 `test_abi_version_is_v19`——本票不 bump）；前端 **74 files / 718**；根 eslint **0 error / 123 warning**；session + file-transfer wasm 产物重建并入 `resources/`。

**与票 11（pi）合并后复验（2026-09-20 04:0x）**：

| 门禁 | 结果 |
| --- | --- |
| 桌面 `cargo test --no-fail-fast` | lib **1050 passed / 0 failed**（我的 1041 + 票 11 的 9）+ `src/main.rs` 0 + 8 个集成目标全 ok + doc-tests 1 passed / 2 ignored，**零 flake** |
| 插件 crate `cargo test` | session **123 / 0**（含票 11 新增）；file-transfer **58 / 0**；auto-task **24 / 0** |
| SDK crate `cargo test` | **85 / 0** |
| 前端 `node node_modules/vitest/vitest.mjs run` | **74 files / 718 tests 全绿**（EXIT=0；首轮曾有 1 例 5s 超时——票 11 新增的 router「error 态深链兜底」用例在机器满载时超时，单跑与空载复跑均绿） |
| 根 `node node_modules/eslint/bin/eslint.js .` | **0 error / 123 warning**（全为存量） |
| 产物一致性 | session 源 manifest / 产物 `plugin.json` / wasm 内嵌 manifest 均 **21 项 api**（我的 19 + 票 11 的 `annotate` / `devices-connect-list`）；产物 mtime 晚于全部插件源改动；file-transfer 产物晚于其唯一源改动（`auth_center.rs`） |

- **交叉核验**：票 10 的四处接线（WIT 四函数 / `component.rs` 四 impl / `host_impl/session.rs` 四实现 / `actions.rs` + `session_action_bridge.rs`）在票 11 落地后逐点复查**均在位未被覆盖**；`SessionManager::rename_session` 仍在（`session_manager.rs:967`），闭环用例随全量套件绿。

### 6. 实施期发现与取舍（需收尾票 / 后续票处理）

1. **票 09 遗漏：`file-transfer` 消费方 trait 未同步 `session-create`** → wasm 构建期防漂移比对必红（票 09 未重建该插件产物，故一直未暴露；本票重建时才炸）。已一并补登 `session-create` + 本票四项（trait 与 manifest 19 项精确一致）。
2. **测试基建修正**：`setup_wasm_runtime` 里会话管理器与配置管理器各持一个无 schema 的内存库——重启执行端要读配置（`SessionStorage::get_config`），非共用同一库时恒报 `Config not found` / `no such table`。已改为**共用同一内核库 + init_schema**（生产同构：单主库）。`host_impl::tests::build_host_ctx` 未改（本票单测改用 `create_session_from_spec` 播种会话，不碰配置表）。
3. **PTY master 前提**：`resize` 需要已启动的会话（master 就绪）——「只创建不启动」的会话 resize 显性报 `PTY master 不可用`（既有语义）。闭环里显式走 `start_existing_session`（**不做归属登记** → 正统端仍为空，正好是裁决态 1 的起点）。
4. **重启即启动**：`restart_session` 真实 spawn；闭环中进程由 `remove` 的句柄 drop 终止（已确证 `PtySession::drop` 在最后一个引用时 kill）。完成信号用 `subscribe_restart()` 广播而非轮询存在性（重启期间旧记录已移除、存在性轮询会提前通过而读到重启前状态——实测踩到）。
5. **改名不发新线协议事件**（D1 自守边界：线协议形状不变）：改名结果经会话列表拉取可见；桌面 UI 与移动端刷新随票 13 / 后置适配专项。
6. **`manifest-gen.js` 的 `RUST_PERMISSION_RULES` 缺 `session_*` 一条**（`session_create` / `session_close` 早已缺）：新插件经 SDK 调四原语不会自动推导 `session:write`。存量缺口，不属本票；建议与 `session:*` 面无业务语义整理一起补。
7. **既有 flake 观察**（与本票无关）：`pty::pty_process::tests::session_with_private_sink_bypasses_business_output_bus` 在**全量并行**负载下偶发失败（`自备 sink 应收到进程输出`），单跑稳定通过——该用例在「收到终态事件」与「sink 写落」之间没有同步点，属既有竞态（本票新增测试 spawn 进程提高并行负载从而暴露）。未改该用例（越界）。
8. **文档计数（归票 18）**：`host-session` 面（配置面 + 四动作原语）在 `bedcode-desktop/docs/code-map.md` 无独立小节；AGENTS §7 的 ABI 计数仍写 v17（应为 v19）。
9. 移动端零改动：`host-session` 新函数不跟演（desktop 独有，ADR 0022 双端偏离），会话动作与尺寸裁决的移动端路径仍走宿主 `SessionManager` 执行器 —— **行为不变**（这也是本票保留内核裁决分支的理由）。

---

- [x] `host-session` 追加重启、移除、改名、带请求端标识的尺寸调整四项原语（均为既有 interface 函数级追加；注解写入与连接清单归 11）
- [x] 尺寸裁决规则搬入插件：正统端判定与覆盖确认策略在插件侧决策，内核只提供登记与执行；单测覆盖「无渲染端 / 单端 / 多端争用 / 端下线」四态
      —— 「端下线」以显式 force 承载力（自动判定需票 11 的连接清单，见 §2 说明）
- [x] 重启与移除的编排（事件顺序、注册表清理、失败可见）搬入插件，宿主执行器保留；对照测试证明事件与状态序列等价
- [x] 原语全部先权限门再属主，缺权限与非法参数可见报错
- [x] 宿主真实 wasm 闭环补会话动作成功路径用例（重启 / 移除 / 改名 / 尺寸裁决），与既有原语闭环矩阵同形态
- [x] 桌面 `cargo test` + `pnpm run test:run` 全绿；测试后清理残留进程
