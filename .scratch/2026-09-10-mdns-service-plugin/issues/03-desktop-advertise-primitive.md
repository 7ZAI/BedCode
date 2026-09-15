# 03: 桌面 advertise 原语 + 属主校验 + 双表 purge

**What to build:** mDNS 基础能力服务的广播能力（需求②扩展性核心）。插件可经原语广播自己的服务——服务类型、实例名、端口、TXT 键值全部由插件构造传入，宿主零拼装（需求红线：基础服务不包含任何业务性代码）；句柄带属主，跨插件 stop / 查询一律拒绝（需求①业务隔离）；advertise 句柄带周期 re-announce 续期（mdns-sd 注册后不主动周期广播，须手动续期）；插件停用时 purge 同时回收其全部浏览与广播句柄且不误伤他人。

advertise config JSON 契约（决策型 type shape，来自 spec v2 §4.2，宿主只校验 serviceType 非空，其余原样透传）：

```json
{
  "serviceType": "_bedcode-peer._tcp.local.",
  "instanceName": "bedcode-3f2a1b",
  "port": 19000,
  "txtRecords": { "id": "...", "name": "...", "ver": "1", "cap": "3" }
}
```

**Blocked by:** 02

**Status:** done (2026-09-15)

## Acceptance criteria

- [ ] `advertise` / `stop-advertise` / `is-advertising` 三个宿主原语实现，advertise 注册带周期 re-announce，stop 时注销并回收 re-announce 任务
- [ ] config 解析：serviceType 非空校验；instanceName 缺省时宿主默认 `{plugin}-{短指纹}`（显式优先）；txtRecords 原样透传（含中文值不转义损坏）
- [ ] 属主校验：B 插件 stop / is-advertising A 的句柄 → 拒绝（权限门后追加属主仲裁）
- [ ] purge 双表（BROWSERS + ADVERTISERS）仅回收本人句柄，不误伤他插件
- [ ] 单测全绿：属主拒绝、purge 双表、未知句柄幂等、advertise 状态翻转（advertise → is-advertising true → stop → false）、TXT 透传