# 03: broadcast_sync 切统一 publish，Handler 瘦身（migrate）

**What to build:** 插件事件**只有一条**进入 WS 广播的宿主路径：`broadcast_sync` → `publish(HostSyncEvent)` → 瘦 `SyncEventHandler`（信封 + 源设备排除 + 广播）。Handler 不再按会话/任务变体做业务 match，不再 Debug 重格式化状态；票 09 的必填校验前移到插件产出口。移动端收到的同步推送与迁移前逐字节一致（排除语义、`SessionRemoved` 空名照发等行为锁保持）。

**Blocked by:** 02（AppEvent + HostSyncEvent + 线格式对齐双轨就绪）.

**Status:** done（2026-09-24 落地并全量门禁绿：宿主 lib 1044/0、7 个集成 target 全绿、
SDK 169/0、插件 304/0。防回接锁已做变异自检。票末记一处票面与 ABI 的冲突记账）

- [x] `wasm_core/host_api/events.rs::broadcast_sync` 改为反序列化 `SyncEvent` → `validate` → `publish(HostSyncEvent)`；权限门 `broadcast` 不变
- [x] `AppContext.sync_tx` / `lib.rs` 装配切到新事件类型（或直接 matcher `publish`）；容量常量语义不变
- [x] `SyncEventHandler` 瘦身：`to_sync_payload` + `source_device` 排除 + `Message::sync_data`；**删除** 11 分支字段搬运与状态 `format!("{:?}")`
- [x] 必填字段校验前移插件 `broadcast_sync` 调用前；宿主 `validate` 失败 → WASM `Err`，不静默丢（**ABI 限制见票末第三节**）
- [x] 集成测试 `broadcast_shutdown` / `pty_session_chain` / `http_auth_biometric` / `ws_auth_rules` 切新路径全绿
- [x] 形为锁：源设备排除、`SessionRemoved` 幂等广播、缺字段不伪造推送
- [x] 旧 `From` 生产调用点清零（类型可仍编译，供 04 删）；针对性测试 + 相关集成 target 绿

---

## 实施记录（2026-09-24）

### 一、生产路径迁移

| 触点 | 迁移 |
| --- | --- |
| `wasm_core/host_api/events.rs::broadcast_sync` | 解析 `SyncEvent` → `HostSyncEvent::from` → `block_on_async(events::publish(..))`；`Err` 一律带操作上下文。**删掉**「取 `AppContext.sync_tx` + 启动早期静默丢弃」那段：改由 `publish` 的 `NoSource` 显性失败（装配缺失不再是 Ok） |
| `events/sync_handler.rs` | 实现段只剩三步：折载荷 → 取源设备 → 广播。删 8 个 `handle_*` 方法（含 11 处字段搬运）、`format!("{:?}").to_lowercase()` 的状态重格式化、以及 `info!("Processing event: {:?}", event)`（那是对业务枚举的 Debug 倾倒）；广播失败改结构化 `error!`（`device` / `error` 字段） |
| `system/app_context.rs` + `lib.rs` | `sync_tx: broadcast::Sender<HostSyncEvent>`（保留字段：lib.rs 拿它 `register_source::<HostSyncEvent>`，`publish` 按类型查这张表）；`SYNC_EVENT_BROADCAST_CAPACITY = 64` 语义不变，仅注释改指新类型 |
| `tests/{broadcast_shutdown,pty_session_chain,http_auth_biometric,ws_auth_rules}.rs` | 类型整体切换（含 `broadcast_shutdown` 头部对触发路径的描述改成「插件发 `SyncEvent::SessionRemoved` → 宿主 publish → 处理器排除发送者」） |

`block_on_async` 用的是 `wasm_core/runtime_util` 里既有的同步↔async 桥
（`host_api/fs.rs`、`host_api/status.rs` 同一条路），没有为事件面新开桥。

### 二、票 09 三条「缺字段不广播」的去处

类型化载荷（票 02）之后，「缺会话概要 / 缺会话名」这类事件**在插件侧根本构造不出来**，
所以：

- 退役：`incomplete_session_payloads_are_not_broadcast`（条件不可达，留着就是假绿测试）。
- 替代锁，三处：
  1. SDK `test_sync_event_variants_align_payload_shapes` —— 同构折算失败即 panic；
  2. `HostSyncEvent::validate` —— 折不成出站形状 → `publish` 在**投递前** `Err`；
  3. 插件 `launch.rs` —— `summary_for` 返回 `Err` 时 warn + 跳过广播（票 02）。
- **未新增抑制**：`session_name` 为空串的 Stopped / Removed 历史上照广播（Removed 的
  「未知会话仍广播移除」是 P1-b 行为锁），本票按字节保持不变，
  `session_removed_with_empty_name_still_broadcasts` 继续钉住。

