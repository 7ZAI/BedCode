# 移动端 WS 客户端引擎抽根：`bedcode-ws-client-engine` 能力 crate

> Date: 2026-10-09
> Status: **已实施**（用户指令：「移动端将 ws 客户端抽象出来作为能力 crate lib 包放到 packages 下，并封装成通用能力」；
> 落地记录见文末 §5，ADR = `docs/adr/0043-ws-client-engine-crate.md`）
> 前置：ADR 0041（移动端 host-websocket 客户端域，D1 明写「条件触发转 B：抽 client-only 子 crate 到根 `packages/`」）、
> ADR 0042 / 双端共享 lib spec（M1–M5：mDNS 引擎抽根的同款手法先例）、ADR 0035（能力域 crate 化）、ADR 0040（双端共享实现核）。

---

## 1. 目标

把移动端宿主 WS 出站连接引擎（现 `bedcode-mobile/packages/bedcode-wasm-core/src/manager/runtime/host_impl/ws.rs`，
约 1,350 行：句柄表 / 权限门 / 属主仲裁 / reader-writer 双任务 / 心跳 / 自动重连 /
帧信封 / 停用回收）抽成**仓库根 `packages/bedcode-ws-client-engine`** 的通用能力 crate：

- **默认形态 = 纯引擎 + 端口抽象**（AGENTS §0 路径基准 / §5 无业务内核）：零 WIT、零 SDK
  （双端 `bedcode_plugin_api*`）、零平台（tauri）、零机制内核（host-kit）依赖；
- 平台差异面（权限门 / 事件投递 / 宿主运行时任务 / jwt 代发 token / 重连退避策略）
  全部经端口 [`ports::WsClientPorts`] 注入；
- 移动端宿主收窄为**薄适配器 + `MobileWsClientPorts`**（与 M3 mDNS 收窄同形）。

**行为零变化**：WIT / ABI（移动 v17）零变更、5 原语签名与错误文案逐字保留、事件 topic 与
帧信封形状逐字保留、权限位（`ws:client`）与 fail-closed 语义不变、停用回收语义不变。

## 2. 边界（三条）

1. **只抽移动端实现**。桌面 `bedcode-server-websocket` 的客户端段仍与 actix 服务器栈 +
   `bedcode-server-core` 过滤器链同 crate 耦合，不并入本 crate（ADR 0041 D1 的桌面侧条件
   仍未满足）；两端「同构但分叉」的显式重复在客户端段继续存在，但移动端这一份自此是
   **通用 crate**（任何宿主可直接引用）。
2. **不做 wss / 不做编排**（ADR 0041 D5 保持）：仅 `ws://`，退避重连/心跳为引擎参数，
   订阅协议 / ack 编排归插件。
3. **服务端域不跟演**（ADR 0018/0041 D2 保持）：只有客户端 5 函数 + `purge_for_plugin`；
   不引入 `ws:server` 权限词汇。

## 3. 落地内容

| # | 落点 | 内容 |
| --- | --- | --- |
| 1 | `packages/bedcode-ws-client-engine/src/engine.rs` | 引擎主体：`connect`（async，握手 + 双任务 + 句柄登记 + open 事件）/ `send_text` / `send_binary` / `close` / `is_connected` / `purge_for_plugin`；心跳（30s Ping + 3× 静默判死）、自动重连（退避窗口钳制 + scheduled 事件 + 换句柄）、帧信封、close code 语义 |
| 2 | `.../src/ports.rs` | `WsClientPorts`（7 方法）+ `ReconnectPolicy` + `WsTask` / `BoxedTask` 任务面 |
| 3 | `.../src/wire.rs` + `wire/drift_lock.rs` | wire 词汇自持（事件名 / topic 拼法 / 帧 kind + 头长 / `ws:client` 权限字面量），漂移锁与移动 SDK `host/ws.rs` + `permission.rs` 行级比对 |
| 4 | `.../src/boundary_lock.rs` | 边界锁：生产源码零平台/SDK/wasm-core/host-kit/WIT 绑定层；生产依赖清单零内部 crate（治理 crate 规则：单测必须在 `src/` 内，无 crate 根 `tests/`） |
| 5 | `bedcode-mobile/packages/bedcode-wasm-core/src/manager/runtime/host_impl/ws.rs` | 收窄为薄适配器：5 原语 + purge 转发（`guarded_host_call` + `block_on_async` 留在宿主侧）+ `MobileWsClientPorts` |
| 6 | `bedcode-mobile/packages/bedcode-wasm-core/Cargo.toml` | 依赖 `bedcode-ws-client-engine`（path 上跳三级） |
| 7 | 锁与文档 | `mobile_host_websocket_client_domain_lock.rs`（修陈旧路径 + 覆盖新面）· fork 边界锁注释（共享锚点白名单）· `capability_crates_no_product_ids` 登记新 crate · 移动 code-map · CHANGELOG 双语 · ADR 0043 |

