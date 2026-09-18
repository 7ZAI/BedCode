# 03 — 注册表开放化与属主回收

**What to build:** 让 WS 会话注册表能承载「不止终端与事件」的通道，并且每条连接都知道自己属于谁。通道类型从闭合枚举改为开放种类（终端 / 事件 / 插件），注册条目带上属主与端点标识，注册表新增按端点寻址、按属主批量回收、按端点批量断开三种能力——这是插件端点「停用即回收、跨插件不可互操作」的地基。

表示方式已定案（spec §3.2 A2）：种类保持可拷贝的轻量枚举 + 条目上另挂属主与端点标识两个可选字段；**不**把带字符串的变体塞进原枚举（那会破坏其拷贝语义并向调用点扩散）。

**Blocked by:** 02

**Status:** done（2026-09-18）

- [x] 通道种类可表达「插件端点」，条目携带属主与端点标识
- [x] 注册表新增：按端点标识寻址、按属主批量回收（只碰本人，宿主登记与他人条目不受影响）、按端点批量断开（可指定关闭码）
- [x] 既有广播目标计算语义零变化：仅已认证事件通道、设备名排除、同指纹去重——既有单测原样通过，插件通道不会被卷入广播
- [x] 新增单测：按属主回收只命中本人；按端点断开；未知标识幂等（不 panic、返回未命中）
- [x] `cargo test` 全绿

## Comments

### 2026-09-18 实施记录

**表示方式（spec §3.2 A2 定案）**

- `ChannelType` → **`ChannelKind`**（保持 `Copy`）：`Terminal` / `Event` / **`Plugin`**（新增）；
- 条目另挂 `owner: Option<String>`（属主插件 id）与 `endpoint_id: Option<String>`（端点标识）——不给原枚举加 String 变体，保住 `Copy` 语义；
- 注册入口改为参数结构 `WsRegistration`（client_id / socket_addr / actor_addr / channel_kind / owner / endpoint_id），避免 6 参长签名；
- 骨架构造改为 `ConnSpec`（addr / channel_kind / bound_session / owner / endpoint_id）+ handler，`WsConnBase::new(spec, handler)`；`new_for_session` / `new_event` 保持原签名（调用点零改动）。

**新增注册表能力**

| 方法 | 语义 |
| --- | --- |
| `list_by_endpoint(endpoint_id)` | 端点域寻址：在线客户端摘要 |
| `endpoint_client_count(endpoint_id)` | 端点在线数（`list-endpoints` 用） |
| `is_endpoint_client(endpoint_id, client_id)` | 端点域校验（单发前的错配寻址防护） |
| `send_to_endpoint_client(endpoint_id, client_id, text)` | 端点内单发；错配 → `Err`（不消费句柄） |
| `broadcast_to_endpoint(endpoint_id, text) -> usize` | 端点内广播 → 成功入队数（部分失败不回滚，明细 debug） |
| `disconnect_by_endpoint(endpoint_id, code, reason) -> usize` | 按端点批量断开：摘除条目 + 下发 Close，返回命中数（幂等） |
| `purge_for_plugin(owner, code, reason) -> Vec<String>` | 按属主批量回收：只碰 `owner == Some(plugin_id)`，返回被回收 client_id 列表 |

内部实现 `take_matching`（sessions → addr_to_client_id，与 `register`/`unregister` 同锁序）+ `close_removed`（`try_send(CloseConnection)`，fire-and-forget，避免阻塞调用方）。

**顺带落地（票 05/B6 的前置，编译强制项）**

- `WsConnBase` 新增 `CloseConnection { code, reason }` actor 消息 + Handler（`ctx.close(Other(code))` + `stop`）——踢出 / 端点注销 / 属主回收的关闭出口；
- `TrafficChannel::WsPlugin`（`as_str() = "ws-plugin"`）落地（`ChannelKind::Plugin` 的过滤链映射必须有值，否则 match 不可穷尽）；
- `link_crypto.rs` 两处穷尽匹配同步：`should_process` 对 `WsPlugin` 恒 `false`（不参与链路加密，D9），`on_inbound`/`on_outbound` 归入 `on_ws_frame` 分支；
- `ChannelKind::Plugin` 不参与设备在线判定（`stopping` 的 offline 分支恒 `false`）。

**验证证据**

- `cd bedcode-desktop/src-tauri && cargo test` → lib **854 passed / 0 failed**（原 849 + 新增 5），集成测试全绿；
- 既有 `broadcast_targets` / `device_online_queries` 用例**断言逐字未改**，仅枚举名随重命名替换；`entry()` 5 参签名保持（owner/endpoint_id 默认 None），新增 `entry_owned()`；
- 新增用例：`broadcast_targets_exclude_plugin_channel`（插件通道不被卷入广播）、`purge_for_plugin_only_hits_owner`、`disconnect_by_endpoint_hits_only_that_endpoint`、`endpoint_queries_unknown_id_are_idempotent`、`endpoint_targeted_send_and_broadcast`；
- 端点域用例改用 `local_registry()`（独立实例）而非全局单例，避免与 `device_online_queries` 并行互相清空（首轮实测到的 2 个失败即此竞态，已修）；
- `grep -rn "ChannelType\|channel_type"` 于 `registry.rs`/`conn.rs` → 无残留；新增文件 `rustfmt --check` 无差异；`cargo check --lib` 于 `server/{ws,filter,link_crypto}` 无告警。

## Comments
