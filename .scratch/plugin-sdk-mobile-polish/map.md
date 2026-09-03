# Map: Plugin SDK Mobile 完善

## Notes

- 用户明确：架构框架参考桌面端，功能函数不照抄（移动端场景不同）。
- 任务按依赖排序：01 CommandArgs → 02 host traits → 03 WasmHost impl → 04 abi → 05 invoke_command 签名 → 06 mark_plugin_error（SDK）→ 07 宿主闭环 → 08 签名表校验 → 09 Dialog → 10 前端状态上报 → 11 Notification → 12 迁移+测试 → 13 遗留处理（权限对齐 + ABI v3）。
- 宿主已依赖 SDK（Cargo `bedcode-plugin-api-mobile` + tsconfig alias `@binblink/plugin-sdk-mobile`），无需新增依赖。

## Decisions so far

- [x] 01 CommandArgs：直接移植桌面端实现（通用工具，无平台差异）→ 见 01 号 issue Answer
- [x] 02 host/ trait 体系：架构借鉴，功能按移动端现状（9 个子 trait，去桌面特有能力）
- [x] 03 WasmHost 实现全部子 trait：unit struct + 纯 trait 方法 + Result<_, HostError> 错误语义
- [x] 04 abi.rs 名称常量 + 签名表：v2 修复 on_app_startup 导出名漂移
- [x] 05 invoke_command 统一为 `(name, args: Value)`：宏内解析，线协议不变
- [x] 06 host_mark_plugin_error（SDK 侧）：HostLog trait 方法 + extern + abi 签名
- [x] 07 宿主注册 mark_plugin_error + 状态闭环：status_reporter 回调（Error + 持久化 + 前端通知）
- [x] 08 宿主 ABI 签名表校验：WasmRuntime::verify_abi（临时 Store + linker.get）
- [x] 09 DialogAPI：SDK 类型 + 宿主 dialog-host 队列 + PluginDialogHost.vue 渲染 + 单测
- [x] 10 前端 status API：StatusAPI + plugin_report_ready command（Error → Activated 自愈）
- [x] 11 NotificationAPI：tauri-plugin-notification 封装
- [x] 12 迁移 + 全量测试：两个插件适配，SDK/宿主/前端测试全绿，清理 wasm-bindgen
- [x] 13 遗留处理：
  - 权限名统一（前端 `ui:navtab` / `ui:input`，auto-task manifest 同步）
  - ABI v3：out_ptr 结果传递 + `__bedcode_abi_version` + `__bedcode_deallocate` 配对回收
    （消除 FFI-safe 警告 + 线性内存泄漏），宿主实例化时 ABI 版本协商
  - 两个插件 wasm32 导出签名逐项比对签名表 OK，全部构建/测试通过
- [x] 14 遗留修复：
  - **WASM 插件权限校验**：WasmPluginState 注入 granted_permissions，13 个敏感 host fn
    调用前校验（storage/terminal:input/network:http/fs:read/fs:write/bus），storage 默认授予
  - ai-chatbox manifest 补 network:http
  - auto-task 清理 wasm-bindgen；WasmRuntime::new 未用参数、loader.rs import、
    前端 validatePermissions/VALID_PERMISSIONS 死代码删除
  - plugin/ 编译警告清零，全部构建/测试通过

## 完成状态

全部 14 个任务完成。验证全绿：宿主 cargo test 46 passed、SDK 2 passed、
前端 vitest 12 passed、vue-tsc 无错误、`npm run build` 成功、两个插件 wasm32 编译成功。

## 备注

- 移动端 ABI 现为 v3，与桌面端 out_ptr 方案一致（线协议相同风格）。
- 前端 `ui:input` 权限名语义为"终端工具栏注册"（继承 Rust 侧定义，命名保留）。
- WASM host fn 权限校验已落地；emit_event/notify/log/mark_plugin_error 及
  session noop 不校验（设计内）。
