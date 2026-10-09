# 移动端 host-notify 域：Android 原生通知/震动/声音能力封装（ABI v18）

> Date: 2026-10-09
> Status: **实施中**（用户指令：「移动端 安卓的原生代码提供的能力 请封装成宿主函数 对 wasm-app 和插件使用 比如通知服务 文件访问读写 包括设置震动 声音等」）
> 前置：ADR 0018（移动契约独立）/ 0019（双端 WIT 锁版）/ 0022（宿主原语判据）/ 0040（fork 核）；票 15 阶段 B v17 破坏性收缩先例

---

## 1. 用户拍板（2026-10-09）

1. **范围**：通知增强（震动/声音开关 + 权限查询/请求）+ 独立震动 + 提示音；文件面维持现状（host-fs 8 原语已覆盖读写）；其余 Android 原生能力（设备信息 / 全文件访问 / 前台服务 / 状态栏）本轮不纳入。
2. **接口策略**：新建 `host-notify` 域并**收编** `host-events.notify` → ABI 17→18 **破坏性收缩**（host-events 回归纯事件语义，只剩 emit）。
3. **权限位**：新增 `notify`（fail-closed——通知/震动/声音是用户打扰面，独立成位，先例 ws:client / auth）。

## 2. 契约（WIT 草案）

```wit
interface host-notify {
    notify: func(title: string, body: string, options-json: string) -> result<_, string>;
    check-permission: func() -> result<bool, string>;
    request-permission: func() -> result<bool, string>;
    vibrate: func(duration-ms: u32) -> result<_, string>;
    play-sound: func() -> result<_, string>;
}
```

- `options-json`（camelCase，字段可省）：`{ vibrate?: bool, sound?: bool }`，缺省均 true（与退役前 host-events.notify 行为一致）。
- `host-events` 删除 `notify`（只剩 `emit`）。
- **须重编译**：v17 产物在 v18 宿主实例化期因缺失 import 函数被点名失败（fail-visible ②）——内置插件随 APK 同分发，须重建全部内置 wasm-app 产物。

## 3. 落点清单（单一事实源逐处）

| # | 落点 | 内容 |
| --- | --- | --- |
| 1 | `packages/plugin-sdk-mobile/rust/wit/bedcode.wit` | 新接口 + host-events 收缩 + world import + abi 注释 |
| 2 | SDK `rust/src/host/notify.rs`（新）· `events.rs` · `mod.rs` · `wasm_host.rs` | `HostNotify` trait（5 方法）+ `HostEvents` 删 notify + HostApi 组合 + WASM import 绑定 |
| 3 | SDK `rust/src/abi.rs` · `permission.rs` | ABI_VERSION 18；`PERMISSION_NOTIFY = "notify"` + 表 + API 映射 |
| 4 | fork crate `host_impl/notify.rs` | 5 函数逻辑层（权限门 + options 严格解析 + Android 分支） |
| 5 | fork crate `component.rs` | host_events impl 删 notify；host_notify impl 5 委托；linker 加行 |
| 6 | fork crate `host_api/ports.rs` · `test_support.rs` | notify 端口 5 方法 + UnimplementedPorts + MockPorts 转发 |
| 7 | 宿主 `plugin/host_ports.rs` | 端口实现（Kotlin TaskNotificationPlugin） |
| 8 | Kotlin `TaskNotificationPlugin.kt` · `TaskNotificationManager.kt` | showPluginNotification 参数化 + pluginVibrate / pluginPlaySound |
| 9 | 锁 `tests/sdk_wit_contract_locks.rs` | A1（16→17 import / 22→23 接口 / ABI 18）+ A3（host-events 减 notify、host-notify 行） |
| 10 | 文档 | CHANGELOG 双语 · code-map · ADR 0018 偏离表登记 |

## 4. 门禁

1. fork crate `cargo test --features test-support`（含 A1/A2/A3/A4 锁 + notify 域单测 + 组件全链路）；
2. 移动宿主 `cargo test` 全量；
3. Kotlin `./gradlew :app:compileUniversalDebugKotlin`（改 Kotlin 必跑）；
4. 变异自检：权限门删检 → 测红；A1/A3 表漂移 → 锁红；
5. 内置 wasm-app 产物重建（v18 破坏性收缩的交付前提）。

## 5. 实施记录（2026-10-09）

- **落地全链路完成**：WIT / SDK（HostNotify + NotifyOptions + PERMISSION_NOTIFY）/ fork crate
  （host_impl/notify.rs 5 原语 + component.rs 接线 + ports 5 方法）/ 宿主 host_ports.rs /
  Kotlin（showPluginNotification 参数化 + pluginVibrate / pluginPlaySound + vibrateOnce /
  playSoundOnce 抽取）/ 锁 A1+A3 / 文档（CHANGELOG 双语、code-map、plugin-dev-mobile、
  ADR 0018、plugin-development-checklist ABI 数字、android-backup 恢复副本同步）。
- **门禁结果**：SDK `cargo check --features wasm` 过；fork crate 全量 **285 + 4 + 4 全绿**；
  移动宿主 `cargo check` 过；宿主全量 `--no-fail-fast` **21 目标 328 passed / 2 failed**——
  两失败均为**其他会话在途基线**（`session_control_new_face_stays`：其 `http_engine.rs`
  重构把 `request.get("jwtAuth")` 换行化；`mobile_terminal_retained_face_stays`：其 ws 引擎
  抽根把 `jwt_auth == Some(true)` 移出 `host_impl/ws.rs`，退役锁 needle 未同步；本会话
  零触碰两处文件）；Kotlin gradlew `BUILD SUCCESSFUL`；根 eslint **0 error**（110 既有
  warning）；移动端前端全量 `pnpm run test:run` **67 文件 / 733 用例全绿**；
  变异自检 3/3；Gradle daemon 已停、测试端口零占用。
- **变异自检 3/3**（`mutation_check.py`，全部还原 + hash 校验）：① 权限门旁路 → notify 域
  测试红；② WIT 函数名漂移 → **编译期即红**（E0407/E0046，比锁更早的 fail-visible 层）；
  ③ A3 锁表漂移 → A3 红（判据：WIT 函数集 vs 对照表）。
- **产物重建**：三个内置插件（terminal-session / file-transfer / ai-chatbox）经
  `pnpm run plugins:build` 全量重编（日志含 `Compiling bedcode-plugin-api-mobile` +
  各插件 crate，产物 v18 绑定）。
- **重要事实澄清（fail-visible 精确语义）**：**组件 import 按实际使用面声明**（实测：
  terminal-session 产物含 host-events / host-terminal-stream / host-websocket 字符串，
  不含未使用的 host-notify）。故 v18 破坏性收缩的失败面 = **引用了 `host-events.notify`
  的产物**；未引用者照常加载。现有内置插件零消费者 → 本轮 WIT 注释 / abi.rs / CHANGELOG /
  ADR 均按此精确表述。**注意**：SDK 改动（如 permission.rs）会触发下游重编，但仅改 WIT
  时需确认产物重建（本仓 SDK 无 build.rs 跟踪 WIT；本轮实际由 SDK 源码改动带动重编）。
- **测试夹具同步点（易漏）**：`packages/plugin-component-test/src/lib.rs` 的
  `AbiGuest::version()` 是**手写常量**（须与 SDK `ABI_VERSION` 同步）——ABI bump 时
  必须同批更新，否则 `component.rs` 的 roundtrip / terminal-session 全链路测试红于
  「组件版本 != SDK 版本」断言。wasm-apps 侧走 `wasm_entry!` 宏自动跟随，无此问题。
