# 桌面端插件宿主运行时加固 — 追加记录

> 关联：`.scratch/wasmtime-component-migration/handoff.md`（迁移主体已完结）、
> `.scratch/mobile-wasmtime-component-migration/`（移动端对应迁移）
> 日期：2026-08-15 · 会话：代码审查（Standards/Spec 双轴）后的加固收尾
> 状态：已实现，随工作区未提交改动一并落地

## 背景

组件模型迁移（tickets 01–09）与插件身份校验（fcecf0dbc）落地后，真机/调试会话暴露三类运行时问题，本次以「统一异常通道 + 阻塞重入安全 + 子进程清理体验」三个方向收尾。

## 改动清单

### 1. 插件运行时异常统一上报前端（host.rs）

- 新增 `PluginHost::notify_plugin_runtime_error(plugin_id, kind, error)`：
  - 覆盖三类场景：`panic`（宿主函数 panic 穿透 wasmtime，catch_unwind 兜底）、`trap`（wasm trap / 导出绑定失败 / store 中毒，已调度自动重载）、`recovery_failed`（自动重载失败，插件进入 Error 态）
  - 日志**始终**全量记录（重载循环期间不丢现场）；前端 toast 按插件节流（`PLUGIN_RUNTIME_ERROR_NOTIFY_INTERVAL_SECS = 15s`），trap 重载风暴只弹一次
  - 无 AppContext（测试/无头）降级为纯日志
- 事件名：`PLUGIN_RUNTIME_ERROR`（`system/constants/event.rs`），载荷 `{plugin_id, plugin_name, kind, error}`
- 测试：`notify_plugin_runtime_error_throttle_and_no_app_context`（节流表只记录一次 + 无头不 panic）

### 2. block_on_async 重入安全（wasm_runtime.rs）

- 问题实证：`dispatch_to_wasm → 插件 on_message → host http_fetch` 嵌套调用 `block_on_async`，嵌套 `block_in_place`/`handle.block_on` 均 panic（"Cannot start a runtime from within a runtime"，见 panic.log wasm_runtime.rs:82 FATAL），panic 穿透污染 wasmtime Store、插件永久不可用
- 方案：`thread_local!` 重入标志 + `BlockInPlaceGuard`（RAII，panic 穿透时 Drop 复位标志，避免线程残留 `true` 恒走新线程路径）；重入时改在**新线程上 block_on**（无 enter 守卫、非 worker，任意 flavor 合法），外层同步 join

### 3. 子进程清理不闪黑窗（wasm_runtime/host_impl/process.rs）

- Windows `taskkill /F /T` 增加 `CREATE_NO_WINDOW`（0x0800_0000），超时杀进程组时不再弹控制台黑窗

### 4. 实例生命周期日志（host/commands.rs、host/app_cli.rs、host/services.rs）

- catch_unwind 接管 + 统一异常通道接线；组件实例 Drop 记录生命周期日志；命令/服务注册错误带上下文

## 验证

- 桌面端主 crate `cargo check` 0 error；`cargo test --lib notify_plugin_runtime_error` 通过
- 移动端 `component.rs` 同构小改（生命周期日志），随移动端测试回归

## 遗留

- 移动端 wasmtime 组件迁移的 host_impl 尚未接入同一统一异常通道（移动端无前端 toast 需求，暂以日志为准）