## 4. 门禁

1. 新 crate `cargo test`（引擎单测：权限门 fail-closed / 属主隔离 / 句柄生命周期真实握手 /
   close code 语义 / 队列满 fail-fast / purge 回收 / 入参校验 / 帧信封形状 / 重连窗口钳制 +
   漂移锁 + 边界锁）；
2. 移动 fork crate `cargo test --features test-support`（薄适配器 + 组件全链路
   `ws_client_domain_full_loop_with_real_component` 回归）；
3. 移动宿主 `cargo test`（全部集成目标，含 ws 客户端域边界锁）；
4. 变异自检（漂移锁 / 边界锁 / 适配器映射各至少 1 处）；
5. 文档联动与零 ABI / 零 WIT 变更核对。

---

## 5. 实施记录（2026-10-09）

**落地面**：§3 表 1–7 全部完成。引擎 13 例 + 漂移锁 3 例 + 边界锁 4 例住 crate 内；移动端
fork crate `host_impl/ws.rs` 收窄为 333 行薄适配器（-1,465 / +268 diff）+ `MobileWsClientPorts`；
ADR 0043、CHANGELOG 双语、两端 code-map、锁索引均已同步。

**门禁证据**：

| 门禁 | 结果 |
| --- | --- |
| 新 crate `cargo test` | **20 绿**（引擎 13 + 漂移 3 + 边界 4） |
| fork crate `cargo test --features test-support` | **285 + 4 + 4 绿**（含 `ws_client_domain_full_loop_with_real_component`） |
| 移动宿主 `cargo test` | **22 测试目标全绿**（245 lib + 全部集成目标） |
| 变异自检 | **3/3**：漂移锁改副本值 → 红；边界锁注入宿主类型名 → 红；适配器权限门旁路 → 红（全部还原 + sha256 核对逐字一致） |
| 零 ABI / 零 WIT | 本任务不触碰 `bedcode.wit` 与 SDK `abi.rs`（工作区内 WIT/abi 改动属同期 host-notify 票） |
| 桌面 `capability_crates_no_product_ids` | **未实跑**（磁盘 100% 满 + 桌面 target 已清空，全量重建需 10G+）；以等价预检脚本 `.scratch/2026-10-09-ws-client-engine-crate/gate-precheck-desktop-lock.py` 复刻该锁 C-3/C-4/C-4b/C-5 判据 → PASS（残余风险：脚本与 Rust 实现存在复刻偏差） |

**本票顺带修复的两处锁面**（均属抽根扫描面，非扩权）：

1. `retired_mobile_terminal_link_lock.rs`：保留面两条针脚随真源迁到引擎 crate
   （`jwt_auth == Some(true)` / `fn run_reconnect(`），并补适配器侧 `fn global_token(&self) -> String`；
   注意该锁的 `mobile_root()` 实为 `src-tauri`，根 `packages/` 路径需上跳两级。
2. `retired_mobile_session_control_face_lock.rs`：`request.get("jwtAuth")` 针脚被宿主
   `host_api/http_engine.rs` 的 rustfmt 折行打断（语义零变化），改钉格式不敏感形态 `.get("jwtAuth")`。

**发现但未处置（非本票面，如实上报）**：`bedcode-host-api-core`（9b26ab1e4 入库）与
`bedcode-headless-host-probe`（3015b392a 入库）两个 crate 既未登记进
`SCANNED_CRATES` 也未进 `PENDING_SCAN_CRATES` ⇒ 桌面该锁的 C-3 反向断言在现有 HEAD 即为红；
归属于各自票据补登记（本票只登记 `bedcode-ws-client-engine`）。
