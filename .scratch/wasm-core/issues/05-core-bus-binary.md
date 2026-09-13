# 05: core-bus：二进制载荷与背压

**What to build:** 插件间消息总线支持两种载荷格式：JSON 文本（现状，行为零回归）与二进制字节列（零 JSON 编解码，可传非 UTF-8 与大载荷）。订阅方声明格式偏好，格式不匹配的投递被拒绝并 warn；每订阅者有界队列，满则丢弃 + warn（plugin_id/topic 结构化字段）+ 丢弃计数进监控模块。WIT 契约新增二进制承载形式，ABI bump，桌面/移动双端契约同步演进（ADR 0019，wasmtime 47 不动）。

**Blocked by:** 01（五模块骨架归位）、03（丢弃计数进监控）

**Status:** resolved

- [x] JSON 路径现有测试全部保留且全绿（零回归）
- [x] 二进制 roundtrip：非 UTF-8 字节、大载荷（MB 级）收发字节一致
- [x] JSON 订阅者收不到二进制消息（拒绝 + warn 断言）
- [x] 队列满丢弃：丢弃数正确且进监控指标
- [x] WIT 双端文件同步、ABI bump 记录（两端 wit 文件 diff 语义一致）
- [x] `cargo test` 全绿

## Comments

- 2026-09-13 完成。核心设计（勘察结论落地）：

  **契约（双端同步）**：`host-bus` 增 `publish-binary(topic, list<u8>)` / `subscribe-binary(topic)`；新增 interface `events-binary`（`on-message-binary(topic, sender, list<u8>)`，**无返回值观察型回调**——`TypedFunc::call` 的 results 必须是 core 层元组，`result<_,string>` 无法直接调用，故取无返回语义，guest 错误走 host-log）。`events-binary` **不进 `plugin` world 的必选导出**（旧插件无该导出会实例化失败），宿主实例化后 `instance.get_typed_func` 动态探测，缺失容忍为 None（旧插件只收 JSON）；SDK 用独立 world `plugin-binary` + 独立文件 `wasm_binary.rs` 生成绑定（避免 export! 宏重名），`wasm_entry!` 无条件导出默认空实现。ABI bump：桌面 10→11 / 移动 8→9（注释记录 v10/v8 = host-peer 收缩终态）。

  **总线（桌面 plugin/bus.rs）**：`BusSubscriber` 加 format/tx/metrics；**每订阅者有界队列**（容量 64）+ spawn 消费任务（recv → 动态读 dispatcher → dispatch），慢订阅者只阻塞自己；publish 侧格式检查（不匹配拒绝 + warn + 计数，不进队列）、`try_send` 满则丢弃 + warn（plugin_id/topic 结构化字段）+ 计数。丢弃/拒绝计数进 core-monitor（`PluginMetrics.bus.dropped / format_rejected`，snapshot 加 `bus` 字段，additive）。monitor 经 `MessageBus::set_monitor` 注入（PluginHost::init_message_bus，先于任何订阅）。`dispatch_to_wasm` 按 `payload_binary.is_some()` 路由 `on_message` / `on_message_binary`。

  **移动端**：无 core-monitor，丢弃/拒绝计数自持（`MessageBus::dropped_total()/format_rejected_total()`，注释说明桌面端进监控）；DeliveryJob 通道 unbounded→有界（64）+ try_send 满丢弃；格式过滤/二进制收发/动态探测与桌面同语义。

  **BusMessage**：增 `payload_binary: Option<Vec<u8>>` + `#[serde(default)]`（老端 JSON 缺字段可解析，增量演进）；JSON 消息 payload_binary=None、二进制消息 payload=Null。

  **测试**：桌面 bus.rs 新增 4 用例（非 UTF-8/MB 级 roundtrip、JSON↔二进制双向格式拒绝+计数、队列满丢弃守恒律 delivered+dropped==TOTAL）；移动端 message_bus.rs 新增 3 用例（同语义，内部计数）。两端 plugin-component-test 的 `abi.version()` 硬编码同步（桌面 11 / 移动 9）。

  **验证**：桌面 cargo test 634 全绿（含既有 JSON 路径全部用例）、移动端 cargo test 282 全绿（279+3 新）；SDK 双端 cargo test 全绿（桌面 75 / 移动 63）；fmt 自查改动区域干净（dev 基线 345 文件漂移不纳入）。

- 2026-09-13 补记（票据 06 发现并修复）：本票据的二进制链路实际有三处断点，导致 v11 二进制 guest 回调此前**未真正接通**——(1) 宿主动态探测用平名 `iface#func`，`Instance::get_func` 的 str 查找恒不命中；(2) 双端 SDK `HostBus` 缺 `bus_publish_binary` / `bus_subscribe_binary`，而 `WasmPlugin::on_message_binary` 文档又要求用 `host-bus.subscribe-binary` 声明偏好，SDK 侧无从声明二进制订阅；(3) guest 侧端到端覆盖缺失。本票据 checklist「二进制 roundtrip」当时仅在总线层验证。修复与新增端到端用例见 `06-system-components.md`。
