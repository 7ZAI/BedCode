# 04: peer-net 共享 daemon 接线 + 全局桥接退役（D1/D3）

**What to build:** 对等网络节点发现收敛到 mDNS 基础能力服务。peer-net 引擎不再自建 daemon，改为使用 MdnsService 共享守护（register / browse / cache 逻辑不变）；节点身份广播经基础服务登记（内部语义 owner=host），其 TXT / ServiceInfo 仍由 peer-net 引擎构造（节点身份/证书/能力位是引擎语义，基础服务零业务代码——D3 定案）；面向插件的全局发现桥接（全局 `mdns:found`/`mdns:lost` 推送）与缓存重发通道整体退役（D1 定案，cache 由引擎内部自持、行为不变）；兜底语义保留——宿主节点身份广播随 peer-net 启动自动注册。

**Blocked by:** 02, 03

**Status:** done (2026-09-15)

## Acceptance criteria

- [ ] `spawn_peer_mdns_daemon` 不再自 new daemon，改为使用 MdnsService 全局守护（`ServiceDaemon` 共享实例传入）；disable_virtual_interfaces 不在引擎侧重复执行
- [ ] 节点身份广播经基础服务登记（owner=host），TXT 由 peer-net 引擎构造，MdnsService 只做注册 + 句柄登记
- [ ] 全局 `mdns:found` / `mdns:lost` 桥接 + discovery-refresh 缓存重发通道移除（无遗留订阅者）
- [ ] peer-net 集成测试全绿：发现 / 在线判定 / 能力通告行为等价（无功能回退）