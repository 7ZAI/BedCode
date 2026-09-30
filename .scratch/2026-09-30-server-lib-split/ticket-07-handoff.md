# 票 07 完成交接（server-lib-split 实施票 07）

Status: ✅ 完成（跨端 7/8 绿；唯一红的 8th 是本票**未触及**的真实产品缺陷，见 §4）
Date: 2026-09-30
spec: `.scratch/2026-09-30-server-lib-split/spec.md` §7.4（本次实施裁决已写进 spec）

---

## 1. 改了什么（七个文件）

| 文件 | 改动 |
| --- | --- |
| `src-tauri/src/server/composition.rs` | 新增 `install_server_ports()`——全局端口注册表的**唯一**装配点 |
| `src-tauri/src/server/crate_boundary_lock.rs` | **新增**：8 项 crate 边界锁（5 断言 + 3 判据契约例） |
| `src-tauri/src/server.rs` | 挂 `#[cfg(test)] pub(crate) mod crate_boundary_lock` + 模块头补票 07 |
| `src-tauri/src/lib.rs` | bootstrap 改调 `install_server_ports()`（唯一行改动） |
| `src-tauri/tests/{pty_session_chain,http_auth_biometric,ws_auth_rules,broadcast_shutdown}.rs` | 4 处内联 `ports::init(assemble())` → `install_server_ports()` |
| `src-tauri/src/wasm_core/manager/host/tests/wasm_flow_test.rs` | 3 个退役面锁的扫描面扩到六个 crate（抽 `collect_retired_domain_rs_files()` 共用遍历） |
| `src-tauri/src/wasm_core/manager/host/tests/l2_gating_test.rs` | `L2_SCAN_ROOTS` 补 core / peer-net / crypto-engine（`base` 仍排除） |
| `src-tauri/src/wasm_core/manager/host/api_bridge.rs` | 会话命令面锁扫描面同样扩到六个 crate + 跳过 crate 侧结构锁自身 + 记 M6 判据边界 |
| `cross-end-tests/Cargo.toml` / `tests/common/desktop_ctx.rs` / `tests/harness_selfcheck.rs` | rig 走同一装配面 + 装配顺序对齐 GUI + 自检新增第 2 段 |

## 2. 头号前置（票 06 遗留的阻塞）已解：跨端 0/7 → 7/8

根因（与票 06 记录一致）：rig 只建 `AppContext` 不装端口 → 网关
`ports::get() == None` 判 `PassThrough` → 插件登记的宿主别名全 404。

**实施比 spec 更强一步**：spec 写的是「给 cross-end-tests 加依赖 + 在 rig 里调
`ports::init(assemble())`」；实施把装配**收进宿主组合根的单一函数**，GUI 与 5 个
harness 全调它。理由：spec 的写法让两个装配面「文本相同、结构可漂移」，单一函数让
漂移不可能。`bedcode-server-base` 依赖仍加，但**只用于自检直读**（判据不经过被测代码）。

**rig 装配顺序也改了**：建 AppContext → 装端口 → 激活插件（生产 bootstrap 同款）。
① `assemble()` 取总线走 `AppContext::try_global()`，早一步会装**占位空总线**
（插件 `bus-subscribe` 永收不到互调，表现为 5s 超时而非订阅竞态）；
② 激活期要登记端点，端点表与 `PluginInvoker` 此刻都该就位。

## 3. 锁的变异自检（M1-M7 全杀，M6 未杀已记为边界）

| 探针 | 手法 | 结果 |
| --- | --- | --- |
| M1 | http `[dev-dependencies]` 加 `bedcode-server-websocket` | 🔴 **编译通过**、锁报「横向/越级边违反」——最真实的越线形态 |
| M2 | http 加 `bedcode-server-peer-net`（面 → 引擎域） | 🔴 |
| M3 | 登记表加一个不存在的 crate | 🔴 同时打出「拆分产物缺失」+「宿主清单缺少」，证明枚举非空转 |
| M4 | 宿主另建同时引用两面 crate 的生产模块（`host_port.rs` 旁路） | 🔴 |
| M5 | peer-net 里加 `HISTORY_CAP:` | 🔴 打红并点名 `packages/…/lib.rs:1828` |
| M6 | WS 面里加**裸** `list_sessions` | 🟢 **未杀**（见下） |
| M6b | WS 面里加 `commands::list_sessions` 形态 | 🔴 |
| M7 | peer-net 里加 `ports.get().auth_center.enforce_connection_policy(...)` | 🔴 L2 gating 锁打红 |

