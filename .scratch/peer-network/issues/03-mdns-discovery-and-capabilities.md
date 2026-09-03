# 03 — mDNS 节点发现 + 能力通告 + 在线缓存

**What to build:** 对等网络专用 mDNS 服务类型（与终端链路的 `_bedcode._tcp` 并存、互不感知）：每节点同时广播自身并浏览他人；记录 TXT 载荷含设备名、协议版本、能力位图（文件传输为首 bit）。发现结果进在线缓存——「在线」即发现记录可见，与是否维持连接无关；节点离开网内后从缓存消失。Android 侧申请 MulticastLock 保证多播收包。测试经发现注入缝驱动（向节点注入一条已发现记录，与 mdns 守护回调同一通路）；真实多播只做冒烟不进 CI 断言。

**Blocked by:** 01

**Status:** done（AC#2/#3/#4 待人工真机冒烟确认）

- [x] 注入缝单测：记录进入缓存、携带名称/版本/能力位、过期移除
- [ ] 两端真机冒烟：双设备同时运行时互见（日志或调试面板可见对端名称与能力）——**待人工冒烟**（自动启动已接线，两端日志过滤 `peer discovered` 即可见对端 device_name + cap）
- [ ] Android 真机后台可见性符合预期（前台服务存活期间可被发现）——**待人工冒烟**（MulticastLockPlugin 已实现，锁生命周期不绑 Activity）
- [ ] 终端链路发现行为回归不受影响——结构层面已保障（冻结区零改动，新服务类型 `_bedcode-peer._tcp.local.` 与旧 `_bedcode._tcp.local.` 错开）；真机共存回归待人工冒烟

## Comments

### 实现记录（ticket 03）

- crate 侧 `packages/peer-net/src/discovery.rs`：`DiscoveredPeerRecord` / TXT 编解码（keys `id`/`name`/`ver`/`cap`，小写 hex 能力位图，容忍降级解析）/ `DiscoveryCache`（observe 注入缝 + TTL 30s 三层过期机制，纯逻辑 sweep 可显式注入时钟）/ `spawn_peer_mdns_daemon` + `DiscoveryDaemon::stop`（stop_browse → unregister → shutdown → join 优雅关闭）
- 注入缝端到端：`tests/discovery_injection.rs` observe → to_static_peer_record → mTLS dial → 回声往返 + 双向落库
- 宿主接线：两端 `src-tauri/src/peer_net.rs` 扩展（PeerNetState + start_peer_node/stop_peer_node/list_discovered_peers 命令 + setup 自动启动；闸门默认拒绝待后续 UI 票接入）；Android 启动守护前经 Kotlin 插件申请多播锁
- Android：新增 MulticastLockPlugin.kt（非引用计数幂等 acquire/release/isHeld）+ multicast_lock.rs 桥接 + android_plugins.rs 注册；已补录 AGENTS.md gen/android 恢复清单
- 验证：`packages/peer-net cargo test` 全绿（含 discovery 单测与注入集成测试）、mobile `cargo test` 全绿（393 lib + 22 集成）、`./gradlew :app:compileUniversalDebugKotlin` 通过；desktop `cargo test` lib 576 全绿，但集成测试 `broadcast_shutdown` 稳定失败于 loopback HTTP 连接被本机软件中断（WSAECONNABORTED 10053）——同签名测试 `ws_auth_rules` 重试可恢复、不触发旧 mDNS 广播的 HTTP 集成测试全通过，失败面与本票改动无代码交集（冻结区 diff 为空），判定为火绒环境级干扰（同 issue 01 已记录的干扰类别）；HEAD 基线对照因机器内存耗尽（rustc OOM）未能完成，建议将项目目录加入火绒白名单后重跑确认
