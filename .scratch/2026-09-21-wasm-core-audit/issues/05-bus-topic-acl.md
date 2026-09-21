# 05: 消息总线 topic 命名空间与互调接收侧（P0-4）

**What to build:** 总线成为真正的强制边界：owner 作用域 topic（`<base>.<owner>`）**只有属主能订阅、只有宿主能发布**，普通插件无法向他人 topic 伪发布；插件互调的回复通道校验接收者身份。修完后 AGENTS §5「插件间只经互调 API 与消息总线通信」这句话才有安全含义——此前该通道是无锁的。

**Blocked by:** 无（可与 02/04 并行；但文档同步要与 06 一起提交）

**Status:** ready-for-agent

## 现状（已复核）

- 订阅侧零校验：`host_impl/bus.rs:73-101`（`bus_subscribe` / `bus_subscribe_binary` / `bus_unsubscribe`）把任意 topic 串直投 `subscribe_wasm`；
- 发布侧只有 `bedcode.api.*` 目标门（`host_impl/bus.rs:9-34`），普通 topic 显式放行（其测试 `:228-232` 自证）；
- 宿主事件以 `sender = "host"` 发布（`ws.rs:968`、`pty.rs:317`），派发只跳过 sender 自身（`bus.rs:246`）；
- 三处文档承诺与现实相反：`bedcode.wit:262-265`、code-map:163、`.scratch/2026-09-19-pty-base-service/spec.md:129`（均称「非属主物理上订阅不到」）；
- 未逐行复核（本票内确认）：`host_impl/api.rs:39-50` ReplyHandler 不校验 `msg.sender`，配合上述订阅面可订阅他人 api topic 抢答。

## 验收

- [ ] **先落红测**：插件 B 订阅 `pty:exit.<A>` / `ws:client-connect.<A>` 收到 A 的事件（当前应成功=红），修复后被拒；插件 B 向 `pty:exit.<A>` 伪发布（当前应成功=红），修复后被拒
- [ ] 机制选型落地并在 Comments 记取舍：(A) 总线内建 owner 段解析与订阅/发布 ACL；(B) 宿主为每个插件提供**私有 topic 前缀**（`<plugin_id>::<topic>`）+ owner 作用域 topic 只经宿主定向投递；推荐 B——把「约定」变成「命名空间」，且天然防前缀抢占（`host-websocket` 服务端域的命名空间注入已是同型先例，code-map:155-157）
- [ ] 宿主定向投递事件（pty/ws/mdns/task）只投递给属主订阅，跨属主订阅在 Rust 端拒绝且错误 fail-visible（不静默丢弃）
- [ ] `bedcode.api.*` 的回复通道：ReplyHandler 校验 `msg.sender` 与被调用声明匹配，非目标插件抢答被拒（ADR 0017 层 2 补齐；层 1 现状保持）
- [ ] 向后兼容：既有 4 个内置插件的 topic 用法逐一核对（`plugins/session`、`file-transfer`、`ai-chatbox`、`agent-hub`），需要改插件侧调用点的列进本票，禁止只改宿主留下静默断流
- [ ] 文档同步：`bedcode.wit` 注释、code-map:163、pty spec:129 三处「物理订阅不到」的表述改为实现真能兑现的口径；若函数语义有变按 ADR 0022 登记双端偏离（移动端的 `plugin/message_bus.rs` 是否跟演写进 Comments）
- [ ] 门禁：`cargo test` 全绿（含新隔离用例）+ 总线吞吐/背压既有断言不回归

## Comments

- 2026-09-21 立项：来源 spec §4-P0-4。