新的处理器测试面（13 条）除行为透出外，还把出站 JSON 逐字钉住：
`session_stopped_broadcasts_without_envelope_field_on_wire`（信封字段不出站）、
`task_queue_changed_omits_absent_optional_keys`（缺省键不出现）、
`session_status_changed_passes_wire_status_through`（`waitingInput` 原样透传，
取代旧 Debug 垃圾口径）。

### 三、票面与 ABI 的冲突（重要记账）

票面写「宿主 `validate` 失败 → WASM `Err`」。实测 WIT：

```wit
interface host-events {
    emit: func(event-name: string, payload-json: string);
    broadcast-sync: func(event-json: string);          // ← 无返回值
    notify: func(title: string, body: string) -> result<_, string>;
}
```

`broadcast-sync` 是 **unit 返回**，而 D5 定死本专项不动 WIT/ABI
（`host-events.broadcast-sync` 签名保持 `event-json: string`）。所以宿主侧的
`Err` 只能落在**宿主日志**：`runtime/component.rs:421` 的导入壳
`tracing::error!(error = %e, "host_events.broadcast_sync failed")` —— 插件看不到异常。

据此，票 03 的「不静默丢」按可达口径成立：畸形/旧格式/折不成载荷/无事件源四种失败
**都有点名 `error!`**，且**没有**任何一条被降级成「Ok 但没推送」；
「载荷自足」的责任因此必须留在生产者侧（票 02 已做），不能指望回传错误。
若将来要让插件感知投递失败，需要 `broadcast-sync` 加 `-> result<_, string>`
= 一次 ABI bump，属另线决策。

### 四、防回接锁与变异自检

`sync_handler_does_not_interpret_session_variants`：只扫 `#[cfg(test)]` **之前**的实现段
（测试里逐变体构造是必要的），命中 `SyncPayload::` / `match event` / `match &event` /
`DesktopSyncEvent::` / `SessionStatus` / `format!("{:?}"` 即列违规。

变异自检（证伪「锁是否恒真」）：临时往实现段塞

```rust
const _MUTATION_PROBE: Option<SyncPayload> = Some(SyncPayload::SessionRemoved { … });
```

→ 用例转红并列出行号；撤探针后复绿。**探针已删除**，未留在仓库里。

### 五、CI 门禁补口（用户批准）

`.github/workflows/test.yml` 桌面 job 在 `Cargo test (desktop)` 之前新增：

```yaml
- uses: Swatinem/rust-cache@v2
  with: { workspaces: bedcode-desktop/packages/plugin-sdk-desktop/rust }
- name: Cargo test (desktop plugin SDK)
  run: cargo test --manifest-path bedcode-desktop/packages/plugin-sdk-desktop/rust/Cargo.toml
```

理由：票 01/02 把跨端 wire 形状锁主战场迁进 SDK，而原门禁只在两端 `src-tauri` 跑
`cargo test`、`sdk-publish.yml` 对 Rust 侧只 `cargo check` —— 改 wire 形状的 PR
在合并门禁上无人看守。移动端 SDK（`plugin-sdk-mobile`）本次未补，留作后续对称项。

### 六、门禁实跑

| 门禁 | 结果 |
| --- | --- |
| 宿主 `cargo test --lib` | **1044 passed / 0 failed**，`[skip]` 计数 0 |
| 集成 `broadcast_shutdown` / `http_auth_biometric` / `pty_session_chain` / `ws_auth_rules` | 1 / 1 / 1 / 1 passed，0 failed |
| 集成 `build_manifest_smoke` / `link_crypto_http` / `server_integration`（顺带回归） | 1 / 4 / 1 passed，0 failed |
| SDK `cargo test --lib` | 169 passed / 0 failed |
| 插件 native `cargo test --lib` | 304 passed / 0 failed |
| 插件 `cargo check --target wasm32-wasip3` | 通过 |
| 残留进程 / 端口 | 测试后 `ps` + `ss -tlnp` 无 bedcode/actix/deps 残留 |
| 产物新鲜度 | `resources/plugins/desktop/*` 四个 `.wasm` 的 mtime 均晚于各自插件源码；Rust 侧无硬编码 `wasmHash` 字面量（清单校验走加载期自洽） |
| 前端 `pnpm run test:run` / `eslint` | 未跑（票 01–03 零前端改动；票 04 全量时一并跑） |

### 七、并发在途记账

本票实现期间，同 worktree 另一场会话在 `wasm_core/manager/runtime*` 的在途改动
两次挡住宿主 test target 编译（先 `WASIP3_NIGHTLY` 常量暂缺，后
`Mutex`/`RwLock`/`HashMap` 经 `use super::*` 不可达），期间只跑通
`cargo check --lib` + SDK + 插件三条独立门禁；对侧落地后复跑得上表全部数。
未碰对侧文件、未替它补 import。

