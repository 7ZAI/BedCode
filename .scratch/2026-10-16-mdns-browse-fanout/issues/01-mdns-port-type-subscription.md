# 票 01 — server-base 端口词汇：MdnsTypeSubscription + MdnsPort 扩展

Status: resolved
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

## Comments

### 改动摘要

- `packages/bedcode-server-base/src/ports.rs`
  - 新增值形态 `MdnsTypeSubscription`：私有字段 `receiver: flume::Receiver<mdns_sd::ServiceEvent>` +
    `guard: Box<dyn std::any::Any + Send + Sync>`；出口仅 `new(receiver, guard)` 与
    `into_parts()`（同时移交双方）；无 `Clone`、无只取单方的构造 / 访问器（spec §2.3 硬性要求，
    类型文档写明「生命周期不可脱钩」及两类禁止形态）。
  - `MdnsPort` 新增 `subscribe_type(&self, service_type: &str) -> std::result::Result<MdnsTypeSubscription, String>`
    与 `request_type_requery(&self, service_type: &str)`；既有三方法声明及其注释一字未动
    （仅 trait 顶部文档增补浏览面两方法的边界说明：browse 只由中枢代持、不得自行直调）。
- `packages/bedcode-server-base/Cargo.toml`：新增 `flume = "0.12"`（注释写明与 peer-net / mdns-sd
  传递依赖同源）。
- `packages/bedcode-server-base/Cargo.lock`：随直依赖更新（详见「偏差」3）。

### 测试（守卫生命周期契约，新增 3 例 / 全在既有 `ports.rs` 测试模块）

退订守卫测试替身 `DropCountingGuard`（Drop 原子计数 = 退订被触发的可观察证据）：

1. `dropping_subscription_runs_guard_exactly_once_and_closes_channel`：句柄存活期守卫不释放；
   drop 句柄 ⇒ 守卫 Drop 恰一次 + 通道对配对发送端 `Disconnected`（receiver 不脱钩独立存活）。
2. `into_parts_moves_receiver_and_guard_without_premature_drop`：移交不释放守卫（计数 0）、
   守卫原样移交（`downcast_ref` 同一对象）、接收端仍连通（send → `recv_timeout` 收到同一事件）、
   守卫释放恰一次。
3. `subscription_handoff_across_threads_drops_guard_exactly_once`：`Send + Sync` 编译期断言 +
   跨线程 drop 时退订恰一次、通道关闭。

变异自检（临时注入后已还原）：① `new` 提前释放守卫 → 3 个订阅用例全红；② `into_parts`
释放守卫并替换为替身 → 用例 2 红（计数断言 + `downcast_ref` 断言）。

### 门禁命令与结果

- `cd packages/bedcode-server-base && cargo test`
  → `test result: ok. 28 passed; 0 failed; 0 ignored`（lib，基线 25 + 新增 3）；
  doc-tests `0 passed; 0 failed; 1 ignored`（既有 ignored，非本票引入）。全绿。
- `cargo fmt --check`（crate 根）：ports.rs 干净。注：全 crate fmt 会顺带暴露
  `process.rs` 尾部缺换行的**存量漂移**，已还原未纳入本票改动（最小改动）。
- `cargo clippy --all-targets`：见实施汇报（无新增 warning / error）。

### 与票面的偏差与理由

1. 行号核对：票面 `MdnsPort` 段 `:198` 与改动前实际一致，无漂移。
2. `subscribe_type` 返回类型写作 `std::result::Result<MdnsTypeSubscription, String>`（限缩到
   `std::result`）：本文件 `Result` 已是 `crate::error::Result` 别名（AppError），既有三方法
   均用 `std::result::Result<_, String>`，与之一致；语义与票面 `Result<_, String>` 相同。
3. `cargo` 直加 `flume = "0.12"`（默认特性）后 Cargo.lock 特性合一：与 mdns-sd 的
   `default-features = false` 传递依赖合并为默认特性（async/select/eventual-fairness），
   flume 的 `fastrand` 边进入本 crate lock——与 peer-net / wasm-core 的直依赖形状一致
   （票面要求同版，peer-net 也是默认特性直依赖），非依赖升级。
4. 测试落点：追加在 `ports.rs` 既有 `#[cfg(test)] mod tests`（AGENTS §6 用例少 → 同文件形态），
   未新建测试文件。
5. `MdnsPort` trait 顶部文档增补两句浏览面说明（既有三方法的 doc 与声明零改动）；类型与
   方法的注释按 AGENTS §6 与实现同步（含票 04 消费 `into_parts` 的成对持有约束）。

### 未决问题

- 无阻塞项。两点跨票约定留痕（不属本票验收面）：
  1. 守卫「Drop ⇒ 退订恰一次」的枢纽语义由 T2 中枢实现保证（本票只钉值形态不可脱钩）；
  2. T3/T4 的 `PeerMdnsBrowse` glue 消费 `into_parts` 时必须继续成对持有 receiver + guard
     （只取 receiver 丢守卫即退订泄漏——类型文档已写明该约束）。
