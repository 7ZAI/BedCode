# 07 — P2：插件 SDK 的 async 绑定能力 + 最小 async 原语夹具

**Type:** task
**Spec:** `../spec.md`（§5 W5 + §6 阶段 P2 + §10 output-ack 关系）
**Blocked by:** 01（A1 重入结论）、04（A4 工具链结论）、06（P1 属主化——夹具要验证「await 宿主信号**且同期处理其它命令**」，该性质由 P1 交付）
**Status:** **retired（2026-09-27）** —— 见文末 `## 改判记录`：A4 已判 WIT 层 `async func` 不可用（async-lifted 导出入口即 abort），本票要验证的「guest 自己 await」形态前提不成立；宿主实现侧 async（spec §5 前置门禁改判路径）**不需要** SDK 改动（WIT 保持同步、guest 代码零变化），故本票无实现面。

**What to build:** 给插件 SDK（`packages/plugin-sdk-desktop/`）增加 async 绑定生成路径，并交付一个**仅测试用**的最小 async 原语夹具，验证「guest 能 await 宿主信号且同期处理其它命令」。

**工作内容：**

1. **SDK async 绑定生成路径**：`wit-bindgen` `async: true`（feature `async`）的 guest 绑定接入 SDK（默认仍走同步路径，§4.4「不半 async 半 sync」由契约层把关）；依赖票 04 的工具链结论（可用配置 / 替代方案）。
2. **最小 async 原语夹具**（仅测试用）：
   - 测试专用 world（**不得塞进生产 `bedcode.wit` / 生产 plugin world**——探针纪律：不得把探针接口塞进生产 WIT 兜底）；形态参照既有 `packages/plugin-p3-async-host-import-test/`；
   - 宿主实现「等宿主信号」的 concurrent 注册（`func_wrap_concurrent` 手写，`bindgen!` 不生成，见 CM-async spec F6）；
   - 夹具插件验证两条性质：① await 宿主信号后拿到结果（唤醒链路）；② 同一实例内，await 挂起期间另一命令能完成（= 「同期处理其它命令」，依赖 P1）。
3. **SDK 发布流程预排**：SDK 对外 trait / 生成路径变化时按 `docs/knowledge/sdk-publish.md` 评估版本与发布（本票若只加内部夹具可不触发发布，写明判定）。

**Out of scope:**

- 不做生产原语的 async 化（那是 P3 票 08 及以后）。
- 不改移动端契约（ADR 0018 独立）；双端 WIT 副本只随 P3 的真实 WIT 变更同步（票 08）。
- 不做候选 ②③④⑤ 评估（票 09）。

**Acceptance（spec §6 P2 门禁）：**

- [ ] SDK 存在 async 绑定生成路径（默认同步路径保持可用，SDK 测试全绿）。
- [ ] 夹具插件：await 宿主信号成功唤醒并返回预期值；**同期**处理其它命令成功（挂起期间第二命令完成）。
- [ ] 测试专用 world 与生产 WIT 隔离证明（断言生产 `plugin` world / ABI 零变更）。
- [ ] 针对性测试（`cargo test <filter>`）通过；收尾桌面端 `cargo test` 全量绿。
- [ ] 相关坑位记录进 `docs/knowledge/plugin-development-checklist.md`「需要等待的原语怎么写」（或留至票 10 统一收口，写明归属）。

## 改判记录（2026-09-27）

**退役依据（两条独立事实）**：

1. **前提不成立（A4，票 04）**：WIT 层 `async func` 在锁定工具链（wit-bindgen 0.60.0 + wasmtime 48.0.3 +
   `wasm32-wasip3`）下不可用——async-lifted 导出入口即 abort（`async_support.rs:560`
   `context_get().is_null()`），连「不含 await 的 async 导出」同样如此 ⇒ 「guest 侧 async 绑定生成路径」
   无消费对象。
2. **改判路径不需要 SDK**：spec §5 前置门禁把「按需异步化」改判为**宿主实现侧 async**
   （`func_wrap_async`，WIT 签名保持同步）——宿主的注册方式变化对 guest 完全透明，SDK 与插件零改动。
   而票 08 的实测又进一步判定该路径**收益不足以落地**（实例级门使同实例串行与 async 无关），
   故本票连「为改判路径补 SDK 支持」的余地也不存在。

**保留资产**：P0 探针（`packages/plugin-p3-async-host-import-test/` + `runtime/tests/p3_async_host_import.rs`）
与其结论（宿主实现侧 async 可用、`func_wrap_async` 让出宿主线程、同实例仍串行）仍是后续任何
async 议题的输入；`test_p3_async_import_pending_second_explicit_call_same_instance`（票 08 新增）
把「实例级门」钉成可执行边界锁。

**未来重启条件**（任一满足再评估本票）：① wit-bindgen/wasmtime 修好 async-lifted 导出（A4 的
未收敛差异定位清楚）；② 运行时支持实例级并发进入（票 08 边界锁转红）——两者都成立时，「guest 自己
await」才可能带来用户可见收益。

## 关联证据

- `packages/plugin-sdk-desktop/rust/Cargo.toml:30`（`wit-bindgen = "=0.60.0", features = ["macros"]`）、`packages/plugin-sdk-desktop/rust/wit/bedcode.wit`（生产 WIT 单一事实源）
- 既有测试专用 world 形态：`packages/plugin-p3-async-host-import-test/`（`wit/p3-async-host-probe.wit` + fixture + 宿主探针 `runtime/tests/p3_async_host_import.rs`）
- CM-async spec F7（guest async 支持：`async_support` 的 `block_on` / `yield_async` / futures / streams）、F6（`bindgen!` 不生成 concurrent 注册）
- spec §10：与 output-ack P2 的关系（本改造成功后「guest 自己 await」可省掉宿主主动回调那条路，但 P2 不互斥，优先级下降）