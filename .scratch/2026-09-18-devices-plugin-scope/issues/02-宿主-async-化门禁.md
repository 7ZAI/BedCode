# 02: 宿主 async 化门禁（A0-3 + A1 + A4）

**What to build:** 桌面插件运行时从 wasip2 sync store 切换到 async store + wasi-p3 async linker（wasmtime-wasi 48 p3 模块）；全部既有 wasip2 插件零回归；wasip3 fixture 组件闭环（async `get-random-bytes` 返回熵 / async 时钟可读 / 燃料限额 async 语义生效，含续费复核 A4）。**这是 spec §10.6 的硬门禁**：p3 模块实验性风险在此把关，失败则回退「wasip2 产物 + async store」过渡，wasi3 仍为最终目标。

**Blocked by:** 01

**Status:** done（2026-09-19，门禁**通过**——证据见下；回退路径未触发）

## 门禁结论：通过 ✅

wasmtime-wasi 48 `p3` 模块 + CM_ASYNC async store 在桌面端全链路实证可用：
- wasip3 组件实例化（`instantiate_async`）✅
- async 导出调用（`call_async`，Store async-required 下同步 call 被强制拒绝）✅
- async `get-random-bytes` 返回熵 / async 时钟可读 ✅
- 燃料限额 async 语义生效（消耗记账 + Out of fuel trap + 续费）✅
- 既有 wasip2 / unknown-unknown 插件与 fixture 在 async store 下零回归 ✅

## 实施内容

1. **机制预验证**（/tmp/wasip3-probe）：独立探针实证 wasmtime 48 + CM_ASYNC +
   `p3::add_to_linker` + `Store::new`（48 无 new_async；`async_support()` 配置已废弃
   no-op）下：wasip3 组件 instantiate_async 成功、sync call 被拒、call_async 正确
   执行、燃料 1e6→996061 记账、燃料=1 trap；同步组件（零 wasi 导入）sync/async
   双路径均可用（7+8=15）——同步插件零回归的关键证明。
2. `bindgen!` 增 `exports: { default: async }`：全部导出绑定生成 async 变体
   （`call_*` → async fn，内部 `TypedFunc::call_async`），既有同步导出接口契约不变。
3. `Config::wasm_component_model_async(true)`（wasm_runtime.rs Engine 构建处）。
4. 实例化 `linker.instantiate` → `instantiate_async`（经既有 `block_on_async` 驱动，
   多线程/current_thread/无 runtime 三路径重入安全，含 wasm 宿主函数回环）。
5. 全部导出调用点（activate/deactivate/invoke/on_*/manifest/ws 回调/能力转发
   `call_capability_export`）→ `call_async` 包装；`call_capability_export` 增
   `Params: Send` / `Results: Send + 'static` 约束（block_on_async 未来边界）。
6. **p2（wasi 0.2）adapter 切换**：`p2::add_to_linker_sync` → `p2::add_to_linker_async`
   ——实证：sync adapter 在 async store 下被调用线程处于 tokio runtime 上下文时
   （block_on_async 已进入）内部 ambient block_on 重入 panic
   （`test_wasi_preopen_std_fs_e2e` 失败根因）；async adapter 在 fiber 内原生 await，
   与 wasip3 p3 同机制，preopen e2e 复绿。
7. **wasip3 fixture async 闭环测试**（`test_wasip3_fixture_async_closure`）：
   构建 packages/plugin-wasip3-test（pinned nightly + wasm32-wasip3，产物直出
   Component）→ async store 实例化 → `read-clock`（wasi:clocks，unix_ms>0）→
   `get-random`（getrandom 0.4，wasi:random get-random-bytes）：
   64 hex 非全零 + 两次调用结果不同（新鲜熵）。
8. Cargo.toml：`wasmtime-wasi = { version = "48", features = ["p3"] }`。

## 验证证据（2026-09-19 实测）

- `cargo test --lib`：**927 passed / 3 failed**——3 个失败全部在
  `pty::pty_process::tests`（并发 pty-base-service 线在途改动，非本票；其失败行
  552→565 随会话推进还在变动）。plugin::manager 全部 289 测试绿：
  - `test_component_roundtrip` / `test_sdk_plugin_component_roundtrip` /
    `test_sdk_plugin_binary_bus_roundtrip` / `test_sdk_plugin_api_call_roundtrip`
    —— 全部 fixture 在 async store 下往返通过
  - `test_wasi_preopen_std_fs_e2e` —— wasip2 std::fs preopen（async adapter 下复绿）
  - `test_ai_chatbox_wasip2_artifact_loads` —— wasip2 真实产物导入全部解析
  - `test_component_fuel_watchdog` / `test_component_fuel_exhaustion_traps` ——
    燃料 async 语义 + Out of fuel trap + 续费
  - `test_component_trap_poisons_store_and_reinstantiate_recovers` —— trap 隔离
  - `test_wasip3_fixture_async_closure` —— **新增**，A1 闭环（0.18s）
- `cargo check --lib` EXIT=0（非测试面无新增错误；unused import / clone_on_copy /
  useless_format 等 10 条告警均为改动前既有模式，见 §§）
- 移动端零改动；SDK guest 绑定零改动（`exports: { default: async }` 只作用于宿主
  侧 bindgen 生成的调用绑定，SDK 侧 wasm_entry! 产物接口不变——夹具即证）

## 边界与转票

- 存量 4 桌面插件切换 wasip3 产物 → 票 03（构建链 + CI + resources/plugins 重建）
- 中止点复核：`/tmp/wasip3-probe`、`/tmp/wasip3-validate` 为独立探针工程，未入仓库；
  结论沉淀于 docs/knowledge/wasip3-toolchain.md 与本节
- pty 线 3 失败 + pty_process.rs L210 lock-across-await 观察项 → 并行线在途，
  本票不触碰（AGENTS.md §11 在途改动禁止整文件回滚/越线修改）