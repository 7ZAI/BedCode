# 02: 桌面 MdnsService：单守护 + browse 事件定向投递

**What to build:** mDNS 基础能力服务的桌面内核落地。全局唯一 `ServiceDaemon`（init 时关闭虚拟网卡），替代当前每个 browser 独立 daemon 的结构（消灭同类双 daemon 同绑 5353 互抢多播包的历史病灶）；browse 事件从全局广播 `mdns:found`/`mdns:lost` 改为按属主定向投递 `mdns:found.<plugin-id>` / `mdns:lost.<plugin-id>`，payload 增量追加 serviceType / browserId（既有 4 字段 shapes 不变）。当前无插件实际调用 browse 原语，行为契约由本票据的单测锁定，为后续消费插件（file-transfer）迁移铺路。

**Blocked by:** 01

**Status:** done (2026-09-15)

## Acceptance criteria

- [ ] 单守护：所有 browse 订阅共享一个 `ServiceDaemon`（LazyLock 单例），不再 per-browser 新建 daemon；`disable_virtual_interfaces` 仅初始化执行一次
- [ ] 事件定向投递：browse 事件发布到 `mdns:found.<owner>` / `mdns:lost.<owner>`（owner = 发起 browse 的插件 id）；payload 增量加 serviceType / browserId，既有字段（instanceName/addresses/port/txtRecords）原样保留
- [ ] 自播回显过滤保留（TXT `id` == 本机节点 id 即跳过，行为与现状一致）
- [ ] 单测覆盖：定向 topic 断言、自播过滤、stop-browse 未知句柄幂等（既有用例不回归）