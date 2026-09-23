# 14: wasip3 测试夹具落后 `pty_quota`（宿主 `cargo test --lib` 在 HEAD 即 6 红）

**What to build:** 让 `cargo test --lib` 在干净 dev 上回到零红。`PluginManifest` 早在
`c5e3d86d3`（P0–P1 前置链收口，声明式配额）就多了 `pty_quota: Option<usize>` 字段，
但宿主测试内构建的 WASM 夹具 `bedcode-plugin-wasip3-test` 的结构体字面量没跟演——
它不是「测试配置忘了改」，而是**测试内 fixture 构建链把结构体字面量当契约用**，
字段追加即编译红，且只在跑到依赖该夹具的用例时才暴露。

**Blocked by:** 无。

**Status:** needs-info

## 现象（2026-09-24 01:35 实测，`cargo test --lib` = 1145 passed / 8 failed）

```
Compiling bedcode-plugin-wasip3-test v0.1.0 (bedcode-desktop/packages/plugin-wasip3-test)
error[E0063]: missing field `pty_quota` in initializer of `PluginManifest`
  --> packages/plugin-wasip3-test/src/...:28  (fn manifest() -> PluginManifest { PluginManifest { … } })
```

红掉的 6 项全部是「测试内构建 wasip3 夹具」的用例：

| 用例 | 位置 |
| --- | --- |
| `a03_probe::a03_p1a_sync_host_impl_under_async_store` | `src/wasm_core/manager/runtime/tests/a03_probe.rs` |
| `a03_probe::a03_p1c_sync_call_under_async_store_mechanism` | 同上 |
| `a03_probe::a03_p2_fuel_async_semantics` | 同上 |
| `a03_probe::a03_p2_resource_limiter_async_semantics` | 同上 |
| `a03_probe::a03_p5_short_call_overhead_microbench` | 同上 |
| `wasi_e2e::test_wasip3_fixture_async_closure` | `…/tests/wasi_e2e.rs` |

## 归因（为什么不是后续改动带出来的）

- 字段来源：`packages/plugin-sdk-desktop/rust/src/types.rs:106 pub pty_quota: Option<usize>`，
  由 `c5e3d86d3` 引入，**该文件当前处于 HEAD 态**（无未提交改动）。
- 夹具侧：`bedcode-desktop/packages/plugin-wasip3-test/` **整体无未提交改动**（HEAD 态）。
- 两侧都在 HEAD ⇒ HEAD 自身即红，任何构建该夹具的用例都过不去。
- 与票 13 同族：P1-b 系列把契约往前推，**周边消费者（另一插件的镜像 trait / 测试内夹具）
  没有跟演**；差别是票 13 堵在插件构建链与 dev 起跑，本票堵在宿主测试门禁。
- 为什么 landed 时没暴露：dev 不触发 workflow（AGENTS §11），而 `test.yml` 的桌面段
  `cargo test` 会直接把这一片红带到合并时。

## 验收标准

- [ ] `cd bedcode-desktop/src-tauri && cargo test --lib` 零红（用上表 6 项转绿为判据）
- [ ] 夹具的 `PluginManifest` 构造改为**默认值 + 定点覆盖**（`..Default::default()` 或等价
      构造），使「SDK 追加可选字段」不再连带红——否则每次字段追加都要再修一次夹具
- [ ] 若裁决为「夹具必须显式列出每个字段」（把 SDK 字段追加变成强制跟演点），
      则改为在 ADR / AGENTS §7 插件检查清单里写明该隐式契约，并说明谁负责跟演
- [ ] 不改 `pty_quota` 的语义与区间仲裁（`PLUGIN_PTY_SESSIONS_CEILING_PER_PLUGIN` 等）

## 边界与不做

- 不动 SDK 的 manifest 字段定义（本票只修消费者）。
- 不顺手修票 13（不同机制、不同处置）。

## Comments

（发现于票 02 门禁实跑，2026-09-24。票 02 的 `--lib` 结果里这 6 项已按本票归因，
不计入票 02 的改动面。）

### 2026-09-24 01:35 · 现象已被并发批次就地补掉，本票只剩裁决点

`bedcode-desktop/packages/plugin-wasip3-test/src/lib.rs` 的在途 diff 就是加了一行
`pty_quota: None,`（作者：同日并发的
`.scratch/2026-09-24-host-crypto-business-downsink/` 线，其工作区改动，非本专项提交）。
效果实测：`cargo test --lib` 从 `1145 passed / 8 failed` 变 **`1153 passed / 0 failed`**。

因此本票的**紧急部分作废**，但两件事没被解决、也**不该由本会话代改对侧文件**：

1. **HEAD 仍然红**。该修复未提交，任何从 `90fdbcb8e` 起树跑 `--lib` 的人（含票 01 复跑、
   含 CI 桌面段）都会吃到这 6 项。判据：等对侧把它带进提交后，本票按「已随对侧落地」关闭。
2. **复发面没动**：修复形态是「字面量再补一格」，也就是**下次 SDK `PluginManifest`
   追加字段仍然编译红**——上面「验收标准」第 2/3 条（默认值构造 vs 显式逐字段的强制跟演点）
   仍是开放裁决。本票转 `needs-info`：留给对侧批次或本专项自取，但裁决要落在文档里，
   不能每次靠撞红才发现。
