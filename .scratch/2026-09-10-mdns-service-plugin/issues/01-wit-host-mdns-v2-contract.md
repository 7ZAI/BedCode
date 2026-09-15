# 01: host-mdns v2 契约扩展 + 双端 SDK 重生成

**What to build:** mDNS 基础能力服务的契约层（expand）。WIT `host-mdns` 在既有 browse/stop-browse 旁新增 advertise / stop-advertise / is-advertising 三个纯原语函数（签名与语义见 spec v2 §4.2），双端 SDK（desktop/mobile）rust 绑定与 TS 同步重生成，ABI 版本同步 bump（桌面 12→13、移动 10→11，ADR 0019 双端同版）。契约无业务语义——配置为纯引擎参数，供后续宿主实现与插件消费。

**Blocked by:** None (can start immediately)

**Status:** done (2026-09-15)

## Acceptance criteria

- [ ] WIT `host-mdns` 新增 `advertise(config-json)` / `stop-advertise(advertise-id)` / `is-advertising(advertise-id)`；`browse` / `stop-browse` 签名不变
- [ ] 双端 SDK 重新生成后 cargo check 通过；SDK vitest 全绿；ABI 协商测试通过（桌面 12→13 / 移动 10→11）
- [ ] 双端契约同版（ADR 0019），wasmtime 47 不动，仅 wit-bindgen 重生成
- [ ] 既有 SDK 测试零破坏性变更（browse/stop-browse 语义不变）