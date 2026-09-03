# 08 — 宿主 ABI 签名表校验（契约防漂移）

Type: task
Status: resolved
Blocked by: 04

## 问题

宿主 `register_host_functions` 手工注册 21 个 host function，与 SDK 侧 `extern "C"` 声明
无编译期约束。任一名称/参数数/返回数不一致都静默漂移。

## 任务

1. 宿主 `wasm_runtime.rs`：注册完成后遍历 `bedcode_plugin_api_mobile::abi::HOST_FN_SIGNATURES`，
   用 `linker.get(NAMESPACE, name)` 检查函数存在且 `func.ty().params().len()` / `results().len()` 匹配。
2. 不匹配返回错误（插件系统启动失败即暴露，而非运行时崩溃）。
3. 不需要为移动端插件导出加签名校验测试（WASM 模块由 plugins 独立编译，非宿主职责）。

## Answer

## Answer

WasmRuntime::verify_abi(&self, host_ctx)：创建临时 Store 遍历 HOST_FN_SIGNATURES，
用 linker.get(store, NAMESPACE, name) 解析 Extern → into_func() → ty(&store)，
校验参数数/返回值数与签名表一致，漂移返回 AppError::Plugin（启动期暴露）。
manager.rs init_wasm_runtime 在注入 host_ctx 后调用。宿主 cargo check 通过。

## 验收

- 宿主 `cargo test` 通过（含一个故意改签名即失败的测试）。
- 现有 21 + 1（mark_plugin_error）个 host function 全部在签名表内且注册一致。
