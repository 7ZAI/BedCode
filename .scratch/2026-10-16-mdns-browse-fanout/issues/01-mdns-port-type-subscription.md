# 票 01 — server-base 端口词汇：MdnsTypeSubscription + MdnsPort 扩展

Status: ready-for-agent
Type: task

## 目标

`bedcode-server-base` 补齐「类型级浏览中枢」的宿主端口词汇（spec §2.3 接口形态 / §6 D1-D7）。

- `packages/bedcode-server-base/src/ports.rs`（`MdnsPort` 段 `:198` 起）：
  - 新增 `subscribe_type(&self, service_type: &str) -> Result<MdnsTypeSubscription, String>`
  - 新增 `request_type_requery(&self, service_type: &str)`
  - 既有三方法（`shared_daemon` / `register_host_service` / `stop_host_service`）一字不动
- 新增订阅值形态 `MdnsTypeSubscription`（本 crate）：
  - 持 `flume::Receiver<mdns_sd::ServiceEvent>` + `Box<dyn Any + Send + Sync>` 守卫（守卫 Drop = 退订）
  - **硬性要求**：不得提供 receiver 与守卫生命周期脱钩的构造（spec §2.3；退订泄漏由票 02 单测 4 钉）
  - 形态建议：私有字段 + `new(receiver, guard)` + `into_parts()`（宿主 glue 转换用，票 04 消费）
- `packages/bedcode-server-base/Cargo.toml`：`flume = "0.12"`（与 peer-net / mdns-sd 传递依赖同版）

## 门禁

- `cd packages/bedcode-server-base && cargo test` 全绿
- 守卫生命周期语义有实际断言（unit-test-discipline；拒绝无断言 / 恒真断言）
