# Spec: Plugin SDK Mobile 完善（对齐桌面端）

## 背景

移动端插件 SDK（`bedcode-mobile/packages/plugin-sdk-mobile`）相对桌面端（`plugin-sdk-desktop`）存在明显差距：

1. **Rust 侧**：缺 `args.rs`（CommandArgs）、`host/` trait 体系（HostApi 共同契约）、`abi.rs`（ABI 单一事实来源）；`WasmPlugin::invoke_command` 签名与桌面端不一致（`&str` vs `Value`）；无生命周期失败上报（`mark_plugin_error`）。
2. **前端侧**：缺弹窗（Dialog）、通知、状态上报等扩展能力；I18n 已具备。
3. **宿主侧**：已依赖 SDK（Cargo + tsconfig alias），但 `wasm_runtime.rs` 手工注册 host functions，无 ABI 签名表约束，契约漂移风险。

## 目标

1. **共同 trait 约束函数签名**：移植桌面端 `host/` trait 体系（`HostApi` + 子 trait + `HostError`），`WasmHost` 实现全部子 trait，宿主侧 `WasmHostContext` 同样实现 —— 两端共享同一契约，签名漂移在编译期暴露。
2. **插件生命周期上报**：启用时插件可通过生命周期函数上报启动成功/失败。Rust/WASM 侧对齐桌面端 ABI v4 的 `host_mark_plugin_error`；前端 TS 插件增加 `context.reportStatus()` 等上报 API，loader 集成。
3. **前端完备扩展性**：新增 DialogAPI（弹窗）、NotificationAPI，补齐 I18nAPI 周边能力。
4. **宿主依赖收口**：宿主 Rust + 前端均以 SDK 为单一依赖源，移除手工契约。

## 非目标

- 不移植桌面端 SQL 前缀隔离、会话监听等桌面特有能力（移动端会话走 WebSocket，语义不同）。
- 不做 WASM ABI 大版本升级（保持 v1 元组返回线协议，避免宿主/插件联动返工），仅在 SDK 内部统一 trait 签名。

## 验收标准

- `cargo test`（SDK rust + 宿主）通过。
- `npm run test:run`（宿主前端）通过。
- ai-chatbox / auto-task 插件编译通过并适配新签名。
- 插件可通过生命周期函数上报启动成功/失败，宿主状态正确流转（Activated / Error）。
- 前端插件可调用 `context.dialogs` 弹出对话框，宿主渲染正常。