**M6 已写进锁的文档注释**：会话命令面锁的 Rust 侧 needle 只认 `commands::` 限定
形态，面 crate 里的裸名不被拦（Rust 裸名会大量误中无关标识符；裸名函数进不了
`generate_handler!` 也就调不到）。兜底是对偶退役面 `retired_session_observation_*`。

## 4. ⚠️ 新发现：**本票未修**，需单独立票

`cross-end-tests/tests/terminal_output_pressure.rs` 阶段 A（C-101「客户端正常确认时
输出零缺口」）**三次运行稳定失败**：

```
缺口位置=[5404, 8383]，示例=[(6640, Some(11298)), (14276, Some(34069))]
缺口位置=[6500, 7989, 7990]，示例=[(8553, Some(13093)), (14581, Some(1)), (1, Some(33764))]
缺口位置=[4642, 7621]，示例=[(6064, Some(10517)), (13495, Some(33791))]
```

- **不是本票引入**：端口未装时 `/api/sessions/*/input` 走路由表 404，产出命令送不到
  插件，阶段 A 只会「收齐末序号超时」。这条用例在本票之前从未跑到这一步。
- **不是测试前提错**：`SESSION_PTY_RING_BYTES` = 4 MiB，阶段 A 产出 ≈ 360 KiB，
  `PtyRing` 不可能淘汰 ⇒ 缺口只可能来自链路某一跳的**静默丢弃**（游标越过未拉取字节），
  且 `resync_count = 0` ⇒ **无 fail-visible 信号**。
- **待查线索**：第二次运行出现 `(14581, Some(1))` 与 `(1, Some(33764))`——序号**中途
  回落到 1 再跳到 33764**。阶段 A 只有一个会话，所以不像环重锚（重锚应从最旧存活字节
  续拉 = 大序号 +1），更像「另一条流的字节串进同一 recorder」或「重基准帧被按旧偏移
  切片」。入口：移动端 `TerminalLinkManager` 帧→会话路由 × 插件 `ws_terminal`/`output`
  游标推进的并发交互。
- 按 §8「真源换了地方就要 fail-visible」：**红灯保持亮着**，不静默降级、不放宽断言。

## 5. 验证记录

- 宿主 `cargo test --no-fail-fast`：**881 lib 绿 / 0 红 / 1 ignored** + 8 个集成 target 全绿
  （`broadcast_shutdown` / `build_manifest_smoke` / `error_envelope_integration` /
  `http_auth_biometric` / `link_crypto_http` / `pty_session_chain` / `server_integration` /
  `ws_auth_rules`）
- 锁相关 19 项（8 新 + 3 退役面 + L2 + api_bridge + 杂）全绿
- `cross-end-tests`：7/8 绿（fail_closed_flow / harness_selfcheck / jwt_rotate_reconnect /
  lifecycle_flow / pairing_auth_flow / session_http_flow / terminal_ws_flow）；
  `terminal_output_pressure` 红（见 §4）
- 六个 server lib crate：本票零改动，未复跑（票 06 记录 194 项绿）
- `rustfmt --check`：新增/改动文件全 OK（`api_bridge.rs` / `wasm_flow_test.rs` /
  `server.rs` 的既有 fmt 差异与 HEAD 完全同数同位，**未顺手重排**）
- `pnpm exec eslint .`：**0 error** / 117 warning（与票 06 同数，无回归）
- 无遗留监听端口 / 测试进程；清了 168 个 `/tmp/bedcode-*userplugins-*` 残留

## 6. 未做（明确划界）

- **§4 的终端输出丢字节缺陷**（产品缺陷 + 需要的产品级归因，不在票 07 范围）
- 票 08：测试迁移 + 插件侧集成测试试点（D10）
- 票 09：文档（code-map / AGENTS §3 / `check-target-size.js` 补仓库根 `target/`）
- `pnpm run tauri:dev` 冒烟（无 GUI 环境）
- 移动端 `cargo test` 全量（本票零移动端改动）
