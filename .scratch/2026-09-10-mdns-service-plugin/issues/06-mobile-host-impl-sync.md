# 06: 移动端 host_impl 同步 + peer-net 接线

**What to build:** 将桌面 mDNS 基础能力服务按同构落地移动端：共享单守护 + advertise 原语 + 事件定向投递 + 属主校验 + 双表 purge；移动端 peer-net 同样收敛到共享守护、全局桥接退役；Android 多播锁随单守护常驻获取（幂等，不再随浏览句柄增删——落地 spec v2 §8 的既有注释计划）。红线：终端链路 mDNS（`_bedcode._tcp.local.`，远程终端配对）不受任何影响。双端契约同版（ADR 0019）。

**Blocked by:** 02, 03

**Status:** done (2026-09-15)

## Acceptance criteria

- [ ] 移动端 host_impl 与桌面同构：单一守护、advertise / stop-advertise / is-advertising、定向事件投递、属主校验、purge 双表
- [ ] 移动端 peer-net 接线到共享守护，全局发现桥接退役（与桌面 04 同口径）
- [ ] Android 多播锁随单守护首次使用获取、常驻持有（幂等，不随 browse 句柄增删）
- [ ] 移动端 cargo test 通过 + `./gradlew :app:compileUniversalDebugKotlin` 通过
- [ ] 终端链路 mDNS（discovery/advertiser 模块）行为不变